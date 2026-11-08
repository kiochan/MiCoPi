//! Opt-in exported-callback measurements, separate from host/UI/tablet latency.
use super::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    time::Instant,
};

struct Allocator;
thread_local! {
    static TRACK: Cell<bool> = const {Cell::new(false)};
    static ALLOCATIONS: Cell<usize> = const {Cell::new(0)};
}
fn allocated() {
    let _ = TRACK.try_with(|track| {
        if track.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocated();
        System.alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocated();
        System.alloc_zeroed(layout)
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocated();
        System.realloc(pointer, layout, size)
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        System.dealloc(pointer, layout)
    }
}
// This module is test-only. Production plugin allocation behavior is unchanged.
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn pixel(index: usize) -> u32 {
    let alpha = match index % 11 {
        0 => 0,
        1 => 37,
        _ => 255,
    };
    (alpha << 24)
        | (((index * 17 + 31) as u32 & 255) << 16)
        | (((index / 97 + 71) as u32 & 255) << 8)
        | ((index / 13 + 23) as u32 & 255)
}
fn coverage(index: usize) -> u8 {
    ((index / 7 + index / 257) & 255) as u8
}
fn node(kind: u32, a: u32, b: u32, mode: u32, amount: f32) -> abi::CompositeNode {
    abi::CompositeNode {
        kind,
        a,
        b,
        mode,
        amount,
    }
}

fn reference(side: usize, budget: u32, masked: bool) -> abi::HistogramResult {
    // All measured canvases/budgets are exact squares. Deliberately do not use
    // the production grid, filter, alpha, LUT or instruction evaluator helpers.
    let grid = match budget {
        4096 => 64,
        65536 => 256,
        _ => unreachable!(),
    };
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
            let index = y * side + x;
            let value = pixel(index);
            let alpha = value >> 24;
            if alpha == 0 || masked && alpha * u32::from(coverage(index)) / 255 == 0 {
                continue;
            }
            result.nontransparent_pixels += 1;
            let mut rgb = [(value >> 16) & 255, (value >> 8) & 255, value & 255];
            if masked {
                rgb = [(2 * (255 - rgb[0])).min(255), 255 - rgb[1], rgb[2]];
            }
            for (channel, value) in rgb.into_iter().enumerate() {
                result.bins[channel][value as usize] += 1;
                result.bins[3][value as usize] += 1;
            }
        }
    }
    result
}

fn measure(
    label: &str,
    expected: &abi::HistogramResult,
    mut call: impl FnMut(&mut abi::HistogramResult) -> i32,
) {
    const WARM: usize = 9;
    let mut times = [0.; WARM];
    let mut output = abi::HistogramResult::default();
    let mut first = 0.;
    for iteration in 0..=WARM {
        ALLOCATIONS.with(|count| count.set(0));
        TRACK.with(|track| track.set(true));
        let started = Instant::now();
        let status = call(black_box(&mut output));
        let elapsed = started.elapsed().as_secs_f64() * 1000.;
        TRACK.with(|track| track.set(false));
        assert_eq!(status, abi::OK);
        assert_eq!(
            ALLOCATIONS.with(Cell::get),
            0,
            "{label}: callback allocated"
        );
        assert_eq!(
            (
                output.grid_width,
                output.grid_height,
                output.sampled_pixels,
                output.nontransparent_pixels
            ),
            (
                expected.grid_width,
                expected.grid_height,
                expected.sampled_pixels,
                expected.nontransparent_pixels
            )
        );
        assert_eq!(
            output.bins, expected.bins,
            "{label}: independent centered sample reference"
        );
        if iteration == 0 {
            first = elapsed;
        } else {
            times[iteration - 1] = elapsed;
        }
    }
    times.sort_by(f64::total_cmp);
    println!("histogram_cost scope={label} samples={} first_ms={first:.6} warm_median_ms={:.6} warm_p95_ms={:.6} callback_allocations=0 independent_bins=true",expected.sampled_pixels,times[WARM/2],times[WARM-1]);
}

