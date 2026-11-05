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

### Optional pixel-transfer tail

`SelectionApiTransfer` / `MicopiSelectionApiTransfer` appends a synchronous `transfer` callback after the unchanged fixed `SelectionApiEdges` layout. Read only a fully advertised tail. Kind 0 extracts an in-bounds rectangle, multiplying straight alpha by optional coverage once; kind 1 places a rectangle with signed coordinates and clipping, requires absent coverage and leaves pixels outside the overlap unchanged. Source and destination have local origin zero, sides/stride at most 4096, validated storage, and disjoint allocations. Source and coverage are immutable; no borrowed pointer survives the call. The kernel allocates no memory. The host owns payload hashing, capacity checks, layer insertion, history and event publication. See [Session clipboard](clipboard.md).

## Optional exact geometry table

`micopi_geometry_v1` returns independent `GeometryApi` / `MicopiGeometryApi` with ABI version 1 and advertised byte size. The host reads the fixed header first and requires the complete table before inspecting callbacks. An absent export or callback rejects the requested operation; older unrelated capabilities remain usable. Existing v1 tool/action/compositor/selection table layouts are unchanged.

`Geometry` has `kind` 0 clockwise quarter turn, 1 counterclockwise quarter turn, 2 half turn, 3 horizontal flip or 4 vertical flip, then signed destination offsets `x`/`y` (each within ±4096). Both callbacks are synchronous, with disjoint immutable source and mutable destination storage; no pointer survives a call. Pixel descriptors have local origin zero. Coverage output is dense with exactly width × height bytes. Sides and strides are 1–4096, stride is at least width and storage is bounded to 4096² elements. Destination size may differ from source; transformed coordinates outside it are clipped, uncovered pixels become zero and stride padding stays untouched. The kernel allocates no buffers and leaves source unchanged. The host owns validation, buffer budgets, mask compression, locks/selection clipping, history and publication. See [Canvas geometry](canvas-editing.md#exact-rotation-and-flips).

## Optional image resampling table

`micopi_resample_v1` returns independent `ResampleApi` / `MicopiResampleApi` (ABI 1, complete advertised size). Read the fixed header before callbacks; absent exports/callbacks and truncated/incompatible tables reject image sizing while retaining unrelated legacy capabilities. No prior table prefix changes.

`Resample.method` is 0 nearest pixel center or 1 bilinear. Pixel and coverage callbacks borrow disjoint source/destination storage synchronously, using the exact geometry dimensions/stride/storage limits and local pixel origins zero. Destination dimensions determine the scale; masks have dense output of exactly width × height bytes. Padding remains untouched. Same-sized images copy exact words even with bilinear selected. The nearest/bilinear kernels own only bounded axis plans: nearest at most 32 KiB on x64, bilinear at most 192 KiB for both axes; no full-image scratch or retained pointers. The host owns output buffers, mask encoding, budgets, structural history and outcome publication. Neither callback executes on pen MOVE or verified replay. See [Image size](canvas-editing.md#image-size-and-resampling) for sampling/alpha/rounding rules.

### Optional area capability

`ResampleApiExt` / `MicopiResampleApiExt` appends `features` after the unchanged `ResampleApi` prefix. Read this tail only when the advertised size covers the complete extended layout, including padding. Bit `RESAMPLE_AREA` / `MICOPI_RESAMPLE_AREA` (`0x4`) permits method 2. Prefix-only and truncated-tail tables retain methods 0/1; the host rejects area before invoking their callbacks. The painting DLL advertises the complete tail. Unknown feature bits do not enable area. Callback signatures, descriptor limits and pointer lifetimes stay unchanged.

Method 2 integrates exact source cell overlap on shrinking axes, using bilinear pixel centers on equal/enlarging axes. Integer unnormalized horizontal sums feed vertical sums without intermediate quantization. All products fit `u64` under the 4096-side bound. It caches two source rows by parity and reuses one destination accumulator, plus a width-only axis plan. On x64 peak native scratch is 480 KiB for pixels, 192 KiB for coverage; source/output buffers belong to the host. Same-sized public requests retain exact raw words. No new dependency, retained pointer or drawing-path work is introduced. See [image sizing](canvas-editing.md#image-size-and-resampling) for alpha, color and edge rules.

## Optional trim analysis table

`micopi_trim_v1` returns independent `TrimApi` / `MicopiTrimApi` (ABI 1) with one synchronous `bounds` callback. The host reads its fixed header and requires the complete advertised table before callbacks; an absent export/callback rejects trim without disabling other plugin capabilities. Existing tool/action/selection/geometry/resampling tables are unchanged.

`Trim` has `mode` (0 alpha threshold, 1 top-left RGBA, 2 bottom-right RGBA), `tolerance` (0–255) and `sides` (top `0x1`, right `0x2`, bottom `0x4`, left `0x8`; a nonzero subset). The source is immutable scoped ARGB storage with origin zero and the geometry dimensions/stride/length limits. The output `TrimBounds` must be aligned and disjoint from source pixels. No pointer survives the call; the scanner allocates nothing and never writes source/padding. Return all-zero bounds when every source pixel matches; otherwise return a nonempty in-bounds rectangle after applying the side flags. Unsupported options, invalid descriptors or aliases fail before output writes.

The host samples the existing merged composite, validates empty/nonempty rectangle invariants with wide arithmetic and maps all-matching results to a no-op full frame. It owns selection/frame preparation, history and outcome publication. Native code does not execute commands, mutate document ownership or run on pen MOVE. See [automatic trim](canvas-editing.md#automatic-trim) for color/alpha matching and read-only query behavior.

### Optional integer movement table

`micopi_move_v1` returns independent `MoveApi` / `MicopiMoveApi` (ABI 1, complete size-checked header/table) with synchronous `pixels` and `mask` callbacks. Missing exports/callbacks reject movement without disabling other capabilities; existing table prefixes remain unchanged. `Move` has `kind` 0 whole layer, 1 selected pixels/current mask, or 2 mask only, and signed integer `dx`, `dy` within ±4096. Pixels rejects kind 2; masks rejects kind 0.

Source/destination dimensions match, sides/strides are 1–4096, storage is bounded to 4096² elements, stride is at least width and pixel origins are zero. Input descriptors remain immutable; output storage is separate and disjoint from all sources. Coverage is absent for whole-layer pixels and required at matching dimensions for selected pixels. Mask output is dense, exactly width × height bytes. No pointer escapes the call. Validate all descriptors before writes; preserve stride padding, zero uncovered destinations and clip outside the frame. Whole-layer rows use clipped slice copies; selected pixels use an inverse gather from the original source/mask. The kernel allocates no memory. The host owns staged output buffers, mask encoding, validation, alpha-lock guards, undo and outcome publication. See [Move and nudge](canvas-editing.md#move-and-nudge) for coverage/overlap semantics and bounds.

### Optional affine sampling table

`micopi_affine_v1` returns independent `AffineApi` / `MicopiAffineApi` (ABI 1). Read its header first and require the complete advertised fixed table before accessing `pixels`/`mask`/`bounds`; absent exports/callbacks reject the requested transform while older capabilities remain available. Invalid table headers reject plugin loading. Existing v1 prefixes and SDK layouts remain unchanged.

`Affine` / `MicopiAffine` contains `kind` (0 layer, 1 selected pixels/current mask, 2 mask only), `method` (0 nearest, 1 bilinear) and six finite forward coefficients: `x'=m0*x+m1*y+m2`, `y'=m3*x+m4*y+m5`. Linear coefficients are bounded by ±256, translations by ±4194304, and absolute determinant must be at least `1e-8`. Pixel callback rejects kind 2; mask callback rejects kind 0. Host component limits are stricter; public ordered local X/Y shears use the same matrix table. Perspective requires a separate future contract.

Source/destination frames match; sides/strides are 1–4096, stride is at least width, origins are zero and storage is bounded to 4096² elements. Pixel and coverage sources are immutable and disjoint from mutable outputs. Coverage is absent for whole-layer pixels and required at matching dimensions for selected pixels. Dense mask output has exactly width × height bytes. Descriptors validate before writes, stride padding remains untouched and no pointer outlives the call. Inverse gathering avoids source/destination overlap feedback. Kernels allocate no buffers or execute host commands. See [Free Transform](canvas-editing.md#free-transform) for exact sampling, alpha, clipping, preview and publication semantics.

The affine `bounds` callback receives immutable full-frame raw pixels and writes `TrimBounds` for all pixels with alpha > 0, ignoring hidden RGB. All-zero bounds means an empty layer. The host validates native rectangles with wide arithmetic; it uses content bounds for layer handles/pivots and rejects empty layer transforms. Pixel sampling still covers the full frame. Bounds reuse the native alpha scanner and do not require a separate trim extension to be exported. Selected modes use the existing positive-coverage container bounds. `canvas.transform.plan` exposes geometry without a pixel sampler or document mutation.
