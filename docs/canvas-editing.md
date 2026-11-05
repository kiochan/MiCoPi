# Canvas geometry

Use **Edit > Canvas Size…** (**Ctrl+Alt+C**) to expand or contract the document frame. Enter the new width and height in pixels and select one of nine anchor positions. The anchor determines which edge or corner stays fixed. Added space is transparent; pixels beyond the new frame are clipped. Pixel colors and scale are unchanged.

Use **Edit > Crop Canvas…** to enter an exact rectangle: X/Y locate its top-left corner, and Width/Height specify its size. The canvas shows the rectangle with a thirds guide while the dialog is open. Apply commits one edit; Cancel, Escape and the close button leave the document untouched. A dialog whose source document or dimensions changed must be reopened.

Both operations affect every layer, including hidden and transparency-locked layers. They preserve layer names, stable IDs, active layer, opacity, blend modes, lock flags, image graphs and workflow graphs. The transparency lock governs painting; a document framing operation moves/clips the whole document coordinate system. Undo restores the original dimensions and all clipped stored pixels, including hidden RGB under zero alpha. Redo reapplies the frame.

## Public actions

```json
{"action":"canvas.resize","args":{"width":1600,"height":1200,"anchor":"center"}}
{"action":"canvas.crop","args":{"x":100,"y":80,"width":1000,"height":800}}
```

`canvas.resize` requires width and height, each an integer from 1 to 4096. The optional anchor defaults to `center` and accepts `top_left`, `top`, `top_right`, `left`, `center`, `right`, `bottom_left`, `bottom` or `bottom_right`. Centered offsets round toward negative infinity: enlarging 12 pixels to 17 adds 2 pixels on the left and 3 on the right; shrinking 12 to 7 removes 3 on the left and 2 on the right.

`canvas.crop` requires integer x, y, width and height. Its nonempty rectangle must be entirely inside the existing canvas. Both actions enforce the document budget of 64 million pixels across all layers. Invalid bounds, dimensions, anchors and stale expected revisions fail without changing the committed document. Identical framing adds no undo entry.

The actions are discoverable in command search, schema-derived workflow nodes, JSONL and MCP. They support document-atomic pipelines, expected revisions, project persistence and verified outcome replay. A failed atomic group restores dimensions, pixels, graphs and prior history; playback applies recorded results without rerunning commands. Existing recording and SDK formats are unchanged.

## Implementation and verification

Framing is a host-owned document structure operation. It copies retained row slices without resampling, blending or invoking a pixel filter. Existing layer buffers move into a shared undo record; undo/redo exchanges dimensioned buffer sets. Unshared sets move directly; a transaction checkpoint retaining a saved set requires a copy when that set becomes editable again. Checkpoints share saved sets without eagerly cloning their pixels. History accounts for retained pixel buffers under the existing retention policy. Geometry changes invalidate the complete render surface and replay tile grid; ordinary strokes retain tiled history and their typed input path. No dependency or native ABI change is needed.

Regression coverage checks every anchor with odd expansion/contraction, exact per-layer pixel oracles, hidden/locked layers, linked filters, texture dimensions, cancellation, stale dialogs, atomic failure, project reopen and replay. Runtime smoke compares JSONL, MCP and GUI output and checks geometry with linked graphs. The feature scope was informed by PhotoCraft's [everyday document editing tools](https://github.com/storytold/photocraft#-the-everyday-essentials); implementation follows MiCoPi's document/history boundaries.

## Exact rotation and flips

![Document quarter turn with transformed selection and navigator](screenshots/canvas-quarter-turn.png)

The **Image** menu offers Canvas Size/Crop alongside these document-atomic actions. They take an empty argument object and return the typed `canvas.state` result. Command search, configurable shortcuts (unbound by default), workflow nodes, JSONL, MCP and headless execution share the same registrations.

| Action | Operation |
| --- | --- |
| `canvas.rotate_90_cw` | Clockwise quarter turn; swap width/height |
| `canvas.rotate_90_ccw` | Counterclockwise quarter turn; swap width/height |
| `canvas.rotate_180` | Half turn; keep dimensions |
| `canvas.flip_horizontal` | Mirror left/right |
| `canvas.flip_vertical` | Mirror top/bottom |

Every raw layer participates, including hidden and transparency-locked layers. Current and Reselect masks follow the same permutation with exact fractional coverage. Stored 32-bit pixels, including hidden RGB under zero alpha, are permuted without resampling or blending. Layer IDs, names, locks, opacity, blend modes, active index and image/workflow graphs remain unchanged; filters render from transformed raw sources rather than being baked. A free image graph is retained as-is. These are document edits, distinct from the planned non-destructive view rotation.

The painting DLL's optional `micopi_geometry_v1` table supplies tiled, allocation-free pixel/mask kernels. The host stages validated masks and all layer outputs before publishing one dimension-aware undo entry. A mask exceeding the existing compressed selection budget rejects before publication. Identical output creates no undo entry. Atomic or journal failure restores dimensions, masks, pixels and history. Outcome replay applies saved buffers without invoking geometry DLLs; existing projects, recording hashes and ABI prefixes remain supported. No work is added to pen MOVE.

`layer.rotate_90_cw` and `layer.rotate_90_ccw` are also available in Layers actions/context menus. They operate on the visible active layer, preserve canvas dimensions and keep masks fixed. Rotate the full raw layer, center its swapped rectangle in the existing frame with offsets `floor((width-height)/2)` and `floor((height-width)/2)`, clear uncovered destination pixels to transparent, then blend the destination through the current selection. Thus the selection clips **where the result is written**, not a separately rotated selection bounding box. Alpha lock rejects changes to transparency or hidden RGB under zero alpha. Opaque square-layer color permutations remain permitted.

MiCoPi currently stores each layer within the document frame: layer quarter turns **clip out-of-frame pixels**. Undo restores them, but a reverse quarter turn after clipping cannot recover them. Use document rotation to retain all content. Off-canvas storage/Reveal All and interactive pivot/free transform remain planned. See [the runnable geometry example](../examples/automation/geometry.jsonl).

The Image menu uses intrinsic label/keycap layout and configured spacing so long English/CJK titles do not overlap shortcut badges. Tests cover asymmetric and strided buffers, tile boundaries, fractional masks, missing/truncated tables, compressed-mask limits, locks, structural identity, undo/redo, atomic/journal failure, inert workflow editing, project reopening and verified outcomes.
