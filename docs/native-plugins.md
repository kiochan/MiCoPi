# Native DLL Plugins and ABI v1

## Levels preparation prerequisite

The bundled compositor exports optional `micopi_levels_v1`, with a 16-byte
`LevelsApi` prefix and a complete 24-byte `LevelsApiRender` tail. ABI version 1
provides synchronous `prepare` and optional `render` callbacks. This is a
native prerequisite; the host does not yet register a Levels document effect.
The loader caches the optional callback and rejects malformed advertised tables.
Native inventory reports `filters.levels_prepare` and `filters.levels_render`
separately. The host adapter validates endpoints/gamma and keeps unsuccessful
callback output private. Discovery performs no per-contact or per-MOVE lookup.
Existing compositor tables and legacy instruction IDs remain unchanged.

`LevelsParameters` is 32 bytes: version/size at offsets 0/4, unsigned input black,
input white, output black and output white at 8/12/16/20, and double gamma at 24.
Endpoints must be bytes, input white must exceed black, and finite gamma is
0.1–10. Preparation normalizes/clamps input, raises it to `1/gamma`, maps the
output endpoints and rounds half-up. Reversed output endpoints are supported.
The caller supplies at least 256 writable bytes. Success writes exactly the
first 256 bytes; invalid arguments or overlapping parameter/output spans reject
before writes. No buffers or pointers are retained. Prepared tables require no
per-pixel power calculation.

The optional render callback at offset 16 receives the usual nodes, layers and
raw/effective mask descriptors plus a borrowed LUT array/count before frame
dimensions and output. Each `CompositeLut` is exactly 256 bytes; at most 64 tables
(16 KiB) may be supplied. Kind 9 reads prior image slot `a`, uses LUT index
`mode`, and requires `b=0, amount=1`. It preserves alpha while mapping encoded
RGB channels. All instructions, including unused ones, are checked before writes;
output must not overlap LUT storage, and output row padding remains intact.
Legacy render callbacks reject kind 9. Preparation-only 16-byte tables remain
supported; partial rendering tails and missing advertised callbacks reject.
Document plan caching and public nodes/adjustments remain planned; ABI and native
runtime checks await CI.

The shared engine `Renderer` contract exposes `supports_levels`, `prepare_levels`
and `render_prepared`. Its legacy defaults reject LUT plans explicitly. The native
adapter checks mask, Mix and blend-mode availability before the synchronous call,
and delegates plans without LUTs to the existing renderer path. Typed engine
Levels intent serializes only the five parameters; derived tables remain separate.

`PreparedLevelsPlan` holds one renderer and at most 64 intent/table pairs. A stable
provider and unchanged ordered intent list reuse table storage without native
calls or allocation. Rebuild validates all intents first, prepares changed slots
in private storage and replaces the cache only on complete success. A different
renderer prepares every slot. Rendering borrows the cached tables. Steady LUT
storage is at most 16 KiB; rebuilding may temporarily retain another 16 KiB,
plus bounded intent/vector metadata. This cache is not yet installed in document
graphs; no per-MOVE preparation or frame-sized buffer is introduced.

## Loading and compatibility

MiCoPi loads library DLLs declared by plugin manifests and merges their describe JSON contributions at startup. The host does not link plugin crates. Independent plugins may register actions, tools, public state, nodes and button panels.

This is a synchronous in-process Windows x64 ABI. A DLL shares process permissions and can terminate the host through a crash or panic=abort. There is no crash isolation or hot replacement. Restart after updating or enabling/disabling a loaded DLL.

## Layer pixel access

Action contributions can declare `pixel_access` as `none`, `pixels` or `geometry`. It describes access to existing layer pixels, independently of `read_only` and `document_atomic`. Read-only actions resolve to `none`; conflicting declarations reject the package. Opaque mutating DLL actions without a declaration conservatively resolve to `geometry`. `app.describe` exposes `pixelAccess`; registry inventory preserves the effective native declaration.

- `none` forbids direct mutable pixel callbacks. An action can still issue permitted host dispatches, each with its own target-aware checks.
- `pixels` permits a raw pixel edit when the current layer is not pixel-locked.
- `geometry` additionally requires the current layer not be position-locked.