#[test]
#[ignore = "Opt-in Release native histogram allocation and sampling measurement"]
fn bounded_histogram_cost() {
    assert!(
        !cfg!(debug_assertions),
        "Run this ignored measurement with --release"
    );
    let base = super::micopi_histogram_v1();
    assert!(!base.is_null());
    assert_eq!(unsafe { (*base).abi_version }, 1);
    assert!(
        unsafe { (*base).struct_size as usize } >= std::mem::size_of::<abi::HistogramApiPlan>()
    );
    let api = unsafe { &*base.cast::<abi::HistogramApiPlan>() };
    let raw = api.base.sample.unwrap();
    let plan = api.sample_plan.unwrap();
    println!("Scope: exported typed callbacks, 1024/4096 square sources, 4096/65536 centered samples, 2/64 instruction plans and masked master/RGB tables. First/warm calls include preflight plus sampling; exclude source/table preparation, host caches/history/serialization/UI and physical tablet latency. Thread-local test allocator counts callback alloc/zeroed/realloc attempts. No machine-specific timing assertions; no image output allocation.");
    for side in [1024usize, 4096] {
        let pixels = (0..side * side).map(pixel).collect::<Vec<_>>();
        let mask = (0..side * side).map(coverage).collect::<Vec<_>>();
        let layers = [abi::CompositeLayer {
            data: pixels.as_ptr(),
            len: pixels.len(),
            opacity: 255,
            visible: 1,
            mode: 0,
        }];
        let masks = [abi::CompositeMask {
            data: mask.as_ptr(),
            len: mask.len(),
            stride: side as u32,
            constant: 0,
        }];
        let simple = [node(0, 0, 0, 0, 1.), node(3, 0, 0, 0, 1.)];
        let mut chain = vec![node(0, 0, 0, 0, 1.)];
        for i in 1..63 {
            chain.push(node(1, i - 1, 0, 0, 1.));
        }
        chain.push(node(3, 62, 0, 0, 1.));
        let mixed = [
            node(0, 0, 0, 0, 1.),
            node(6, 0, 0, 0, 0.),
            node(7, 0, 1, 0, 1.),
            node(9, 2, 0, 0, 1.),
            node(10, 3, 0, 1, 1.),
            node(3, 4, 0, 0, 1.),
        ];
        let tables = [
            abi::CompositeLut {
                values: std::array::from_fn(|i| 255 - i as u8),
            },
            abi::CompositeLut {
                values: std::array::from_fn(|i| (i * 2).min(255) as u8),
            },
            abi::CompositeLut {
                values: std::array::from_fn(|i| i as u8),
            },
            abi::CompositeLut {
                values: std::array::from_fn(|i| 255 - i as u8),
            },
        ];
        for budget in [4096, 65536] {
            let request = abi::HistogramRequest {
                abi_version: 1,
                struct_size: std::mem::size_of::<abi::HistogramRequest>() as u32,
                width: side as u32,
                height: side as u32,
                stride: side as u32,
                max_samples: budget,
            };
            let expected = reference(side, budget, false);
            measure(&format!("raw_{side}"), &expected, |out| unsafe {
                raw(
                    black_box(&request),
                    black_box(pixels.as_ptr()),
                    pixels.len(),
                    out,
                )
            });
            for (name, nodes, with_masks) in [
                ("identity_2", simple.as_slice(), false),
                ("invert_64", chain.as_slice(), false),
                ("mask_master_rgb_6", mixed.as_slice(), true),
            ] {
                let expected = if with_masks {
                    reference(side, budget, true)
                } else {
                    expected
                };
                measure(&format!("{name}_{side}"), &expected, |out| unsafe {
                    plan(
                        black_box(&request),
                        black_box(nodes.as_ptr()),
                        nodes.len(),
                        layers.as_ptr(),
                        layers.len(),
                        masks.as_ptr(),
                        if with_masks { 1 } else { 0 },
                        tables.as_ptr(),
                        if with_masks { 4 } else { 0 },
                        out,
                    )
                });
            }
        }
        assert!(pixels
            .iter()
            .copied()
            .enumerate()
            .all(|(i, p)| p == pixel(i)));
        assert!(mask
            .iter()
            .copied()
            .enumerate()
            .all(|(i, p)| p == coverage(i)));
    }
}

