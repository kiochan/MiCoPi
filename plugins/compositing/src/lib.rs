//! Stateless region compositor. One DLL call per damaged region, no image intermediates.
use micopi_plugin_api as abi;
pub fn over(dst: u32, src: u32, mode: u32) -> u32 {
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
    let out = &mut *output;
    let (w, h, stride) = (out.width as usize, out.height as usize, out.stride as usize);
    if w == 0
        || h == 0
        || width == 0
        || height == 0
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
    let layers = std::slice::from_raw_parts(layers, layer_count);
    if layers.iter().any(|l| {
        l.data.is_null()
            || l.len < width as usize * height as usize
            || l.opacity > 255
            || l.visible > 1
            || l.mode > 7
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
                        || n.mode > 7
                        || !(0. ..=1.).contains(&n.amount)
                }
                3 => n.a as usize >= i,
                _ => true,
            }
        {
            return abi::ERROR;
        }
    }
    let pixels = std::slice::from_raw_parts_mut(out.data, out.len);
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
static COMPOSITOR: abi::CompositorApi = abi::CompositorApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::CompositorApi>() as u32,
    render: Some(render),
};
#[no_mangle]
pub extern "C" fn micopi_plugin_v1() -> *const abi::PluginApi {
    &BASE
}
#[no_mangle]
pub extern "C" fn micopi_compositor_v1() -> *const abi::CompositorApi {
    &COMPOSITOR
}