The package-level `action_access` map can annotate actions after DLL contributions are loaded, without changing an existing DLL's descriptor or ABI table. IDs must belong to that package; unknown IDs and conflicting declarations fail registration atomically. Built-in package overlays classify fills/filters and pixel-producing orchestration as `pixels`, layer transforms as `geometry`, and read/configuration operations as `none`. Older opaque extensions must explicitly narrow their declaration to use pixel-only effects on position-locked layers. Contribution changes require restart.

```json
{"action_access":{"my_plugin.adjust":"pixels","my_plugin.translate":"geometry","my_plugin.inspect":"none"}}
```

The host checks locks before invoking opaque actions and again at every mutable region loan, including after nested dispatch changes the active target. Denied loans are reported even if a plugin ignores a callback failure. Immutable sampling and selection snapshots remain available. Typed tool contacts cache write permission at Down: Brush/Eraser can write only to an unlocked target; Picker/Hand/Zoom cannot obtain mutable pixels. Callbacks recheck the read-only scope and current lock. These are bounded field checks, without new per-MOVE JSON, registry lookup, instance creation or image copies. Existing struct prefixes, table sizes and callback signatures are unchanged. This is an in-process extension contract, not memory isolation from arbitrary native code.

## SDK and builds

Generic mutable RGBA loans still reject an A8 painting target, and generic opaque
geometry remains guarded on mask-bearing documents. The known built-in
[integer owner Move](raster-owner-movement.md) instead validates an explicit
domain, stages native outputs from immutable sources and commits a checked
RGBA/A8 pair. Extensions can dispatch that registered action; it does not grant
opaque geometry callbacks unrestricted access or add a new ABI loan.

Isolated [layer groups](layer-groups.md) and transparent image nodes use an optional complete `CompositorApiFeatures` tail. The existing `CompositorApi` prefix and render callback stay unchanged; its advertised `struct_size` must cover the appended `u64 capabilities` before the host reads `COMPOSITOR_TRANSPARENT`. Kind 4 is a constant transparent source with zero `a`, `b`, `mode` and `amount`. New hosts retain prefix-only DLLs for original plans and reject unsupported groups/transparent graphs before installation. Extended DLLs retain original four-kind behavior. Input/output spans must be bounded and disjoint; rendering retains one dirty-region call and 64 per-pixel `u32` slots without image intermediates.

