use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::ffi::c_void;
struct Kernel {
    operation: Operation,
    alpha_locked: bool,
    selection: abi::Coverage,
}
enum Operation {
    Solid {
        color: u32,
    },
    Fill {
        x: usize,
        y: usize,
        color: u32,
        tolerance: u32,
        contiguous: bool,
        expand: i32,
        feather: u32,
    },
    Gradient {
        from: [f32; 2],
        to: [f32; 2],
        a: u32,
        b: u32,
        radial: bool,
        spread: super::area::Spread,
    },
}
fn color(value: &Value, opacity: f32) -> Result<u32, String> {
    let s = value.as_str().ok_or("Missing color")?;
    let hex = s
        .strip_prefix('#')
        .ok_or("Color must be #RRGGBB or #RRGGBBAA")?;
    if ![6, 8].contains(&hex.len()) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid color".into());
    }
    let n = u32::from_str_radix(hex, 16).map_err(|_| "Invalid color")?;
    let (rgb, a) = if hex.len() == 6 {
        (n, 255)
    } else {
        (n >> 8, n & 255)
    };
    Ok(rgb | (((a as f32 * opacity).round() as u32) << 24))
}
unsafe extern "C" fn render(user: *mut c_void, buffer: *mut abi::Pixels) -> i32 {
    if user.is_null() || buffer.is_null() {
        return abi::ERROR;
    }
    let b = &mut *buffer;
    if b.data.is_null()
        || b.width == 0
        || b.height == 0
        || b.stride != b.width
        || b.len != b.width as usize * b.height as usize
    {
        return abi::ERROR;
    }
    let p = std::slice::from_raw_parts_mut(b.data, b.len);
    let kernel = &*user.cast::<Kernel>();
    match &kernel.operation {
        Operation::Solid { color } => {
            if kernel.alpha_locked {
                super::area::fill_color::<true>(p, *color)
            } else {
                super::area::fill_color::<false>(p, *color)
            }
        }
        Operation::Fill {
            x,
            y,
            color,
            tolerance,
            contiguous,
            expand,
            feather,
        } => {
            if !kernel.selection.data.is_null() {
                let m = kernel.selection;
                if (m.width, m.height) != (b.width, b.height)
                    || m.stride < m.width
                    || m.len < (m.height as usize - 1) * m.stride as usize + m.width as usize
                {
                    return abi::ERROR;
                }
                let allowed = (std::slice::from_raw_parts(m.data, m.len), m.stride as usize);
                (if kernel.alpha_locked {
                    super::fill_mask::apply_selected::<true>
                } else {
                    super::fill_mask::apply_selected::<false>
                })(
                    p,
                    b.width as usize,
                    b.height as usize,
                    *y * b.width as usize + *x,
                    *color,
                    *tolerance,
                    *contiguous,
                    *expand,
                    *feather,
                    allowed,
                );
            } else if *expand != 0 || *feather != 0 {
                (if kernel.alpha_locked {
                    super::fill_mask::apply_locked
                } else {
                    super::fill_mask::apply
                })(
                    p,
                    b.width as usize,
                    b.height as usize,
                    *y * b.width as usize + *x,
                    *color,
                    *tolerance,
                    *contiguous,
                    *expand,
                    *feather,
                );
            } else if *contiguous {
                (if kernel.alpha_locked {
                    super::area::fill_locked
                } else {
                    super::area::fill
                })(
                    p,
                    b.width as usize,
                    b.height as usize,
                    *x,
                    *y,
                    *color,
                    *tolerance,
                )
            } else {
                (if kernel.alpha_locked {
                    super::area::fill_all_locked
                } else {
                    super::area::fill_all
                })(p, *y * b.width as usize + *x, *color, *tolerance)
            }
        }
        Operation::Gradient {
            from,
            to,
            a,
            b: color,
            radial,
            spread,
        } => (if kernel.alpha_locked {
            super::area::gradient_locked
        } else {
            super::area::gradient
        })(
            p,
            b.width as usize,
            *from,
            *to,
            *a,
            *color,
            *radial,
            *spread,
        ),
    }
    abi::OK
}
pub unsafe fn invoke(
    host: &abi::HostApi,
    doc: &Value,
    args: &Value,
    handler: &str,
) -> Result<Value, String> {
    let w = doc["width"].as_u64().ok_or("Missing canvas width")? as usize;
    let h = doc["height"].as_u64().ok_or("Missing canvas height")? as usize;
    let opacity = args.get("opacity").and_then(Value::as_f64).unwrap_or(1.);
    if !opacity.is_finite() || !(0. ..=1.).contains(&opacity) {
        return Err("Invalid opacity".into());
    }
    let operation = if handler == "fill_color" {
        Operation::Solid {
            color: color(&args["color"], opacity as f32)?,
        }
    } else if handler == "fill" {
        let x = args["x"]
            .as_u64()
            .filter(|v| *v < w as u64)
            .ok_or("Fill seed outside canvas")? as usize;
        let y = args["y"]
            .as_u64()
            .filter(|v| *v < h as u64)
            .ok_or("Fill seed outside canvas")? as usize;
        let tolerance = args.get("tolerance").and_then(Value::as_u64).unwrap_or(0);
        if tolerance > 255 {
            return Err("Invalid fill tolerance".into());
        }
        let expand = args.get("expand").and_then(Value::as_i64).unwrap_or(0);
        let feather = args.get("feather").and_then(Value::as_u64).unwrap_or(0);
        if !(-32..=32).contains(&expand) || feather > 32 {
            return Err("Fill edge controls must be within 32 pixels".into());
        }
        Operation::Fill {
            x,
            y,
            color: color(&args["color"], opacity as f32)?,
            tolerance: tolerance as u32,
            expand: expand as i32,
            feather: feather as u32,
            contiguous: args
                .get("contiguous")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        }
    } else {
        let point = |x: &str, y: &str| -> Result<[f32; 2], String> {
            let number = |k: &str| {
                args[k]
                    .as_f64()
                    .filter(|v| v.is_finite() && (-4096. ..=8192.).contains(v))
                    .map(|v| v as f32)
                    .ok_or_else(|| format!("Invalid {k}"))
            };
            Ok([number(x)?, number(y)?])
        };
        let from = point("x0", "y0")?;
        let to = point("x1", "y1")?;
        if (to[0] - from[0]).hypot(to[1] - from[1]) < 0.001 {
            return Err("Gradient endpoints must differ".into());
        }
        let kind = args.get("kind").and_then(Value::as_str).unwrap_or("linear");
        if !["linear", "radial"].contains(&kind) {
            return Err("Invalid gradient kind".into());
        }
        let spread = match args.get("spread").and_then(Value::as_str).unwrap_or("pad") {
            "pad" => super::area::Spread::Pad,
            "repeat" => super::area::Spread::Repeat,
            "reflect" => super::area::Spread::Reflect,
            _ => return Err("Invalid gradient spread".into()),
        };
        let mut a = color(&args["start_color"], opacity as f32)?;
        let mut b = color(&args["end_color"], opacity as f32)?;
        if args
            .get("reverse")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            std::mem::swap(&mut a, &mut b);
        }
        Operation::Gradient {
            from,
            to,
            a,
            b,
            radial: kind == "radial",
            spread,
        }
    };
    let active = doc["active_layer"].as_u64().ok_or("Missing active layer")? as usize;
    let mut selection = abi::Coverage::default();
    if handler == "fill" {
        if host.struct_size >= std::mem::size_of::<abi::HostApiSelection>() as u32 {
            let host = &*(host as *const abi::HostApi).cast::<abi::HostApiSelection>();
            if (host.selection)(host.base.context, &mut selection) != abi::OK {
                return Err("Selection snapshot unavailable".into());
            }
            if selection.data.is_null() {
                if selection.len != 0
                    || selection.width != 0
                    || selection.height != 0
                    || selection.stride != 0
                {
                    return Err("Invalid empty selection snapshot".into());
                }
            } else if (selection.width as usize, selection.height as usize) != (w, h)
                || w > 4096
                || h > 4096
                || w == 0
                || h == 0
                || selection.stride < selection.width
                || selection.stride > 4096
                || selection.len > 4096 * 4096
                || (h - 1)
                    .checked_mul(selection.stride as usize)
                    .and_then(|v| v.checked_add(w))
                    .is_none_or(|v| v > selection.len)
            {
                return Err("Invalid selection snapshot dimensions or storage".into());
            }
        } else if doc["selection"]["active"].as_bool().unwrap_or(false) {
            return Err("Host does not provide selection-aware fill input".into());
        }
    }
    let mut kernel = Kernel {
        operation,
        selection,
        alpha_locked: doc["layers"][active]["alpha_locked"]
            .as_bool()
            .unwrap_or(false),
    };
    if (host.edit_region)(
        host.context,
        abi::Region {
            x: 0,
            y: 0,
            width: w as u32,
            height: h as u32,
        },
        Some(render),
        (&mut kernel as *mut Kernel).cast(),
    ) != abi::OK
    {
        return Err("Pixel edit rejected by host".into());
    }
    Ok(json!({"width":w,"height":h}))
}

