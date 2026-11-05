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

## Image size and resampling

![Aspect-locked Image Size dialog in the actual application](screenshots/image-size.png)

Use **Image > Image Size…** (**Ctrl+Alt+I**, configurable) to scale every layer and both current/Reselect selection masks. Aspect lock is on initially; changing either dimension derives the other from the source ratio, rounded to the nearest integer with half ties upward. Unlock to set dimensions independently. The dialog is inert until Apply: Cancel/Escape/close leaves pixels and history untouched. Reopen it if its document identity or dimensions changed. The configured appearance metrics determine dialog width and gaps. Explicit older shortcut preferences take priority over the new default.

`canvas.image_size` is document-atomic and returns the typed `canvas.state` result. It shares command search, workflow nodes, JSONL, MCP and headless execution. The action accepts:

| Parameter | Contract |
| --- | --- |
| `width`, `height` | Integer pixels, 1–4096. With aspect lock supply **exactly one**; with lock off supply both. |
| `lock_aspect` | Defaults to `true`; derive the omitted dimension from the current source ratio. |
| `method` | `bilinear` (default), `nearest` or `area` (area averaging on shrinking axes, bilinear on enlarging axes). |

```json
{"action":"canvas.image_size","args":{"width":1600,"method":"nearest"}}
{"action":"canvas.image_size","args":{"width":1200,"height":900,"lock_aspect":false,"method":"bilinear"}}
```

Derived dimensions outside 1–4096, missing/ambiguous dimensions, unknown arguments/methods, stale expected revisions and the existing 64 Mi-pixel all-layer budget reject before output allocation/publication. Derived zero or oversized dimensions are rejected rather than silently clamped. An identical size preserves every raw stored word and adds no undo entry.

Sampling maps destination pixel centers into the source. Nearest uses `floor((destination+0.5)*source_size/target_size)` with edge clamping; the selected raw 32-bit word, including hidden RGB, is copied exactly. Bilinear uses the four neighboring source centers with clamped edges and exact rational fixed-point weights. Alpha is interpolated, RGB is weighted by source alpha then unpremultiplied; rounded zero-alpha results have RGB zero. Channel and alpha rounding is half-up. Filtering uses the document's existing encoded 8-bit RGB values, not linear-light/ICC color management. Masks use the same nearest or bilinear coordinates, independently interpolating coverage bytes.

Bilinear smooths enlargement and small reductions but is **not an area/anti-aliasing filter for substantial downscaling**. Choose **Area (Downscale)** to integrate the source footprint of every destination pixel on each shrinking axis. For example, reducing an eight-pixel row to two pixels averages each group of four; a one-pixel white line over black contributes 25% gray instead of disappearing between sample centers. Noninteger ratios use exact overlap fractions. Enlarging or unchanged axes use bilinear centers; mixed-axis scaling combines these rules. The default remains bilinear, retaining prior workflow/action behavior.

Area sampling accumulates alpha and alpha-weighted RGB, rounds only after both axes, then unpremultiplies; coverage masks use the same footprint weights. Filtering remains in encoded RGB, not linear light. A box footprint suppresses aliasing during reductions without negative-lobe ringing, but is not a claim of Photoshop bicubic, Lanczos or ideal frequency rejection. No command promises lossless downsampling; undo retains original buffers. The native kernel reuses two horizontally filtered source rows and one accumulator row, with at most 480 KiB pixel scratch (192 KiB for masks) on x64, rather than a full intermediate image. Old resampling DLLs without the complete advertised area-capability tail reject `area` explicitly; nearest/bilinear and identity sizing remain available.

