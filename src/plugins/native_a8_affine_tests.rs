//! Optional raw-A8 affine dispatch and actual-DLL immutable sampling contracts.
use super::*;
use std::cell::Cell;

const GUARD: usize = 13;
const SENTINEL: u8 = 231;
thread_local! {
    static CALLS: Cell<usize> = const { Cell::new(0) };
    static STATUS: Cell<i32> = const { Cell::new(0) };
}

unsafe extern "C" fn dispatch(
    request: *const abi::A8Affine,
    source: *const abi::CompositeMask,
    width: u32,
    height: u32,
    output: *mut abi::A8Pixels,
) -> i32 {
    CALLS.with(|calls| calls.set(calls.get() + 1));
    assert_eq!((width, height), (3, 2));
    assert_eq!((*request).fill, 255);
    assert_eq!((*request).matrix, [1., 0., 0.5, 0., 1., -1.]);
    assert!((*source).data.is_null());
    assert_eq!(
        ((*source).len, (*source).stride, (*source).constant),
        (0, 0, 73)
    );
    assert_eq!(
        ((*output).width, (*output).height, (*output).stride),
        (3, 2, 5)
    );
    (*output).data.write(37);
    STATUS.with(Cell::get)
}
fn table() -> abi::A8AffineApi {
    abi::A8AffineApi {
        abi_version: 1,
        struct_size: std::mem::size_of::<abi::A8AffineApi>() as u32,
        sample: Some(dispatch),
    }
}
fn plugin() -> Arc<NativePlugin> {
    let mut registry = super::super::Registry::bundled();
    registry
        .native_plugins
        .remove("painting")
        .expect("stage the matching painting DLL before tests")
}
fn request(method: u32, fill: u32, matrix: [f64; 6]) -> abi::A8Affine {
    abi::A8Affine {
        method,
        fill,
        matrix,
    }
}
fn uniform(value: u32) -> abi::CompositeMask {
    abi::CompositeMask {
        data: std::ptr::null(),
        len: 0,
        stride: 0,
        constant: value,
    }
}
fn dense(bytes: &[u8], width: usize, height: usize, stride: usize) -> abi::CompositeMask {
    assert!(stride >= width && bytes.len() >= GUARD + height * stride);
    abi::CompositeMask {
        data: unsafe { bytes.as_ptr().add(GUARD) },
        len: height * stride,
        stride: stride as u32,
        constant: 0,
    }
}
fn output(bytes: &mut [u8], width: usize, height: usize, stride: usize) -> abi::A8Pixels {
    abi::A8Pixels {
        data: unsafe { bytes.as_mut_ptr().add(GUARD) },
        len: height * stride,
        width: width as u32,
        height: height as u32,
        stride: stride as u32,
        origin_x: 0,
        origin_y: 0,
    }
}
fn storage(width: usize, height: usize, stride: usize) -> Vec<u8> {
    assert!(width <= stride);
    vec![SENTINEL; GUARD * 2 + height * stride]
}
fn active(bytes: &[u8], width: usize, height: usize, stride: usize) -> Vec<u8> {
    (0..height)
        .flat_map(|y| {
            bytes[GUARD + y * stride..GUARD + y * stride + width]
                .iter()
                .copied()
        })
        .collect()
}
fn guards(bytes: &[u8], width: usize, height: usize, stride: usize) {
    for (i, &sample) in bytes.iter().enumerate() {
        let touched = i >= GUARD && i < GUARD + height * stride && (i - GUARD) % stride < width;
        if !touched {
            assert_eq!(sample, SENTINEL, "guard/padding offset{i}");
        }
    }
}

