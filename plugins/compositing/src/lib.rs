//! Stateless region compositor. One DLL call per damaged region, no image intermediates.
use micopi_plugin_api as abi;
#[cfg(test)]
mod blend_tests;
mod blends;
mod mask_effects;
#[cfg(test)]
mod mask_tests;
mod masks;
#[cfg(test)]
mod mix_tests;
pub fn over(dst: u32, src: u32, mode: u32) -> u32 {
    if mode >= abi::BLEND_COLOR_BURN {
        return blends::over(dst, src, mode);
    }
    let a = src >> 24;
    if a == 0 {
        return dst;
    }
    let da = dst >> 24;
    let out_a = a + (da * (255 - a) + 127) / 255;
    let mut out = out_a << 24;
    for shift in [0, 8, 16] {
        let s = (src >> shift) & 255;
        let d = (dst >> shift) & 255;
        let b = match mode {
            1 => (s * d + 127) / 255,
            2 => 255 - ((255 - s) * (255 - d) + 127) / 255,
            3 => {
                if d < 128 {
                    (2 * s * d + 127) / 255
                } else {
                    255 - (2 * (255 - s) * (255 - d) + 127) / 255
                }
            }
            4 => s.min(d),
            5 => s.max(d),
            6 => (s + d).min(255),
            7 => s.abs_diff(d),
            _ => s,
        };
        let c =
            ((255 - a) * da * d + (255 - da) * a * s + a * da * b + out_a * 127) / (out_a * 255);
        out |= c.min(255) << shift;
    }
    out
}
fn opacity(p: u32, amount: u32) -> u32 {
    (p & 0xffffff) | (((p >> 24) * amount / 255) << 24)
}
// Keep Mix-only register pressure out of the existing renderer instruction loop.
#[inline(never)]
fn mix(before: u32, after: u32, amount: u32) -> u32 {
    if amount == 0 {
        return before;
    }
    if amount == 255 {
        return after;
    }
    let before_weight = (before >> 24) * (255 - amount);
    let after_weight = (after >> 24) * amount;
    let weight = before_weight + after_weight;
    let alpha = (weight + 127) / 255;
    if alpha == 0 {
        return 0;
    }
    let mut result = alpha << 24;
    for shift in [0, 8, 16] {
        let numerator =
            ((before >> shift) & 255) * before_weight + ((after >> shift) & 255) * after_weight;
        let channel = (numerator + weight / 2) / weight;
        result |= channel << shift;
    }
    result
}
pub fn filter(p: u32, mode: u32, amount: f32) -> u32 {
    if p >> 24 == 0 {
        return p;
    }
    let r = ((p >> 16) & 255) as f32;
    let g = ((p >> 8) & 255) as f32;
    let b = (p & 255) as f32;
    let gray = (r * 54. + g * 183. + b * 19.) / 256.;
    let mut out = p & 0xff000000;
    for (v, shift) in [(r, 16), (g, 8), (b, 0)] {
        let v = match mode {
            0 => v + (255. - 2. * v) * amount,
            1 => v + (gray - v) * amount,
            2 => v + amount * 255.,
            3 => (v - 127.5) * (1. + amount).powi(2) + 127.5,
            4 => gray + (v - gray) * (1. + amount),
            _ => v,
        };
        out |= (v.round().clamp(0., 255.) as u32) << shift;
    }
    out
}
fn span<T>(pointer: *const T, count: usize) -> Option<(usize, usize)> {
    let bytes = count.checked_mul(std::mem::size_of::<T>())?;
    if pointer.is_null() || bytes > isize::MAX as usize {
        return None;
    }
    let start = pointer as usize;
    Some((start, start.checked_add(bytes)?))
}
fn overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}
unsafe extern "C" fn render(
    nodes: *const abi::CompositeNode,
    count: usize,
    layers: *const abi::CompositeLayer,
    layer_count: usize,
    width: u32,
    height: u32,
    output: *mut abi::Pixels,
) -> i32 {
    if output.is_null()
        || layers.is_null()
        || layer_count == 0
        || layer_count > 16
        || count > 64
        || (count > 0 && nodes.is_null())
    {
        return abi::ERROR;
    }
    let out = &*output;
    let (w, h, stride) = (out.width as usize, out.height as usize, out.stride as usize);
    if w == 0
        || h == 0
        || width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || out.data.is_null()
        || stride < w
        || out
            .origin_x
            .checked_add(out.width)
            .is_none_or(|v| v > width)
        || out
            .origin_y
            .checked_add(out.height)
            .is_none_or(|v| v > height)
        || (h - 1)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(w))
            .is_none_or(|n| n > out.len)
    {
        return abi::ERROR;
    }
    let used = (h - 1) * stride + w;
    let Some(destination) = span(out.data, used) else {
        return abi::ERROR;
    };
    // The writable slice includes row padding. Reject aliases before creating
    // that slice or writing any pixels, including aliases of instruction data.
    if span(output, 1).is_none_or(|source| overlaps(destination, source))
        || span(layers, layer_count).is_none_or(|source| overlaps(destination, source))
        || (count > 0 && span(nodes, count).is_none_or(|source| overlaps(destination, source)))
    {
        return abi::ERROR;
    }
    let layers = std::slice::from_raw_parts(layers, layer_count);
    if layers.iter().any(|l| {
        l.data.is_null()
            || l.len < width as usize * height as usize
            || l.opacity > 255
            || l.visible > 1
            || l.mode > abi::BLEND_MODE_MAX
            || span(l.data, width as usize * height as usize)
                .is_none_or(|source| overlaps(source, destination))
    }) {
        return abi::ERROR;
    }
    let nodes = if count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(nodes, count)
    };
    for (i, n) in nodes.iter().enumerate() {
        if !n.amount.is_finite()
            || match n.kind {
                0 => n.a as usize >= layer_count,
                1 => {
                    n.a as usize >= i
                        || n.mode > 4
                        || n.amount < if n.mode <= 1 { 0. } else { -1. }
                        || n.amount > 1.
                }
                2 => {
                    n.a as usize >= i
                        || n.b as usize >= i
                        || n.mode > abi::BLEND_MODE_MAX
                        || !(0. ..=1.).contains(&n.amount)
                }
                3 => n.a as usize >= i,
                4 => n.a != 0 || n.b != 0 || n.mode != 0 || n.amount != 0.,
                5 => {
                    n.a as usize >= i
                        || n.b as usize >= i
                        || n.mode != 0
                        || !(0. ..=1.).contains(&n.amount)
                }
                _ => true,
            }
        {
            return abi::ERROR;
        }
    }
    let pixels = std::slice::from_raw_parts_mut(out.data, used);
    let mut values = [0u32; 64];
    for y in 0..h {
        for x in 0..w {
            let index = (out.origin_y as usize + y) * width as usize + out.origin_x as usize + x;
            let layer = |i: usize| {
                let l = &layers[i];
                if l.visible != 0 {
                    let p = opacity(*l.data.add(index), l.opacity);
                    if p >> 24 == 0 {
                        0
                    } else {
                        p
                    }
                } else {
                    0
                }
            };
            let color = if nodes.is_empty() {
                let mut c = 0;
                for (i, l) in layers.iter().enumerate() {
                    c = over(c, layer(i), l.mode);
                }
                c
            } else {
                for (i, n) in nodes.iter().enumerate() {
                    values[i] = match n.kind {
                        0 => layer(n.a as usize),
                        1 => filter(values[n.a as usize], n.mode, n.amount),
                        2 => over(
                            values[n.a as usize],
                            opacity(values[n.b as usize], (n.amount * 255.).round() as u32),
                            n.mode,
                        ),
                        4 => 0,
                        5 => mix(
                            values[n.a as usize],
                            values[n.b as usize],
                            (n.amount * 255.).round() as u32,
                        ),
                        _ => values[n.a as usize],
                    };
                }
                values[count - 1]
            };
            pixels[y * stride + x] = color;
        }
    }
    abi::OK
}
unsafe extern "C" fn describe() -> abi::Bytes {
    abi::Bytes::borrowed(b"{\"id\":\"compositing\"}")
}
static BASE: abi::PluginApi = abi::PluginApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::PluginApi>() as u32,
    describe: Some(describe),
    create: None,
    destroy: None,
    invoke: None,
};
static COMPOSITOR: abi::CompositorApiMaskEffects = abi::CompositorApiMaskEffects {
    base: abi::CompositorApiMasks {
        base: abi::CompositorApiModes {
            base: abi::CompositorApiFeatures {
                base: abi::CompositorApi {
                    abi_version: 1,
                    struct_size: std::mem::size_of::<abi::CompositorApiMaskEffects>() as u32,
                    render: Some(render),
                },
                capabilities: abi::COMPOSITOR_TRANSPARENT
                    | abi::COMPOSITOR_MIX
                    | abi::COMPOSITOR_MASKS
                    | abi::COMPOSITOR_MASK_EFFECTS,
            },
            supported_blend_modes: abi::BLEND_MODES_ALL,
        },
        render_masked: Some(masks::render_masked),
    },
    create_mask_effects: Some(mask_effects::create),
    destroy_mask_effects: Some(mask_effects::destroy),
    mask_effect_region: Some(mask_effects::region),
    reserve_mask_effects: Some(mask_effects::reserve),
};
#[no_mangle]
pub extern "C" fn micopi_plugin_v1() -> *const abi::PluginApi {
    &BASE
}
#[no_mangle]
pub extern "C" fn micopi_compositor_v1() -> *const abi::CompositorApi {
    &COMPOSITOR.base.base.base.base
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(kind: u32, a: u32, b: u32, mode: u32, amount: f32) -> abi::CompositeNode {
        abi::CompositeNode {
            kind,
            a,
            b,
            mode,
            amount,
        }
    }
    fn zero() -> abi::CompositeNode {
        node(4, 0, 0, 0, 0.)
    }
    fn layer(index: u32) -> abi::CompositeNode {
        node(0, index, 0, 0, 1.)
    }
    fn blend(a: u32, b: u32, mode: u32, amount: f32) -> abi::CompositeNode {
        node(2, a, b, mode, amount)
    }
    fn output(a: u32) -> abi::CompositeNode {
        node(3, a, 0, 0, 1.)
    }
    fn descriptors(sources: &[Vec<u32>]) -> Vec<abi::CompositeLayer> {
        sources
            .iter()
            .map(|pixels| abi::CompositeLayer {
                data: pixels.as_ptr(),
                len: pixels.len(),
                opacity: 255,
                visible: 1,
                mode: 0,
            })
            .collect()
    }
    fn render_pixels(
        nodes: &[abi::CompositeNode],
        sources: &[Vec<u32>],
        width: u32,
        height: u32,
    ) -> Vec<u32> {
        let layers = descriptors(sources);
        let mut result = vec![0x12345678; (width * height) as usize];
        let mut out = abi::Pixels {
            data: result.as_mut_ptr(),
            len: result.len(),
            width,
            height,
            stride: width,
            origin_x: 0,
            origin_y: 0,
        };
        assert_eq!(
            unsafe {
                render(
                    nodes.as_ptr(),
                    nodes.len(),
                    layers.as_ptr(),
                    layers.len(),
                    width,
                    height,
                    &mut out,
                )
            },
            abi::OK
        );
        result
    }
    // Independent normalized straight-alpha equation. Blend channels and output
    // alpha use their 8-bit boundaries. Final color retains the original kernel's
    // 127/255 truncation bias, which is slightly below nearest-half rounding.
    // The epsilon only resolves floating representation at exact integer ties.
    fn reference(dst: u32, src: u32, mode: u32, opacity: u32) -> u32 {
        let dst = if dst >> 24 == 0 { 0 } else { dst };
        let sa = ((src >> 24) * opacity / 255) as f64 / 255.;
        let da = (dst >> 24) as f64 / 255.;
        if sa == 0. {
            return dst;
        }
        let quantize = |value: f64| (value + 1e-9).round().clamp(0., 255.) as u32;
        let alpha = quantize((sa + da * (1. - sa)) * 255.);
        let mut result = alpha << 24;
        for shift in [16, 8, 0] {
            let s = ((src >> shift) & 255) as f64;
            let d = ((dst >> shift) & 255) as f64;
            let channel = match mode {
                1 => s * d / 255.,
                2 => 255. - (255. - s) * (255. - d) / 255.,
                3 if d < 128. => 2. * s * d / 255.,
                3 => 255. - 2. * (255. - s) * (255. - d) / 255.,
                4 => s.min(d),
                5 => s.max(d),
                6 => (s + d).min(255.),
                7 => (s - d).abs(),
                _ => s,
            };
            let mixed = quantize(channel) as f64;
            let premultiplied = (1. - sa) * da * d + (1. - da) * sa * s + sa * da * mixed;
            let color = (premultiplied / (alpha as f64 / 255.) + 127. / 255. + 1e-9)
                .floor()
                .clamp(0., 255.) as u32;
            result |= color << shift;
        }
        result
    }

    #[test]
    fn optional_table_retains_v1_prefix_transparent_capability_and_modes() {
        let base = unsafe { &*micopi_compositor_v1() };
        assert_eq!(base.abi_version, 1);
        assert_eq!(
            base.struct_size as usize,
            std::mem::size_of::<abi::CompositorApiMaskEffects>()
        );
        let complete = unsafe { &*(micopi_compositor_v1().cast::<abi::CompositorApiFeatures>()) };
        assert_eq!(
            complete.capabilities,
            abi::COMPOSITOR_TRANSPARENT
                | abi::COMPOSITOR_MIX
                | abi::COMPOSITOR_MASKS
                | abi::COMPOSITOR_MASK_EFFECTS
        );
        let modes = unsafe { &*(micopi_compositor_v1().cast::<abi::CompositorApiModes>()) };
        assert_eq!(modes.supported_blend_modes, abi::BLEND_MODES_ALL);
        assert!(base.render.is_some());
        let masks = unsafe { &*(micopi_compositor_v1().cast::<abi::CompositorApiMasks>()) };
        assert!(masks.render_masked.is_some());
        let effects = unsafe { &*(micopi_compositor_v1().cast::<abi::CompositorApiMaskEffects>()) };
        assert!(
            effects.create_mask_effects.is_some()
                && effects.destroy_mask_effects.is_some()
                && effects.mask_effect_region.is_some()
                && effects.reserve_mask_effects.is_some()
        );
    }

    #[test]
    fn isolated_group_opacity_is_applied_once_after_overlapping_children() {
        let sources = vec![vec![0xffff0000], vec![0xff0000ff]];
        let nodes = [
            zero(),
            layer(0),
            layer(1),
            blend(0, 1, 0, 1.),
            blend(3, 2, 0, 1.),
            blend(0, 4, 0, 0.5),
            output(5),
        ];
        assert_eq!(render_pixels(&nodes, &sources, 1, 1), [0x800000ff]);
        let propagated = [
            zero(),
            layer(0),
            layer(1),
            blend(0, 1, 0, 0.5),
            blend(3, 2, 0, 0.5),
            output(4),
        ];
        assert_eq!(render_pixels(&propagated, &sources, 1, 1), [0xc05500aa]);
        let nested = [
            zero(),
            layer(0),
            layer(1),
            blend(0, 1, 0, 1.),
            blend(3, 2, 0, 1.),
            blend(0, 4, 0, 0.5),
            blend(0, 5, 0, 0.5),
            output(6),
        ];
        assert_eq!(render_pixels(&nested, &sources, 1, 1), [0x400000ff]);
        assert_eq!(sources, [vec![0xffff0000], vec![0xff0000ff]]);
    }

    #[test]
    fn group_boundary_modes_and_alpha_match_independent_straight_alpha_reference() {
        let opaque = [
            0xff4080c0, 0xff202018, 0xffa0a0c8, 0xff414030, 0xff404020, 0xff8080c0, 0xffc0c0e0,
            0xff4040a0,
        ];
        for (mode, expected) in opaque.into_iter().enumerate() {
            let sources = vec![vec![0xff804020], vec![0xffff0000], vec![0xff4080c0]];
            let nodes = [
                zero(),
                layer(0),
                layer(1),
                layer(2),
                blend(0, 2, 0, 1.),
                blend(4, 3, 0, 1.),
                blend(1, 5, mode as u32, 1.),
                output(6),
            ];
            assert_eq!(render_pixels(&nodes, &sources, 1, 1), [expected]);
            for da in [0, 1, 64, 127, 128, 254, 255] {
                for sa in [0, 1, 64, 127, 128, 254, 255] {
                    for opacity in [0, 128, 255] {
                        let backdrop = (da << 24) | 0x804020;
                        let foreground = (sa << 24) | 0x4080c0;
                        let sources = vec![vec![backdrop], vec![foreground]];
                        let nodes = [
                            zero(),
                            layer(0),
                            layer(1),
                            blend(0, 2, 0, 1.),
                            blend(1, 3, mode as u32, opacity as f32 / 255.),
                            output(4),
                        ];
                        assert_eq!(render_pixels(&nodes, &sources, 1, 1),
                            [reference(backdrop, foreground, mode as u32, opacity)],
                            "mode={mode}, backdrop alpha={da}, source alpha={sa}, opacity={opacity}");
                    }
                }
            }
        }
    }

    #[test]
    fn hidden_empty_and_sixteen_source_groups_need_no_extra_source_layer() {
        let sources: Vec<_> = (0..16).map(|i| vec![0xff000000 | (i * 0x10203)]).collect();
        assert_eq!(render_pixels(&[zero(), output(0)], &sources, 1, 1), [0]);
        let hidden = [
            zero(),
            layer(0),
            layer(1),
            blend(0, 1, 0, 1.),
            blend(3, 2, 0, 1.),
            blend(0, 4, 0, 0.),
            output(5),
        ];
        assert_eq!(render_pixels(&hidden, &sources, 1, 1), [0]);
        let mut instructions = vec![zero()];
        let mut tail = 0;
        for index in 0..16 {
            let slot = instructions.len() as u32;
            instructions.push(layer(index));
            instructions.push(blend(tail, slot, 0, 1.));
            tail = slot + 1;
        }
        instructions.push(output(tail));
        assert_eq!(instructions.len(), 34);
        assert_eq!(
            render_pixels(&instructions, &sources, 1, 1),
            render_pixels(&[], &sources, 1, 1)
        );
        let one = vec![vec![0x00112233, 0x804080c0]];
        assert_eq!(
            render_pixels(&[layer(0), output(0)], &one, 2, 1),
            [0, 0x804080c0]
        );
    }

    #[test]
    fn dirty_region_uses_global_source_coordinates_and_preserves_output_padding() {
        let sources = vec![(0..20).map(|i| 0xff000000 | i).collect()];
        let layers = descriptors(&sources);
        let nodes = [zero(), layer(0), blend(0, 1, 0, 0.5), output(2)];
        let mut result = vec![0x12345678; 10];
        let mut out = abi::Pixels {
            data: unsafe { result.as_mut_ptr().add(1) },
            len: 8,
            width: 2,
            height: 2,
            stride: 4,
            origin_x: 2,
            origin_y: 1,
        };
        assert_eq!(
            unsafe {
                render(
                    nodes.as_ptr(),
                    nodes.len(),
                    layers.as_ptr(),
                    layers.len(),
                    5,
                    4,
                    &mut out,
                )
            },
            abi::OK
        );
        assert_eq!(
            result,
            [
                0x12345678, 0x80000007, 0x80000008, 0x12345678, 0x12345678, 0x8000000c, 0x8000000d,
                0x12345678, 0x12345678, 0x12345678
            ]
        );
    }

    #[test]
    fn invalid_instruction_alias_and_region_bounds_fail_before_any_output_write() {
        let sources = vec![vec![0xffabcdef; 4]];
        let mut layers = descriptors(&sources);
        let mut result = vec![0x12345678; 4];
        let mut out = abi::Pixels {
            data: result.as_mut_ptr(),
            len: 4,
            width: 2,
            height: 2,
            stride: 2,
            origin_x: 0,
            origin_y: 0,
        };
        for nodes in [
            vec![node(0, 1, 0, 0, 1.)],
            vec![output(0)],
            vec![zero(), blend(2, 0, 0, 1.)],
            vec![zero(), node(1, 1, 0, 0, 1.)],
            vec![node(4, 1, 0, 0, 0.)],
            vec![node(4, 0, 1, 0, 0.)],
            vec![node(4, 0, 0, 1, 0.)],
            vec![node(4, 0, 0, 0, 1.)],
            vec![node(4, 0, 0, 0, f32::NAN)],
            vec![zero(); 65],
        ] {
            assert_eq!(
                unsafe {
                    render(
                        nodes.as_ptr(),
                        nodes.len(),
                        layers.as_ptr(),
                        layers.len(),
                        2,
                        2,
                        &mut out,
                    )
                },
                abi::ERROR
            );
            assert_eq!(result, [0x12345678; 4]);
        }
        let nodes = [zero(), output(0)];
        for dimension in [0, 4097] {
            assert_eq!(
                unsafe {
                    render(
                        nodes.as_ptr(),
                        nodes.len(),
                        layers.as_ptr(),
                        1,
                        dimension,
                        2,
                        &mut out,
                    )
                },
                abi::ERROR
            );
        }
        out.len = 3;
        assert_eq!(
            unsafe {
                render(
                    nodes.as_ptr(),
                    nodes.len(),
                    layers.as_ptr(),
                    1,
                    2,
                    2,
                    &mut out,
                )
            },
            abi::ERROR
        );
        out.len = 4;
        out.origin_x = 1;
        assert_eq!(
            unsafe {
                render(
                    nodes.as_ptr(),
                    nodes.len(),
                    layers.as_ptr(),
                    1,
                    2,
                    2,
                    &mut out,
                )
            },
            abi::ERROR
        );
        out.origin_x = 0;
        layers[0].len = 3;
        assert_eq!(
            unsafe {
                render(
                    nodes.as_ptr(),
                    nodes.len(),
                    layers.as_ptr(),
                    1,
                    2,
                    2,
                    &mut out,
                )
            },
            abi::ERROR
        );
        layers[0].len = 4;
        out.data = (usize::MAX - 3) as *mut u32;
        assert_eq!(
            unsafe {
                render(
                    nodes.as_ptr(),
                    nodes.len(),
                    layers.as_ptr(),
                    1,
                    2,
                    2,
                    &mut out,
                )
            },
            abi::ERROR
        );
        out.data = result.as_mut_ptr();
        layers[0].data = result.as_ptr();
        assert_eq!(
            unsafe {
                render(
                    nodes.as_ptr(),
                    nodes.len(),
                    layers.as_ptr(),
                    1,
                    2,
                    2,
                    &mut out,
                )
            },
            abi::ERROR
        );
        layers[0].data = sources[0].as_ptr();
        out.data = layers.as_mut_ptr().cast();
        assert_eq!(
            unsafe {
                render(
                    nodes.as_ptr(),
                    nodes.len(),
                    layers.as_ptr(),
                    1,
                    2,
                    2,
                    &mut out,
                )
            },
            abi::ERROR
        );
        assert_eq!(result, [0x12345678; 4]);
        assert_eq!(sources[0], [0xffabcdef; 4]);
    }
}
