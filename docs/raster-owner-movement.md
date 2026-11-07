# Raster layer and mask movement

The existing `canvas.move` and `canvas.nudge` actions support whole raster-layer
and raw raster-mask movement. Desktop Move (V), arrow-key nudges, command forms,
schema-derived nodes, JSONL and MCP share these handlers. Native plugins perform
the exact integer sampling; the host owns admission, history and verified outcomes.

This is a bounded owner-movement subset. Group-mask movement, selected-pixel
movement on mask-bearing documents remain guarded. [Owner affine Free
Transform](owner-affine.md) separately supports captured raster sets, linked
companions and independent raw raster masks.
Persistent [layer links](layer-links.md) extend whole-layer movement to explicitly
captured raster sets. Off-canvas storage remains unavailable.

## Choose the domain explicitly

| Request | Moved data | Retained data |
| --- | --- | --- |
| `target: "auto"` with a raster painting mask selected | That entire raw A8 plane | Its owner's pixels and all other masks |
| Auto without a mask target | Existing selected-pixel or active-layer behavior | Existing unmasked contracts; selected pixels remain unsupported when raster masks exist |
| `target: "layer"` | Active raster pixels and only its owned **linked** mask | Unlinked masks, every other owner and both pixel selections |
| `mask_id: N`, with target omitted or Auto | Entire raw raster-owned mask N, independently of painting target and active raster | Owner pixels, active/selected layers, painting target and both pixel selections |
| `target: "selection_mask"` | Current pixel-selection outline | Reselect selection, raster pixels and every raw owner mask |

Auto gives the selected painting mask precedence over a current pixel selection.
An explicit layer target still means the active raster, even when a different
owner's mask is selected for painting. Selecting or moving a mask does not make
its owner the active painting layer.

`mask_id` is an optional stable-ID input on both existing actions. It overrides
stored movement target preferences when target is omitted or Auto. Combining it
with an explicit Layer, Selected Pixels or Selection Outline target is a literal
`invalid_arguments` error, before earlier declared pipeline operations execute.
Existence, owner and capabilities are contextual checks at execution, so a prior
pipeline node can create a mask and bind its returned ID into a later move.

The existing movement preference enum is unchanged; Free Transform has its own
[explicit owner-scope resolver](owner-affine.md).
There is no new `raster_mask` preference value. The resolved event-context label
`raster_mask` describes the executed domain rather than a new input enum.

```json
{"action":"canvas.move","args":{"target":"layer","dx":2,"dy":-1}}
{"action":"canvas.move","args":{"mask_id":1,"dx":-2,"dy":0}}
{"action":"canvas.nudge","args":{"mask_id":1,"direction":"right","large":true}}
```

Use the actual ID returned by `mask.create`; the value above is illustrative.
See the [runnable example](../examples/automation/mask-owner-movement.jsonl).
Nudge uses configured ordinary/large steps from the existing movement settings.

## Sampling and protection

Offsets are integer document pixels, limited to ±4096, positive right/down.
Outputs remain canvas-sized. New RGBA space is transparent zero; new raw A8
space is **255, neutral reveal**. Off-frame samples are clipped. Undo restores
them; an inverse move after committing does not recover clipped data.

Linked masks follow their raster even when disabled. Link, enable, density and
numeric feather settings remain unchanged; effective coverage is invalidated
and recomputed rather than baked. Filters, opacity, blending and group gates
stay attached to their original stable owners.

Whole-layer movement honors pixel/position locks and effective visibility. The
existing alpha-lock rule also rejects changed alpha or altered hidden RGB under
zero alpha. Mask-only movement honors its raster owner's position lock and
effective visibility, while remaining independent of that owner's pixel and
alpha locks. An unrelated owner's locks do not restrict selection-outline edits.

Masked movement requires idle Brush **and** Hand contacts. Unsupported group-mask
and selected-pixel requests reject with `mask_editing_pending`; supported moves
reject live contacts with `stroke_active`. Admission happens before implicit
contact completion, native staging or publication. Explicit raster-mask requests
never silently fall back to moving the active layer or a pixel selection.

Complete source storage, logical raw/effective budgets and renderer capabilities
are preflighted, including hidden and disabled branches. Required movement and
raw-A8 sampler callbacks must be available before output allocation. This known
built-in path does not relax generic native geometry permissions or expose a new
mutable host loan.

## One edit and one verified outcome

The host stages at most one RGBA output and one raw A8 output from immutable
borrowed sources. It validates both before publishing either. One compound edit
owns changed raster tiles and the old raw mask plane; it does not invent a
composition snapshot when metadata is unchanged. Undo/Redo restores both domains
together and retains the established active-layer/history behavior.

Unchanged planes keep their existing allocations and content revisions. A changed
mask gets a new content revision, stale leases/effective coverage are invalidated,
and only its byte-tile namespace is marked for replay. Unrelated owners, IDs,
allocators, source filters and the saved transform-repeat recipe remain intact.

Zero offsets or identical outputs add no Undo entry. The existing action audit
and revision rules still apply. Atomic or journal failure restores storage,
metadata, target, history and verification head rather than leaving half a move.

Unmasked event context retains its existing `target`, `dx` and `dy` shape. Masked
layer context adds `layer_id` and `linked_mask_id`; raw-mask context reports
`target: "raster_mask"`, `mask_id` and `owner_layer_id`. Outcomes store verified
RGBA/A8 bytes, so replay never reruns the sampler or requires its DLL.

## Desktop feedback and cost

Tool Options shows the resolved domain/owner and whether a linked mask follows.
The gesture captures document identity, revision and destination IDs at DOWN;
MOVE updates only an outline and integer delta. Release submits one guarded
registered action. Cancellation or stale release of that contact-free masked
outline clears local ownership without ending a later external native contact.
Starting a new mouse/pen contact with a temporary Hand/Pick/Zoom override is a
deliberate takeover: the UI commits the prior native contact before sending the
new DOWN, preserving its paint. This is distinct from cancelling the outline;
raw pointer clients retain their existing explicit sample-phase contract.

Only the affected mask is sampled. The cold output bound is one 4096² RGBA frame
(64 MiB) and one A8 frame (16 MiB), separate from existing bounded history,
effective caches, compositor and replay storage. This is a staging bound, not a
measured total-process peak. No registry discovery, JSON, image copying or native
resampling is added to individual drag MOVE events.

## Verification and remaining work

Focused engine, Session and actual-input regressions cover raw byte references,
locks, active contacts, single Undo, unchanged buffers, checkpoint isolation and
failure rollback. The transport smoke uses independent tiny inverse-coordinate
references alongside JSONL/MCP/GUI parity, projects and DLL-free replay:

```powershell
cargo test --locked -j 1 --lib owner_movement -- --test-threads=1
python scripts/smoke-owner-movement.py --cli <built-cli> --gui <built-desktop>
```

Stage matching native DLLs and follow the local CPU/priority limits in
[Development](development.md). Synthetic pointer tests do not establish physical
tablet compatibility. Full affine/perspective/repeat geometry, group masks,
selected A8 transfers, batches and merges remain separate requirements in the
[mask design](raster-masks-design.md) and [editing plan](photoshop-essentials.md).
