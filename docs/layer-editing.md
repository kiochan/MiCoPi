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

New documents use a linked image stack: inserting, duplicating, moving and deleting layers updates the applied composition chain. Filters follow stable identities and duplication copies the source layer's filters. Graph and layer blend modes stay synchronized. In legacy/free graphs, image sources keep referring to the same layer after structural edits, but explicit wiring retains its own order and new layers must be connected manually. Deleting a free-graph referenced layer is rejected; remove or retarget its source node first. See [Image Node Editor](node-editor.md).

The final layer cannot be deleted. Existing limits remain 16 layers, 4096 pixels per side, 64 Mi pixels across a document and a 64 MiB undo budget. Linked structural edits also respect the 64-node graph limit. Names must be nonblank, single-line and at most 512 UTF-8 bytes.

## Scope and verification

Implemented layer editing covers stable identities, rename/duplicate/reorder/delete, consolidation and exact pixel transforms. Canvas resize/crop has a separate [geometry contract](canvas-editing.md). Clipboard, masks, arbitrary rotation/scaling, groups and PSD interchange remain future work in the [roadmap](../ROADMAP.md). PhotoCraft's GPU/web frontend and wider module architecture are not adopted.

Regression coverage checks graph remapping, undo/redo pixels, stable ID allocation, failed atomic groups, saved projects, old verified journals and new replay frame parity. Native transform tests compare asymmetric images and overlapping translations with an independent out-of-place oracle. Buffer pointer checks verify reorder and ordinary delete/undo reuse pixel allocations; atomic checkpoints may share removed undo buffers and can require a copy on restoration.

## Transparency lock

`layer.set` accepts optional `alpha_locked` with either the existing index or stable ID selector. It defaults to false. A property change that combines lock, opacity or blend properties creates one composition undo entry. Duplicates inherit their source lock; new layers start unlocked. Locking preserves raw pixel alpha during painting and color processing. Layer removal, history navigation and project replacement remain structural operations. See [Painting tools](painting-tools.md) for behavior, shortcuts and native compatibility.

## Merge, flatten and stamp

The Layers menu, each layer's context menu and Edit menu expose these shared actions. Shortcuts remain configurable in Settings.

| Action | Default shortcut | Result |
| --- | --- | --- |
| `layer.merge_down` | Ctrl+E | Bake the selected layer and its immediate lower neighbor into one normal raster layer, retaining the lower layer's ID/name |
| `layer.merge_visible` | Ctrl+Shift+E | Bake the visible stack, retaining the top visible layer's ID/name and leaving hidden layers intact |
| `layer.flatten` | Unbound | Bake the rendered image into one visible normal layer, removing all other layers and replacing its image graph with a linked raster stack |
| `layer.stamp_visible` | Ctrl+Alt+Shift+E | Copy the current composite into a new top layer with a fresh ID, retaining all source layers |

Merge Down accepts the usual optional `index` or `layer_id`; other actions operate on the document's visible/rendered stack and take no arguments. All accept the action envelope's expected revision. Their schemas are discoverable through `app.describe`, automatically become workflow nodes, and run through JSONL, MCP and GUI attachment. See [the runnable merge example](../examples/automation/layer-merge.jsonl).

Pixel baking uses the existing native compositor; it must be loaded. Rendering and graph validation finish before source layers are replaced. Filters on baked branches are removed once, while hidden/surviving branches retain their filter and node identities. Each operation creates one structural undo entry. History owns the removed buffers; untouched hidden layers retain their allocations. Atomic batches restore pixels, graph, active layer, allocator and history after a later failure. Verified replay restores recorded results without rerunning the compositor.

The supported contracts are explicit:

- Merge Down requires both neighboring layers to be visible and unlocked. A pair above other layers must use Normal blending; backdrop-dependent modes require Merge Visible or Flatten Image. The pair is baked in isolation, so 8-bit alpha quantization can affect subsequent compositing over a lower backdrop.
- Merge Visible retains hidden layers, their locks, raw pixels and filters. With only one visible layer it is a no-op; with none it fails. Its visible result matches the previous composite.
- Flatten Image discards hidden layers and the editable image graph, with undo support. It **preserves canvas transparency** rather than inserting a white Photoshop Background layer. It is the explicit path for baking a free graph.
- Merge Down, Merge Visible and Stamp Visible require linked composition, even when a free graph is disabled. Destructive merges reject alpha-locked source layers; stamp leaves locked sources untouched.
- Stamp keeps sources visible. On a transparent canvas, displaying the stamped composite above its sources can increase opacity; hide the sources when a single baked appearance is desired.

Missing compositors, failed renders, invalid IDs, unsupported graph/blend combinations and layer/pixel limits fail before layer replacement. The existing native ABI and project/replay formats are unchanged.
