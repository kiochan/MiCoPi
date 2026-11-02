use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::ffi::c_void;
enum Operation {
    Fill {
        x: usize,
        y: usize,
        color: u32,
        tolerance: u32,
        contiguous: bool,
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
    match &*user.cast::<Operation>() {
        Operation::Fill {
            x,
            y,
            color,
            tolerance,
            contiguous,
        } => {
            if *contiguous {
                super::area::fill(
                    p,
                    b.width as usize,
                    b.height as usize,
                    *x,
                    *y,
                    *color,
                    *tolerance,
                )
            } else {
                super::area::fill_all(p, *y * b.width as usize + *x, *color, *tolerance)
            }
        }
        Operation::Gradient {
            from,
            to,
            a,
            b: color,
            radial,
            spread,
        } => super::area::gradient(
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
    let mut operation = if handler == "fill" {
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
        Operation::Fill {
            x,
            y,
            color: color(&args["color"], opacity as f32)?,
            tolerance: tolerance as u32,
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
    if (host.edit_region)(
        host.context,
        abi::Region {
            x: 0,
            y: 0,
            width: w as u32,
            height: h as u32,
        },
        Some(render),
        (&mut operation as *mut Operation).cast(),
    ) != abi::OK
    {
        return Err("Pixel edit rejected by host".into());
    }
    Ok(json!({"width":w,"height":h}))
}
