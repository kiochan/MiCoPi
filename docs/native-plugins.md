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

## Document transaction capability (host 0.21+)

Native action descriptors may set `"document_atomic":true`. The host then snapshots document/history metadata using shared tiles, stages nested host actions, and rolls back document changes on failure. Unmarked mutating native actions cannot run inside an atomic pipeline. This contract excludes plugin-private state and external side effects; it is not a DLL sandbox. C ABI tables remain unchanged. See [Atomic Operations](atomic-operations.md).

## Optional stroke options (unreleased)

`micopi_tool_options_v1` exports a `ToolOptionsApi`/`MicopiToolOptionsApi` table with version 1, its struct size and a configure callback. The host validates its header and calls configure once before painting DOWN, using the state created by the v1 tool API. Configure must not invoke host functions and must reject mid-contact changes. `ToolOptions` contains version/size, `spacing` (0.01–2, diameter fraction, default 0.11) and `minimum_opacity` (0–1, default 0). Copy options into the contact; never retain the borrowed pointer. The Rust SDK and C header define the exact layouts.

Existing tool/action tables remain unchanged. A missing extension accepts defaults; unsupported custom settings fail before exposing pixels. A new DLL with an older host uses defaults. Configure rejection follows ordinary tool failure rollback. There is no extra options call on MOVE. See [Painting tools](painting-tools.md) for persistence and legacy recording compatibility.

### Alpha-lock capability tail

An extended tool options table uses `ToolOptionsApiExt` / `MicopiToolOptionsApiExt`: the unchanged base table is followed by `features`. Inspect this tail only when the base `struct_size` covers the complete extended table. `TOOL_OPTION_ALPHA_LOCK` / `MICOPI_TOOL_OPTION_ALPHA_LOCK` uses mask `0x1`. For a supporting tool, the host supplies `ToolOptionsExt` with an unchanged options prefix and `flags` tail, using the extended size in the prefix. Freeze this flag into the contact at DOWN. Unknown flag bits and truncated tails are rejected. Legacy prefix-only calls reset alpha lock to false; existing v1 table/parameter layouts remain intact.

The host rejects locked painting if this capability is absent. Locked brushes blend source-atop and preserve original pixel alpha, including unchanged storage for alpha-zero pixels. Eraser contacts are rejected by the host. Tile snapshots validate locked typed writes; a violation rolls back the stroke. Fully transparent locked regions can return success without invoking a pixel transform or capturing undo tiles. `edit_region` validates coverage against its original staged region before commit. Alpha-preserving filters may execute; plugins cannot silently replace the locked coverage.

### Selection capability and kernels

`TOOL_OPTION_SELECTION` / `MICOPI_TOOL_OPTION_SELECTION` uses bit `0x2`. Supporting tools receive `ToolOptionsSelection`, which appends `Coverage` to the existing `ToolOptionsExt` prefix. Only inspect it when `struct_size` covers the whole tail and the flag is present. The host retains the immutable full-canvas byte buffer across the contact; copy the descriptor, not the bytes. A prefix-only configure or absent flag resets selection to unrestricted. Reject truncated tails, invalid strides/dimensions, null active masks and sample/canvas dimension mismatches before painting. Configure still runs only before DOWN. Older DLLs without the capability reject selected contacts.

The independent painting DLL also exports optional `micopi_selection_v1` with version/size, synchronous `mix` and `rasterize` callbacks. The host caches the validated table at load. `mix` receives immutable original pixels, a separate mutable output region and full-canvas coverage, honoring each region's stride/origin. `rasterize` receives a bounded typed `SelectionShape`, prior coverage and a separate contiguous output buffer; kinds/modes are defined in the SDK and C header. No input pointers or output pointers may outlive the synchronous call. Shape rasterization uses reusable row intersections for polygons rather than testing every polygon edge at every pixel. The host owns validation, compression, undo and commit. Neither extension changes existing v1 tool/action/compositor tables or executes during outcome replay.

### Native action selection input

The host may supply `HostApiSelection` / `MicopiHostApiSelection`, appending a `selection(context, Coverage*)` callback to the unchanged `HostApi` prefix. Check `struct_size` before reading the tail. On success, an all-zero descriptor means unrestricted editing; otherwise the descriptor borrows immutable action-entry coverage until the outer invocation returns. The host retains the allocation even if nested dispatch replaces the document or selection. The snapshot does not change after nested mutations; it is an input snapshot, not a live query. The callback is read-only, does not finish strokes or change revision, and rejects reentry inside a pixel callback.

The default native fill uses this snapshot to constrain source traversal and edge processing, then lets the host blend fractional coverage once at commit. It validates dimensions, stride and storage before pixel access. With an older prefix-only host, unrestricted fill retains its old path; a reported active selection without typed input is rejected. Old DLLs may ignore the tail and receive only host output clipping, so rebuild/stage matching DLLs to obtain selection-aware connectivity. The default unselected fill fast path is unchanged. Replay restores recorded outcomes and does not query this callback.

### Optional color-selection tail

`SelectionApiColor` / `MicopiSelectionApiColor` appends a synchronous `color` callback to the unchanged `SelectionApi` prefix. Read it only when `struct_size` covers the entire extension; a prefix-only or partially advertised tail retains mixing/geometry support but has no color selection. The host rejects unsupported color commands without changing the document. New optional extensions must preserve this fixed-size layout.

`SelectionColor` defines kinds 0 (seeded Wand), 1 (Grow) and 2 (Similar). Modes 0–3 match shape combinations; Grow/Similar require mode 1 and positive prior coverage. The immutable source descriptor covers the full document with origin zero, dimensions/stride bounded to 4096 and validated storage. Prior coverage may be absent for Wand, but must match source dimensions when present. Output is a separate contiguous byte buffer of exactly width × height. No source/mask/output pointer may escape the call or alias mutable output. The SDK retains its existing `Pixels` layout; its data is read-only for this callback. Tolerance is 0–255; `contiguous` is 0/1. See [Selections](selections.md) for exact RGBA, palette and fractional-coverage semantics.

### Optional selection-edge tail

`SelectionApiEdges` / `MicopiSelectionApiEdges` appends `edges` after the unchanged `SelectionApiColor` prefix. Inspect it only when the advertised size covers the complete extension. Prefix-only, color-only and partially advertised edge tables retain their earlier capabilities but have no edge kernel. The host caches callbacks and rejects unsupported edge actions explicitly. Neither earlier prefix is enlarged or reordered.

The synchronous callback receives fixed-layout `SelectionEdges` (`kind` 0 expand, 1 contract, 2 feather, 3 smooth; integer `radius` 1–256; `edge` 0 extend, 1 zero), immutable full-canvas coverage and a separate contiguous output buffer of exactly width × height bytes. Dimensions/strides/storage follow the bounded Coverage contract. Null/inactive masks, malformed buffers and unsupported options fail before writes. Source/output storage must not alias, and pointers cannot outlive the call. No document pixels, JSON, host actions or external effects are required by this kernel. The host owns output validation/compression, undo, publication and verified replay; see [Selections](selections.md#selection-edges) for exact sampling rules.