#[cfg(test)]
mod tests {
    use super::*;
    static EDITS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    unsafe extern "C" fn dispatch(
        _: *mut c_void,
        _: abi::Bytes,
        _: abi::Bytes,
        _: abi::Reply,
    ) -> i32 {
        abi::ERROR
    }
    unsafe extern "C" fn edit(
        _: *mut c_void,
        _: abi::Region,
        _: Option<abi::Transform>,
        _: *mut c_void,
    ) -> i32 {
        EDITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        abi::ERROR
    }
    unsafe extern "C" fn selection(context: *mut c_void, out: *mut abi::Coverage) -> i32 {
        *out = std::ptr::read_unaligned(context.cast::<abi::Coverage>());
        abi::OK
    }
    #[test]
    fn selected_fill_rejects_old_host_and_invalid_selection_descriptors() {
        EDITS.store(0, std::sync::atomic::Ordering::Relaxed);
        let args = json!({"x":0,"y":0,"color":"#ff0000"});
        let doc = json!({"width":2,"height":2,"active_layer":0,"selection":{"active":true}});
        let base = abi::HostApi {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::HostApi>() as u32,
            context: std::ptr::null_mut(),
            dispatch,
            edit_region: edit,
        };
        assert!(unsafe { invoke(&base, &doc, &args, "fill") }
            .unwrap_err()
            .contains("selection-aware fill input"));
        let coverage = [255u8; 4];
        let mut mask = abi::Coverage {
            data: coverage.as_ptr(),
            len: 3,
            width: 2,
            height: 2,
            stride: 2,
        };
        let mut host = abi::HostApiSelection { base, selection };
        host.base.struct_size = std::mem::size_of::<abi::HostApiSelection>() as u32;
        host.base.context = (&mut mask as *mut abi::Coverage).cast();
        assert!(unsafe { invoke(&host.base, &doc, &args, "fill") }
            .unwrap_err()
            .contains("snapshot dimensions or storage"));
        mask = abi::Coverage {
            data: std::ptr::null(),
            len: 1,
            ..abi::Coverage::default()
        };
        assert!(unsafe { invoke(&host.base, &doc, &args, "fill") }
            .unwrap_err()
            .contains("empty selection snapshot"));
        std::hint::black_box(mask);
        assert_eq!(EDITS.load(std::sync::atomic::Ordering::Relaxed), 0);
    }
}
