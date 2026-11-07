//! Read-only bounded centered-grid sampling. No images or pointers are retained.
use micopi_plugin_api as abi;
#[cfg(test)]
#[path = "histogram_cost.rs"]
mod cost;
#[path = "histogram_plan.rs"]
mod plan;
unsafe extern "C" fn sample(
    request: *const abi::HistogramRequest,
    data: *const u32,
    len: usize,
    output: *mut abi::HistogramResult,
) -> i32 {
    if request.is_null()
        || data.is_null()
        || output.is_null()
        || (data as usize) % std::mem::align_of::<u32>() != 0
    {
        return abi::ERROR;
    }
    let header = std::ptr::read_unaligned(request.cast::<abi::Header>());
    let destination_header = std::ptr::read_unaligned(output.cast::<abi::Header>());
    if header.abi_version != 1
        || header.struct_size < std::mem::size_of::<abi::HistogramRequest>() as u32
        || destination_header.abi_version != 1
        || destination_header.struct_size < std::mem::size_of::<abi::HistogramResult>() as u32
    {
        return abi::ERROR;
    }
    let p = std::ptr::read_unaligned(request);
    if !(1..=4096).contains(&p.width)
        || !(1..=4096).contains(&p.height)
        || p.stride < p.width
        || !(1..=abi::HISTOGRAM_MAX_SAMPLES).contains(&p.max_samples)
    {
        return abi::ERROR;
    }
    let Some(used) = (p.height as usize - 1)
        .checked_mul(p.stride as usize)
        .and_then(|n| n.checked_add(p.width as usize))
    else {
        return abi::ERROR;
    };
    if used > len {
        return abi::ERROR;
    }
    let Some(source) = super::span(data, len) else {
        return abi::ERROR;
    };
    let Some(parameters) = super::span(request, 1) else {
        return abi::ERROR;
    };
    let Some(destination) = super::span(output, 1) else {
        return abi::ERROR;
    };
    if super::overlaps(source, destination) || super::overlaps(parameters, destination) {
        return abi::ERROR;
    }
    let (gw, gh) = grid(&p);
    let mut result = abi::HistogramResult {
        grid_width: gw,
        grid_height: gh,
        sampled_pixels: u64::from(gw) * u64::from(gh),
        ..Default::default()
    };
    for row in 0..gh {
        let y = ((2 * row as usize + 1) * p.height as usize) / (2 * gh as usize);
        for col in 0..gw {
            let x = ((2 * col as usize + 1) * p.width as usize) / (2 * gw as usize);
            let pixel = *data.add(y * p.stride as usize + x);
            if pixel >> 24 == 0 {
                continue;
            }
            result.nontransparent_pixels += 1;
            for (channel, shift) in [16, 8, 0].into_iter().enumerate() {
                let value = ((pixel >> shift) & 255) as usize;
                result.bins[channel][value] += 1;
                result.bins[3][value] += 1;
            }
        }
    }
    std::ptr::write_unaligned(output, result);
    abi::OK
}

