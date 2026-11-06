//! Opt-in typed-callback measurements, separate from host, Session and tablet latency.
use super::*;
use std::{hint::black_box, time::Instant};

const SIDE: usize = 4096;
const AREA: usize = SIDE * SIDE;
const DENSITY: u32 = 203;
const ROI: [usize; 4] = [2032, 2032, 32, 32];
const ROI_STRIDE: usize = 40;
const WARM_RUNS: usize = 128;

struct Kernel {
    table: &'static abi::CompositorApiMaskEffects,
    instance: *mut c_void,
}
impl Kernel {
    fn new() -> Self {
        let base = crate::micopi_compositor_v1();
        assert!(!base.is_null());
        assert_eq!(unsafe { (*base).abi_version }, 1);
        assert!(
            unsafe { (*base).struct_size as usize }
                >= std::mem::size_of::<abi::CompositorApiMaskEffects>()
        );
        let table = unsafe { &*base.cast::<abi::CompositorApiMaskEffects>() };
        let instance = unsafe { table.create_mask_effects.unwrap()() };
        assert!(!instance.is_null());
        Self { table, instance }
    }
    fn allocations(&self) -> [(usize, usize); 3] {
        let state = unsafe { &*self.instance.cast::<State>() };
        [
            (state.first.as_ptr() as usize, state.first.capacity()),
            (state.second.as_ptr() as usize, state.second.capacity()),
            (
                state.columns.as_ptr() as usize,
                state.columns.capacity() * 4,
            ),
        ]
    }
    fn lengths(&self) -> [usize; 3] {
        let state = unsafe { &*self.instance.cast::<State>() };
        [
            state.first.len(),
            state.second.len(),
            state.columns.len() * 4,
        ]
    }
    fn call(
        &self,
        request: &abi::MaskEffectRequest,
        source: &abi::CompositeMask,
        output: &mut abi::A8Pixels,
    ) -> i32 {
        unsafe {
            (self.table.mask_effect_region.unwrap())(
                black_box(self.instance),
                black_box(request),
                black_box(source),
                SIDE as u32,
                SIDE as u32,
                black_box(output),
            )
        }
    }
    fn reserve(&self, radius: u32) -> i32 {
        unsafe {
            (self.table.reserve_mask_effects.unwrap())(
                black_box(self.instance),
                SIDE as u32,
                SIDE as u32,
                radius,
            )
        }
    }
}
impl Drop for Kernel {
    fn drop(&mut self) {
        unsafe { (self.table.destroy_mask_effects.unwrap())(self.instance) };
    }
}

fn output(bytes: &mut [u8], roi: [usize; 4], stride: usize) -> abi::A8Pixels {
    abi::A8Pixels {
        data: bytes.as_mut_ptr(),
        len: bytes.len(),
        stride: stride as u32,
        origin_x: roi[0] as u32,
        origin_y: roi[1] as u32,
        width: roi[2] as u32,
        height: roi[3] as u32,
    }
}

// Independent prefix-sum reference. It uses neither the production rolling
// window nor its intermediate buffers. Its full influence patch is sufficient
// for the requested samples because the three pass radii sum to R.
fn oracle(source: &[u8], roi: [usize; 4], radius: usize) -> Vec<u8> {
    let left = roi[0].saturating_sub(radius);
    let top = roi[1].saturating_sub(radius);
    let width = (roi[0] + roi[2] + radius).min(SIDE) - left;
    let height = (roi[1] + roi[3] + radius).min(SIDE) - top;
    let mut current = Vec::with_capacity(width * height);
    for y in top..top + height {
        current.extend_from_slice(&source[y * SIDE + left..y * SIDE + left + width]);
    }
    let mut next = vec![0; current.len()];
    let mut prefix = vec![0u64; width.max(height) + 1];
    for r in [(radius + 2) / 3, (radius + 1) / 3, radius / 3] {
        if r == 0 {
            continue;
        }
        let count = (2 * r + 1) as u64;
        for horizontal in [true, false] {
            let (lines, length) = if horizontal {
                (height, width)
            } else {
                (width, height)
            };
            for line in 0..lines {
                let index = |position: usize| {
                    if horizontal {
                        line * width + position
                    } else {
                        position * width + line
                    }
                };
                prefix[0] = 0;
                for position in 0..length {
                    prefix[position + 1] = prefix[position] + u64::from(current[index(position)]);
                }
                for position in 0..length {
                    let begin = position.saturating_sub(r);
                    let end = (position + r + 1).min(length);
                    let mut sum = prefix[end] - prefix[begin];
                    sum += r.saturating_sub(position) as u64 * u64::from(current[index(0)]);
                    sum += (position + r + 1).saturating_sub(length) as u64
                        * u64::from(current[index(length - 1)]);
                    next[index(position)] = ((2 * sum + count) / (2 * count)) as u8;
                }
            }
            std::mem::swap(&mut current, &mut next);
        }
    }
    let mut expected = Vec::with_capacity(roi[2] * roi[3]);
    for y in 0..roi[3] {
        for x in 0..roi[2] {
            let value = u32::from(current[(roi[1] - top + y) * width + roi[0] - left + x]);
            expected.push(((value * DENSITY + 255 * (255 - DENSITY) + 127) / 255) as u8);
        }
    }
    expected
}

