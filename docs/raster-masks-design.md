# Raster masks: implementation design

This document tracks the staged implementation of first-class, paintable raster masks. The raster-mask feature item in the [editing checklist](photoshop-essentials.md#p4--layer-organization-and-masks) remains unchecked until the complete document, input, composition, persistence and presentation contract passes its acceptance checks.

The [native A8 painting and composition prerequisites](native-plugins.md#optional-native-mask-composition) and the [document/event foundation](raster-mask-storage.md) are implemented. The foundation includes raw planes, byte-tile history, generated mask composition, MICOPI04 projects and verified A8 replay. Public mask commands, the Session/desktop painting route, density/feather effects, complete geometry/structural editing and mask UI remain planned below. These foundations must not be presented as a usable public mask feature. Untouched documents retain their existing serialization and recording hashes. The implementation is independent and imports no PhotoCraft source.

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

These engine types do not yet provide registered mask actions or a plugin painting route. The [storage guide](raster-mask-storage.md#ownership-and-limits) specifies their current metadata contract; settle public action/state schemas in the public-workflow gate. Each layer or group owns at most one mask. Owners use explicit kind tags because layer and group numeric IDs occupy different namespaces. Mask IDs have their own document-scoped, monotonic allocator, bounded to the existing stable-ID integer range. Committed Undo does not reuse an allocated ID; failed atomic execution restores the allocator.

Every raw A8 plane has the current canvas dimensions. Zero hides and 255 reveals; it is coverage, not an RGBA image with transparency. The separate budget is 64 MiB of logical A8 samples across at most 32 owner masks, with existing 4096-pixel side bounds. Validation counts logical samples even if uniform storage avoids materializing an all-white or all-black plane. Future duplication and geometry must use the same checked area/count arithmetic and owner-uniqueness checks before publication.

Only metadata belongs in `Composition.raster_masks`: the versioned collection, allocator and paint target. The entire default field is omitted so untouched documents retain legacy metadata and recording hashes; an explicit record requires `version`, `items` and `next_id`. Raw byte planes never enter that JSON metadata. Uniform in-memory storage materializes once at contact start and does not trigger whole-plane copy-on-write during MOVE.

## Input and history

The engine now resolves owner visibility, dimensions and composition requirements at `begin_mask_stroke`. Its lease caches the mask slot and storage epoch independently of content revisions. The remaining Session/desktop route must also validate target permissions and cache the native tool instance at DOWN. A typed mutable A8 loan carries a borrowed region, dimensions, stride and origin; it is separate from the legacy RGBA loan. Existing RGBA actions reject a mask target and are never silently redirected into a mask.

The native prerequisite reuses the painting DLL's pressure curve, size/opacity dynamics, spacing, interpolation and stabilizer for the A8 sink. The remaining host contact route must freeze an explicit grayscale paint value at contact start. Brush interpolates coverage toward that value; Eraser reveals toward 255. The native fractional pixel-selection snapshot clips both once. Preserve the exact native integer rules and independent byte references when exposing these tools through Session.

The host snapshots each touched 64 by 64 A8 tile once per contact. History has a distinct byte-tile namespace keyed by mask ID; it does not reinterpret bytes as `u32` pixels or invent raster-layer indices. Metadata edits share the existing composition history. Added, removed and replaced planes move into structural Undo ownership; ordinary edits and atomic checkpoints do not clone the entire mask.

The implemented engine boundaries are `begin_mask_stroke(mask_id)` to return a resolved lease, `prepare_mask_region(lease, x, y, width, height)` to validate scalar identity/epoch and snapshot touched byte tiles, and `mask_region` for the scoped native descriptor. The lease contains document identity, mask ID, slot and storage epoch; each new contact invalidates old leases even when reusing dense storage. These helpers use the existing finish/rollback orchestration, without a separate mask Undo stack. Whole-plane mutation helpers remain cold, validate a complete staged replacement and publish through that same history path.

`HistoryCheckpoint` preserves mask allocation/contact state, while verified replay state shares immutable byte tile cells for rollback. Restoration includes raw planes, metadata, target, Undo/Redo, dirty state and the already verified composite without invoking a renderer. The future Session mask contact must remain one Undo entry and one revision/outcome, with the existing bounded provisional sample segments. No per-MOVE JSON, compression, file I/O, discovery, instance creation or full-frame copying is permitted.

## Effective coverage and composition

Enable is implemented; disabled masks reveal fully. Density and feather remain planned non-destructive effects. Storage accepts bounded metadata, but live composition and renderer-dependent mutation reject density other than 255 or feather other than 0, including on hidden or disabled masks. Verified outcome restoration retains those values without rendering. The future kernel evaluates feather before density and retains raw A8 values. For byte density `d`, the proposed density rule is `255 - round_half_up((255 - feathered) * d / 255)`, so zero density reveals fully and full density preserves the feathered coverage. Define border extension and arithmetic exactly, and retain an all-white mask unchanged at canvas edges.

Feather requires a bounded native region/halo algorithm and reusable scratch, not two new full-canvas temporaries for every sample. If the chosen separable approximation uses three box passes, derive its total support from those passes rather than expanding dirty output by only one radius. Parameter changes invalidate the whole affected mask result; a paint edit invalidates the exact influence region.

Implemented layer masks apply after the layer's enabled source filters and before that layer is blended, combining layer opacity once. Isolated group masks gate the group's internal result once. Pass Through group masks interpolate the completed before/after change, combined with group opacity once; attenuating every child separately is incorrect. Mask and group gates remain active when source filters are disabled.

Image composition now has distinct coverage ports and bounded instructions for mask sources, application and masked before/after mixing. The old `CompositeLayer` prefix and ordinary render callback remain intact. Optional native descriptors/callbacks are size/capability checked, and mask-dependent mutation/publication preflights coverage support. Generated masks stay within the existing graph instruction/value budget; editable Layer Filters do not accept generated mask gates as source. Editing a workflow remains inert.

The first native prerequisite uses `A8Pixels`, a size-checked 40-byte `ToolHostA8` extension and tool flag 4 for explicit A8 support. Its separate compositor extension is a 40-byte table with capability 4, a 24-byte `CompositeMask` descriptor and node kinds 6–8 for typed coverage source/application/masked mix. A null mask data pointer is accepted only for a validated uniform constant descriptor. Group opacity and spatial coverage combine once as `round_half_up(opacity_byte * coverage / 255)` before the premultiplied before/after mix. These native primitives do not provide document ownership, persistence, feather caches or public mask editing on their own.

## Structural and geometry rules

The following behaviors remain acceptance requirements. Until each path stages masks correctly, existing mask-unaware structural, geometric and cut/paste edits reject mask-bearing documents with `mask_editing_pending`. Corresponding plans/previews are guarded before finishing a live contact. See the [current boundary list](raster-mask-storage.md#temporary-editing-boundaries); these guards preserve data rather than implementing the eventual operations.

| Operation | Required mask behavior |
| --- | --- |
| Canvas crop, anchored size, document flip/quarter turn, Image Size | Reframe or sample every mask with the canvas, independent of the mask's link flag; include masks in preflight cost/budget plans |
| Pixel-layer Move/Free Transform | Transform the owner's linked mask with pixels under one history/outcome; leave an unlinked mask in canvas coordinates |
| Mask-target Move/Free Transform | Transform only the selected mask; do not move the owner's raw pixels |
| Selection-only geometry | Preserve raw layer/group masks |
| Layer/group duplicate | Copy owned mask samples under fresh mask IDs; do not share mutable storage |
| Owner delete | Remove its masks and choose a valid fallback paint target; Undo restores owners, IDs and samples |
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

## Shared actions and presentation (planned)

Register owner-targeted create/delete/apply/invert, enable/link/density/feather, selection conversion, mask query/snapshot and paint-target operations. Use stable mask/owner IDs, explicit schemas, expected revision guards and atomic capability declarations. Resolve dynamic workflow inputs at execution. Read-only queries must not finish a contact or mutate pixels/history; idle-only mutations preserve active Brush/Hand contacts on rejection.

The planned compiled Layers UI must show adjacent pixel and grayscale-mask thumbnails with distinct paint-target feedback. Selecting a mask differs from inspecting a group or changing the multi-layer selected set. Show whether painting affects Pixels or Mask, and keep controls responsive under configured spacing and CJK labels. All document edits must dispatch the registered handlers. Commands, shortcuts, schema-derived nodes, public state, MCP and headless must share the same implementation; presentation-only inspection must explicitly report headless unavailability.

## Delivery and acceptance gates

1. **Native prerequisites — implemented:** independent typed A8 painting/composition capabilities, exact byte references, dirty/stride/alias/prewrite rejection, legacy-prefix and missing-tail tests. This gate does not expose document masks.
2. **Document and event foundation — implemented:** bounded identity/ownership/raw storage, A8 tiled history, rollback, generated composition, MICOPI04 payload and verified outcome encoding. Regression coverage includes old hashes, old-reader rejection, uniform/random/boundary planes, malformed payloads, journal failures and DLL-free playback. See [Raster Mask Storage](raster-mask-storage.md).
3. **Editing completeness — planned:** Session-routed pressure/stabilized brush and revealing Eraser, selection clipping, invert/density/feather/apply, linked/unlinked geometry, duplication/deletion and every existing merge/flatten/clipboard boundary. Replace each temporary guard with validated atomic support while retaining source pixels and masks on rejection.
4. **Public workflow — planned:** registered mask action/state schemas and nodes, dynamic result/input binding, MCP/headless and desktop parity, clear paint-target UI, English/Chinese/Japanese/Korean layouts and inert inspection/cancellation.
5. **Measured delivery — planned:** dirty-region/feather costs, typed-path throughput and full-command/binary/storage costs with scopes stated; reviewed real screenshots, runnable examples, portable-package checks, English guides and changelog, then a coherent feature commit.

The full raster-mask checklist is complete only after all five gates pass. Synthetic fixtures do not establish physical tablet or driver compatibility. Clipping masks remain a separate planned coverage relationship; they must reuse the same composition and outcome foundations without being claimed as implemented by raster masks alone.

## Implementation ownership

| Location | Responsibility |
| --- | --- |
| `crates/plugin-api/` and its C header | Optional A8 tool/compositor contracts, layout constants and ABI fixtures |
| `plugins/painting/`, `plugins/compositing/` | Implemented typed grayscale/coverage kernels and references; density/feather kernels remain planned |
| `src/engine/raster_masks.rs`, `src/engine/raster_mask_editing.rs` | Metadata/owner validation, raw A8 planes, monotonic IDs, scoped leases and byte-tile history |
| `src/engine/document.rs`, `src/engine/raster_mask_project.rs` | Document/history ownership and bounded MICOPI04 persistence |
| `src/engine/mask_composition.rs`, `src/engine/graph.rs`, `src/engine/groups.rs` | Typed coverage nodes, generated layer/group gates, cached descriptors and capability preflight |
| `src/engine/replay.rs`, `src/engine/replay/masks.rs` | Shared byte cells, versioned optional mask changes, hash domains, verification and DLL-free restoration |
| `src/engine/mask_boundary.rs`, `src/application/mask_boundary.rs` | Temporary rejection boundaries for mask-unaware edits and plans |
| `src/application/atomic.rs`, `src/application/replay.rs` | Shared checkpoints, complete save verification and failure publication; mask contact orchestration remains planned |
| `src/plugins/` | Capability loading; future cached A8 contact loans and mask action/state dispatch; no duplicated pixel algorithms |
| `plugins/layers/` and existing geometry adapters | Planned compiled target/thumbnail controls, mask-aware staged geometry and registered command dispatch |
| `tests/`, `scripts/`, `examples/`, `docs/` | Compatibility fixtures, transport checks, executable examples, reviewed captures and public contracts |
