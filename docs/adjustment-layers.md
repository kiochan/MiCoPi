# Adjustment layers

Adjustment layers are real non-raster stack items: they filter the accumulated
image below their position without changing source pixels or occupying a raster
slot. This source increment supports the five registered scalar filters
**Invert, Grayscale, Brightness, Contrast and Saturation** plus typed **Levels and Curves**, Normal effect mixing,
byte opacity, visibility, groups and optional owned A8 masks. Runtime validation
is pending CI; no new reviewed screenshots or measured performance results are
claimed here.

This is distinct from a source Layer Filter, which processes one raster before
stack composition. Additional multi-parameter color adjustments, clipping
relationships and other adjustment blend modes remain planned. Registering a
schema does not provide an algorithm that the compositor does not implement.

## Affect the accumulated image

At an adjustment, let `B` be the running composite below it and
`A = Filter(B, parameters)`. Normal adjustment opacity interpolates the change:

```text
lower siblings --> B --------------------> Mix / MaskedMix --> next sibling
                   \--> registered Filter --> A      ^
                                              opacity / owned mask
```

Without a mask, the generated graph uses premultiplied `Mix(B,A,opacity)`.
With an enabled mask, it uses `MaskedMix(B,A,coverage,opacity)`, combining byte
opacity and effective A8 coverage once with the existing half-up rules. Disabled
masks reveal the full adjustment. Density and feather change effective coverage,
not raw samples. Zero coverage or zero opacity returns `B`; full strength returns
`A`. The supported filters, Levels and Curves retain input alpha, so the adjustment retains the backdrop's
alpha and empty transparency stays empty. Source-over blending a filtered copy
would increase partial alpha and is not this operation.

Scope follows stack order and the existing group seed:

- At root, adjust the complete accumulated lower root siblings. Higher siblings
  are unaffected.
- In an Isolated group, adjust only accumulated children inside its transparent
  seed. The group's own mask, opacity and blend apply afterward once.
- In Pass Through, the seed includes the parent backdrop. An adjustment can
  affect artwork below/outside that folder, contained by the nearest Isolated
  ancestor. The group's Mix/MaskedMix fades the complete change once.

The global Node Compositing toggle bypasses both source Layer Filters and
adjustments while retaining group opacity, masks and composition policies.
Stored definitions remain present. Hidden or globally bypassed definitions still
receive complete parameter, renderer-capability and generated-node-budget
preflight; hiding an unsupported definition cannot make it safe to publish.

## Inspect independently of painting

Use **New Adjustment** in Layers and choose a registered filter. Its optional
**Create Mask from Selection** captures the current selection once into a real
owned A8 plane; later selection edits do not alter that mask. The live selection
is not an implicit composition input.

The mixed stack displays adjustments beside rasters and folders. Inspecting an
adjustment opens its local property draft while retaining the active raster
painting layer and raster multi-selection. Apply submits one shared command;
Cancel discards the draft. A dirty draft uses its captured source revision and
requires refresh after an external edit. **Inspect Painting Layer** returns to
raster inspection. Brush/Eraser can explicitly target an adjustment's mask using
the existing pressure/stabilization and mask-target contracts. Selecting an
adjustment row alone does not redirect pixel painting.

The generated **Actual Composition** contains the affect-below Filter and
Mix/MaskedMix chain. Editable **Source Layer Filters** remain raster-owned
branches; generated adjustment nodes are not submitted as the source graph.

## Shared actions and registered schemas

| Contract | Purpose |
| --- | --- |
| `adjustment.catalog` | Read registered filter aliases, parameter schemas and defaults |
| `adjustment.query`, state `adjustments.state` | Inspect current adjustments, including their placement |
| `adjustment.get` | Inspect an explicit stable `adjustment_id` |
| `adjustment.create` | Insert one validated effect at `parent_id`/`index`, optionally snapshotting a selection into its owned mask |
| `adjustment.set` | Apply properties/parameters to an explicit adjustment |
| `adjustment.move` | Move an explicit adjustment to a parent and remaining-child index |
| `adjustment.delete` | Remove one adjustment and only its owned mask |
| `adjustment.inspect` | Queue the desktop inspector without changing document content; unavailable headless |

For example, place an Invert adjustment above the first root raster in a new
canvas, then inspect the shared state:

```json
{"action":"adjustment.create","args":{"node_type":"compositing.filter","parameters":{"filter":"invert","amount":1},"parent_id":0,"index":1}}
{"action":"adjustment.query","args":{}}
```

Use existing IDs and valid placement in an established document. Stack indices
count mixed sibling items in bottom-to-top order; public rows are displayed
top-first. Mutators expose stable result IDs and revision for typed follow-ups.
Inspector requests return a queued presentation result, not an applied edit.
Document actions/state serve desktop, nodes, JSONL, MCP and headless through the
same Session. Workflow editing remains inert until Run.

The `adjustments` package depends on `layers` and `compositing`. Effects reference
registered image Filter, Levels or Curves aliases through `node_type`. Scalar filters use
typed `filter` and `amount`; Levels uses the registered nested `parameters` object.
Registry schema validation checks alias ownership/binding and its
allowed parameters, including hidden/bypassed definitions on project publication.
Engine validation independently bounds finite amounts: Invert/Grayscale use
0–1, the other three filters use −1–1. Projects store validated definitions,
not callbacks, executable schemas or per-pixel JSON actions.

