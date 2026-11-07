# Raster masks: implementation design

This document tracks the staged implementation of first-class, paintable raster masks. The raster-mask feature item in the [editing checklist](photoshop-essentials.md#p4--layer-organization-and-masks) remains unchecked until the complete document, input, composition, persistence and presentation contract passes its acceptance checks.

The [native A8 prerequisites](native-plugins.md#optional-native-mask-composition), [document/event foundation](raster-mask-storage.md), [public editing subset](raster-mask-editing.md) and [document-wide geometry](mask-canvas-geometry.md) are implemented. The editing subset provides eleven registered actions, typed pressure/stabilized painting, density/feather, selection conversion, raster-layer raw-alpha application and compiled Layers controls. Canvas framing/crop, document rotations/flips, Image Size and Trim stage every raw mask with shared history/outcomes. Plain [single-layer owner copy/delete](mask-owner-editing.md) carries raw masks and paint-target rules through compound history. [Group creation and hierarchy movement](mask-group-editing.md) preserve every existing owner and raw plane without A8 tile changes. [Subtree deletion](mask-group-delete.md) removes consumed raster/group owner masks through one compound history edit. [Subtree duplication](mask-group-duplicate.md) copies all descendant owners under fresh IDs and independent raw buffers, with mapped targets/selection and one compound history edit. Owner linked/unlinked geometry, ungrouping, batches, merge/clipboard handling and full-feature delivery remain open. The full raster-mask item stays unchecked. Untouched documents retain their existing serialization and recording hashes. The implementation is independent and imports no PhotoCraft source.

## Ownership and implemented data model

The host owns identity, raw surfaces, history, dirty regions and verified outcomes. Native plugins own pixel algorithms. The mutable mask surface is distinct from the immutable, compressed `selection::Mask`; constructing or recompressing a selection object for every paint sample is not a mask storage implementation.

The engine module is `src/engine/raster_masks.rs`, with these storage types (abridged):

```rust
enum MaskOwner {
    Layer { id: u64 },
    Group { id: u64 },
}

struct MaskMetadata {
    id: u64,
    owner: MaskOwner,
    enabled: bool,
    linked: bool,
    density: u8,
    feather_radius_px: u16,
}

enum PaintTarget {
    LayerPixels,
    Mask { id: u64 },
}

enum SamplePlane {
    Uniform(u8),
    Dense(Vec<u8>),
}

struct MaskPlane {
    id: u64,
    samples: SamplePlane,
    content_revision: u64,
}
```

The [storage guide](raster-mask-storage.md#ownership-and-limits) specifies the metadata contract; the [editing guide](raster-mask-editing.md#registered-actions-and-public-state) specifies public action/state schemas. Each layer or group owns at most one mask. Owners use explicit kind tags because layer and group numeric IDs occupy different namespaces. Mask IDs have their own document-scoped, monotonic allocator, bounded to the existing stable-ID integer range. Committed Undo does not reuse an allocated ID; failed atomic execution restores the allocator.

Every raw A8 plane has the current canvas dimensions. Zero hides and 255 reveals; it is coverage, not an RGBA image with transparency. The separate budget is 64 MiB of logical A8 samples across at most 32 owner masks, with existing 4096-pixel side bounds. Validation counts logical samples even if uniform storage avoids materializing an all-white or all-black plane. Document geometry uses the same checked area/count arithmetic and owner-uniqueness checks before publication; supported single-layer/group duplication and future owner geometry retain these rules.

Only metadata belongs in `Composition.raster_masks`: the versioned collection, allocator and paint target. The entire default field is omitted so untouched documents retain legacy metadata and recording hashes; an explicit record requires `version`, `items` and `next_id`. Raw byte planes never enter that JSON metadata. Uniform in-memory storage materializes once at contact start and does not trigger whole-plane copy-on-write during MOVE.

## Input and history

The engine resolves owner visibility, dimensions and composition requirements at `begin_mask_stroke`. Its lease caches the mask slot and storage epoch independently of content revisions. The Session/desktop route validates capabilities and contact permissions and reuses the native tool instance at DOWN. A typed mutable A8 loan carries a borrowed region, dimensions, stride and origin; it is separate from the legacy RGBA loan. Existing RGBA actions reject a mask target and are never silently redirected into a mask.

The native route reuses the painting DLL's pressure curve, size/opacity dynamics, spacing, interpolation and stabilizer for the A8 sink. Brush freezes encoded-RGB grayscale as `floor((54*R+183*G+19*B+128)/256)` at contact start and interpolates coverage toward it; Eraser and pen eraser reveal toward 255. The frozen fractional pixel-selection snapshot clips both once. Whole-plane Fill/Invert ignore the pixel selection and retain effect properties. Native integer rules and independent byte references remain part of the shared contract.

The host snapshots each touched 64 by 64 A8 tile once per contact. History has a distinct byte-tile namespace keyed by mask ID; it does not reinterpret bytes as `u32` pixels or invent raster-layer indices. Metadata edits share the existing composition history. Added, removed and replaced planes move into structural Undo ownership; ordinary edits and atomic checkpoints do not clone the entire mask.

The implemented engine boundaries are `begin_mask_stroke(mask_id)` to return a resolved lease, `prepare_mask_region(lease, x, y, width, height)` to validate scalar identity/epoch and snapshot touched byte tiles, and `mask_region` for the scoped native descriptor. The lease contains document identity, mask ID, slot and storage epoch; each new contact invalidates old leases even when reusing dense storage. These helpers use the existing finish/rollback orchestration, without a separate mask Undo stack. Whole-plane mutation helpers remain cold, validate a complete staged replacement and publish through that same history path.

`HistoryCheckpoint` preserves mask allocation/contact state, while verified replay state shares immutable byte tile cells for rollback. Restoration includes raw planes, metadata, target, Undo/Redo, dirty state and the already verified composite without invoking a renderer. Mask painting retains the existing per-successful-sample Session revision semantics. A completed contact uses one stroke Undo entry; verified outcomes are recorded in bounded provisional segments and finalized at the contact boundary. The ordinary typed MOVE path does not serialize JSON, discover tools, create instances or clone whole-plane storage. Recording boundaries may flush a segment while the native contact remains active. Effect processing reads only the demanded output's halo, which can span the canvas for a large radius.

## Effective coverage and composition

Disabled masks reveal fully. Density and feather are implemented non-destructive native effects; raw A8 values remain unchanged. For radius `R` in 0–256, the kernel uses `a=floor((R+2)/3)`, `b=floor((R+1)/3)`, `c=floor(R/3)` in the order `H(a), V(a), H(b), V(b), H(c), V(c)`. Each pass clamps to the canvas edge and rounds its byte mean half-up. Density follows feather: `255 - floor(((255-feathered)*d+127)/255)`. Zero density reveals fully; full density preserves feathered coverage; an all-white plane stays white at the edges. Nondefault effects require the optional effects capability even on hidden or disabled masks. Verified outcome restoration remains renderer-independent.

The total influence support is exactly `R` per axis. A raw edit expands output damage by that support, and the native callback reads only the demanded region plus its required halo. Parameter changes invalidate the entire effective plane. Raw and effective storage have separate 64 MiB logical budgets; the cached native instance reuses two byte buffers bounded to 32 MiB plus a 16 KiB column workspace. Uniform-mask DOWN reserves capacity without zeroing scratch or running a full-frame feather kernel, then materializes constant raw/effective storage once. Dense cold rebuilds may still evaluate the whole frame. See the [exact effect and cache contract](raster-mask-editing.md#non-destructive-effective-coverage).

Implemented layer masks apply after the layer's enabled source filters and before that layer is blended, combining layer opacity once. Isolated group masks gate the group's internal result once. Pass Through group masks interpolate the completed before/after change, combined with group opacity once; attenuating every child separately is incorrect. Mask and group gates remain active when source filters are disabled.

Image composition now has distinct coverage ports and bounded instructions for mask sources, application and masked before/after mixing. The old `CompositeLayer` prefix and ordinary render callback remain intact. Optional native descriptors/callbacks are size/capability checked, and mask-dependent mutation/publication preflights coverage support. Generated masks stay within the existing graph instruction/value budget; editable Layer Filters do not accept generated mask gates as source. Editing a workflow remains inert.

The first native prerequisite uses `A8Pixels`, a size-checked 40-byte `ToolHostA8` extension and tool flag 4 for explicit A8 support. Its separate compositor extension is a 40-byte table with capability 4, a 24-byte `CompositeMask` descriptor and node kinds 6–8 for typed coverage source/application/masked mix. A null mask data pointer is accepted only for a validated uniform constant descriptor. Group opacity and spatial coverage combine once as `round_half_up(opacity_byte * coverage / 255)` before the premultiplied before/after mix. These native primitives do not provide document ownership, persistence, feather caches or public mask editing on their own.

The optional 72-byte x64 effects table retains that 40-byte prefix and adds create/destroy/region/reserve callbacks under capability `0x8`. It rejects incomplete tails, missing callbacks, invalid descriptors and aliasing before output writes. Raw inversion uses the region callback's explicit `INVERT_RAW` flag only with radius zero and density 255. See [Native Plugins](native-plugins.md#optional-native-mask-effects) for offsets and lifecycle.

## Structural and geometry rules

The independent [raw A8 sampler](native-plugins.md#optional-raw-a8-sampler)
provides validated native reframe, orthogonal geometry and resampling over
borrowed dense/uniform sources. The host's [document geometry integration](mask-canvas-geometry.md)
stages all owner planes with RGBA and pixel selections, checks the complete
stable-ID bundle and budgets, then publishes one Canvas history/outcome. Hidden,
disabled and unlinked masks participate; expansion fills new raw-mask space with
255. Density and numeric feather are retained, and effective coverage is rebuilt
under its canonical rounding policy. Owner-specific geometry remains separate.

The table combines the implemented document subset with remaining acceptance requirements. Until each pending path stages masks correctly, existing mask-unaware owner-geometry, structural and cut/paste edits reject mask-bearing documents with `mask_editing_pending`. Corresponding plans/previews are guarded before finishing a live contact. See the [current boundary list](raster-mask-storage.md#temporary-editing-boundaries); these guards preserve data rather than implementing the eventual operations.

| Operation | Required mask behavior |
| --- | --- |
| Canvas crop, anchored size, document rotation/flip, Image Size, Trim — implemented | Reframe or sample every mask with the canvas, independent of enable/visibility/link flags; include raw/effective masks in preflight cost/budget plans |
| Pixel-layer integer Move — implemented | Move the active raster and its linked mask under one raw-only history/outcome; leave an unlinked mask in canvas coordinates |
| Raster-mask integer Move — implemented | Move only an explicit or selected raster-owned raw plane, with neutral-white expansion, owner position/visibility checks and unchanged painting target/active raster |
| Selection-outline integer Move — implemented | Move the current pixel selection, preserving Reselect and every raw layer/group mask |
| Owner affine Free Transform — source increment, CI pending | Transform exact captured raster IDs and their linked masks, or one raster-owned raw mask; retain selections, one recipe/Undo and exact-scope Repeat; group/projective/copy domains remain guarded; see [Owner Affine](owner-affine.md) |
| Group-mask movement — planned | Define group geometry separately; moving one group mask does not transform its subtree |
| Group create/create-selected/move — implemented | Retain all owner/mask IDs, raw planes, targets and selected/active raster IDs; preflight the source and candidate hierarchy/capabilities; publish one structural history/outcome without A8 tile changes |
| Group ungroup — planned | Define removal of the group's coverage/opacity boundary and safely remove or transfer its owned mask |
| Single-layer duplicate — implemented | Copy one owned raw plane and properties under a fresh mask ID, preserve source opacity/filters and apply explicit target mapping in one Undo/outcome |
| Group/subtree duplicate — implemented | Copy every descendant owner plane under ordered fresh IDs and independent storage after complete source/candidate preflight; map selected source masks and copied raster selection through one compound history edit |
| Batch layer duplicate — planned | Copy every requested owner plane under fresh IDs after aggregate budget/identity preflight; do not share mutable storage |
| Single-layer delete — implemented | Remove only its owned mask; fall back to Pixels only when that removed mask was selected, retaining unrelated targets/folders/planes |
| Group/subtree delete — implemented | Remove every consumed raster/group owner mask, retain unrelated storage/targets, return original-order consumed-ID arrays and restore all removed slots through one compound history edit |
| Batch layer delete — planned | Remove every consumed owner mask and define all target/selection fallbacks under one bounded edit |
| Merge Down/selected-only merge | Bake masks with the same selected projection/filter gates; remove only consumed raster masks, preserve every retained group mask and unselected surface |
| Merge Group | Bake descendants but retain/transfer the outer mask onto the replacement raster without applying that outer gate twice |
| Merge Visible | Bake effective masks in the complete visible result, retain masks needed by surviving hidden content/folders |
| Stamp Visible | Create the composed raster without destructively changing source masks |
| Flatten | Bake effective coverage into the final raster and remove the obsolete mask ownership |
| Raw clipboard or selected copy/cut | Define whether mask data is copied with its owner; preserve the established raw-pixel versus composed-pixel distinction explicitly |

Quarter-turn axis swaps, unequal physical density and current/Reselect selection masks retain their existing contracts. Group transformations remain a separate planned feature; do not imply that a group mask alone implements them. New geometry paths must stage masks alongside pixels and selection, validate the final metadata and publish all storage atomically. Existing geometry cannot silently drop masks or retain old-sized byte buffers.

## Project and replay persistence

MICOPI04 is implemented for mask-aware projects. It retains the raster prefix and bounded 1 MiB composition metadata block, then stores a separate raw mask directory/payload and a length-prefixed optional recording. Documents that have never used mask state retain the exact MICOPI02/03 writer paths. A retained mask allocator still selects MICOPI04 after the last mask is deleted; old readers reject the new magic rather than silently discarding state. See the [exact format and bounds](raster-mask-storage.md#micopi04-project-format).

Every A8 plane derives its dimensions from the checked canvas frame. Each expected metadata ID occurs exactly once; uniform and raw A8 encodings have exact lengths. Loading rejects extra/missing/duplicate IDs, unknown encodings, bad owners, overflow, truncated payloads and trailing bytes. Signature sniffing and native-project destination protection recognize MICOPI04. The cold writer canonicalizes an all-constant dense plane to uniform encoding without changing live storage or allocating an extra image, and stages the file before atomic replacement.

Replay has a separate optional `mask_changes` namespace keyed by stable mask ID and tile, with byte blocks, uniform compression, backward references and sparse XOR deltas. It does not overload `Change.layer`: the existing raster count remains the composite-surface sentinel. A8 tile references are separate from RGBA references and edge-tile lengths are validated. Mask addition/deletion follows final metadata identities; a raster-count change alone does not re-record unchanged mask planes. A canvas-dimension change requires complete replacement coverage for the new frame.

Replay `State` shares immutable A8 tile cells and restores them with metadata and recorded display pixels. Sorted mask IDs and byte tile digests enter a distinct hash domain only when planes exist; untouched legacy states keep their original digest bytes. The archive remains MCRPLY02 with a required `mask_version: 1` event extension once mask state is used, including allocator-only state. Default-omitted event fields preserve old hashes, and strict old event readers reject the extension instead of ignoring masks. See the [replay extension](raster-mask-storage.md#replay-extension).

Session saving is a full verification boundary for every raw plane, including changes made through public engine helpers. `matches_document`, recorded-state validation, archive loading, atomic checkpoints, Undo/Redo and embedded-project recording validation include raw A8 cells. Playback restores the recorded raw masks and composite without loading tools/compositor DLLs or re-executing commands.

## Shared actions and presentation

The Layers package registers eleven commands for owner-targeted create/delete/apply/invert, properties, Fill, selection conversion, query/snapshot and paint target, plus the `layers.masks` state resource. Stable mask/owner IDs, explicit schemas and guaranteed result ports support dynamic workflow bindings. Public action/state nodes, MCP and headless use the same handlers. Queries preserve live contacts and revision; mutations are idle-only and declare document-atomic capability. Workflow dry runs remain inert, and stale run guards use the shared expected revision contract.

The compiled Layers UI shows adjacent pixel/raw-grayscale mask thumbnails, distinct target borders and a painting-owner label. Selecting a mask differs from inspecting a group or changing the multi-layer selected set. Properties use configured spacing and presentation localization; density/feather drafts dispatch once on release and reject stale document/revision state. All edits dispatch registered handlers. Complete multilingual/cross-transport delivery evidence and reviewed current captures remain part of the final gate.

`mask.apply` currently accepts raster owners only. It bakes effective coverage into raw alpha and removes the mask while retaining filters, layer opacity and blend mode. This can round differently from live mask composition: alpha 255, coverage 128 and opacity 137 produce live alpha 69 but raw-alpha bake followed by legacy opacity produces 68. Group application and appearance-preserving subtree baking remain separate work. See [Apply is raw-alpha baking](raster-mask-editing.md#apply-is-raw-alpha-baking).

## Delivery and acceptance gates

1. **Native prerequisites — implemented:** independent typed A8 painting/composition capabilities, exact byte references, dirty/stride/alias/prewrite rejection, legacy-prefix and missing-tail tests. This gate does not expose document masks.
2. **Document and event foundation — implemented:** bounded identity/ownership/raw storage, A8 tiled history, rollback, generated composition, MICOPI04 payload and verified outcome encoding. Regression coverage includes old hashes, old-reader rejection, uniform/random/boundary planes, malformed payloads, journal failures and DLL-free playback. See [Raster Mask Storage](raster-mask-storage.md).
3. **Editing completeness — partial:** Session pressure/stabilized Brush and revealing Eraser, fractional selection clipping, whole-plane edits, density/feather, raster-owner raw-alpha Apply and document-wide framing/crop/rotation/flip/Image Size/Trim are implemented. Plain single-layer owner copy/delete is implemented with exact source copies, target mapping and one compound structural history edit. Owner-preserving group creation, selected-raster grouping and hierarchy movement are implemented without raw A8 changes. Group/subtree deletion removes consumed owner masks with explicit fallbacks, one compound history/outcome and checkpoint-safe restoration. Group/subtree duplication copies independent raw planes with distinct ID orders, mapped target/selection and the same compound history foundation. [Integer raster-owner movement](raster-owner-movement.md) supports linked pairs, independent raw raster masks and selection outlines with one raw-only compound edit. [Owner affine Free Transform](owner-affine.md) adds captured raster/mask scopes, linked companions and exact-ID Repeat with validation pending CI. Owner projective/copy transforms, group-mask movement, selected-pixel transfers, ungrouping, batches and every merge/flatten/clipboard boundary remain planned. Replace temporary guards only with validated atomic staging and verified outcomes.
4. **Public workflow — verified subset:** eleven registered actions, metadata state and schema-derived nodes expose guaranteed result bindings, MCP/headless dispatch and compiled target/property controls. Independent JSONL/MCP/GUI references, atomic failures, read-only contacts and twenty English/CJK captures verify this subset. They do not complete the structural/geometry matrix in gate 3.
5. **Measured delivery — verified public subset:** native references, rejection/ABI checks and bounded scratch benchmarks exercise specific scopes. The source-matched Release build, full desktop/headless suites, independent transport references, twenty real multilingual captures and default-plugin portable startup passed. A reviewed panel capture, runnable example and English guides/changelog document the subset. See [Development](development.md#public-raster-mask-editing-checks) for actual runs, timing exclusions, binary/storage costs and the remaining geometry/structural limits.

The full raster-mask checklist is complete only after all five gates pass. Synthetic fixtures do not establish physical tablet or driver compatibility. Clipping masks remain a separate planned coverage relationship; they must reuse the same composition and outcome foundations without being claimed as implemented by raster masks alone.

## Implementation ownership

| Location | Responsibility |
| --- | --- |
| `crates/plugin-api/` and its C header | Optional A8 tool/compositor contracts, layout constants and ABI fixtures |
| `plugins/painting/`, `plugins/compositing/` | Typed grayscale/coverage kernels, bounded region/halo effects and independent references |
| `src/engine/raster_masks.rs`, `src/engine/raster_mask_editing.rs` | Metadata/owner validation, raw A8 planes, monotonic IDs, scoped leases and byte-tile history |
| `src/engine/document.rs`, `src/engine/raster_mask_project.rs` | Document/history ownership and bounded MICOPI04 persistence |
| `src/engine/mask_composition.rs`, `src/engine/graph.rs`, `src/engine/groups.rs` | Typed coverage nodes, generated layer/group gates, cached descriptors and capability preflight |
| `src/engine/mask_effects.rs`, `src/engine/mask_apply.rs` | Derived coverage caches, exact influence damage, cold reservation and staged raw-alpha application |
| `src/engine/replay.rs`, `src/engine/replay/masks.rs` | Shared byte cells, versioned optional mask changes, hash domains, verification and DLL-free restoration |
| `src/engine/mask_boundary.rs`, `src/application/mask_boundary.rs` | Temporary rejection boundaries for mask-unaware edits and plans |
| `src/application/atomic.rs`, `src/application/replay.rs` | Shared checkpoints, complete save verification and failure publication |
| `src/plugins/raster_masks.rs`, `src/plugins/tools.rs`, `src/plugins/native.rs` | Registered schemas/state/dispatch, cached typed A8 contacts and optional effects lifecycle; no duplicated pixel algorithms |
| `plugins/layers/masks.rs` | Compiled target/raw-thumbnail/property controls dispatching shared commands |
| `src/application/canvas_geometry.rs`, `src/application/image_size.rs` | Read-only shared frame/unit plans, separate raster/A8 budgets, contact state and capability reporting |
| `src/plugins/mask_geometry.rs`, `src/engine/mask_geometry.rs` | Borrowed native A8 staging and complete stable-ID all-plane Canvas publication/history/cache/replay invalidation |
| `src/engine/mask_owner_editing.rs`, `src/plugins/layers.rs` | Single-layer candidate ownership/budget planning, source-mapped compilation, copied/removed plane publication and guaranteed result IDs |
| `plugins/layers/panel.rs`, `plugins/workbench/command_panel.rs` | Shared idle admission through pending controls and argument drafts without finishing active contacts |
| `src/engine/group_editing.rs`, `src/plugins/groups.rs`, `src/application/mask_group_editing_tests.rs` | Owner-preserving group creation/movement, mapped opacity preflight, numeric normalization, unchanged raw-plane and history/result contracts |
| `src/engine/document.rs`, `src/engine/raster_mask_editing.rs`, `src/engine/mask_group_editing.rs`, `src/engine/group_editing.rs`, `src/application/mask_group_delete_tests.rs` | Shared immutable multi-plane structural history, fresh copied-owner/consumed-owner staging, one-step subtree copying/deletion and rollback/persistence contracts |
| Existing owner-geometry/group/batch adapters | Planned owner geometry, ungrouping and baking to replace remaining temporary guards |
| `src/plugins/owner_movement.rs`, `src/engine/owner_movement.rs`, `plugins/canvas/movement.rs` | Bounded integer owner/mask destination resolution, native single-plane staging, compound raw-only Undo and contact-safe outline gestures |
| `tests/`, `scripts/`, `examples/`, `docs/` | Compatibility fixtures, transport checks, executable examples, reviewed captures and public contracts |
