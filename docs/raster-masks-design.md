# Raster masks: implementation design

This document specifies planned first-class, paintable raster masks. It is an implementation plan, not an announcement that masks are available. The raster-mask feature item in the [editing checklist](photoshop-essentials.md#p4--layer-organization-and-masks) remains unchecked until the complete document, input, composition, persistence and presentation contract passes its acceptance checks.

The [native A8 painting and composition prerequisites](native-plugins.md#optional-native-mask-composition) are implemented and independently verified. Document storage, byte-tile history, persistence, geometry and public/UI integration remain planned below. Those prerequisites must not be presented as a usable mask feature. Existing documents, tools, IDs and recordings keep their current behavior until a document explicitly opts into the new mask contract. The implementation is independent and imports no PhotoCraft source.

## Ownership and proposed data model

The host owns identity, raw surfaces, history, dirty regions and verified outcomes. Native plugins own pixel algorithms. The mutable mask surface is distinct from the immutable, compressed `selection::Mask`; constructing or recompressing a selection object for every paint sample is not a mask storage implementation.

The proposed engine module is `src/engine/raster_masks.rs`, with these logical types:

```rust
type MaskId = u64;

enum MaskOwner {
    Layer { id: u64 },
    Group { id: u64 },
}

struct MaskMetadata {
    id: MaskId,
    owner: MaskOwner,
    enabled: bool,
    linked: bool,
    density: u8,
    feather_radius_px: u16,
}

enum PaintTarget {
    LayerPixels,
    Mask { id: MaskId },
}

struct MaskPlane {
    id: MaskId,
    pixels: Vec<u8>,
    generation: u64,
}
```

These are proposed names, not an exported Rust or JSON ABI. Settle the exact registered schemas before exposing them. Each layer or group owns at most one mask. Owners use explicit kind tags because layer and group numeric IDs occupy different namespaces. Mask IDs have their own document-scoped, monotonic allocator, bounded to the existing stable-ID integer range. Committed Undo must not reuse an allocated ID; failed atomic execution restores the allocator.

Every raw A8 plane has the current canvas dimensions. Zero hides and 255 reveals; it is coverage, not an RGBA image with transparency. A provisional separate budget is 64 MiB of logical A8 samples across at most 32 owner masks, with existing 4096-pixel side bounds. Count logical samples even if a uniform storage representation avoids materializing an all-white or all-black plane. Validate checked area/count arithmetic and owner uniqueness before allocation, duplication, geometry or publication.

Only metadata belongs in `Composition`: an optional mask metadata collection, allocator and paint target. Omit an empty collection, unused allocator and default `LayerPixels` target from serialization so untouched documents retain legacy metadata and recording hashes. Raw byte planes never enter that JSON metadata. An optional uniform in-memory representation may materialize once at contact start; it must never trigger whole-plane copy-on-write during MOVE.

## Input and history

At DOWN, validate and resolve the chosen target, owner visibility, write permissions, dimensions, surface generation and native capability once. Cache the mask slot and generation with the native tool instance for the contact. A typed mutable A8 loan carries a borrowed region, dimensions, stride and origin; it is separate from the legacy RGBA loan. Existing RGBA actions must never be silently redirected into a mask.

The painting DLL reuses its pressure curve, size/opacity dynamics, spacing, interpolation and stabilizer for the A8 sink. Freeze an explicit grayscale paint value at contact start. Brush interpolates coverage toward that value; Eraser reveals toward 255. The current fractional pixel-selection mask clips both through its existing immutable entry snapshot, applying coverage once. Grayscale conversion and tool behavior need exact documented integer rules and independent byte references.

The host snapshots each touched 64 by 64 A8 tile once per contact. Extend history with a distinct A8 tile type and mask identity; do not reinterpret bytes as `u32` pixels or invent raster-layer indices. Metadata edits share the existing composition history. Added, removed and replaced planes move into structural Undo ownership; ordinary edits and atomic checkpoints must not clone the entire mask.

Proposed cold/hot engine boundaries are `begin_mask_stroke(mask_id)` to return a resolved lease, `prepare_mask_region(lease, rectangle)` to validate scalar identity/generation and snapshot touched byte tiles, and a scoped region loan to the cached native callback. The lease contains document identity, mask ID, slot and generation. Use the existing finish/rollback orchestration for the contact; do not create a separate mask Undo stack. Whole-plane mutation helpers remain cold, validate a complete staged replacement and publish through that same history path.

`HistoryCheckpoint` must preserve mask allocation/contact state, while verified replay state shares immutable byte tile cells for rollback. Restoring a failed native or journal operation restores raw planes, metadata, target, Undo/Redo, dirty state and the already verified composite without invoking the failing renderer again. A stroke remains one Undo entry and one revision/outcome, with bounded provisional sample segments as today. No per-MOVE JSON, compression, file I/O, discovery, instance creation or full-frame copying is permitted.

## Effective coverage and composition

Enable, density and feather are non-destructive parameters; retain raw A8 values. Disabled masks reveal fully. Feather is evaluated before density. For byte density `d`, the proposed density rule is `255 - round_half_up((255 - feathered) * d / 255)`, so zero density reveals fully and full density preserves the feathered coverage. Bound radius, define border extension and arithmetic exactly, and retain an all-white mask unchanged at canvas edges.

Feather requires a bounded native region/halo algorithm and reusable scratch, not two new full-canvas temporaries for every sample. If the chosen separable approximation uses three box passes, derive its total support from those passes rather than expanding dirty output by only one radius. Parameter changes invalidate the whole affected mask result; a paint edit invalidates the exact influence region.

Layer masks apply after the layer's enabled source filters and before that layer is blended. Isolated group masks gate the group's internal result once. Pass Through group masks interpolate the completed before/after change, combined with group opacity once; attenuating every child separately is incorrect.

Image composition needs distinct coverage ports and bounded instructions for mask sources, application and masked before/after mixing. Retain the old `CompositeLayer` prefix and ordinary render callback. Add optional size/capability-checked native descriptors/callbacks; old hosts retain their prefixes, and new hosts reject missing coverage capability before exposing or loading dependent editing state. Keep mask topology and scratch costs within the graph's explicit instruction/value budget. Editing a workflow remains inert.

The first native prerequisite uses `A8Pixels`, a size-checked 40-byte `ToolHostA8` extension and tool flag 4 for explicit A8 support. Its separate compositor extension is a 40-byte table with capability 4, a 24-byte `CompositeMask` descriptor and node kinds 6–8 for typed coverage source/application/masked mix. A null mask data pointer is accepted only for a validated uniform constant descriptor. Group opacity and spatial coverage combine once as `round_half_up(opacity_byte * coverage / 255)` before the premultiplied before/after mix. These native primitives do not provide document ownership, persistence, feather caches or public mask editing on their own.

## Structural and geometry rules

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

Current projects store raster bytes followed by a bounded 1 MiB metadata block and optional archive. Add a new binary project version for mask-aware documents rather than packing raw masks into that JSON block. Preserve the exact MICOPI02/03 writer path for documents that have never used mask state. A document with retained mask allocator state still needs the new format after its last mask is deleted; an old reader must not silently discard that state.

The new payload should keep the existing raster prefix, then store bounded composition metadata, a separately bounded mask-plane directory/payload and an optional length-prefixed recording. Reserve its exact version and byte layout during host integration, with matching fixtures. Derive every A8 plane's dimensions from the checked canvas frame. Each expected metadata ID occurs exactly once; uniform and raw A8 encodings have exact lengths. Reject extra/missing/duplicate IDs, unknown encodings, bad owners, overflow, truncated payloads and trailing bytes before replacing a live document or file. Update signature sniffing and native-project destination protection together. Stage the file before atomic replacement as today.

Replay needs a separate optional `mask_changes` namespace keyed by stable mask ID and tile, with byte blocks, uniform compression, backward references and sparse XOR deltas. Do not overload `Change.layer`: the existing raster count is the composite-surface sentinel. Keep A8 tile references separate from RGBA references and validate lengths at edge tiles. Mask addition/deletion follows final metadata identities; a raster-count change alone must not require re-recording every unchanged mask plane. A canvas-dimension change requires complete replacement coverage for the new frame.

Extend replay `State` with shared immutable A8 tile cells and restore them before metadata/display publication. Include sorted mask IDs and byte tile digests in a distinct hash domain only when the state has mask planes; untouched legacy states keep their original digest bytes. Default-omit new event fields so existing fixture hashes remain exact. Old strict readers must clearly reject mask-bearing outcomes, rather than accepting a recording while ignoring masks. Whether the archive receives a new magic or a versioned optional event extension must be settled and covered by old-reader rejection fixtures before publication.

Saving is a full verification boundary for every raw plane, including public-engine callers. `matches_document`, recorded-state validation, archive loading, atomic checkpoints, Undo/Redo and embedded-project recording validation must all include the raw A8 cells. Playback restores the recorded raw masks and composite without loading tools/compositor DLLs or re-executing commands.

## Shared actions and presentation

Register owner-targeted create/delete/apply/invert, enable/link/density/feather, selection conversion, mask query/snapshot and paint-target operations. Use stable mask/owner IDs, explicit schemas, expected revision guards and atomic capability declarations. Resolve dynamic workflow inputs at execution. Read-only queries must not finish a contact or mutate pixels/history; idle-only mutations preserve active Brush/Hand contacts on rejection.

The compiled Layers UI shows adjacent pixel and grayscale-mask thumbnails with distinct paint-target feedback. Selecting a mask differs from inspecting a group or changing the multi-layer selected set. Show whether painting affects Pixels or Mask, and keep controls responsive under configured spacing and CJK labels. All document edits dispatch the registered handlers. Commands, shortcuts, schema-derived nodes, public state, MCP and headless share the same implementation; presentation-only inspection explicitly reports headless unavailability.

## Delivery and acceptance gates

1. **Native prerequisites:** independent typed A8 painting/composition capabilities, exact byte references, dirty/stride/alias/prewrite rejection, legacy-prefix and missing-tail tests. This gate does not make masks available in documents.
2. **Document and event integration:** bounded identity/ownership/raw storage, A8 tiled history, rollback, new project payload and verified outcome encoding. Test old hashes, old-reader rejection, uniform/random/boundary planes, partial journal/file failures and DLL-free playback.
3. **Editing completeness:** pressure/stabilized brush and revealing Eraser, selection clipping, enable/invert/density/feather/apply, linked/unlinked geometry, duplication/deletion and every existing merge/flatten/clipboard boundary. Retain source pixels and masks on rejected operations.
4. **Public workflow:** registered schemas/state/nodes, dynamic result/input binding, MCP/headless and desktop parity, clear paint-target UI, English/Chinese/Japanese/Korean layouts and inert inspection/cancellation.
5. **Measured delivery:** dirty-region/feather costs, typed-path throughput and full-command/binary/storage costs with scopes stated; reviewed real screenshots, runnable examples, portable-package checks, English guides and changelog, then a coherent commit.

The full raster-mask checklist is complete only after all five gates pass. Synthetic fixtures do not establish physical tablet or driver compatibility. Clipping masks remain a separate planned coverage relationship; they must reuse the same composition and outcome foundations without being claimed as implemented by raster masks alone.

## Planned implementation ownership

| Location | Responsibility |
| --- | --- |
| `crates/plugin-api/` and its C header | Optional A8 tool/compositor contracts, layout constants and ABI fixtures |
| `plugins/painting/`, `plugins/compositing/` | Typed grayscale painting, coverage/density/feather kernels and independent native references |
| `src/engine/raster_masks.rs` (proposed) | Mask metadata, identity/owner validation, raw A8 planes, lease and dirty-region helpers |
| `src/engine/document.rs` | A8 history variants, plane ownership in structural/canvas edits and new binary project payload |
| `src/engine/replay.rs` | Byte tile cells, optional mask changes, hash domains, verification and DLL-free restoration |
| `src/application/atomic.rs`, `src/application/replay.rs` | Shared checkpoints, pointer/action boundaries, complete save verification and failure publication |
| `src/plugins/` | Capability loading, cached typed loans and shared action/state dispatch; no duplicated pixel algorithms |
| `plugins/layers/` and existing geometry adapters | Compiled target/thumbnail controls, staged geometry and registered command dispatch |
| `tests/`, `scripts/`, `examples/`, `docs/` | Compatibility fixtures, transport checks, executable examples, reviewed captures and public contracts |
