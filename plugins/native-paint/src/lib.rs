//! This crate links only the ABI SDK and JSON parser, never the MiCoPi host.
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::ffi::c_void;
mod area;
mod area_action;
mod fill_mask;
mod hsl_math;
mod hue_saturation;
mod spatial;
#[derive(Default)]
struct Instance {
    calls: u64,
}
unsafe extern "C" fn describe() -> abi::Bytes {
    abi::Bytes::borrowed(include_bytes!("contributions.json"))
}
unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(Instance::default())).cast()
}
unsafe extern "C" fn destroy(state: *mut c_void) {
    if !state.is_null() {
        drop(Box::from_raw(state.cast::<Instance>()));
    }
}
struct Kernel {
    checkerboard: bool,
    cell: usize,
    spatial: Option<spatial::Spatial>,
    hue_saturation: Option<hue_saturation::Parameters>,
}
unsafe extern "C" fn transform(user: *mut c_void, buffer: *mut abi::Pixels) -> i32 {
    if user.is_null() || buffer.is_null() {
        return abi::ERROR;
    }
    let kernel = &*user.cast::<Kernel>();
    let buffer = &mut *buffer;
    if buffer.width == 0
        || buffer.height == 0
        || buffer.stride != buffer.width
        || buffer.len != buffer.width as usize * buffer.height as usize
        || buffer.data.is_null()
    {
        return abi::ERROR;
    }
    let pixels = std::slice::from_raw_parts_mut(buffer.data, buffer.len);
    if let Some(parameters) = kernel.hue_saturation {
        for pixel in pixels {
            *pixel = parameters.apply(*pixel);
        }
        return abi::OK;
    }
    if let Some(operation) = kernel.spatial {
        spatial::apply(
            pixels,
            buffer.width as usize,
            buffer.height as usize,
            operation,
        );
        return abi::OK;
    }
    for y in 0..buffer.height as usize {
        for x in 0..buffer.width as usize {
            let p = &mut pixels[y * buffer.stride as usize + x];
            if kernel.checkerboard {
                *p = if ((x + buffer.origin_x as usize) / kernel.cell
                    + (y + buffer.origin_y as usize) / kernel.cell)
                    % 2
                    == 0
                {
                    0xff3d8ede
                } else {
                    0xffecb96b
                };
            } else if *p >> 24 != 0 {
                *p = (*p & 0xff000000) | (!*p & 0x00ffffff);
            }
        }
    }
    abi::OK
}
unsafe fn run(
    state: *mut c_void,
    handler: abi::Bytes,
    args: abi::Bytes,
    host: *const abi::HostApi,
) -> Result<Value, String> {
    if state.is_null() || host.is_null() {
        return Err("missing plugin instance or host".into());
    }
    let host = &*host;
    if host.abi_version != abi::ABI_VERSION
        || host.struct_size < std::mem::size_of::<abi::HostApi>() as u32
    {
        return Err("incompatible host".into());
    }
    let handler = std::str::from_utf8(handler.as_slice()).map_err(|e| e.to_string())?;
    let args: Value = serde_json::from_slice(args.as_slice()).map_err(|e| e.to_string())?;
    let state = &mut *state.cast::<Instance>();
    state.calls += 1;
    if handler == "stats" {
        return Ok(json!({"implementation":"independent Rust DLL","calls":state.calls}));
    }
    if ![
        "invert",
        "checkerboard",
        "invert_region",
        "flip_horizontal",
        "flip_vertical",
        "rotate_180",
        "translate",
        "fill",
        "gradient",
        "fill_color",
        "hue_saturation",
    ]
    .contains(&handler)
    {
        return Err(format!("unknown native handler: {handler}"));
    }
    let mut reply = Vec::<u8>::new();
    let target = abi::Reply {
        context: (&mut reply as *mut Vec<u8>).cast(),
        write: abi::collect,
    };
    if (host.dispatch)(
        host.context,
        abi::Bytes::borrowed(b"canvas.state"),
        abi::Bytes::borrowed(b"{}"),
        target,
    ) != abi::OK
    {
        return Err("canvas.state failed".into());
    }
    let document: Value = serde_json::from_slice(&reply).map_err(|e| e.to_string())?;
    if handler == "fill" || handler == "gradient" || handler == "fill_color" {
        return area_action::invoke(host, &document["result"], &args, handler);
    }
    let region = if handler == "invert_region" {
        let number = |key: &str| {
            args[key]
                .as_f64()
                .filter(|v| v.is_finite() && v.fract() == 0. && *v >= 0. && *v <= u32::MAX as f64)
                .map(|v| v as u32)
                .ok_or_else(|| format!("invalid {key}"))
        };
        abi::Region {
            x: number("x")?,
            y: number("y")?,
            width: number("width")?,
            height: number("height")?,
        }
    } else {
        abi::Region {
            x: 0,
            y: 0,
            width: document["result"]["width"]
                .as_u64()
                .ok_or("missing width")? as u32,
            height: document["result"]["height"]
                .as_u64()
                .ok_or("missing height")? as u32,
        }
    };
    let cell = args["cell"].as_f64().unwrap_or(16.).clamp(1., 256.) as usize;
    let mut kernel = Kernel {
        hue_saturation: if handler == "hue_saturation" {
            Some(hue_saturation::parse(&args)?)
        } else {
            None
        },
        spatial: match handler {
            "flip_horizontal" => Some(spatial::Spatial::FlipHorizontal),
            "flip_vertical" => Some(spatial::Spatial::FlipVertical),
            "rotate_180" => Some(spatial::Spatial::Rotate180),
            "translate" => {
                let offset = |key: &str| {
                    args[key]
                        .as_i64()
                        .filter(|v| (-4096..=4096).contains(v))
                        .map(|v| v as i32)
                        .ok_or_else(|| format!("invalid {key}"))
                };
                Some(spatial::Spatial::Translate {
                    dx: offset("dx")?,
                    dy: offset("dy")?,
                })
            }
            _ => None,
        },
        checkerboard: handler == "checkerboard",
        cell,
    };
    if (host.edit_region)(
        host.context,
        region,
        Some(transform),
        (&mut kernel as *mut Kernel).cast(),
    ) != abi::OK
    {
        return Err("pixel region rejected by host".into());
    }
    Ok(
        json!({"implementation":"independent Rust DLL","calls":state.calls,"width":region.width,"height":region.height}),
    )
}
unsafe extern "C" fn invoke(
    state: *mut c_void,
    handler: abi::Bytes,
    args: abi::Bytes,
    host: *const abi::HostApi,
    reply: abi::Reply,
) -> i32 {
    // Unwinding must never cross the C ABI; panic=abort builds remain process-fatal.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(state, handler, args, host)
    }));
    let (status, value) = match outcome {
        Ok(Ok(value)) => (abi::OK, value),
        Ok(Err(error)) => (abi::ERROR, json!({"error":error})),
        Err(_) => (abi::ERROR, json!({"error":"native plugin panicked"})),
    };
    let bytes = serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec());
    if reply.send(&bytes) != abi::OK {
        return abi::ERROR;
    }
    status
}
static API: abi::PluginApi = abi::PluginApi {
    abi_version: abi::ABI_VERSION,
    struct_size: std::mem::size_of::<abi::PluginApi>() as u32,
    describe: Some(describe),
    create: Some(create),
    destroy: Some(destroy),
    invoke: Some(invoke),
};
#[no_mangle]
pub extern "C" fn micopi_plugin_v1() -> *const abi::PluginApi {
    &API
}
