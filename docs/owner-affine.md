# Owner and linked Free Transform

Whole raster layers, their linked raw masks and a single explicit raster mask
share an affine Free Transform path. The same `canvas.transform` action serves
desktop controls, workflow nodes, JSONL, MCP and headless clients. This increment
has source and regression coverage; runtime validation is pending CI. The
existing [Free Transform guide](canvas-editing.md#free-transform) describes the
component controls and legacy selection/perspective paths.

## Capture the domain explicitly

| Input | Exact domain |
| --- | --- |
| `layer_ids: [2,1]` | Those one to sixteen unique stable raster IDs in caller order, plus each owner's currently linked mask |
| `mask_id: 7` | Only that raw A8 plane; it must belong to a raster, which need not be active |
| Omitted selectors | Existing target preferences and Auto priority; no implicit raster-link expansion in shared/headless execution |

`layer_ids` permits omitted, `auto` or `layer` target, overrides stored target
preferences and excludes `mask_id`. `mask_id` permits omitted or `auto` target.
Integral JSON numbers such as `2.0` normalize to the same stable IDs. Literal
duplicates, invalid IDs and domain conflicts reject before a declared macro
prefix executes; existence, locks and current ownership validate at execution.

For linked or mask-bearing whole-layer drafts, the desktop captures the active
raster's complete persistent link set once and submits explicit IDs. It does
not widen the draft as links change. A selected painting mask takes Auto priority; an explicit
Layer target chooses the raster domain despite a pixel selection. Explicit
target arrays are also usable without persistent links. Painting remains on the
active raster or explicitly selected mask, independently of these targets.

```json
{"action":"canvas.transform.plan","args":{"layer_ids":[2,1],"angle":15}}
{"action":"canvas.transform","args":{"layer_ids":[2,1],"angle":15,"method":"bilinear"}}
{"action":"canvas.transform","args":{"mask_id":7,"translate_x":2,"method":"nearest"}}
{"action":"canvas.transform.repeat","args":{}}
```

Use actual existing IDs and the normal expected-revision guard when applying a
captured plan. Editing a workflow never executes these actions.
See [the runnable owner-affine example](../examples/automation/owner-affine.jsonl).

## One mapping over raw sources

The control bounds are the union of all targeted raw RGBA pixels with alpha
greater than zero and all carried/explicit raw A8 samples unequal to neutral
reveal **255**. The default pivot is the center of that union. A completely
empty union falls back to the full canvas, so empty rasters and uniform-white
masks still have a usable frame. Bounds do not include hidden RGB, layer
opacity, filters, mask density or feather. Disabled linked masks participate in
the raw union and transform along with their owners.

Every target uses the same forward document-edge matrix, pivot and sampler.
Nearest samples inverse-mapped destination pixel centers with `floor`;
bilinear uses the established Q16, half-up sampling contracts. RGBA interpolation
is premultiplied encoded RGB. Raw A8 interpolates scalar bytes with explicit
per-tap exterior fill. The entire source frame is sampled, not just its control
bounds. Exposed RGBA is zero and exposed raw A8 is **255**. Off-frame data clips;
one Undo restores it. See [Native raw-A8 affine](native-a8-affine.md).

A whole-layer target carries only its owned **linked** mask, including disabled
masks. Owned unlinked masks, group masks and unrelated raw planes stay fixed.
Mask-only transforms move just that plane regardless of its linked flag. Current
and Reselect pixel selections remain fixed and do not clip either whole-owner
domain. Density/feather, enabled/link flags, filters, opacity, blends, hierarchy,
active/selected raster IDs and the paint target retain their identities.

## Admission, history and repetition

Read-only plans may inspect an active Brush or Hand contact without finishing it
or allocating transformed frames. They validate the source, target eligibility,
renderer/effect capabilities, bounds callbacks and required samplers. Owner plans
therefore require the relevant native capabilities; a geometry result is not a
promise that later Apply will succeed. Apply, pixel preview and Repeat require
idle contacts and revalidate before staging.

Every raster target must be effectively visible and honor pixel and position
locks. Its staged RGBA must also satisfy the existing alpha-lock rule, including
hidden words under zero alpha. Mask-only geometry honors the raster owner's
position lock and effective visibility, but ignores its pixel/alpha locks. One
invalid target rejects the complete operation; no subset publishes.

All RGBA/A8 outputs validate before publication. Raw child edits and the saved
repeat recipe share one history group, one Undo/Redo and one verified outcome.
Unchanged planes retain their buffers and do not create fake byte-tile edits.
A nonidentity mapping can still update recipe metadata if samples are equal.
An identity action leaves the prior recipe intact. Native, render, journal and
atomic failures restore the protected document/history state. Outcome replay
restores recorded results without executing samplers or other DLL commands.

An owner recipe stores a strict bounded scope beside the matrix and method:

```json
{"version":1,"layer_ids":[2,1],"mask_ids":[7]}
```

The mask list is the exact owned-linked companion order derived from the raster
array. A mask-only scope instead has empty `layer_ids`, one `mask_ids` entry and
`mask_owner_layer_id`. Repeat reuses these captured raster IDs even if the active
layer or persistent raster links later change. It never silently expands or
retargets. Raster repeats revalidate the exact current linked-mask pairings;
mask-only repeats revalidate the captured owner. Missing IDs or changed pairings
reject without replacing the saved recipe. Each repeat resamples current raw
data with the saved matrix; it is not a non-destructive transform stack.

The scope uses fixed bounded internal storage and preserves the small `Copy`
recipe. Its optional wire field is absent from legacy recipes, retaining their
supported bytes/hashes. New owner metadata requires a reader that understands
the strict owner target; older readers must not silently discard it. Projects,
Undo, atomic checkpoints and verified outcomes retain the scope.

## Preview and cost limits

Gestures update a captured control draft; they do not resample owners for every
MOVE. Idle pixel preview stages the captured targets and composes borrowed
untouched rasters/caches into a temporary surface. It does not publish pixels,
history, recipes or replay events. Apply uses the shared operation and the
captured document/revision guard; stale drafts require reopening. Presentation
commands explicitly report `ui_unavailable` in headless mode.

The output-staging ceiling is **192 MiB**:
`width × height × (4 × raster_count + mask_count)`, checked before allocation.
Uniform inputs reserve possible dense output cost. Preview separately allows at
most **64 MiB** for its composite and **64 MiB** for necessary derived A8
overrides. A preview-budget failure may disable Preview while leaving Apply
available. Existing native feather scratch (up to **32 MiB** plus its bounded
column workspace) is separate. These are logical component limits, not a
measured process-memory peak or a latency guarantee. No source-frame clones or
per-pen-MOVE JSON, registry lookups or link discovery are added.

## Remaining boundaries

Owner scopes support affine controls only. `quad`, projective owner recipes and
Copy Repeat reject; the existing unmasked active-layer/selected-pixel perspective
and copy-repeat paths retain their own contracts. Group-mask geometry,
selected-pixel affine transforms/transfers and selection-outline affine transforms
(`target: "selection_mask"`) on mask-bearing documents remain guarded. Integer
selection-outline Move/nudge remains available and leaves raw owner masks fixed.
A group mask is not a request to transform its subtree. Alignment/distribution
and off-canvas storage remain planned.

Authored engine/Session/UI and native-reference checks are distinct from executed
CI results. No new reviewed screenshot, physical-tablet compatibility or measured
performance result is claimed here. See [Development](development.md) for the
verification workflow and [Layer Links](layer-links.md) for persistent metadata.