crates/plugin-api contains dependency-free repr(C) types, function tables and include/micopi_plugin.h. SDK 1.3.0 retains ABI 1 and adds optional Flow/stroke-origin tails; see the [SDK README](https://github.com/kiochan/MiCoPi/blob/main/crates/plugin-api/README.md). Independent examples are painting, native-paint, ai-assistant and compositing. The SDK source package builds without the host or GUI crates.

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

The optional micopi_compositor_v1 table adds region rendering without changing action/tool ABI v1. The host validates JSON topology and supplies immutable layers, typed instructions and a separate dirty-region output buffer. At most 64 instructions and 16 layers are supported. The plugin performs all filter/blend pixel math. The current kernel implements 26 [color blend modes](blend-modes.md), preserving IDs 0–7 and their existing arithmetic. See [Compositor](../plugins/compositing/README.md) and [Image Editor](node-editor.md).

The existing Features capability field advertises `COMPOSITOR_TRANSPARENT=1` for kind 4 and `COMPOSITOR_MIX=2` for kind 5. Mix reads prior slots `a=before` and `b=after`, requires `mode=0` and finite `amount` in 0–1, and interpolates premultiplied colors at `round(amount*255)`. Endpoint weights preserve exact input words. Interior output alpha rounds the weighted alpha divided by 255; zero rounded alpha yields zero. RGB rounds half-up against the unrounded weighted alpha. The integer kernel uses the existing fixed scratch and is kept out of the legacy renderer's inlined paths. No instruction or callback struct grows. See [Pass Through](pass-through.md) for the complete group and rounding contract.

The optional `CompositorApiModes` / `MicopiCompositorApiModes` extension retains the original callback prefix and transparent-source Features tail. Its complete x64 table is 32 bytes; a `u64` supported-mode bitmap follows at offset 24, with bit `n` declaring native mode `n`. Hosts read this bitmap only when the advertised size covers the complete tail. Prefix-only and 24-byte Features tables retain the mandatory legacy mask `0xff`; a complete bitmap omitting any baseline bit is rejected, while subsets of appended IDs 8–25 are allowed. Unknown bits do not enable additional host modes. Updating the DLL requires restart.

`graph.read.available_blend_modes`, `graph.catalog.available_blend_modes` and public `compositing.graph` expose current availability with the CPU Normal fallback. Unsupported new-mode properties, graphs and staged projects reject before publication, including hidden layers/groups, disabled effects and unused branches. The interface retains unavailable choices as disabled entries. Existing v1 callbacks, per-region execution and fixed per-pixel scratch remain unchanged; outcome replay restores saved surfaces without invoking the compositor.

`graph.read.renderer_mix`, `graph.catalog.renderer_mix` and public `compositing.graph` report Mix support; hierarchy state exposes `pass_through_available`. The bundled DLL advertises both feature bits. A DLL without Mix retains its existing isolated-group behavior when transparent support is present, and rejects Pass Through policies or explicit Mix nodes before publication, including hidden/disabled/disconnected content. Version-1 isolated group metadata remains unchanged. Pass Through opts into tree version 2; outcome replay restores recorded metadata and surfaces without requesting the new instruction.

### Optional native mask composition

`CompositorApiMasks` / `MicopiCompositorApiMasks` appends `render_masked` at offset 32 after the unchanged Modes prefix, giving a complete 40-byte x64 table. `COMPOSITOR_MASKS` / `MICOPI_COMPOSITOR_MASKS` is bit `0x4`. The host reads the callback only when both the capability and complete size are present; a declared capability with a truncated table or null callback rejects plugin loading. Without the bit, the callback is ignored. Older 16/24/32-byte tables retain their prior capabilities, and older hosts keep calling the original render entry.

The new synchronous callback receives the existing instructions/layers followed by a mask descriptor array/count, global width/height and separate output. `CompositeMask` is 24 bytes on x64: immutable byte pointer, element count, `u32 stride` and `u32 constant`. A null pointer is valid only for a uniform constant 0–255 with zero length/stride. A nonnull pointer requires constant zero, stride at least the frame width and storage through the final row. At most 32 descriptors are accepted. Buffers, descriptor arrays, instructions and output descriptor cannot overlap writable output; validate alignment, all spans and every instruction before writes. No pointer is retained.

Masked-renderer kinds 6/7/8 add coverage source (`a` is the descriptor index, other fields zero), ApplyMask (`a` image, `b` coverage, `mode=0`) and MaskedMix (`a` before, `b` after, `mode` coverage slot). Slots carry distinct image/coverage types within the same 64-word scratch budget; references must precede the instruction, and the final slot must be an image. Types validate even on disconnected instructions. ApplyMask and MaskedMix require finite `amount` in 0–1. Set `o=round_half_up(amount*255)` and `t=round_half_up(o*coverage/255)` once. ApplyMask uses `floor(alpha*t/255)`, retains RGB when that alpha is nonzero and otherwise returns zero. MaskedMix uses the existing exact premultiplied Mix at `t`, including its endpoint identities. The original callback rejects kinds 6–8. An unmasked call through the new host adapter retains the legacy callback path.

These primitives are used by the implemented [document/history/project foundation](raster-mask-storage.md), [public editing subset](raster-mask-editing.md), [document-wide geometry](mask-canvas-geometry.md), [single-layer owner copy/delete](mask-owner-editing.md), [owner-preserving group creation/movement](mask-group-editing.md), [mask-aware subtree deletion](mask-group-delete.md) and [subtree duplication](mask-group-duplicate.md). Generated coverage gates are not independently creatable editable source-filter nodes. Registered mask actions/state become workflow nodes through their schemas. [Owner affine transforms](owner-affine.md) preserve raw ownership through shared staging; group-mask geometry and ungroup/batch/merge/selected-pixel transfer integration remain planned in [Raster Masks Design](raster-masks-design.md).

### Optional native mask effects

`CompositorApiMaskEffects` / `MicopiCompositorApiMaskEffects` retains the complete
40-byte Masks prefix and appends four callbacks on x64: `create_mask_effects` at
offset 40, `destroy_mask_effects` at 48, `mask_effect_region` at 56 and
`reserve_mask_effects` at 64, making the table 72 bytes. Capability
`COMPOSITOR_MASK_EFFECTS` / `MICOPI_COMPOSITOR_MASK_EFFECTS` is `0x8` and requires
the Masks bit `0x4`. The host reads these callbacks only when both the capability
and complete tail are advertised. Null callbacks, a truncated table (including
an incomplete 64-byte experimental effects tail), or effects without Masks reject
loading. Without the effects bit the tail is ignored; the 40-byte masked-render
prefix and older prefixes retain their existing contracts.

Create/destroy own an opaque effects instance. The host lazily caches one per
loaded compositor behind its lifecycle guard and destroys it before unloading
the DLL. It does not create instances per MOVE. Callbacks are synchronous; no
host image or output pointer is retained. The instance owns two reusable byte
buffers, at most 32 MiB of logical scratch, plus a `u32` column workspace of at
most 16 KiB. Capacities do not shrink between regions.

`MaskEffectRequest` is 16 bytes: `u32 flags`, `density`, `feather_radius` and
`reserved`. Normal flags are zero, density is 0–255, radius is 0–256, and reserved
is zero. The region callback receives that request, an immutable full-frame
`CompositeMask`, the frame width/height, and a separate mutable `A8Pixels` output
ROI with canvas-relative origin. Frame sides are 1–4096. The uniform descriptor
retains its null-pointer/zero-length/zero-stride/byte-constant rules; dense input
has constant zero and validated full-frame storage. Output stride padding is
preserved. Descriptor metadata, all source/output spans and persistent instance
scratch must be disjoint from writable output. Invalid flags, bounds, sizes,
alignment or aliasing reject before output writes.

Feather uses split radii `a=floor((R+2)/3)`, `b=floor((R+1)/3)`, `c=floor(R/3)` in
the order `H(a),V(a),H(b),V(b),H(c),V(c)`. Each pass clamps to the canvas edge and
rounds the byte mean as `floor((sum+r)/(2*r+1))`; zero-radius passes are identities.
Total support is exactly `R` on each axis. Density runs after all passes:
`255-floor(((255-feathered)*density+127)/255)`. All-white input stays white at
edges. Region evaluation copies only its demanded ROI plus the necessary halo
into reused scratch and writes only the ROI. Uniform input, density zero and
radius-zero paths do not need the feather scratch.

`MASK_EFFECT_INVERT_RAW` / `MICOPI_MASK_EFFECT_INVERT_RAW` is flag `1`, admitted
only with radius zero and density 255. It writes `255-raw` and is a separate cold
raw edit; it does not invert the derived feather/density output. Unknown flag
combinations reject. Public `mask.invert` uses this native operation and preserves
effect metadata.

The reserve callback receives instance, frame width/height and radius. It reserves
full-frame byte and column capacities without changing lengths, zero-filling or
running a pixel kernel. Uniform-mask contact preparation uses it before raw and
constant effective storage materialize once. Later dirty regions fit that warmed
capacity; dense cold rebuilds may still perform full-frame work. The host owns
effective-cache invalidation and expands raw damage by exactly `R`. Raw and
effective host A8 planes each have separate 64 MiB logical budgets; neither
effective caches nor native scratch enter history or project/replay payloads.

Missing effects support rejects nondefault density/feather before live mutation
or project publication, including hidden/disabled branches. Verified outcome
restoration remains DLL-independent. Independent kernel/reference, stride/ROI,
border, malformed descriptor, alias/prewrite, reservation and old-tail tests live
with the compositor and host bridge; their synthetic scope does not establish
tablet compatibility.

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

### Optional A8 painting target

`TOOL_OPTION_A8_TARGET` / `MICOPI_TOOL_OPTION_A8_TARGET` is bit `0x4` in the existing options feature/flag tails. Supporting tools freeze this explicit target at DOWN. It cannot combine with alpha lock; selection clipping remains independent. `ToolHostA8` / `MicopiToolHostA8` appends an optional `a8` region-loan callback to the unchanged 32-byte tool-host prefix, yielding a complete 40-byte x64 table. Read it only for an explicitly configured A8 target after checking the complete size and nonnull callback. Missing, truncated or lost A8 hosts reject the contact rather than calling the RGBA loan.

`A8Pixels` mirrors the 40-byte `Pixels` descriptor with a mutable byte pointer; lengths and strides count bytes. Its transform is synchronous and scoped to one region. The host captures byte undo tiles before lending storage, preserves identity/permissions for the contact and rolls back the complete stroke on failure. It separately rejects an RGBA loan on an A8 target and an A8 loan on an RGBA target; action `HostApi.edit_region` keeps its RGBA contract. The typed Session Brush/Eraser route issues A8 contacts using a cached mask lease and tool instance, with capabilities/options prepared before raw materialization at DOWN. Read-only, reentrant and stale loans reject without creating a second editing path.

The painting DLL reuses pressure, spacing, interpolation and stabilization. Only its dab sink differs. Brush freezes `(54*R+183*G+19*B+128)/256` using integer division on encoded RGB; Eraser and pen eraser reveal toward 255. With effective dab byte `a`, blend `round_half_up((old*(255-a)+target*a)/255)`. In Dab mode, frozen fractional selection coverage multiplies each dab strength once; Stroke mode instead applies it once to cumulative influence. Validate region/origin/stride/storage, reject misaligned metadata before dereferencing, and reject writable storage overlapping the descriptor, dab parameters or immutable selection before forming a mutable slice. Preserve row padding. Final UP/CANCEL stabilization failures return an error after clearing the contact; the host must roll back all its captured tiles. No JSON, mask compression, whole-plane copy or extra options call occurs per MOVE.

### Stroke opacity and Flow: SDK 1.3

`TOOL_OPTION_STROKE_OPACITY` / `MICOPI_TOOL_OPTION_STROKE_OPACITY` is feature
`0x8`. It negotiates both Flow and capped stroke opacity. As a configured flag it
selects Stroke mode; nondefault Flow in Dab mode still requires the complete
tail, with this flag clear. `ToolOptionsStroke` / `MicopiToolOptionsStroke`
retains the `ToolOptionsSelection` prefix, appending `float flow` at offset 56
and a zero `u32 reserved` at 60, for 64 bytes on x64. Flow must be finite and in
0–1. Prefix-only options reset Flow to 1 and mode to Dab. Unknown flags,
incomplete requested tails, nonzero reserved values and changes during a contact
reject. `ToolParameters` and all previous v1 prefixes remain unchanged.

For a capped contact, `ToolHostStroke` / `MicopiToolHostStroke` retains the
40-byte `ToolHostA8` prefix and appends `features` at 40, `tile_side` at 44,
`pixels_origin` at 48 and `a8_origin` at 56, giving 64 bytes. Require the complete
size, tile side 64, the matching `TOOL_HOST_ORIGIN_RGBA` (`1`) or
`TOOL_HOST_ORIGIN_A8` (`2`) bit, and a nonnull callback. Only the admitted target
is advertised. Default contacts retain the previous 32-byte RGBA / 40-byte A8
host shapes; this feature does not expand generic action pixel access.

An origin loan receives a region and a synchronous transform with an immutable
`OriginPixels` or `OriginA8Pixels` descriptor beside mutable current storage.
The host prepares its existing Undo originals and splits callbacks at 64×64 tile
intersections. Coordinates/extents match; original stride is the edge-aware Undo
tile stride while current stride can span the canvas. Validate both descriptors
and complete spans, preserve padding and reject aliases before writes. These
views allocate/copy nothing; first-touch Undo capture still has its existing
cost. Neither pointer may survive the callback. Busy/reentry, read-only,
pixel/alpha-lock and frozen A8-lease checks remain enforced; any callback failure
rolls back the complete contact, including across recording chunk boundaries.

The bundled DLL accumulates bounded `u16` influence in sparse tiles and
recomposes live against those originals. It never derives originals by inverting
rounded destination pixels. At most 4096 active coverage tiles contain 32 MiB of
samples; after UP/Cancel at most 64 tiles (512 KiB) are retained per cached native
state, plus bounded metadata. Flow 1 / Dab retains the previous exact raster
path without allocating or consulting stroke coverage. Registry native inventory and `painting.tool`
capabilities expose `flow` / `stroke_opacity` availability. See [Brush opacity and
Flow](stroke-opacity.md) for the numeric recurrence, pressure/selection behavior,
persistence and pending-CI verification scope.

Run `python scripts/check-native-abi.py` with the documented portable compiler, or pass `--cc <existing-x64-clang>`, to compile the C11 and C++17 layout/callback fixtures without creating binary output. Rust SDK tests independently retain old prefix offsets and assert the new layouts.

### Analytic tip geometry

`TOOL_OPTION_TIP_GEOMETRY` / `MICOPI_TOOL_OPTION_TIP_GEOMETRY` uses bit `0x10`
in the existing options feature/flag contract. `ToolOptionsTip` /
`MicopiToolOptionsTip` retains the complete 64-byte `ToolOptionsStroke` prefix,
then appends `u32 tip_shape` at 64, `float roundness` at 68, `float angle` at 72
and a zero reserved word at 76: 80 bytes on x64. Round is 0, Square is 1;
roundness is finite 0.25–1 and angle is finite -180–180 clockwise document
degrees. Read the whole tail only when size and capability permit it; reject
unknown shape IDs/flags, truncated requested tails, reserved data and changes
during a contact. Prefix-only options reset Round/1/0 geometry.

Any nondefault field requires this capability, even a nonzero angle on a circle.
The host rejects unsupported intent before DOWN exposes mutable RGBA or raw A8
storage. Default circles retain old arithmetic and prefix compatibility. The
bundled DLL caches trigonometry once per contact, uses bounded rotated regions
and row culling, and keeps UI contours outside the ABI. Public provider/tool
discovery adds `tip_geometry`; it does not infer hardware tilt support. See
[Analytic Brush Tips](brush-tips.md) for coverage approximations, persistence,
presets and pending-CI measurement scope.

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

## Optional raw A8 sampler

An independent affine primitive is described in [Native raw-A8 affine](native-a8-affine.md).
Its `micopi_a8_affine_v1` export accepts a forward matrix, nearest/bilinear
sampling and explicit exterior fill over borrowed dense/uniform bytes. The host
loads/caches this separate ABI-1 table without changing earlier samplers. The
complete 16-byte sample prefix remains supported; a size-gated 24-byte extension
can additionally expose neutral-relative raw bounds. [Owner/linked Free
Transform](owner-affine.md) requires the relevant cached sample/bounds callbacks
and separately supplies document admission, exact scope, history and replay.

`micopi_a8_sampler_v1` returns an independent `A8SamplerApi` /
`MicopiA8SamplerApi`. On x64 its 32-byte layout has the version/size header,
then mandatory `reframe`, `geometry` and `resample` callbacks at offsets 8, 16
and 24. The host accepts ABI version 1 and a complete table; an absent export
retains existing capabilities, while a malformed advertised table rejects the
native package. Existing tool, selection, geometry and resampling prefixes do
not change. This prerequisite alone does not enable document mask geometry.

All three callbacks synchronously borrow a request, immutable `CompositeMask`,
explicit source width/height and mutable `A8Pixels` output. A uniform source has
null data, zero length/stride and a byte-valued constant. A dense source has
constant zero and validated strided bytes. Both source and target sides are
1–4096; strides are at least width and at most 4096, storage is at most 4096²
bytes, and the output is a complete frame with origin zero. Padding is retained.
Uniform inputs do not allocate or materialize a source plane.

`A8Reframe` is 16 bytes: signed destination placement `x`/`y`, byte-valued
`fill` stored as `u32`, and zero `reserved`. Offsets are within ±4096; crop uses
negative offsets. `A8Geometry` is 16 bytes: the existing 12-byte `Geometry`,
then byte-valued `fill`. Geometry kinds retain the old quarter/half-turn and
flip IDs. Clipped coordinates are discarded and uncovered output receives the
explicit fill. `Resample.method` retains nearest/bilinear/area IDs 0/1/2 and
their existing integer arithmetic. Dense geometry and sampling share the old
native byte kernels; no compressed pixel-selection adapter is required.

Alignment, address spans, layouts, request options and source/output/metadata
aliasing validate before any writable pixel reference. Failure leaves output
untouched. No pointers survive a callback, and no sampler instance or per-point
dispatch is introduced. The host owns target allocation, raw-plane budgets,
identity/ownership, history and verified publication. See
[Raster Masks Design](raster-masks-design.md#structural-and-geometry-rules) for
the separate integration requirements.

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

`Affine` / `MicopiAffine` contains `kind` (0 layer, 1 selected pixels/current mask, 2 mask only), `method` (0 nearest, 1 bilinear) and six finite forward coefficients: `x'=m0*x+m1*y+m2`, `y'=m3*x+m4*y+m5`. Linear coefficients are bounded by ±256, translations by ±4194304, and absolute determinant must be at least `1e-8`. Pixel callback rejects kind 2; mask callback rejects kind 0. Host component limits are stricter; public ordered local X/Y shears use the same matrix table. Perspective uses the independent projective table below.

Source/destination frames match; sides/strides are 1–4096, stride is at least width, origins are zero and storage is bounded to 4096² elements. Pixel and coverage sources are immutable and disjoint from mutable outputs. Coverage is absent for whole-layer pixels and required at matching dimensions for selected pixels. Dense mask output has exactly width × height bytes. Descriptors validate before writes, stride padding remains untouched and no pointer outlives the call. Inverse gathering avoids source/destination overlap feedback. Kernels allocate no buffers or execute host commands. See [Free Transform](canvas-editing.md#free-transform) for exact sampling, alpha, clipping, preview and publication semantics.

The affine `bounds` callback receives immutable full-frame raw pixels and writes `TrimBounds` for all pixels with alpha > 0, ignoring hidden RGB. All-zero bounds means an empty layer. The host validates native rectangles with wide arithmetic; legacy single-layer handles/pivots reject empty raw content. Owner scopes instead union all raster-alpha bounds with raw A8 support unequal to 255, falling back to the complete canvas when that union is empty. Pixel sampling still covers the full frame. Bounds reuse the native alpha scanner and do not require a separate trim extension to be exported. Selected modes use the existing positive-coverage container bounds. Legacy `canvas.transform.plan` can expose geometry without a pixel sampler; owner plans preflight their required sampler/bounds capabilities. Both remain read-only and permit contact inspection.


### Optional projective sampling table

`micopi_projective_v1` returns independent `ProjectiveApi` / `MicopiProjectiveApi` (ABI 1), with `pixels` and `mask` callbacks. Require its complete 24-byte x64 table after reading only the header; missing symbols/callbacks reject the requested sampling, while incompatible/null/truncated table headers reject plugin loading. The affine table remains 32 bytes and its structure/export are unchanged.

`Projective` / `MicopiProjective` is 80 bytes on x64: `kind` and `method` at offsets 0/4, then nine row-major doubles at offset 8. Kinds and methods match affine: layer/selected/outline and nearest/bilinear. The matrix is a complete forward homography including its final denominator coefficient; it is not restricted to an origin denominator of one. All values must be finite. Linear numerator terms are bounded by ±256, translations by ±4194304 and the denominator row by ±16; absolute determinant must be at least `1e-8`. SDK clients should use the host's positive normalization by the maximum absolute denominator coefficient, since face/horizon thresholds apply to supplied values. See [Perspective Corners](canvas-editing.md#perspective-corners) for source-domain validation and formula.

The callback inverts once before rasterization and inverse-gathers from immutable frame/mask descriptors. It shares the established affine descriptor validation, padding, identity preservation, nearest/bilinear premultiplied-alpha sampling, selected extraction/clear/source-over and dense mask loops. Those generic loops specialize the affine coordinate closure; no virtual function call, per-pixel allocation or JSON lookup is added to the old path. Projective coordinate mapping uses one reciprocal per output sample and rejects the back face/horizon before sampling. Descriptor dimensions, origins, length and disjointness rules match affine; pixel callback rejects kind 2 and mask callback rejects kind 0. Output staging, mask encoding, alpha locks, history and replay remain host responsibilities.

The API uses neither egui objects nor plugin instance creation per sample. Old hosts ignore this export; new hosts with old painting DLLs keep affine support and explicitly report missing projective sampling. New perspective metadata requires a reader that understands its optional denominator row; reading new perspective projects with an old application is not promised. Supported older project/recording fixtures retain their validation hashes.