Levels accepts the same five parameters as a [Levels source node](levels.md),
but operates on the accumulated image below the adjustment. Partial nested
parameter patches merge with the selected provider's defaults/current values;
changing providers resets to that provider's complete defaults. Its dedicated
Input/Output controls remain local until Apply, with the same stale/contact guards.

```json
{"action":"adjustment.create","args":{"node_type":"compositing.levels","parameters":{"parameters":{"gamma":2}}}}
{"action":"adjustment.set","args":{"adjustment_id":1,"parameters":{"parameters":{"input_black":16,"input_white":240}}}}
```

Levels preserves input alpha and uses the existing Normal Mix/MaskedMix strength
once, including Isolated/Pass Through scopes and owned A8 masks. Hidden or globally
bypassed declarations still require the renderer capability before publication.
Derived tables remain in prepared plans; typed pointer MOVE does not prepare them.

Curves accepts the same master/R/G/B object as a [Curves source node](curves.md),
using `parameters.curves`. Apply the master first and then each channel, preserving
alpha; Normal opacity and owned A8 coverage mix the effect once over the backdrop.
Root, Isolated and Pass Through follow the same scope as other adjustments.
The built-in inspector shares the [local curve controls](curves.md#local-curve-controls);
custom aliases retain their schema editor. Shared manual [histogram controls](histograms.md)
show explicit published composite/raw snapshots and hide stale/contact overlays;
they do not sample local drafts or arbitrary adjustment inputs. Partial channel patches preserve other channels and interpolation;
a supplied `points` array replaces the complete point list. Point objects remain
complete (`input` and `output`), with ordered-byte and provider validation.

```json
{"action":"adjustment.create","args":{"node_type":"compositing.curves","parameters":{"curves":{"red":{"points":[{"input":0,"output":255},{"input":255,"output":0}]}}}}}
{"action":"adjustment.set","args":{"adjustment_id":1,"parameters":{"curves":{"master":{"interpolation":"linear"}}}}}
```

Version-3 adjustment records have optional `levels` or `curves` intent tails.
Both are omitted for scalar effects, retaining their old bytes. Typed effects
require neutral legacy `filter=invert, amount=0` compatibility anchors. Multiple
effect tails, non-neutral anchors, invalid curves and mismatched provider records
reject. Old strict readers reject unknown tails. Public action/state parameters
expose only the selected provider's real contract. Prepared channel tables remain
derived cache data; they are not serialized into document or replay intent.

## History, storage and compatibility

At most sixteen adjustment records share the existing depth-eight mixed tree
and complete 64-node generated-plan limit. The node limit can reject a smaller
adjustment count when rasters, filters, groups and mask gates consume capacity.
No transparent full-canvas RGBA buffer is allocated for an adjustment. Property
changes may still damage and record the complete composite; non-destructive
source editing is not a promise of zero memory or render cost.

Create/set/move/delete require idle Brush/Hand contacts and validate the source
and candidate before publication. Optional selection capture preflights mask
counts/logical byte-plane capacity before copying coverage. One Apply publishes
one metadata Undo/revision/outcome; owned-mask creation/removal shares that edit.
Unchanged raster/raw-mask buffers remain attached to their owners. Group creation
and hierarchy moves retain adjustment items; Group Duplicate creates fresh
adjustment/mask IDs with independent raw storage, and Group Delete consumes only
its subtree owners. A removed mask falls back to Pixels only when it was the
paint target; unrelated explicit targets remain selected.

Strict **GroupTree version 3** stores tagged adjustment items, ordered records
and `next_adjustment_id`. Raster DFS projection still matches raster storage
exactly; active painting and raster IDs do not become adjustment IDs. Committed
Undo retains monotonic identity allocation. After deleting all adjustments or
undoing the first creation, a version-3 flat/hierarchical floor retains the
allocator and forces older readers to reject unsupported semantics. A document
that never used adjustments retains its legacy omitted fields/bytes. A floor
alone uses the existing flat composition path and adds no effect processing.

Adjustment-owned masks promote **mask metadata version 2**, retaining that floor
through committed Undo. Raw planes, MICOPI04 byte encoding and event
`mask_version: 1` remain unchanged: owner metadata version and A8 codec version
are separate contracts. Exact atomic/checkpoint and verified-state restoration
restore the incoming floors/counters, including pre-feature metadata. Project
load validates strict ownership and tree shape. Verified replay restores recorded
raw samples and surfaces without invoking a compositor or effect DLL.

## Remaining boundaries and verification

While actual adjustments exist, generic baking/merging, ungrouping, unsupported
layer batches, clipboard/selected-pixel transfers and free-graph conversion stay
guarded. Retained allocator-only floors do not disable the old operations.
Adjustment-mask Move/Free Transform and mask Apply are unsupported; mask painting
and document-wide all-plane geometry retain their own contracts. A group mask is
not an adjustment's effect scope, and a raster link set never widens painting.

Meaningful engine/Session/UI and independent transport checks accompany the
source increment: partial-alpha witnesses, root/Isolated/Pass Through order,
mask/selection independence, global bypass, full-definition admission, strict
floors, subgroup copy/delete, compound history, rollback, projects and DLL-free
outcomes. Execution status belongs to the exact commit's CI result. Physical
tablet compatibility, reviewed captures and latency measurements remain separate
evidence. See [Architecture](architecture.md), [Raster Masks](raster-mask-editing.md)
and [Roadmap](../ROADMAP.md).
