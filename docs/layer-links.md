# Linked layers and integer movement

Layer links keep related ink, color and shading rasters together during Move
without changing where Brush or Eraser paints. They are document relationships,
separate from multi-selection, folders and image-graph connections.

Select several layers and use **Link Selected** in Layers. The chain indicator
shows membership. **Unlink Selected** removes those layers while retaining any
remaining group of two or more. Linking members of existing sets joins those
sets; unrelated sets stay independent. Painting remains on the active raster or
its explicitly selected mask.

## Move a linked set

For a whole-layer Move (V), the desktop captures the active raster's complete
link set at contact start. Dragging changes only the position outline. Release
submits the captured stable IDs and source revision to the shared action. Arrow
nudges use the same whole-layer rules and configured step sizes.

Auto retains its existing priority: a selected painting mask, then selected
pixels, then the active raster. Mask-only and selected-pixel movement do not
expand layer links. An explicit Layer target moves the raster link set even
while a pixel selection exists. Linked Free Transform, align/distribute and
other linked geometric operations are not implemented by this increment.

Every moved raster carries only its owned **linked** raw A8 mask, including a
disabled mask. Unlinked, group and unrelated masks stay fixed. Both current and
Reselect pixel selections, active/selected raster IDs, paint target, filters,
opacity, blend modes and transform-repeat recipe stay attached to their existing
identities. Exposed RGBA is transparent; exposed raw A8 is 255, neutral reveal.
The canvas clips off-frame data; one Undo restores it.

## Shared commands and public state

```json
{"action":"layer.links.set","args":{"layer_ids":[1,3],"linked":true}}
{"action":"layer.links.query","args":{}}
{"action":"canvas.move","args":{"layer_ids":[1,3],"dx":4,"dy":-2}}
{"action":"canvas.nudge","args":{"layer_ids":[1,3],"direction":"right","large":true}}
{"action":"layer.links.set","args":{"layer_ids":[3],"linked":false}}
```

Supply actual existing IDs. Link creation needs at least two unique raster IDs;
unlinking accepts one or more. JSON numeric integral IDs such as `3.0` follow the
existing selector contract. Repeated links or unlinking an unlinked layer do not
create an Undo entry; existing action audit/revision rules still apply.

An explicit `layer_ids` Move/nudge addresses exactly those one to sixteen
rasters. It does not expand persistent links, select targets or require that
they form a stored set. Omitted, Auto or Layer target is allowed and overrides
stored target preferences. `mask_id`, Selected Pixels and Selection Outline
conflict with this input. Duplicate/invalid literal IDs and domain conflicts
reject before a declared macro prefix executes; existence and target locks are
checked when the operation executes.

The desktop resolves its link set once and supplies the explicit array. Clients
can instead choose any exact target array. Omitting `layer_ids` keeps the
existing single-owner API behavior; callers do not acquire a wider mutation
merely because another client changed link metadata.

`canvas.state` and `layer.query` expose canonical `layer_links`. Each returned
raster has `linked_layer_ids`, empty when unlinked. Read-only
`layer.links.query` and plugin state **`layers.links`** additionally expose
`active_linked_layer_ids`: the complete active set, or `[active_layer_id]` for
an unlinked raster. Its declared array port can bind directly to a Move node's
`layer_ids`. Editing a workflow remains inert; running it uses the same Session
handler as desktop, JSONL and MCP. Action/state IDs and editable IDs are never
translated.

## Admission, history and bounded staging

Link changes and explicit-set movement require idle Brush and Hand contacts.
Controls/forms must not complete a contact before shared admission. Cancelling
a contact-free linked outline leaves a later external native contact alive;
deliberate temporary Hand takeover commits the earlier paint before a new Down.
Pressure, stabilization and the single-target typed painting path are unchanged.

Before native output allocation, movement validates every explicit target,
pixel/position/alpha locks, effective visibility, complete source mask storage,
renderer/effect capabilities, sampler availability and aggregate output cost.
One protected owner rejects the complete set. It never silently moves a subset.
The existing alpha-lock rule also protects hidden RGB under zero alpha.

The native kernels sample borrowed sources without changing the active layer.
Every RGBA/A8 result is staged and validated before publication. One existing
history group holds raw child edits, retaining separate raster and byte-tile
namespaces. One Undo/Redo and one verified outcome cover the complete move;
atomic, native, render or journal failure restores the prior complete state.
Equal outputs do not add child history entries. Event context records explicit
`layer_ids`, companion `linked_mask_ids` and offsets with target `layers`.

The fixed staging ceiling is **192 MiB**, checked before the first allocation:
`width × height × (4 × raster_count + linked_mask_count)`. Uniform source masks
still reserve their possible dense result cost. This is an output-staging
ceiling, separate from source storage, bounded history, effective caches,
composite and replay buffers; it is not a measured process-memory peak. No JSON,
image copies, sampling or link discovery is added to drag Move samples or the
typed brush Move path. No dependency or native ABI change is required.

## Persistence and structural edits

Each linked raster stores strict optional metadata:

```json
{"links":{"version":1,"layer_ids":[1,3]}}
```

Every member carries the same sorted complete list, including itself. Validation
requires reciprocal, disjoint, bounded membership of existing raster IDs. The
maximum duplicated ID payload is 2 KiB at sixteen layers. Malformed or unknown
versions reject on import; they are not silently repaired.

Reorder, rename, properties and group hierarchy moves retain relationships by
stable ID. Deleting or merging consumes only removed members and drops residual
singletons. A retained original raster ID can retain its surviving set; fresh
replacement rasters and all copies start unlinked. New Canvas starts with no
links. Undo, projects, atomic checkpoints and verified outcomes retain metadata.

Unlinked metadata omits the field, preserving supported legacy document bytes.
Older hosts with strict layer metadata reject linked documents rather than
discarding the relationship. The new host reads supported old projects and
recordings; outcome restoration requires no algorithm DLL. Compatibility with a
new linked document is distinct from compatibility with an unlinked legacy file.

## Verification

Engine, Session and actual-input tests accompany this feature, including exact
inverse-coordinate pixel references, all-target failure, compound Undo, contact
continuity, independent painting and four-language narrow/wide layout. CI runs
complete desktop/headless and transport checks; pending results are not passing
evidence. Synthetic input does not establish physical tablet compatibility.
Reviewed native screenshots and large-image latency remain separate checks.
