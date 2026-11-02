use super::*;
use crate::{
    automation::{batch, mcp::Server},
    plugins::Registry,
};
use serde_json::{json, Value};
use std::{io::Cursor, path::PathBuf};
fn session() -> Session {
    Session::new(Registry::bundled()).unwrap()
}
#[test]
fn startup_uses_the_selected_tools_manifest_parameters() {
    let mut registry = Registry::bundled();
    registry.tools.get_mut("brush").unwrap().parameters = json!({"radius":19,"color":"#abc123"});
    let s = Session::new(registry).unwrap();
    assert_eq!(s.parameters.radius, 19.);
    assert_eq!(s.parameters.color, "#ABC123");
}
fn apply(s: &mut Session, id: &str, args: Value) -> Value {
    s.apply(Action::new(id, args)).unwrap()
}
fn folder(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-output/runtime")
        .join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}
#[test]
fn selected_tool_controls_canvas_edits_including_custom_brushes() {
    let mut s = session();
    apply(&mut s, "canvas.new", json!({"width":64,"height":64}));
    apply(
        &mut s,
        "brush.set",
        json!({"color":"#FF0000","radius":4,"stabilizer":0,"pressure_size":false}),
    );
    apply(
        &mut s,
        "canvas.stroke",
        json!({"points":[{"x":10,"y":32},{"x":54,"y":32}]}),
    );
    assert_ne!(s.doc.layers[0].pixels[32 * 64 + 32] >> 24, 0);
    apply(&mut s, "tool.select", json!({"tool":"eraser"}));
    apply(
        &mut s,
        "canvas.stroke",
        json!({"points":[{"x":32,"y":24},{"x":32,"y":40}]}),
    );
    assert_eq!(s.doc.layers[0].pixels[32 * 64 + 32] >> 24, 0);
    apply(&mut s, "history.undo", json!({}));
    assert_ne!(s.doc.layers[0].pixels[32 * 64 + 32], 0);
    apply(&mut s, "tool.select", json!({"tool":"picker"}));
    apply(&mut s, "canvas.stroke", json!({"points":[{"x":20,"y":32}]}));
    assert_eq!(s.parameters.color, "#FF0000");
    let before = s.doc.layers[0].pixels.clone();
    apply(&mut s, "tool.select", json!({"tool":"hand"}));
    apply(
        &mut s,
        "canvas.stroke",
        json!({"points":[{"x":10,"y":10},{"x":20,"y":25}]}),
    );
    assert_eq!(s.view.pan_x, 10.);
    assert_eq!(s.view.pan_y, 15.);
    assert_eq!(s.doc.layers[0].pixels, before);
}
#[test]
fn startup_plugin_loading_is_deterministic_and_does_not_overwrite_customization() {
    let dir = folder("plugins").join("packs");
    let first = Registry::load(&dir).unwrap();
    assert_eq!(first.loaded.len(), 5);
    let file = dir.join("painting/manifest.json");
    let mut value: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    value["tools"][0]["title"] = json!("我的画笔");
    std::fs::write(&file, value.to_string()).unwrap();
    let loaded = Registry::load(&dir).unwrap();
    assert_eq!(loaded.tools["brush"].title, "我的画笔");
    std::fs::write(dir.join("broken.json"), "{").unwrap();
    let loaded = Registry::load(&dir).unwrap();
    assert_eq!(loaded.loaded.len(), 5);
    assert_eq!(loaded.diagnostics.len(), 1);
}
#[test]
fn macro_plugins_use_the_same_actions_and_cycles_fail_before_editing() {
    let dir = folder("macro").join("packs");
    Registry::load(&dir).unwrap();
    std::fs::write(dir.join("custom.json"),json!({"api_version":1,"id":"custom","name":"Macros","requires":["painting"],"tools":[{"id":"custom.blue","title":"Blue","kind":"brush","parameters":{"color":"#0000FF","radius":2}}],"actions":[{"id":"custom.draw","title":"Draw","input_schema":{"type":"object","properties":{"points":{"type":"array"}},"required":["points"]},"steps":[{"action":"tool.select","args":{"tool":"custom.blue"}},{"action":"canvas.stroke","args":{"points":{"$arg":"points"}}}]},{"id":"custom.loop","title":"Loop","steps":[{"action":"custom.loop"}]}]}).to_string()).unwrap();
    let mut s = Session::new(Registry::load(&dir).unwrap()).unwrap();
    apply(&mut s, "canvas.new", json!({"width":32,"height":32}));
    apply(&mut s, "custom.draw", json!({"points":[{"x":16,"y":16}]}));
    assert_eq!(s.parameters.color, "#0000FF");
    assert_eq!(s.doc.layers[0].pixels[16 * 32 + 16] & 0xffffff, 0x0000ff);
    let revision = s.revision;
    let pixels = s.doc.layers[0].pixels.clone();
    assert!(s.apply(Action::new("custom.loop", json!({}))).is_err());
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.layers[0].pixels, pixels);
}
#[test]
fn bad_arguments_and_stale_revision_leave_the_document_unchanged() {
    let mut s = session();
    let original = s.state();
    for (id, args) in [
        ("canvas.new", json!({"width":0,"height":64})),
        ("canvas.new", json!({"width":32})),
        ("brush.set", json!({"radius":9999})),
        ("brush.set", json!({"color":"red"})),
        ("layer.select", json!({"index":12})),
        ("canvas.stroke", json!({"points":[]})),
        ("canvas.pointer", json!({"phase":"bad","x":1,"y":1})),
    ] {
        assert!(s.apply(Action::new(id, args)).is_err(), "{id}");
        assert_eq!(s.state(), original);
    }
    let mut request = Action::new("canvas.new", json!({"width":32,"height":32}));
    request.expected_revision = Some(999);
    assert_eq!(s.apply(request).unwrap_err().code, "revision_conflict");
    apply(&mut s, "canvas.new", json!({"width":32.0,"height":32.0}));
    assert_eq!(s.doc.width, 32);
}
#[test]
fn batch_export_is_window_free_and_roundtrips_transparent_pixels() {
    let dir = folder("batch");
    let png = dir.join("画布.png");
    let project = dir.join("画布.mcp");
    let mut s = session();
    let script = [
        Action::new("canvas.new", json!({"width":48,"height":40})),
        Action::new(
            "brush.set",
            json!({"color":"#0077FF","radius":3,"stabilizer":0}),
        ),
        Action::new(
            "canvas.stroke",
            json!({"points":[{"x":5,"y":20},{"x":42,"y":20}]}),
        ),
        Action::new("document.save", json!({"path":project})),
        Action::new("canvas.export", json!({"path":png})),
    ];
    let text = script
        .iter()
        .map(|a| serde_json::to_string(a).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let mut output = vec![];
    batch::run(&mut s, &mut Cursor::new(text), &mut output).unwrap();
    assert_eq!(String::from_utf8(output).unwrap().lines().count(), 5);
    s.doc.compose();
    let reopened = codec::open(&png).unwrap();
    assert_eq!(reopened.layers[0].pixels, s.doc.composite);
    assert_eq!(reopened.layers[0].pixels[0], 0);
    let loaded = crate::engine::Document::load(&project).unwrap();
    assert_eq!(loaded.layers[0].pixels, s.doc.layers[0].pixels);
    assert_eq!(codec::base64(b"f"), "Zg==");
    assert_eq!(codec::base64(b"fo"), "Zm8=");
    assert_eq!(codec::base64(b"foo"), "Zm9v");
}
#[test]
fn mcp_negotiates_lists_plugin_actions_and_dispatches_errors_without_stdout_logs() {
    let mut server = Server::new(session());
    let early = server
        .message(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .unwrap();
    assert_eq!(early["error"]["code"], -32002);
    let init=server.message(json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    assert!(server
        .message(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .is_none());
    let list = server
        .message(json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}))
        .unwrap();
    assert!(list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "canvas.stroke"));
    let result=server.message(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"canvas.new","arguments":{"width":32,"height":24}}})).unwrap();
    assert_eq!(result["result"]["isError"], false);
    assert_eq!(server.backend.doc.width, 32);
    let error=server.message(json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"canvas.new","arguments":{"width":0,"height":24}}})).unwrap();
    assert_eq!(error["result"]["isError"], true);
    assert_eq!(server.backend.doc.width, 32);
    let image=server.message(json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"canvas.snapshot"}})).unwrap();
    assert_eq!(image["result"]["content"][0]["type"], "image");
    let mut stream = vec![];
    server
        .serve(
            &mut Cursor::new("broken\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n"),
            &mut stream,
        )
        .unwrap();
    assert_eq!(String::from_utf8(stream).unwrap().lines().count(), 2);
}

