# Native DLL Plugins and ABI v1

MiCoPi loads library DLLs declared by plugin manifests and merges their describe JSON contributions at startup. The host does not link plugin crates. Independent plugins may register actions, tools, public state, nodes and button panels.

This is a synchronous in-process Windows x64 ABI. A DLL shares process permissions and can terminate the host through a crash or panic=abort. There is no crash isolation or hot replacement. Restart after updating or enabling/disabling a loaded DLL.

## SDK and builds

crates/plugin-api contains dependency-free repr(C) types, function tables and include/micopi_plugin.h. Independent examples are painting, native-paint, ai-assistant and compositing. The SDK source package builds without the host or GUI crates.

```powershell
cargo build --release --locked -p micopi-native-paint
cargo build --release --locked -p micopi-painting-tools
.\scripts\build.ps1 -Portable -PluginsOnly
```

Copy each DLL and manifest.json to its installed package directory beside MiCoPi. Native-paint demonstrates checkerboard generation, whole-layer/region inversion, instance statistics and a node alias. PluginsOnly updates the source plugin DLLs without rebuilding the host. The full build stages binaries and licenses; existing installed manifests are preserved.

## Action ABI

Export micopi_plugin_v1 returning a static PluginApi with ABI version, table size, describe, invoke and optional paired create/destroy. The descriptor ID must match the manifest. create/destroy must be paired; describe must return bounded valid JSON. Required exports and function pointers are validated before registration.

Use C-compatible pointers, lengths and integer status codes only. Do not pass Rust-owned String, Vec, references, traits or egui types across the boundary. Host pointers are borrowed only for the synchronous call. Never retain, free or send them to another thread. JSON arguments/results are limited to 8 MiB; descriptors to 1 MiB.

Reply.write copies output immediately. Each side owns and frees its own allocation. Return nonzero status and structured JSON for failure; unwinding must not cross the C ABI. catch_unwind cannot recover a panic in a panic=abort build.

Registry and instances share the module handle; unload occurs only after the last holder releases it. Each Session creates an independent plugin instance lazily, reuses it and destroys it on the Session execution thread.

## Host actions and pixel regions

HostApi.dispatch calls the same Session actions, discovery, public state and pipeline. Cross-DLL calls are supported, but same-instance reentry, cycles and exhausted budgets are rejected. Host calls inside pixel callbacks are forbidden. read_only actions cannot dispatch mutations or edit_region and do not increment document revision.

HostApi.edit_region checks coordinates, limits and layer visibility, copies the region into a temporary contiguous ARGB buffer, and runs the plugin callback. Success commits changed tiles and records undo; failure commits no part of that region. Metadata is read-only. One successful region submission forms one undo step.

An entire native action is not transactional: earlier submitted regions or child actions survive a later failure. Preserve and report partial completion instead of treating an error as proof that nothing happened.

## Typed point tools: SDK 1.1

A tool DLL exports micopi_tools_v1 in addition to micopi_plugin_v1. ToolApi includes version, size and create/destroy/pointer functions. Tool-only plugins may omit action invoke. Manifests bind positive numeric native handlers; default handlers are brush=1, eraser=2, picker=3, hand=4 and zoom=5. Presets without a handler reuse their kind's default tool.

pointer receives handler, ToolSample, ToolParameters, ToolHost and ToolOutput. Phases are DOWN=0, MOVE=1, UP=2, CANCEL=3. x/y are document coordinates; view_x/y are viewport coordinates; pressure is 0..1; time is wrapping u32 milliseconds. PEN/ERASER flags identify input. Parameters are supplied on DOWN and frozen until UP/CANCEL.

ToolOutput reports DRAWING/PANNING state, optional COLOR_CHANGED RGB and VIEW_CHANGED zoom/pan. UP/CANCEL must clear contact flags. The host validates output before applying it.

ToolHost.pixels snapshots touched 64×64 undo tiles and then lends actual layer storage. There is no temporary region copy. Pixels.stride may exceed width; use data[row*stride+col] and write only width×height. len reaches the final row's end; row gaps are not writable. Never retain/free pointers, mutate metadata, cross threads or reenter the host in callbacks. ToolHost.pick returns composited color and keeps the caller's color over transparent pixels.

Ordinary MOVE uses cached contact/instance state, with no JSON serialization, registry lookup or region copy. First tile contact still allocates an undo snapshot. Nonzero return, failed callback even if ignored by the DLL, or invalid output rolls back the current whole stroke, preserves earlier history and destroys the bad tool instance. A native crash is not recoverable. Missing painting DLLs do not trigger host rasterization.

## Typed compositor: SDK 1.2

The optional micopi_compositor_v1 table adds region rendering without changing action/tool ABI v1. The host validates JSON topology and supplies immutable layers, typed instructions and a separate dirty-region output buffer. At most 64 instructions and 16 layers are supported. The plugin performs all filter/blend pixel math. See [Compositor](../plugins/compositing/README.md) and [Image Editor](node-editor.md).

## Contributions and automation

Manifest/describe states register read-only schema-checked resources. Nodes register aliases and typed image definitions; see [Workflow Nodes](workflow-nodes.md). New metadata fields require the corresponding host version: public state 0.16+, compositor 0.17+, nodes 0.18+. SDK tables remain versioned separately.

All registered native actions appear in MCP and headless. GUI attachment dispatches on the GUI thread. plugins.disabled skips opening disabled DLLs and calling describe. The plugin settings page reports library, loading state and diagnostics. English titles are canonical; display translations never change action/state IDs or handler names.

Build and validation:

```powershell
.\scripts\build.ps1 -Portable -Test
.\scripts\build.ps1 -Portable -Headless -Test -BuildRoot target/headless
python scripts/smoke-runtime.py
python scripts/smoke-workflows.py
```

The explicit DLL lifecycle fixture checks actual loading, ABI rejection, instance isolation, tile edits and undo. Painting tests compare full pixels/history with the previous implementation, including pressure and stabilization. Optional release benchmarks are local measurements, not CI timing thresholds. End-to-end tests compare PNG bytes across JSONL, MCP and GUI.