/// Mathematical inverse lookup, then the published Q16 separable scalar weights.
/// Fixtures use dyadic matrices so the coordinates are exact binary fractions.
fn reference(
    source: &[u8],
    sw: usize,
    sh: usize,
    width: usize,
    height: usize,
    o: &abi::A8Affine,
) -> Vec<u8> {
    let m = o.matrix;
    let determinant = m[0] * m[4] - m[1] * m[3];
    let at = |x: i64, y: i64| -> u64 {
        if x >= 0 && y >= 0 && x < sw as i64 && y < sh as i64 {
            u64::from(source[y as usize * sw + x as usize])
        } else {
            u64::from(o.fill)
        }
    };
    (0..width * height)
        .map(|i| {
            let x = (i % width) as f64 + 0.5 - m[2];
            let y = (i / width) as f64 + 0.5 - m[5];
            let sx = (m[4] * x - m[1] * y) / determinant;
            let sy = (m[0] * y - m[3] * x) / determinant;
            if o.method == 0 {
                return at(sx.floor() as i64, sy.floor() as i64) as u8;
            }
            let (x, y) = (sx - 0.5, sy - 0.5);
            let (ix, iy) = (x.floor() as i64, y.floor() as i64);
            let a = ((x - x.floor()) * 65536.).round() as u64;
            let b = ((y - y.floor()) * 65536.).round() as u64;
            let numerator = at(ix, iy) * (65536 - a) * (65536 - b)
                + at(ix + 1, iy) * a * (65536 - b)
                + at(ix, iy + 1) * (65536 - a) * b
                + at(ix + 1, iy + 1) * a * b;
            ((numerator + (1u64 << 31)) / (1u64 << 32)) as u8
        })
        .collect()
}

#[test]
fn a8_affine_header_decoder_rejects_truncated_tables_before_reading_the_callback() {
    assert!(unsafe { decode_a8_affine_table(std::ptr::null()) }.is_err());
    for size in 0..std::mem::size_of::<abi::A8AffineApi>() as u32 {
        let header = abi::Header {
            abi_version: 1,
            struct_size: size,
        };
        assert!(unsafe { decode_a8_affine_table((&header as *const abi::Header).cast()) }.is_err());
    }
    let mut api = table();
    assert!(unsafe { decode_a8_affine_table(&api) }.is_ok());
    api.struct_size += 16;
    assert!(
        unsafe { decode_a8_affine_table(&api) }.is_ok(),
        "future optional tails do not change this prefix"
    );
    for version in [0, 2, u32::MAX] {
        let header = abi::Header {
            abi_version: version,
            struct_size: std::mem::size_of::<abi::A8AffineApi>() as u32,
        };
        assert!(unsafe { decode_a8_affine_table((&header as *const abi::Header).cast()) }.is_err());
    }
    let mut absent = table();
    absent.sample = None;
    assert!(unsafe { decode_a8_affine_table(&absent) }.is_err());
}

#[test]
fn a8_affine_host_adapter_borrows_uniform_source_output_and_propagates_native_status() {
    let mut plugin = plugin();
    Arc::get_mut(&mut plugin).unwrap().a8_affine = Some(table());
    assert!(plugin.supports_a8_affine());
    let source = uniform(73);
    let o = request(1, 255, [1., 0., 0.5, 0., 1., -1.]);
    let mut bytes = storage(3, 2, 5);
    let mut target = output(&mut bytes, 3, 2, 5);
    let pointer = target.data;
    CALLS.with(|calls| calls.set(0));
    STATUS.with(|status| status.set(abi::OK));
    plugin.a8_affine(&o, &source, 3, 2, &mut target).unwrap();
    assert_eq!(bytes[GUARD], 37);
    assert_eq!(target.data, pointer);
    assert!(source.data.is_null() && source.len == 0 && source.stride == 0);
    guards(&bytes, 3, 2, 5);
    STATUS.with(|status| status.set(-73));
    let error = plugin
        .a8_affine(&o, &source, 3, 2, &mut target)
        .unwrap_err();
    assert!(error.contains("-73") && error.to_ascii_lowercase().contains("affine"));
    CALLS.with(|calls| assert_eq!(calls.get(), 2));
    Arc::get_mut(&mut plugin).unwrap().a8_affine = None;
    assert!(!plugin.supports_a8_affine());
    let before = bytes.clone();
    assert!(plugin.a8_affine(&o, &source, 3, 2, &mut target).is_err());
    assert_eq!(bytes, before);
    CALLS.with(|calls| assert_eq!(calls.get(), 2));
}

