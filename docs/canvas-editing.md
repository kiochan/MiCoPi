# Canvas size and crop

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