#[test]
fn settings_actions_validate_and_preserve_shortcuts_in_headless_mode() {
    let dir = folder("settings");
    let path = dir.join("settings.json");
    let mut s = session();
    apply(
        &mut s,
        "settings.update",
        json!({"path":path,"ui":{"panel_padding":18},"keybindings":{"file.save":["Ctrl+F12"]}}),
    );
    let before = std::fs::read(&path).unwrap();
    let state = apply(&mut s, "settings.read", json!({"path":path}));
    assert_eq!(state["result"]["ui"]["panel_padding"].as_f64(), Some(18.));
    assert_eq!(state["result"]["keybindings"]["file.save"][0], "Ctrl+F12");
    assert!(s
        .apply(Action::new(
            "settings.update",
            json!({"path":path,"ui":{"panel_padding":-10}})
        ))
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn complete_settings_documents_roundtrip_and_repair_invalid_files_transactionally() {
    let dir = folder("full-settings");
    let path = dir.join("settings.json");
    std::fs::write(&path, "broken").unwrap();
    let mut s = session();
    apply(
        &mut s,
        "settings.update",
        json!({"path":path,"document":{"version":1,"painting":{"pressure_gamma":2.,"stabilizer":0.6},"plugins":{"disabled":["quick"]},"extension":{"custom":42}}}),
    );
    let read = apply(&mut s, "settings.read", json!({"path":path}));
    assert_eq!(read["result"]["painting"]["pressure_gamma"], 2.);
    assert_eq!(read["result"]["plugins"]["disabled"], json!(["quick"]));
    assert_eq!(read["result"]["extension"]["custom"], 42);
    let before = std::fs::read(&path).unwrap();
    assert!(s
        .apply(Action::new(
            "settings.update",
            json!({"path":path,"document":{"painting":{"pressure_gamma":0}}})
        ))
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let plugins = dir.join("plugins");
    let _ = Registry::load(&plugins).unwrap();
    let quick = plugins.join("quick");
    std::fs::create_dir_all(&quick).unwrap();
    std::fs::write(
        quick.join("manifest.json"),
        include_str!("../../examples/plugins/quick-tools/manifest.json"),
    )
    .unwrap();
    let loaded = Registry::load_with_disabled(&plugins, &["quick".into()]).unwrap();
    assert!(!loaded.actions.contains_key("quick.blue_stroke"));
    assert!(loaded.available.iter().any(|p| p.id == "quick"));
    let enabled = Registry::load(&plugins).unwrap();
    assert!(enabled.actions.contains_key("quick.blue_stroke"));
}
#[cfg(windows)]
#[test]
#[ignore = "requires separately compiled native plugin DLL; scripts/build.ps1 -Test runs it"]
fn native_plugin_dll_lifecycle() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let registry = Registry::load(&root.join("plugins")).unwrap();
    assert!(
        registry.loaded.iter().any(|p| p == "native_paint"),
        "build the native plugin DLL first: {:?}",
        registry.diagnostics
    );
    assert!(registry.panels.contains_key("native_paint.panel"));
    let mut s = Session::new(registry.clone()).unwrap();
    apply(&mut s, "canvas.new", json!({"width":130,"height":90}));
    apply(&mut s, "native.checkerboard", json!({"cell":8}));
    assert_eq!(s.doc.layers[0].pixels[0], 0xff3d8ede);
    assert_eq!(s.doc.layers[0].pixels[8], 0xffecb96b);
    let original = s.doc.layers[0].pixels.clone();
    apply(
        &mut s,
        "native.invert_region",
        json!({"x":60,"y":60,"width":10,"height":12}),
    );
    assert_ne!(
        s.doc.layers[0].pixels[65 * 130 + 65],
        original[65 * 130 + 65]
    );
    assert_eq!(s.doc.layers[0].pixels[0], original[0]);
    apply(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, original);
    apply(&mut s, "history.redo", json!({}));
    assert_ne!(s.doc.layers[0].pixels, original);
    let snapshot = s.doc.layers[0].pixels.clone();
    let revision = s.revision;
    assert!(s
        .apply(Action::new(
            "native.invert_region",
            json!({"x":129,"y":89,"width":10,"height":10})
        ))
        .is_err());
    assert_eq!(s.doc.layers[0].pixels, snapshot);
    assert_eq!(s.revision, revision);
    assert_eq!(
        apply(&mut s, "native.stats", json!({}))["result"]["calls"],
        4
    );
    let mut second = Session::new(registry).unwrap();
    assert_eq!(
        apply(&mut second, "native.stats", json!({}))["result"]["calls"],
        1
    );
    drop(s);
    assert_eq!(
        apply(&mut second, "native.stats", json!({}))["result"]["calls"],
        2
    );
    drop(second);
    let dir = folder("native-failures");
    let plugins = dir.join("plugins");
    let _ = Registry::load(&plugins).unwrap();
    let bad = plugins.join("broken");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(
        bad.join("manifest.json"),
        r#"{"api_version":1,"id":"broken","name":"broken","library":"missing.dll"}"#,
    )
    .unwrap();
    let registry = Registry::load(&plugins).unwrap();
    assert!(!registry.loaded.iter().any(|p| p == "broken"));
    assert!(!registry.diagnostics.is_empty());
    let disabled = Registry::load_with_disabled(&plugins, &["broken".into()]).unwrap();
    assert!(
        disabled.diagnostics.is_empty(),
        "disabled DLLs must never load"
    );
    let fixture = std::env::var_os("MICOPI_TEST_BAD_DLL")
        .expect("set MICOPI_TEST_BAD_DLL to the incompatible ABI fixture DLL");
    std::fs::copy(fixture, bad.join("missing.dll")).unwrap();
    let registry = Registry::load(&plugins).unwrap();
    assert!(
        registry
            .diagnostics
            .iter()
            .any(|e| e.error.contains("Incompatible plugin ABI")),
        "{:?}",
        registry.diagnostics
    );
    assert!(
        registry.actions.contains_key("canvas.new"),
        "one bad DLL must not disable core actions"
    );
}