#[test]
fn a8_affine_actual_dll_dense_sources_match_inverse_references_with_padding_and_white_exterior() {
    let plugin = plugin();
    assert!(plugin.supports_a8_affine());
    let (sw, sh, stride) = (5, 3, 8);
    let mut source = storage(sw, sh, stride);
    let packed: Vec<_> = (0..sw * sh).map(|i| (i * 17) as u8).collect();
    for y in 0..sh {
        source[GUARD + y * stride..GUARD + y * stride + sw]
            .copy_from_slice(&packed[y * sw..y * sw + sw]);
    }
    let frozen = source.clone();
    for (width, height) in [(5, 3), (7, 5)] {
        for matrix in [
            [1., 0., 0., 0., 1., 0.],
            [1., 0., 1., 0., 1., -1.],
            [-1., 0., 5., 0., 1., 0.],
            [0., -1., 4., 1., 0., -1.],
            [1., 0.5, -0.25, 0., 1., 0.5],
            [2., 0., -0.5, 0., 0.5, 0.25],
        ] {
            for method in [0, 1] {
                for fill in [0, 37, 255] {
                    let o = request(method, fill, matrix);
                    let out_stride = width + 4;
                    let mut bytes = storage(width, height, out_stride);
                    let mut target = output(&mut bytes, width, height, out_stride);
                    let descriptor = dense(&source, sw, sh, stride);
                    plugin
                        .a8_affine(&o, &descriptor, sw as u32, sh as u32, &mut target)
                        .unwrap();
                    assert_eq!(
                        active(&bytes, width, height, out_stride),
                        reference(&packed, sw, sh, width, height, &o),
                        "method{method} fill{fill} matrix{matrix:?}"
                    );
                    guards(&bytes, width, height, out_stride);
                    assert_eq!(source, frozen);
                    assert_eq!(descriptor.data, unsafe { source.as_ptr().add(GUARD) });
                }
            }
        }
    }
}

#[test]
fn a8_affine_uniform_sampling_is_independent_of_legacy_zero_exterior_selection_and_other_tables() {
    let mut plugin = plugin();
    let native = Arc::get_mut(&mut plugin).unwrap();
    assert!(native.affine.is_some() && native.a8_sampler.is_some());
    let old_affine = native.affine.take().unwrap();
    native.a8_sampler = None;
    assert!(!plugin.supports_a8_sampler() && plugin.supports_a8_affine());
    for value in [0, 73, 255] {
        for method in [0, 1] {
            let o = request(method, 255, [1., 0., 0.5, 0., 1., 0.]);
            let mut bytes = storage(3, 2, 5);
            let mut target = output(&mut bytes, 3, 2, 5);
            let source = uniform(value);
            plugin.a8_affine(&o, &source, 3, 2, &mut target).unwrap();
            let expected = reference(&vec![value as u8; 6], 3, 2, 3, 2, &o);
            assert_eq!(active(&bytes, 3, 2, 5), expected);
            guards(&bytes, 3, 2, 5);
            assert!(source.data.is_null() && source.len == 0);
            if value == 73 && method == 1 {
                assert_eq!(expected[0], 164, "half white exterior + half raw73");
                let selection = [73u8; 6];
                let coverage = abi::Coverage {
                    data: selection.as_ptr(),
                    len: 6,
                    width: 3,
                    height: 2,
                    stride: 3,
                };
                let mut legacy = [0u8; 6];
                let options = abi::Affine {
                    kind: 2,
                    method: 1,
                    matrix: o.matrix,
                };
                assert_eq!(
                    unsafe {
                        old_affine.mask.unwrap()(
                            &options,
                            &coverage,
                            legacy.as_mut_ptr(),
                            legacy.len(),
                        )
                    },
                    abi::OK
                );
                assert_eq!(legacy[0], 37, "legacy selection exterior remains zero");
                assert_ne!(legacy[0], expected[0]);
            }
        }
    }
}