fn grid(p: &abi::HistogramRequest) -> (u32, u32) {
    let area = u64::from(p.width) * u64::from(p.height);
    if area <= u64::from(p.max_samples) {
        (p.width, p.height)
    } else {
        let gw = ((f64::from(p.max_samples) * f64::from(p.width) / f64::from(p.height))
            .sqrt()
            .floor() as u32)
            .clamp(1, p.width.min(p.max_samples));
        (gw, p.height.min(p.max_samples / gw).max(1))
    }
}
static API: abi::HistogramApiPlan = abi::HistogramApiPlan {
    base: abi::HistogramApi {
        abi_version: 1,
        struct_size: std::mem::size_of::<abi::HistogramApiPlan>() as u32,
        sample: Some(sample),
    },
    sample_plan: Some(plan::sample_plan),
};
#[no_mangle]
pub extern "C" fn micopi_histogram_v1() -> *const abi::HistogramApi {
    &API.base
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(w: u32, h: u32, stride: u32, budget: u32) -> abi::HistogramRequest {
        abi::HistogramRequest {
            abi_version: 1,
            struct_size: 24,
            width: w,
            height: h,
            stride,
            max_samples: budget,
        }
    }
    #[test]
    fn exact_bins_ignore_zero_alpha_count_partial_alpha_once_and_skip_padding() {
        let data = [
            0x00010203, 0x010a141e, 0x800a1428, 0xff001428, 0xffeeeeee, 0xffeeeeee,
        ];
        let p = request(2, 2, 3, 65536);
        let mut out = abi::HistogramResult::default();
        assert_eq!(
            unsafe { sample(&p, data.as_ptr(), data.len(), &mut out) },
            abi::OK
        );
        assert_eq!(
            (
                out.grid_width,
                out.grid_height,
                out.sampled_pixels,
                out.nontransparent_pixels
            ),
            (2, 2, 4, 3)
        );
        assert_eq!(out.bins[0][10], 1);
        assert_eq!(out.bins[0][0], 1);
        assert_eq!(out.bins[0][238], 1);
        assert_eq!(out.bins[1][20], 2);
        assert_eq!(out.bins[2][40], 1);
        for i in 0..256 {
            assert_eq!(
                out.bins[3][i],
                out.bins[0][i] + out.bins[1][i] + out.bins[2][i]
            );
        }
        assert_eq!(out.bins[3].iter().sum::<u64>(), 9);
    }
    #[test]
    fn centered_sampling_has_independent_two_dimensional_witnesses() {
        let data = (0..16).map(|v| 0xff000000 | v).collect::<Vec<_>>();
        let mut out = abi::HistogramResult::default();
        assert_eq!(
            unsafe { sample(&request(4, 4, 4, 4), data.as_ptr(), 16, &mut out) },
            abi::OK
        );
        assert_eq!((out.grid_width, out.grid_height), (2, 2));
        for i in 0..256 {
            assert_eq!(out.bins[2][i], u64::from([5, 7, 13, 15].contains(&i)));
        }
        assert_eq!(out.bins[0][0], 4);
        assert_eq!(out.bins[1][0], 4);
    }
    #[test]
    fn thin_extreme_and_irregular_grids_never_exceed_the_sampling_budget() {
        for (w, h, budget) in [
            (4096, 1, 1),
            (1, 4096, 1),
            (4096, 2, 1000),
            (17, 19, 31),
            (256, 257, 65536),
        ] {
            let data = vec![0x01010203; (w * h) as usize];
            let mut out = abi::HistogramResult::default();
            assert_eq!(
                unsafe {
                    sample(
                        &request(w, h, w, budget),
                        data.as_ptr(),
                        data.len(),
                        &mut out,
                    )
                },
                abi::OK
            );
            assert!(out.sampled_pixels > 0 && out.sampled_pixels <= u64::from(budget));
            assert_eq!(out.nontransparent_pixels, out.sampled_pixels);
            assert_eq!(out.bins[0][1], out.sampled_pixels);
            assert!(out.grid_width <= w && out.grid_height <= h);
        }
    }
    #[test]
    fn future_prefixes_keep_tail_bytes_and_source_pixels_intact() {
        #[repr(C)]
        struct FutureRequest {
            base: abi::HistogramRequest,
            tail: [u8; 8],
        }
        #[repr(C)]
        struct FutureResult {
            base: abi::HistogramResult,
            tail: [u8; 16],
        }
        let p = FutureRequest {
            base: abi::HistogramRequest {
                struct_size: 32,
                ..request(2, 2, 2, 4)
            },
            tail: [71; 8],
        };
        let mut out = FutureResult {
            base: abi::HistogramResult {
                struct_size: std::mem::size_of::<FutureResult>() as u32,
                ..Default::default()
            },
            tail: [73; 16],
        };
        let source = [0x80112233; 4];
        assert_eq!(
            unsafe { sample(&p.base, source.as_ptr(), source.len(), &mut out.base) },
            abi::OK
        );
        assert_eq!(out.base.struct_size, 8224);
        assert_eq!(out.tail, [73; 16]);
        assert_eq!(p.tail, [71; 8]);
        assert_eq!(source, [0x80112233; 4]);
    }
    #[test]
    fn invalid_headers_capacity_and_aliases_leave_the_output_unchanged() {
        let data = [0xff112233; 4];
        let good = request(2, 2, 2, 4);
        for case in 0..9 {
            let mut p = good;
            let mut out = abi::HistogramResult::default();
            out.sampled_pixels = 99;
            match case {
                0 => p.abi_version = 2,
                1 => p.struct_size = 8,
                2 => p.width = 0,
                3 => p.height = 4097,
                4 => p.stride = 1,
                5 => p.max_samples = 0,
                6 => p.max_samples = 65537,
                7 => out.abi_version = 2,
                _ => out.struct_size = 8,
            }
            assert_eq!(
                unsafe { sample(&p, data.as_ptr(), 4, &mut out) },
                abi::ERROR
            );
            assert_eq!(out.sampled_pixels, 99);
            assert_eq!(out.bins, [[0; 256]; 4]);
        }
        let mut out = abi::HistogramResult::default();
        out.sampled_pixels = 99;
        unsafe {
            assert_eq!(sample(&good, data.as_ptr(), 3, &mut out), abi::ERROR);
            assert_eq!(
                sample(std::ptr::null(), data.as_ptr(), 4, &mut out),
                abi::ERROR
            );
            assert_eq!(sample(&good, std::ptr::null(), 4, &mut out), abi::ERROR);
            assert_eq!(
                sample(&good, data.as_ptr(), 4, std::ptr::null_mut()),
                abi::ERROR
            );
            let header = abi::Header {
                abi_version: 1,
                struct_size: 8,
            };
            assert_eq!(
                sample(
                    (&header as *const abi::Header).cast(),
                    data.as_ptr(),
                    4,
                    &mut out
                ),
                abi::ERROR
            );
            assert_eq!(
                sample(
                    &good,
                    (&out as *const abi::HistogramResult).cast(),
                    std::mem::size_of_val(&out) / 4,
                    &mut out
                ),
                abi::ERROR
            );
        }
        assert_eq!(out.sampled_pixels, 99);
    }
}
