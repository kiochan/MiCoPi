//! Opt-in exported quantization callbacks; never an end-to-end latency claim.
use super::*;
fn reference(pixel: u32, kind: u32, level: u32) -> u32 {
    if pixel >> 24 == 0 {
        return 0;
    }
    let r = (pixel >> 16) & 255;
    let g = (pixel >> 8) & 255;
    let b = pixel & 255;
    let rgb = if kind == 12 {
        let gray = (r as f64 * 54. + g as f64 * 183. + b as f64 * 19.) / 256.;
        if gray >= level as f64 {
            0xffffff
        } else {
            0
        }
    } else {
        let n = (level - 1) as f64;
        let q = |c: u32| ((c as f64 * n / 255.).round() * 255. / n).round() as u32;
        q(r) << 16 | q(g) << 8 | q(b)
    };
    (pixel & 0xff000000) | rgb
}
fn histogram_reference(
    side: usize,
    budget: u32,
    pixels: &[u32],
    kind: u32,
    level: u32,
) -> abi::HistogramResult {
    let grid = (budget as f64).sqrt() as usize;
    assert_eq!(grid * grid, budget as usize);
    let mut result = abi::HistogramResult {
        grid_width: grid as u32,
        grid_height: grid as u32,
        sampled_pixels: budget as u64,
        ..Default::default()
    };
    for row in 0..grid {
        for column in 0..grid {
            let x = (2 * column + 1) * side / (2 * grid);
            let y = (2 * row + 1) * side / (2 * grid);
            let pixel = reference(pixels[y * side + x], kind, level);
            if pixel >> 24 == 0 {
                continue;
            }
            result.nontransparent_pixels += 1;
            for (channel, shift) in [16, 8, 0].into_iter().enumerate() {
                let byte = ((pixel >> shift) & 255) as usize;
                result.bins[channel][byte] += 1;
                result.bins[3][byte] += 1;
            }
        }
    }
    result
}
#[test]
#[ignore = "Opt-in Release quantization ROI and bounded histogram cost"]
fn bounded_histogram_cost_quantization_regions_and_sampling() {
    assert!(
        !cfg!(debug_assertions),
        "Run this ignored measurement with --release"
    );
    let pointer = crate::hsl::micopi_quantization_v1();
    assert!(!pointer.is_null());
    let header = unsafe { std::ptr::read_unaligned(pointer.cast::<abi::Header>()) };
    assert_eq!(header.abi_version, 1);
    assert!(header.struct_size >= std::mem::size_of::<abi::QuantizationApi>() as u32);
    let api = unsafe { &*pointer };
    let render = black_box(api.render.unwrap());
    let sample = black_box(api.histogram.unwrap());
    println!("Scope: exported quantization callbacks on prepared 1024/4096-square immutable RGBA; threshold 54 and posterize 4/7/256, 32/256-square padded center ROIs and 4096/65536 histogram samples. Includes callback preflight, per-call stack/table preparation and evaluation. Excludes input/reference/output allocation, Session/history/replay, PNG/base64, GUI/GPU, DLL loading, process peak and physical tablet latency. One first and nine warm calls; no machine-specific time threshold.");
    for side in [1024usize, 4096] {
        let pixels = (0..side * side).map(pixel).collect::<Vec<_>>();
        let layer = abi::CompositeLayer {
            data: pixels.as_ptr(),
            len: pixels.len(),
            opacity: 255,
            visible: 1,
            mode: 0,
        };
        for (label, kind, level) in [
            ("threshold54", 12, 54),
            ("posterize4", 13, 4),
            ("posterize7", 13, 7),
            ("posterize256", 13, 256),
        ] {
            let nodes = [
                node(0, 0, 0, 0, 1.),
                node(kind, 0, 0, level, 1.),
                node(3, 1, 0, 0, 1.),
            ];
            for roi in [32, 256] {
                measure_plan_roi(
                    &format!("quantization_{label}"),
                    side,
                    roi,
                    &pixels,
                    render,
                    &nodes,
                    &[],
                    |p| reference(p, kind, level),
                );
            }
            for budget in [4096, 65536] {
                let request = abi::HistogramRequest {
                    abi_version: 1,
                    struct_size: 24,
                    width: side as u32,
                    height: side as u32,
                    stride: side as u32,
                    max_samples: budget,
                };
                let expected = histogram_reference(side, budget, &pixels, kind, level);
                measure(
                    &format!("quantization_{label}_{side}_{budget}"),
                    &expected,
                    |output| unsafe {
                        sample(
                            black_box(&request),
                            black_box(nodes.as_ptr()),
                            nodes.len(),
                            black_box(&layer),
                            1,
                            std::ptr::null(),
                            0,
                            std::ptr::null(),
                            0,
                            std::ptr::null(),
                            0,
                            output,
                        )
                    },
                );
            }
        }
        assert!(pixels
            .iter()
            .copied()
            .enumerate()
            .all(|(i, p)| p == pixel(i)));
    }
}
