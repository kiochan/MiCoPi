# Layer editing

The first editing increment draws on [PhotoCraft](https://github.com/storytold/photocraft)'s layer organization and exact transform workflows. It is independently implemented through MiCoPi's document, action and native plugin contracts. No PhotoCraft code or dependencies are imported. MiCoPi keeps its typed pen path, CPU/headless operation, host-owned history and explicit workflow execution.

## Desktop

The Layers panel provides an editable **Layer Name**, **Layer Actions** menu and per-row context menus. **Ctrl+J** duplicates the current layer directly above its source. Up means toward the top of the visible stack. Reordering preserves the active layer by identity. A duplicated layer becomes active; deleting the active layer selects its nearest remaining neighbor. Custom names are document content and stay unchanged when the UI language changes.

Flips, 180° rotation and offsets operate on the selected visible layer's full canvas-sized pixel buffer. Hidden layers must be made visible before a pixel edit, following the existing plugin edit contract. Offsets are signed integer pixels: positive X is right, positive Y is down. Exposed areas become transparent; out-of-bounds pixels are discarded and can be restored with undo. These operations do not resample colors. The native painting plugin must be enabled for pixel transforms.

## Public actions

`canvas.state.layers` exposes `id`, `name` and existing properties. Layer IDs are scoped to one document and remain stable across reorder and save/reopen. A new document starts its own identity space. IDs are never a global document selector.

| Action | Arguments |
| --- | --- |
| `layer.rename` | `name`, optional `layer_id` or `index` |
| `layer.duplicate` | optional `layer_id` or `index` |
| `layer.move` | `to` (zero-based destination), optional `layer_id` or `index` |
| `layer.delete` | optional `layer_id` or `index` |
| `layer.select`, `layer.set` | accept `layer_id` alongside the existing `index` selector |
| `layer.flip_horizontal`, `layer.flip_vertical`, `layer.rotate_180` | no arguments; current layer |
| `layer.translate` | required integers `dx`, `dy`, each from -4096 to 4096; current layer |

Omitting both selectors targets the active layer. If both are supplied they must identify the same layer. Existing index-based clients remain supported; automation that retains a target across structural edits should use `layer_id`. To target a transform by identity, use an atomic `pipeline.run` with `layer.select` followed by the transform. All listed mutations support undo, outcome replay and atomic document pipelines, and are discoverable by the same schemas in CLI, MCP and workflow nodes.

Example JSONL (IDs here belong to a freshly created document):

```json
{"action":"canvas.new","args":{"width":640,"height":480}}
{"action":"layer.rename","args":{"layer_id":1,"name":"Ink"}}
{"action":"layer.duplicate","args":{"layer_id":1}}
{"action":"layer.move","args":{"layer_id":1,"to":1}}
{"action":"layer.translate","args":{"dx":12,"dy":-8}}
```

Image-node sources keep referring to the same layer when another layer is inserted, moved or removed. Deleting a referenced layer is rejected; remove or retarget its source node first. The final layer cannot be deleted. Existing limits remain 16 layers, 4096 pixels per side, 64 Mi pixels across a document and a 64 MiB undo budget. Names must be nonblank, single-line and at most 512 UTF-8 bytes.

## Scope and verification

This increment covers stable identities, layer rename/duplicate/reorder/delete and exact pixel transforms. Selections, masks, arbitrary rotation/scaling, canvas resize/crop, groups, merge and PSD interchange remain future work in the [roadmap](../ROADMAP.md). PhotoCraft's GPU/web frontend and wider module architecture are not adopted.

Regression coverage checks graph remapping, undo/redo pixels, stable ID allocation, failed atomic groups, saved projects, old verified journals and new replay frame parity. Native transform tests compare asymmetric images and overlapping translations with an independent out-of-place oracle. Buffer pointer checks verify reorder and ordinary delete/undo reuse pixel allocations; atomic checkpoints may share removed undo buffers and can require a copy on restoration.
