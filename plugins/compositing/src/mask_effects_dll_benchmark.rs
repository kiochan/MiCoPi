//! Opaque, interleaved old/new DLL evidence. Never inspect either private State.
use super::abi;
use std::{
    ffi::c_void,
    hint::black_box,
    path::{Path, PathBuf},
    time::Instant,
};

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

const SIDE: usize = 4096;
const AREA: usize = SIDE * SIDE;
const FULL_PAIRS: usize = 3;
const ROI_PAIRS: usize = 32;

struct Dll {
    module: *mut c_void,
    instance: *mut c_void,
    create: abi::MaskEffectsCreateFn,
    destroy: abi::MaskEffectsDestroyFn,
    reserve: abi::MaskEffectsReserveFn,
    region: abi::MaskEffectRegionFn,
}
impl Dll {
    fn load(path: &Path) -> Self {
        use std::os::windows::ffi::OsStrExt;
        let absolute = path
            .canonicalize()
            .expect("Build/preserve the benchmark DLL first");
        let wide: Vec<_> = absolute.as_os_str().encode_wide().chain(Some(0)).collect();
        let module = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), 0x1100) };
        assert!(!module.is_null(), "Cannot load {}", absolute.display());
        let symbol = unsafe { GetProcAddress(module, b"micopi_compositor_v1\0".as_ptr()) };
        assert!(!symbol.is_null());
        let entry: abi::CompositorEntry = unsafe { std::mem::transmute(symbol) };
        let pointer = unsafe { entry() };
        assert!(!pointer.is_null());
        let header = unsafe { &*pointer };
        assert_eq!(header.abi_version, 1);
        assert!(
            header.struct_size as usize >= std::mem::size_of::<abi::CompositorApiMaskEffects>()
        );
        let table = unsafe { &*pointer.cast::<abi::CompositorApiMaskEffects>() };
        assert_ne!(
            table.base.base.base.capabilities & abi::COMPOSITOR_MASK_EFFECTS,
            0
        );
        let (create, destroy, reserve, region) = (
            table.create_mask_effects.unwrap(),
            table.destroy_mask_effects.unwrap(),
            table.reserve_mask_effects.unwrap(),
            table.mask_effect_region.unwrap(),
        );
        let instance = unsafe { create() };
        assert!(!instance.is_null());
        Self {
            module,
            instance,
            create,
            destroy,
            reserve,
            region,
        }
    }
    fn reset(&mut self) {
        unsafe { (self.destroy)(self.instance) };
        self.instance = unsafe { (self.create)() };
        assert!(!self.instance.is_null());
    }
    fn timed_reserve(&self, radius: u32) -> f64 {
        let start = Instant::now();
        let status =
            unsafe { (self.reserve)(black_box(self.instance), SIDE as u32, SIDE as u32, radius) };
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(status, abi::OK);
        elapsed
    }
    fn timed_region(
        &self,
        request: &abi::MaskEffectRequest,
        source: &abi::CompositeMask,
        out: &mut abi::A8Pixels,
    ) -> f64 {
        let start = Instant::now();
        let status = unsafe {
            (self.region)(
                black_box(self.instance),
                black_box(request),
                black_box(source),
                SIDE as u32,
                SIDE as u32,
                black_box(out),
            )
        };
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(status, abi::OK);
        elapsed
    }
}
impl Drop for Dll {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.instance);
            FreeLibrary(self.module);
        }
    }
}
fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
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
fn pair(
    old: &Dll,
    new: &Dll,
    request: &abi::MaskEffectRequest,
    source: &abi::CompositeMask,
    old_out: &mut abi::A8Pixels,
    new_out: &mut abi::A8Pixels,
    index: usize,
) -> (f64, f64) {
    if index % 2 == 0 {
        let before = old.timed_region(request, source, old_out);
        (before, new.timed_region(request, source, new_out))
    } else {
        let after = new.timed_region(request, source, new_out);
        (old.timed_region(request, source, old_out), after)
    }
}
fn measure(old_path: &Path, new_path: &Path, label: &str, bytes: &[u8], radius: u32) {
    let mut old = Dll::load(old_path);
    let mut new = Dll::load(new_path);
    let source = abi::CompositeMask {
        data: bytes.as_ptr(),
        len: bytes.len(),
        stride: SIDE as u32,
        constant: 0,
    };
    let request = abi::MaskEffectRequest {
        flags: 0,
        density: 203,
        feather_radius: radius,
        reserved: 0,
    };
    let mut before = vec![0xa5; AREA];
    let mut after = before.clone();
    let (mut full_before, mut full_after) = (
        Vec::with_capacity(FULL_PAIRS),
        Vec::with_capacity(FULL_PAIRS),
    );
    let (mut old_out, mut new_out) = (
        output(&mut before, [0, 0, SIDE, SIDE], SIDE),
        output(&mut after, [0, 0, SIDE, SIDE], SIDE),
    );
    for iteration in 0..FULL_PAIRS {
        old.reset();
        new.reset();
        let (a, b) = pair(
            &old,
            &new,
            &request,
            &source,
            &mut old_out,
            &mut new_out,
            iteration,
        );
        full_before.push(a);
        full_after.push(b);
        assert_eq!(
            before, after,
            "full frame source={label},R={radius},pair={iteration}"
        );
        if label == "dense_white" {
            assert!(after.iter().all(|value| *value == 255));
        }
    }
    println!("mask_effects_dll source={label} R={radius} scope=fresh_full_4096x4096 pairs={FULL_PAIRS} old_median_ms={:.6} new_median_ms={:.6} exact_full_output_bytes=true alternating_order=true",median(&mut full_before),median(&mut full_after));
    drop(before);
    drop(after);
    for side in [32, 32 + 2 * radius as usize] {
        let roi = [SIDE / 2 - side / 2, SIDE / 2 - side / 2, side, side];
        let stride = side + 7;
        let mut before = vec![0xa5; stride * side];
        let mut after = before.clone();
        let allocations = (
            before.as_ptr(),
            before.capacity(),
            after.as_ptr(),
            after.capacity(),
        );
        let (mut old_out, mut new_out) = (
            output(&mut before, roi, stride),
            output(&mut after, roi, stride),
        );
        old.reset();
        new.reset();
        let (reserve_before, reserve_after) =
            (old.timed_reserve(radius), new.timed_reserve(radius));
        let first = pair(&old, &new, &request, &source, &mut old_out, &mut new_out, 0);
        assert_eq!(before, after);
        let (mut times_before, mut times_after) =
            (Vec::with_capacity(ROI_PAIRS), Vec::with_capacity(ROI_PAIRS));
        for iteration in 0..ROI_PAIRS {
            let (a, b) = pair(
                &old,
                &new,
                &request,
                &source,
                &mut old_out,
                &mut new_out,
                iteration,
            );
            times_before.push(a);
            times_after.push(b);
            assert_eq!(
                before, after,
                "ROI source={label},R={radius},side={side},pair={iteration}"
            );
            for y in 0..side {
                assert!(after[y * stride + side..(y + 1) * stride]
                    .iter()
                    .all(|value| *value == 0xa5));
            }
        }
        assert_eq!(
            (
                before.as_ptr(),
                before.capacity(),
                after.as_ptr(),
                after.capacity()
            ),
            allocations
        );
        println!("mask_effects_dll source={label} R={radius} raw_damage=32x32 requested_roi={side}x{side} source_halo={}x{} pairs={ROI_PAIRS} old_reserve_ms={reserve_before:.6} new_reserve_ms={reserve_after:.6} old_first_roi_ms={:.6} new_first_roi_ms={:.6} old_warm_median_ms={:.6} new_warm_median_ms={:.6} exact_full_output_bytes_and_padding=true output_allocations_reused=true alternating_order=true",side+2*radius as usize,side+2*radius as usize,first.0,first.1,median(&mut times_before),median(&mut times_after));
    }
}

