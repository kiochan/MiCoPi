//! A small standalone example an AI can modify without linking host or GUI code.
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::ffi::c_void;
unsafe extern "C" fn describe() -> abi::Bytes {
    abi::Bytes::borrowed(include_bytes!("contributions.json"))
}
unsafe fn call(host: &abi::HostApi, id: &str, args: Value) -> Result<Value, String> {
    let input = serde_json::to_vec(&args).map_err(|e| e.to_string())?;
    let mut output = Vec::<u8>::new();
    let status = (host.dispatch)(
        host.context,
        abi::Bytes::borrowed(id.as_bytes()),
        abi::Bytes::borrowed(&input),
        abi::Reply {
            context: (&mut output as *mut Vec<u8>).cast(),
            write: abi::collect,
        },
    );
    let output: Value = serde_json::from_slice(&output).map_err(|e| e.to_string())?;
    if status != abi::OK {
        return Err(output.to_string());
    }
    Ok(output["result"].clone())
}
unsafe extern "C" fn invoke(
    _: *mut c_void,
    handler: abi::Bytes,
    args: abi::Bytes,
    host: *const abi::HostApi,
    reply: abi::Reply,
) -> i32 {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<Value, String> {
            let host = host.as_ref().ok_or("missing host")?;
            if host.abi_version != abi::ABI_VERSION
                || host.struct_size < std::mem::size_of::<abi::HostApi>() as u32
            {
                return Err("incompatible host".into());
            }
            let args: Value = serde_json::from_slice(args.as_slice()).map_err(|e| e.to_string())?;
            match std::str::from_utf8(handler.as_slice()).map_err(|e| e.to_string())? {
                "context" => call(
                    host,
                    "context.read",
                    json!({"resources":["document.active","workspace.documents","painting.tool","input.pointer","native_paint.stats"]}),
                ),
                "checkerboard" => call(
                    host,
                    "pipeline.run",
                    json!({"steps":[
                        {"id":"before","action":"context.read","args":{"resources":["document.active","native_paint.stats"]}},
                        {"id":"paint","action":"native.checkerboard","args":{"cell":args["cell"].as_u64().unwrap_or(16)}},
                        {"id":"after","action":"canvas.state"}
                    ]}),
                ),
                _ => Err("unknown handler".into()),
            }
        },
    ));
    let (status, value) = match outcome {
        Ok(Ok(v)) => (abi::OK, v),
        Ok(Err(e)) => (abi::ERROR, json!({"error":e})),
        Err(_) => (abi::ERROR, json!({"error":"plugin panicked"})),
    };
    let bytes = serde_json::to_vec(&value).unwrap_or_default();
    if reply.send(&bytes) != abi::OK {
        return abi::ERROR;
    }
    status
}
static API: abi::PluginApi = abi::PluginApi {
    abi_version: abi::ABI_VERSION,
    struct_size: std::mem::size_of::<abi::PluginApi>() as u32,
    describe: Some(describe),
    create: None,
    destroy: None,
    invoke: Some(invoke),
};
#[no_mangle]
pub extern "C" fn micopi_plugin_v1() -> *const abi::PluginApi {
    &API
}