#[test]
fn a8_affine_actual_dll_rejects_invalid_math_and_storage_without_writing_source_or_output() {
    let plugin = plugin();
    let (width, height, stride) = (5, 3, 8);
    for case in 0..22 {
        let mut source = storage(width, height, stride);
        for y in 0..height {
            for x in 0..width {
                source[GUARD + y * stride + x] = (x * 31 + y * 17) as u8;
            }
        }
        let source_before = source.clone();
        let mut bytes = storage(width, height, stride);
        let before = bytes.clone();
        let mut target = output(&mut bytes, width, height, stride);
        let mut descriptor = dense(&source, width, height, stride);
        let mut o = request(1, 255, [1., 0., 0.5, 0., 1., 0.]);
        let (mut sw, mut sh) = (width as u32, height as u32);
        match case {
            0 => o.method = 2,
            1 => o.fill = 256,
            2 => o.matrix[0] = f64::NAN,
            3 => o.matrix[4] = f64::INFINITY,
            4 => o.matrix[0] = 0.,
            5 => o.matrix[0] = 1e-10,
            6 => o.matrix[0] = 257.,
            7 => o.matrix[2] = 4_194_305.,
            8 => {
                descriptor = abi::CompositeMask {
                    len: 1,
                    ..uniform(73)
                }
            }
            9 => {
                descriptor = abi::CompositeMask {
                    stride: 1,
                    ..uniform(73)
                }
            }
            10 => descriptor = uniform(256),
            11 => descriptor.constant = 1,
            12 => descriptor.stride = 4,
            13 => descriptor.len = 1,
            14 => sw = 0,
            15 => sh = 4097,
            16 => target.origin_x = 1,
            17 => target.width = 0,
            18 => target.stride = 4,
            19 => target.len = 1,
            20 => target.data = std::ptr::null_mut(),
            _ => target.data = descriptor.data as *mut u8,
        }
        assert!(
            plugin
                .a8_affine(&o, &descriptor, sw, sh, &mut target)
                .is_err(),
            "case{case}"
        );
        assert_eq!(bytes, before, "output case{case}");
        assert_eq!(source, source_before, "source case{case}");
    }
}

#[test]
fn a8_affine_registry_discovery_reports_the_independent_optional_table() {
    let mut registry = super::super::Registry::bundled();
    let discovery = registry.describe();
    let native = discovery["native"].as_array().unwrap();
    assert_eq!(
        native.iter().find(|p| p["id"] == "painting").unwrap()["samplers"]["raw_a8_affine"],
        true
    );
    assert_eq!(
        native.iter().find(|p| p["id"] == "compositing").unwrap()["samplers"]["raw_a8_affine"],
        false
    );
    let painting = Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap()).unwrap();
    painting.a8_affine = None;
    assert!(!painting.supports_a8_affine() && painting.supports_a8_sampler());
    let mut bytes = storage(3, 2, 5);
    let mut target = output(&mut bytes, 3, 2, 5);
    painting
        .a8_reframe(
            &abi::A8Reframe {
                x: 0,
                y: 0,
                fill: 255,
                reserved: 0,
            },
            &uniform(77),
            3,
            2,
            &mut target,
        )
        .unwrap();
    assert_eq!(
        active(&bytes, 3, 2, 5),
        vec![77; 6],
        "the old sampler remains usable without the new symbol"
    );
    guards(&bytes, 3, 2, 5);
    let discovery = registry.describe();
    let native = discovery["native"].as_array().unwrap();
    assert_eq!(
        native.iter().find(|p| p["id"] == "painting").unwrap()["samplers"]["raw_a8_affine"],
        false
    );
}