#[test]
#[ignore = "Opt-in interleaved baseline/candidate DLL mask kernel comparison"]
fn mask_effects_interleaved_dll_cache_local_cost() {
    assert!(
        !cfg!(debug_assertions),
        "Use --release for kernel measurements"
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let old = std::env::var_os("MICOPI_MASK_BASELINE_DLL")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/mask-effects-native-baseline/micopi_compositing.dll"));
    let new = std::env::var_os("MICOPI_MASK_CANDIDATE_DLL")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root.join(
                "target/mask-effects-native/x86_64-pc-windows-gnu/release/micopi_compositing.dll",
            )
        });
    assert_ne!(old.canonicalize().unwrap(), new.canonicalize().unwrap());
    println!("Opaque actual DLL callbacks; full calculations include fresh native scratch allocation, warm calls follow capacity-only reserve. Requested output for raw32x32 damage expands by R per side; native source halo expands by another R. Excludes host cache allocation, history, replay, composition, UI and physical tablet latency. Baseline={} Candidate={}",old.display(),new.display());
    let white = vec![255; AREA];
    for radius in [8, 256] {
        measure(&old, &new, "dense_white", &white, radius);
    }
    drop(white);
    let pattern = (0..AREA)
        .map(|index| {
            let (x, y) = (index % SIDE, index / SIDE);
            (((x / 97) * 37 + (y / 131) * 61 + ((x * y) >> 9)) & 255) as u8
        })
        .collect::<Vec<_>>();
    for radius in [8, 256] {
        measure(
            &old,
            &new,
            "dense_deterministic_nonuniform",
            &pattern,
            radius,
        );
    }
}