fn assert_region(actual: &[u8], stride: usize, roi: [usize; 4], expected: &[u8]) {
    for y in 0..roi[3] {
        assert_eq!(
            &actual[y * stride..y * stride + roi[2]],
            &expected[y * roi[2]..(y + 1) * roi[2]]
        );
    }
}

fn measure(label: &str, source: Option<&Vec<u8>>, radius: u32) {
    let descriptor = if let Some(source) = source {
        abi::CompositeMask {
            data: source.as_ptr(),
            len: source.len(),
            stride: SIDE as u32,
            constant: 0,
        }
    } else {
        abi::CompositeMask {
            data: std::ptr::null(),
            len: 0,
            stride: 0,
            constant: 255,
        }
    };
    let source_allocation = source.map(|bytes| (bytes.as_ptr(), bytes.capacity()));
    let request = abi::MaskEffectRequest {
        flags: 0,
        density: DENSITY,
        feather_radius: radius,
        reserved: 0,
    };
    let kernel = Kernel::new();
    assert_eq!(kernel.lengths(), [0, 0, 0]);
    assert_eq!(
        kernel.allocations().map(|(_, capacity)| capacity),
        [0, 0, 0]
    );
    let mut full = vec![0; AREA];
    let full_allocation = (full.as_ptr(), full.capacity());
    let mut full_descriptor = output(&mut full, [0, 0, SIDE, SIDE], SIDE);

    let start = Instant::now();
    let status = kernel.call(&request, &descriptor, &mut full_descriptor);
    let first_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(status, abi::OK);
    let scratch = kernel.allocations();
    let scratch_bytes: usize = scratch.iter().map(|(_, capacity)| capacity).sum();
    if source.is_some() {
        assert_eq!(kernel.lengths(), [AREA, AREA, SIDE * 4]);
        assert!(scratch[..2].iter().all(|(_, capacity)| *capacity >= AREA));
        assert_eq!(scratch[2].1, SIDE * 4);
    } else {
        assert_eq!(scratch_bytes, 0);
        assert!(full.iter().all(|value| *value == 255));
    }

    // References, assertions and source/output allocations are outside timers.
    let expected = source.map_or_else(
        || vec![255; ROI[2] * ROI[3]],
        |bytes| oracle(bytes, ROI, radius as usize),
    );
    assert_region(&full[ROI[1] * SIDE + ROI[0]..], SIDE, ROI, &expected);
    let edge = [0, 0, 32, 32];
    let edge_expected = source.map_or_else(
        || vec![255; 32 * 32],
        |bytes| oracle(bytes, edge, radius as usize),
    );
    assert_region(&full, SIDE, edge, &edge_expected);

    let mut local = vec![0xa5; ROI_STRIDE * ROI[3]];
    let local_allocation = (local.as_ptr(), local.capacity());
    let mut local_descriptor = output(&mut local, ROI, ROI_STRIDE);
    // Prime the smaller descriptors/lengths once; preserve full-frame capacities.
    assert_eq!(
        kernel.call(&request, &descriptor, &mut local_descriptor),
        abi::OK
    );
    assert_eq!(kernel.allocations(), scratch);
    let mut samples = Vec::with_capacity(WARM_RUNS);
    for _ in 0..WARM_RUNS {
        let start = Instant::now();
        let status = kernel.call(&request, &descriptor, &mut local_descriptor);
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(status, abi::OK);
        samples.push(elapsed_ms);
    }
    assert_region(&local, ROI_STRIDE, ROI, &expected);
    for y in 0..ROI[3] {
        assert!(local[y * ROI_STRIDE + ROI[2]..(y + 1) * ROI_STRIDE]
            .iter()
            .all(|value| *value == 0xa5));
    }
    assert_eq!(kernel.allocations(), scratch);
    assert_eq!((full.as_ptr(), full.capacity()), full_allocation);
    assert_eq!((local.as_ptr(), local.capacity()), local_allocation);
    assert_eq!(
        source.map(|bytes| (bytes.as_ptr(), bytes.capacity())),
        source_allocation
    );
    let halo_area = (ROI[2] + 2 * radius as usize) * (ROI[3] + 2 * radius as usize);
    assert_eq!(
        kernel.lengths(),
        if source.is_some() {
            [halo_area, halo_area, (ROI[2] + 2 * radius as usize) * 4]
        } else {
            [0, 0, 0]
        }
    );
    samples.sort_by(f64::total_cmp);
    println!("mask_effects_native_4096 source={label} R={radius} density={DENSITY} first_full_ms={first_ms:.3} warm_roi=32x32 warm_runs={WARM_RUNS} warm_min_ms={:.6} warm_median_ms={:.6} warm_p95_ms={:.6} scratch_capacity_bytes={scratch_bytes} warm_halo_samples={halo_area} warm_scratch_len_bytes={} pointers_and_capacities_reused=true exact_center_and_edge_oracles=true", samples[0], samples[WARM_RUNS / 2], samples[(WARM_RUNS * 95 / 100).min(WARM_RUNS - 1)], kernel.lengths().iter().sum::<usize>());
    // The preparation path reserves a fresh instance without running a full-frame
    // kernel. Its first ROI includes local scratch initialization and the kernel.
    let prepared = Kernel::new();
    let start = Instant::now();
    let status = prepared.reserve(radius);
    let reserve_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(status, abi::OK);
    assert_eq!(prepared.lengths(), [0, 0, 0]);
    let prepared_allocations = prepared.allocations();
    assert!(prepared_allocations[..2]
        .iter()
        .all(|(_, capacity)| *capacity >= AREA));
    assert_eq!(prepared_allocations[2].1, SIDE * 4);
    let mut prepared_output = vec![0xa5; local.len()];
    let prepared_output_allocation = (prepared_output.as_ptr(), prepared_output.capacity());
    let mut prepared_descriptor = output(&mut prepared_output, ROI, ROI_STRIDE);
    let start = Instant::now();
    let status = prepared.call(&request, &descriptor, &mut prepared_descriptor);
    let first_roi_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(status, abi::OK);
    assert_eq!(prepared_output, local);
    assert_eq!(prepared.allocations(), prepared_allocations);
    let mut prepared_samples = Vec::with_capacity(WARM_RUNS);
    for _ in 0..WARM_RUNS {
        let start = Instant::now();
        let status = prepared.call(&request, &descriptor, &mut prepared_descriptor);
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(status, abi::OK);
        prepared_samples.push(elapsed_ms);
    }
    assert_eq!(prepared_output, local);
    assert_eq!(prepared.allocations(), prepared_allocations);
    assert_eq!(
        (prepared_output.as_ptr(), prepared_output.capacity()),
        prepared_output_allocation
    );
    prepared_samples.sort_by(f64::total_cmp);
    println!("mask_effects_reserve_4096 source={label} R={radius} reserve_ms={reserve_ms:.6} first_roi_after_reserve_ms={first_roi_ms:.6} warm_median_ms={:.6} warm_p95_ms={:.6} reserved_capacity_bytes={} lengths_after_reserve=0 native_and_output_allocations_reused=true identical_roi_bytes=true", prepared_samples[WARM_RUNS / 2], prepared_samples[(WARM_RUNS * 95 / 100).min(WARM_RUNS - 1)], prepared_allocations.iter().map(|(_, capacity)| capacity).sum::<usize>());
    black_box(full);
    black_box(local);
}

#[test]
#[ignore = "Opt-in Release native mask effect allocation/kernel benchmark"]
fn mask_effects_native_4096_cold_and_warm_cost() {
    assert!(
        !cfg!(debug_assertions),
        "Run this ignored measurement with --release"
    );
    println!("Scope: prepared 4096x4096 source/output, typed exported C callback; first full call includes native scratch allocation+kernel, warm calls reuse native scratch and prepared 32x32 output. Fresh reserve measures capacity allocation only; first ROI includes local initialization+kernel. Excludes host caches/history/replay/composition/UI and physical tablet latency; no machine-specific timing assertions.");
    for radius in [8, 256] {
        measure("uniform_descriptor_white", None, radius);
    }
    let white = vec![255; AREA];
    for radius in [8, 256] {
        measure("dense_white", Some(&white), radius);
    }
    drop(white);
    let pattern = (0..AREA)
        .map(|index| {
            let (x, y) = (index % SIDE, index / SIDE);
            (((x / 97) * 37 + (y / 131) * 61 + ((x * y) >> 9)) & 255) as u8
        })
        .collect::<Vec<_>>();
    for radius in [8, 256] {
        measure("dense_deterministic_nonuniform", Some(&pattern), radius);
    }
}
