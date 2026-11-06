use super::*;

#[test]
fn reserve_validates_before_mutation_and_keeps_lengths_pixels_and_existing_allocations() {
    let mut state = State::default();
    let pointer = (&mut state as *mut State).cast();
    assert_eq!(unsafe { reserve(pointer, 4096, 4096, 256) }, abi::OK);
    assert!(state.first.capacity() >= 4096 * 4096);
    assert!(state.second.capacity() >= 4096 * 4096);
    assert_eq!(state.columns.capacity(), 4096);
    assert!(state.first.is_empty() && state.second.is_empty() && state.columns.is_empty());
    let columns = (state.columns.as_ptr(), state.columns.capacity());
    let allocations = [
        (state.first.as_ptr(), state.first.capacity()),
        (state.second.as_ptr(), state.second.capacity()),
    ];
    state.first.extend_from_slice(&[7, 19, 241]);
    state.second.extend_from_slice(&[53, 89]);
    state.columns.extend_from_slice(&[71, 137]);
    for (width, height, radius) in [
        (0, 1, 0),
        (1, 0, 0),
        (4097, 1, 0),
        (1, 4097, 0),
        (1, 1, 257),
        (u32::MAX, u32::MAX, u32::MAX),
    ] {
        assert_eq!(
            unsafe { reserve(pointer, width, height, radius) },
            abi::ERROR
        );
        assert_eq!(state.first, [7, 19, 241]);
        assert_eq!(state.second, [53, 89]);
        assert_eq!(state.columns, [71, 137]);
        assert_eq!((state.columns.as_ptr(), state.columns.capacity()), columns);
        assert_eq!(
            [
                (state.first.as_ptr(), state.first.capacity()),
                (state.second.as_ptr(), state.second.capacity())
            ],
            allocations
        );
    }
    for (width, height, radius) in [(1, 1, 0), (4096, 4096, 8), (4096, 4096, 256)] {
        assert_eq!(unsafe { reserve(pointer, width, height, radius) }, abi::OK);
        assert_eq!(state.first, [7, 19, 241]);
        assert_eq!(state.second, [53, 89]);
        assert_eq!(state.columns, [71, 137]);
        assert_eq!((state.columns.as_ptr(), state.columns.capacity()), columns);
        assert_eq!(
            [
                (state.first.as_ptr(), state.first.capacity()),
                (state.second.as_ptr(), state.second.capacity())
            ],
            allocations
        );
    }
    assert_eq!(
        unsafe { reserve(std::ptr::null_mut(), 1, 1, 0) },
        abi::ERROR
    );
    assert_eq!(
        unsafe { reserve(1usize as *mut c_void, 1, 1, 0) },
        abi::ERROR
    );
}

#[test]
fn reserved_scratch_matches_fresh_kernels_without_reallocation_or_source_output_writes() {
    let (width, height) = (77usize, 59usize);
    let source: Vec<_> = (0..width * height)
        .map(|index| ((index * 137 + index / width * 53) % 256) as u8)
        .collect();
    let unchanged = source.clone();
    let descriptor = abi::CompositeMask {
        data: source.as_ptr(),
        len: source.len(),
        stride: width as u32,
        constant: 0,
    };
    for radius in [0, 1, 8, 256] {
        let mut reserved = State::default();
        let mut fresh = State::default();
        let mut actual = vec![0xa5; width * height];
        let mut expected = actual.clone();
        assert_eq!(
            unsafe {
                reserve(
                    (&mut reserved as *mut State).cast(),
                    width as u32,
                    height as u32,
                    radius,
                )
            },
            abi::OK
        );
        assert_eq!(source, unchanged);
        assert_eq!(actual, expected);
        assert!(
            reserved.first.is_empty() && reserved.second.is_empty() && reserved.columns.is_empty()
        );
        let columns = (reserved.columns.as_ptr(), reserved.columns.capacity());
        let allocations = [
            (reserved.first.as_ptr(), reserved.first.capacity()),
            (reserved.second.as_ptr(), reserved.second.capacity()),
        ];
        let request = abi::MaskEffectRequest {
            flags: 0,
            density: 203,
            feather_radius: radius,
            reserved: 0,
        };
        let mut output = abi::A8Pixels {
            data: actual.as_mut_ptr(),
            len: actual.len(),
            stride: width as u32,
            origin_x: 0,
            origin_y: 0,
            width: width as u32,
            height: height as u32,
        };
        let mut reference = abi::A8Pixels {
            data: expected.as_mut_ptr(),
            ..output
        };
        assert_eq!(
            unsafe {
                region(
                    (&mut reserved as *mut State).cast(),
                    &request,
                    &descriptor,
                    width as u32,
                    height as u32,
                    &mut output,
                )
            },
            abi::OK
        );
        assert_eq!(
            unsafe {
                region(
                    (&mut fresh as *mut State).cast(),
                    &request,
                    &descriptor,
                    width as u32,
                    height as u32,
                    &mut reference,
                )
            },
            abi::OK
        );
        assert_eq!(actual, expected);
        assert_eq!(source, unchanged);
        assert_eq!(
            (reserved.columns.as_ptr(), reserved.columns.capacity()),
            columns
        );
        assert_eq!(
            [
                (reserved.first.as_ptr(), reserved.first.capacity()),
                (reserved.second.as_ptr(), reserved.second.capacity())
            ],
            allocations
        );
    }
}