// A 180-degree HSL turn reflects each byte about (min + max)/2. This integer
// oracle uses no production HSL conversion, grid, alpha or evaluator helpers.
fn half_turn(value: u32) -> u32 {
    if value >> 24 == 0 {
        return 0;
    }
    let [r, g, b] = [16, 8, 0].map(|shift| (value >> shift) & 255);
    let sum = r.max(g).max(b) + r.min(g).min(b);
    (value & 0xff000000) | ((sum - r) << 16) | ((sum - g) << 8) | (sum - b)
}
fn hsl_reference(side: usize, budget: u32) -> abi::HistogramResult {
    let grid = match budget {
        4096 => 64,
        65536 => 256,
        _ => unreachable!(),
    };
    let step = side / grid;
    let mut result = abi::HistogramResult {
        grid_width: grid as u32,
        grid_height: grid as u32,
        sampled_pixels: budget as u64,
        ..Default::default()
    };
    for row in 0..grid {
        for column in 0..grid {
            let value = half_turn(pixel(
                (row * step + step / 2) * side + column * step + step / 2,
            ));
            if value >> 24 == 0 {
                continue;
            }
            result.nontransparent_pixels += 1;
            for (channel, shift) in [16, 8, 0].into_iter().enumerate() {
                let byte = ((value >> shift) & 255) as usize;
                result.bins[channel][byte] += 1;
                result.bins[3][byte] += 1;
            }
        }
    }
    result
}
fn measure_plan_roi(
    label: &str,
    side: usize,
    roi: usize,
    pixels: &[u32],
    callback: abi::HslRenderFn,
    nodes: &[abi::CompositeNode],
    parameters: &[abi::HslParameters],
    reference: impl Fn(u32) -> u32,
) {
    const WARM: usize = 9;
    const PADDING: u32 = 0xdeadbeef;
    let callback = black_box(callback);
    let stride = roi + 8;
    let origin = (side - roi) / 2;
    let layer = abi::CompositeLayer {
        data: pixels.as_ptr(),
        len: pixels.len(),
        opacity: 255,
        visible: 1,
        mode: 0,
    };
    let expected = (0..roi * stride)
        .map(|i| {
            if i % stride >= roi {
                PADDING
            } else {
                reference(pixels[(origin + i / stride) * side + origin + i % stride])
            }
        })
        .collect::<Vec<_>>();
    let mut output = vec![PADDING; roi * stride];
    let allocation = (output.as_ptr(), output.capacity());
    let mut target = abi::Pixels {
        data: output.as_mut_ptr(),
        len: output.len(),
        width: roi as u32,
        height: roi as u32,
        stride: stride as u32,
        origin_x: origin as u32,
        origin_y: origin as u32,
    };
    let mut times = [0.; WARM];
    let mut first = 0.;
    for iteration in 0..=WARM {
        ALLOCATIONS.with(|count| count.set(0));
        TRACK.with(|track| track.set(true));
        let started = Instant::now();
        let status = unsafe {
            callback(
                black_box(nodes.as_ptr()),
                nodes.len(),
                black_box(&layer),
                1,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                black_box(parameters.as_ptr()),
                parameters.len(),
                side as u32,
                side as u32,
                black_box(&mut target),
            )
        };
        let elapsed = started.elapsed().as_secs_f64() * 1000.;
        TRACK.with(|track| track.set(false));
        assert_eq!(status, abi::OK);
        assert_eq!(ALLOCATIONS.with(Cell::get), 0);
        if let Some((index, (actual, wanted))) = output
            .iter()
            .zip(&expected)
            .enumerate()
            .find(|(_, (a, b))| a != b)
        {
            panic!("{label} ROI mismatch source={side} roi={roi} index={index} actual={actual:08x} expected={wanted:08x}");
        }
        assert_eq!((output.as_ptr(), output.capacity()), allocation);
        assert_eq!(
            (
                target.data as usize,
                target.len,
                target.width,
                target.height,
                target.stride,
                target.origin_x,
                target.origin_y
            ),
            (
                allocation.0 as usize,
                roi * stride,
                roi as u32,
                roi as u32,
                stride as u32,
                origin as u32,
                origin as u32
            )
        );
        if iteration == 0 {
            first = elapsed;
        } else {
            times[iteration - 1] = elapsed;
        }
    }
    times.sort_by(f64::total_cmp);
    println!("{label}_render_cost source_side={side} roi_side={roi} stride={stride} first_ms={first:.6} warm_median_ms={:.6} warm_p95_ms={:.6} callback_allocations=0 output_allocation_reused=true independent_rgba_and_padding=true",times[WARM/2],times[WARM-1]);
}

