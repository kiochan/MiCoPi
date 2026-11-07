use super::*;

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
fn dense(bytes: &[u8], stride: u32) -> abi::CompositeMask {
    abi::CompositeMask {
        data: bytes.as_ptr(),
        len: bytes.len(),
        stride,
        constant: 0,
    }
}

struct Output {
    bytes: Vec<u8>,
    width: usize,
    height: usize,
    stride: usize,
}
impl Output {
    fn new(width: usize, height: usize) -> Self {
        Self {
            bytes: vec![0xa5; 6 + (width + 3) * height],
            width,
            height,
            stride: width + 3,
        }
    }
    fn descriptor(&mut self) -> abi::A8Pixels {
        abi::A8Pixels {
            data: unsafe { self.bytes.as_mut_ptr().add(3) },
            len: (self.height - 1) * self.stride + self.width,
            width: self.width as u32,
            height: self.height as u32,
            stride: self.stride as u32,
            origin_x: 0,
            origin_y: 0,
        }
    }
    fn values(&self) -> Vec<u8> {
        (0..self.height)
            .flat_map(|y| {
                self.bytes[3 + y * self.stride..3 + y * self.stride + self.width]
                    .iter()
                    .copied()
            })
            .collect()
    }
    fn guards(&self) {
        for (index, value) in self.bytes.iter().enumerate() {
            let touched = index >= 3
                && (index - 3) / self.stride < self.height
                && (index - 3) % self.stride < self.width;
            if !touched {
                assert_eq!(*value, 0xa5, "guard {index}");
            }
        }
    }
}

/// Independent continuous reference for dyadic transforms. All axis fractions
/// in these fixtures are exactly representable by the published Q16 weights.
fn reference(
    bytes: Option<&[u8]>,
    constant: u8,
    sw: usize,
    sh: usize,
    stride: usize,
    width: usize,
    height: usize,
    options: &abi::A8Affine,
) -> Vec<u8> {
    let [a, b, tx, c, d, ty] = options.matrix;
    let det = a * d - b * c;
    let at = |x: i64, y: i64| -> f64 {
        if x < 0 || y < 0 || x >= sw as i64 || y >= sh as i64 {
            f64::from(options.fill)
        } else {
            f64::from(bytes.map_or(constant, |bytes| bytes[y as usize * stride + x as usize]))
        }
    };
    (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                let px = x as f64 + 0.5 - tx;
                let py = y as f64 + 0.5 - ty;
                let sx = (d * px - b * py) / det;
                let sy = (a * py - c * px) / det;
                if options.method == 0 {
                    at(sx.floor() as i64, sy.floor() as i64) as u8
                } else {
                    let x = sx - 0.5;
                    let y = sy - 0.5;
                    let ix = x.floor() as i64;
                    let iy = y.floor() as i64;
                    let fx = x - ix as f64;
                    let fy = y - iy as f64;
                    ((at(ix, iy) * (1. - fx) * (1. - fy)
                        + at(ix + 1, iy) * fx * (1. - fy)
                        + at(ix, iy + 1) * (1. - fx) * fy
                        + at(ix + 1, iy + 1) * fx * fy)
                        + 0.5)
                        .floor() as u8
                }
            })
        })
        .collect()
}

#[test]
fn raw_a8_affine_dense_references_cover_inverse_centers_scalar_bilinear_and_padding() {
    let data = [0, 17, 255, 0xee, 0xee, 41, 111, 213, 0xee, 0xee];
    let source = dense(&data, 5);
    let matrices = [
        [1., 0., 0., 0., 1., 0.],
        [1., 0., 1., 0., 1., -1.],
        [1., 0., 0.5, 0., 1., -0.25],
        [2., 0., 0., 0., 0.5, 0.],
        [-1., 0., 3., 0., 1., 0.],
        [0., -1., 2., 1., 0., 0.],
        [1., 0.5, 0., 0., 1., 0.],
        [1., 0., -4096., 0., 1., 4096.],
    ];
    for method in [0, 1] {
        for fill in [0, 91, 255] {
            for matrix in matrices {
                for (w, h) in [(3, 2), (5, 4)] {
                    let options = request(method, fill, matrix);
                    let mut output = Output::new(w, h);
                    assert_eq!(
                        unsafe { sample(&options, &source, 3, 2, &mut output.descriptor()) },
                        abi::OK
                    );
                    assert_eq!(
                        output.values(),
                        reference(Some(&data), 0, 3, 2, 5, w, h, &options),
                        "method{method} fill{fill} {matrix:?}"
                    );
                    output.guards();
                }
            }
        }
    }
    assert_eq!(data, [0, 17, 255, 0xee, 0xee, 41, 111, 213, 0xee, 0xee]);
}