Hidden and transparency-locked layers participate because this changes document coordinates. Names, IDs, active layer, visibility, opacity, blend/lock flags and image/workflow graphs are retained. Source filters are not baked or scaled: their parameter values remain unchanged and they render against the resampled sources. All masks/outputs are staged and validated before one structural undo entry and verified outcome event. A failed later layer, missing optional DLL capability, oversized compressed mask, atomic batch or partial journal write leaves the previous committed document/history intact. Replay restores outcomes without executing a sampler. See [the runnable example](../examples/automation/image-size.jsonl) and [native extension](native-plugins.md#optional-image-resampling-table).

The shared desktop texture cache checks its own dimensions before partial uploads, including GUI Undo and equal-pixel-count quarter turns. Dimension changes rebuild the surface; ordinary drawing retains dirty-region uploads. This protects Canvas and Navigator even when a command has not performed an adapter-level state sync.

## Automatic trim

![Draggable trim controls beside the visible source and crop guide](screenshots/trim-canvas.png)

Use **Image > Trim Canvas…** to preview removal of uniform outer margins. The command is searchable and bindable, with no default shortcut. Choose transparent pixels or the merged image's top-left/bottom-right corner color, set tolerance (0–255), then choose any nonempty set of Top/Right/Bottom/Left. The draggable dialog opens on the right to keep the central artwork visible. The outline and resulting pixel dimensions update only when parameters change. Apply commits one edit; Cancel/Escape/close leaves the document untouched. A changed document/revision or an intervening contact requires reopening the dialog.

The analysis source is the **current visible merged artwork**, including opacity, blend modes and the image graph, without the display checkerboard. Hidden layers, zero-opacity content and the selection do not enlarge the sampled bounds. Applying the resulting frame crops **all raw layers**, including hidden and alpha-locked ones, and reframes both current/Reselect masks. Undo restores the original stored words, dimensions and masks. Names, IDs, active layer, opacity, blend/lock flags and image/workflow graph identities remain unchanged. Filters retain their parameters and rerender rather than being baked.

Transparent mode removes alpha values at or below tolerance, independent of RGB. Corner modes compare straight RGBA against the selected corner: a pixel matches if every channel differs by at most tolerance. Two zero-alpha pixels match regardless of hidden RGB. Retain the bounding rectangle of every nonmatching pixel, then keep original edges for unchecked directions. Matching pixels inside that rectangle are retained; this does not erase a color or flood-fill a border.

If all pixels match, or selected edges have no removable margin, dimensions/pixels/history remain unchanged. In particular, a uniform opaque image in corner-color mode is an all-matching result, not a claim that its artwork is transparent. MiCoPi never creates a zero-sized document. The GUI disables Apply for these no-ops; explicit automation calls still publish their audit event without adding an undo entry.

| Action | Result |
| --- | --- |
| `canvas.trim_bounds` | Read-only `{x,y,width,height,source_width,source_height,revision,drawing,empty,would_change}` |
| `canvas.trim` | Atomic document framing; typed `canvas.state` result |

Both share optional arguments: `mode` is `transparent` (default), `top_left` or `bottom_right`; `tolerance` defaults to 0; `top`, `right`, `bottom`, `left` default to `true`, with at least one required to remain true. Bounds coordinates use current document pixels. Empty/all-matching analysis returns the original frame with `empty: true` and `would_change: false`. Analysis does not finish a provisional typed contact or increment revision; `drawing: true` identifies such a query. The mutation finishes contact and **recomputes** bounds instead of trusting a previously returned rectangle. Use the standard action envelope's expected revision for concurrency guards.

```json
{"action":"canvas.trim_bounds","args":{"mode":"transparent"}}
{"action":"canvas.trim","args":{"mode":"top_left","tolerance":2,"top":false}}
```

The painting DLL owns the source-pixel scan. The host validates its rectangle, prepares bounded selection/frame buffers and publishes through the existing all-layer structural history. Missing callbacks, failed composition, invalid native rectangles, invalid arguments and stale revisions reject before cropping. Atomic failure restores the previous frame/history; verified replay restores recorded outcomes without rerunning the scanner or compositor. JSONL, MCP, command search and action nodes use the same registrations; editing a workflow does not trim. See [the runnable example](../examples/automation/trim.jsonl) and [native extension](native-plugins.md#optional-trim-analysis-table).

Reveal All is still planned: current layer buffers are bounded by the document frame. Clipped pixels retained for undo are not live off-canvas layer content.