fn measure_hsl_roi(side: usize, roi: usize, pixels: &[u32], callback: abi::HslRenderFn) {
    let nodes = [
        node(0, 0, 0, 0, 1.),
        node(11, 0, 0, 0, 1.),
        node(3, 1, 0, 0, 1.),
    ];
    let parameters = [abi::HslParameters {
        hue: 180.,
        saturation: 0.,
        lightness: 0.,
    }];
    measure_plan_roi(
        "hsl",
        side,
        roi,
        pixels,
        callback,
        &nodes,
        &parameters,
        half_turn,
    );
}

#[test]
#[ignore = "Opt-in Release native HSL dirty-region and sampling cost"]
fn bounded_histogram_cost_hsl_regions_and_sampling() {
    assert!(
        !cfg!(debug_assertions),
        "Run this ignored measurement with --release"
    );
    let pointer = crate::hsl::micopi_hsl_v1();
    assert!(!pointer.is_null());
    let header = unsafe { std::ptr::read_unaligned(pointer.cast::<abi::Header>()) };
    assert_eq!(header.abi_version, 1);
    assert!(header.struct_size >= std::mem::size_of::<abi::HslApi>() as u32);
    let api = unsafe { &*pointer };
    let render = api.render.unwrap();
    let sample = black_box(api.histogram.unwrap());
    println!("Scope: negotiated exported HSL callbacks, prepared 1024/4096-square immutable RGBA; 32/256-square padded center ROIs and 4096/65536 centered histogram samples. Includes native parameter preparation/preflight and kernel work. Excludes source allocation, independent references, host/Session caches, undo/replay, UI/GPU and physical tablet latency. No machine-specific time thresholds.");
    for side in [1024usize, 4096] {
        let pixels = (0..side * side).map(pixel).collect::<Vec<_>>();
        for roi in [32, 256] {
            measure_hsl_roi(side, roi, &pixels, render);
        }
        let layer = abi::CompositeLayer {
            data: pixels.as_ptr(),
            len: pixels.len(),
            opacity: 255,
            visible: 1,
            mode: 0,
        };
        let nodes = [
            node(0, 0, 0, 0, 1.),
            node(11, 0, 0, 0, 1.),
            node(3, 1, 0, 0, 1.),
        ];
        let parameters = abi::HslParameters {
            hue: 180.,
            saturation: 0.,
            lightness: 0.,
        };
        for budget in [4096, 65536] {
            let request = abi::HistogramRequest {
                abi_version: 1,
                struct_size: 24,
                width: side as u32,
                height: side as u32,
                stride: side as u32,
                max_samples: budget,
            };
            measure(
                &format!("hsl_half_turn_{side}"),
                &hsl_reference(side, budget),
                |output| unsafe {
                    sample(
                        black_box(&request),
                        black_box(nodes.as_ptr()),
                        3,
                        black_box(&layer),
                        1,
                        std::ptr::null(),
                        0,
                        std::ptr::null(),
                        0,
                        black_box(&parameters),
                        1,
                        output,
                    )
                },
            );
        }
        assert!(pixels
            .iter()
            .copied()
            .enumerate()
            .all(|(i, p)| p == pixel(i)));
    }
}

#[path = "quantization_cost.rs"]
mod quantization_cost;