#[test]
fn raw_a8_affine_uniform_sources_still_respect_exterior_taps_and_fill() {
    for constant in [0, 91, 255] {
        for fill in [0, 255] {
            for method in [0, 1] {
                let source = uniform(constant);
                for matrix in [
                    [1., 0., 0.5, 0., 1., 0.],
                    [1., 0., 1., 0., 1., 0.],
                    [0., -1., 1., 1., 0., 0.],
                ] {
                    let options = request(method, fill, matrix);
                    let mut output = Output::new(3, 2);
                    assert_eq!(
                        unsafe { sample(&options, &source, 1, 1, &mut output.descriptor()) },
                        abi::OK
                    );
                    assert_eq!(
                        output.values(),
                        reference(None, constant as u8, 1, 1, 0, 3, 2, &options)
                    );
                    output.guards();
                }
            }
        }
    }
    for (fill, expected) in [(0, 46), (255, 173)] {
        let mut output = Output::new(1, 1);
        assert_eq!(
            unsafe {
                sample(
                    &request(1, fill, [1., 0., 0.5, 0., 1., 0.]),
                    &uniform(91),
                    1,
                    1,
                    &mut output.descriptor(),
                )
            },
            abi::OK
        );
        assert_eq!(output.values(), vec![expected]);
    }
}

#[test]
fn raw_a8_affine_bilinear_quantizes_axis_weights_before_half_up_scalar_rounding() {
    let source = dense(&[0, 255], 2);
    // Ideal continuous interpolation would round127.498... to127. The Q16
    // half-step rounds its axis weight up to32768, then scalar127.5 up to128.
    let options = request(1, 0, [1., 0., -32767.5 / 65536., 0., 1., 0.]);
    let mut output = Output::new(1, 1);
    assert_eq!(
        unsafe { sample(&options, &source, 2, 1, &mut output.descriptor()) },
        abi::OK
    );
    assert_eq!(output.values(), vec![128]);
    output.guards();
}

#[test]
fn raw_a8_affine_rejects_bad_methods_fill_matrices_and_singular_maps_before_writes() {
    let source = uniform(91);
    let identity = [1., 0., 0., 0., 1., 0.];
    let mut invalid = vec![
        request(2, 0, identity),
        request(0, 256, identity),
        request(0, 0, [1., 2., 0., 2., 4., 0.]),
        request(1, 0, [1., 0., 0., 0., 1e-10, 0.]),
    ];
    for index in 0..6 {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut matrix = identity;
            matrix[index] = value;
            invalid.push(request(1, 255, matrix));
        }
        let mut matrix = identity;
        matrix[index] = if index == 2 || index == 5 {
            4_194_304.5
        } else {
            256.5
        };
        invalid.push(request(0, 0, matrix));
    }
    for options in invalid {
        let mut output = Output::new(3, 2);
        let before = output.bytes.clone();
        assert_eq!(
            unsafe { sample(&options, &source, 3, 2, &mut output.descriptor()) },
            abi::ERROR
        );
        assert_eq!(output.bytes, before);
    }
    // Exact existing admission boundaries remain accepted without a large frame.
    for matrix in [
        [256., 0., 4_194_304., 0., 1., -4_194_304.],
        [1., 0., 0., 0., 1e-8, 0.],
    ] {
        let mut output = Output::new(1, 1);
        assert_eq!(
            unsafe {
                sample(
                    &request(0, 255, matrix),
                    &source,
                    1,
                    1,
                    &mut output.descriptor(),
                )
            },
            abi::OK
        );
        output.guards();
    }
}

#[test]
fn raw_a8_affine_validates_dense_uniform_and_destination_descriptors_before_writes() {
    let data = [0u8, 1, 127, 128, 254, 255];
    let options = request(1, 255, [1., 0., 0., 0., 1., 0.]);
    for case in 0..18 {
        let mut source = dense(&data, 3);
        let mut output = Output::new(3, 2);
        let before = output.bytes.clone();
        let mut target = output.descriptor();
        let (mut sw, mut sh) = (3, 2);
        match case {
            0 => source.constant = 1,
            1 => source.stride = 2,
            2 => source.stride = 4097,
            3 => source.len = 1,
            4 => source.len = 4096 * 4096 + 1,
            5 => {
                source = abi::CompositeMask {
                    data: std::ptr::null(),
                    len: 1,
                    stride: 0,
                    constant: 91,
                }
            }
            6 => {
                source = abi::CompositeMask {
                    data: std::ptr::null(),
                    len: 0,
                    stride: 1,
                    constant: 91,
                }
            }
            7 => source = uniform(256),
            8 => target.origin_x = 1,
            9 => target.origin_y = 1,
            10 => target.stride = 2,
            11 => target.stride = 4097,
            12 => target.len = 1,
            13 => target.len = 4096 * 4096 + 1,
            14 => target.width = 0,
            15 => target.height = 4097,
            16 => sw = 0,
            _ => sh = 4097,
        }
        assert_eq!(
            unsafe { sample(&options, &source, sw, sh, &mut target) },
            abi::ERROR,
            "case{case}"
        );
        assert_eq!(output.bytes, before);
    }
}

#[test]
fn raw_a8_affine_rejects_null_unaligned_metadata_and_all_mutable_aliases() {
    let mut options = request(0, 255, [1., 0., 0., 0., 1., 0.]);
    let mut data = [0u8, 1, 127, 128, 254, 255];
    let mut source = dense(&data, 3);
    let mut output = Output::new(3, 2);
    let mut target = output.descriptor();
    let before = output.bytes.clone();
    unsafe {
        assert_eq!(
            sample(std::ptr::null(), &source, 3, 2, &mut target),
            abi::ERROR
        );
        assert_eq!(
            sample(&options, std::ptr::null(), 3, 2, &mut target),
            abi::ERROR
        );
        assert_eq!(
            sample(&options, &source, 3, 2, std::ptr::null_mut()),
            abi::ERROR
        );
        assert_eq!(
            sample(
                (&options as *const abi::A8Affine)
                    .cast::<u8>()
                    .add(1)
                    .cast(),
                &source,
                3,
                2,
                &mut target
            ),
            abi::ERROR
        );
        assert_eq!(
            sample(
                &options,
                (&source as *const abi::CompositeMask)
                    .cast::<u8>()
                    .add(1)
                    .cast(),
                3,
                2,
                &mut target
            ),
            abi::ERROR
        );
        assert_eq!(
            sample(
                &options,
                &source,
                3,
                2,
                (&mut target as *mut abi::A8Pixels)
                    .cast::<u8>()
                    .add(1)
                    .cast()
            ),
            abi::ERROR
        );
    }
    assert_eq!(output.bytes, before);
    for case in 0..4 {
        let mut target = output.descriptor();
        target.width = 1;
        target.height = 1;
        target.stride = 1;
        target.len = 1;
        target.data = match case {
            0 => data.as_mut_ptr(),
            1 => (&mut options as *mut abi::A8Affine).cast(),
            2 => (&mut source as *mut abi::CompositeMask).cast(),
            _ => (&mut target as *mut abi::A8Pixels).cast(),
        };
        assert_eq!(
            unsafe { sample(&options, &source, 3, 2, &mut target) },
            abi::ERROR,
            "alias{case}"
        );
        assert_eq!(data, [0, 1, 127, 128, 254, 255]);
        assert_eq!(options.matrix, [1., 0., 0., 0., 1., 0.]);
        assert_eq!(output.bytes, before);
    }
}

#[test]
fn raw_a8_affine_entry_is_independent_and_keeps_the_old_sampler_table_complete() {
    let api = unsafe { &*micopi_a8_affine_v1() };
    assert_eq!((api.abi_version, api.struct_size), (1, 16));
    assert!(api.sample.is_some());
    let old = unsafe { &*super::super::a8_sampler::micopi_a8_sampler_v1() };
    assert_eq!((old.abi_version, old.struct_size), (1, 32));
    assert!(old.reframe.is_some() && old.geometry.is_some() && old.resample.is_some());
}
