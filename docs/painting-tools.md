# Painting dynamics, fill and gradients

The Brush panel places **Opacity** and **Flow** together. **Limit opacity per
stroke** keeps one continuous stroke within its opacity limit while Flow controls
how quickly it builds; lift the pen for another coat. **Glazing Marker** selects
40% Opacity, 25% Flow and this mode. Default **Build Up** with Flow 100% preserves
the existing per-dab pixels. See [Brush opacity and Flow](stroke-opacity.md) for
pressure, selection, storage bounds and compatibility. These controls apply to
Brush/Eraser contacts; fill and gradients retain their own opacity semantics.

The Brush panel exposes pressure size/opacity, minimum size and pressure gamma under **Pressure Dynamics**. A response curve previews gamma; 1 is linear, smaller values respond more strongly to light pressure. Minimum size is a fraction of the selected radius. These controls use existing `brush.set` parameters and the typed native pen path. Mouse input ignores pressure; zero-pressure pen samples deposit no paint. Controls change the current Session; use Painting settings to save startup defaults.

[Analytic tips](brush-tips.md) add Round/Square shape, 25–100% roundness and a
fixed clockwise angle shared by Brush/Eraser, the nominal cursor/preview,
searchable defaults and user presets. A narrowed round tip forms an ellipse;
a narrowed square forms a rectangle. Pressure scales both axes. This source
increment and its synthetic cost probe are pending CI; sampled/textured tips
and tilt/rotation dynamics remain planned.

**Dab Spacing** controls stamp distance as a fraction of brush diameter: 0.01–2 (1–200%), with the original 0.11 (11%) default. Larger spacing creates separated marks and reduces stamp callbacks, trading continuous coverage for a different brush appearance. **Minimum Opacity** under Pressure Dynamics retains a fraction of brush opacity at light pressure when pressure opacity is enabled. The response becomes `minimum + (1 - minimum) × pressure^gamma`; zero pressure still deposits nothing. Mouse strokes ignore this floor. Both controls support `brush.set`, project replay metadata and startup settings as `painting.spacing` and `painting.minimum_opacity`.

```json
{"action":"brush.set","args":{"spacing":0.25,"pressure_opacity":true,"minimum_opacity":0.2}}
```

Options are copied into the native contact at DOWN, without extra configuration calls or JSON on MOVE. The optional `micopi_tool_options_v1` extension leaves existing tool tables and parameter sizes unchanged. Older DLLs accept default settings; unsupported custom values fail explicitly. New DLLs use legacy defaults with an older host. Default spacing, minimum opacity, Flow and mode fields are omitted from replay parameter metadata to preserve old recording hashes; public `canvas.state.parameters` and `painting.tool.parameters` report them explicitly. Nondefault recordings require a host that understands the new parameters. Settings stores the new startup defaults as `painting.flow` and `painting.opacity_mode` (`dab` / `stroke`).

Choose **Flood Fill** in the Brush panel, set **Fill Tolerance** and click a pixel. The operation fills the four-connected region on the current visible layer, using the foreground color and brush opacity with source-over blending. Clear **Contiguous Fill** to fill every seed-matching pixel across the current layer, including disconnected regions. The default remains four-connected fill. It samples this layer, rather than the merged canvas. Tolerance is 0–255, compared with the seed in premultiplied 8-bit RGB and alpha; transparent pixels with different hidden RGB therefore belong to the same region at tolerance zero. Higher tolerance compares every candidate with the original seed, rather than accumulating differences along a path.

**Fill Expansion** accepts -32 to 32 pixels. Positive values grow the selected region into neighboring pixels; negative values shrink it. **Fill Feather** accepts 0–32 pixels and softens the resulting boundary with a linear alpha ramp on both sides. Expansion/contraction is applied first, then feathering uses the final region, so merged regions have no internal feather seam and fully contracted regions remain empty. Both defaults are zero. Bounds use eight-neighbor pixel steps (square neighborhoods); the outside of the canvas is unselected, so contraction and feathering also affect canvas edges. Expansion may recolor adjacent linework on the current layer. These controls shape the selected region; tolerance and connected-region discovery still use the original layer pixels.

Choose **Gradient** and drag on the canvas. Release commits one edit across the current visible layer. Foreground/background colors define its ends; brush opacity scales both alphas. **Radial Gradient** uses the start as its center and drag length as its radius. Linear gradients project onto the drag direction. Both blend in premultiplied alpha to avoid transparent color fringes; interpolation uses encoded RGB, without a linear-light color-management conversion. A line previews the drag direction. Escape, focus loss, pen cancellation or switching tools cancels the preview. A changed document, active layer or revision during a drag also prevents committing. Undo restores the previous pixels.

**Extend End Colors** retains the default clamping outside the drag endpoints. **Repeat Gradient** repeats once per drag length, including negative linear projections; its exact period boundary restarts at the first color. **Reflect Gradient** alternates direction on each period. For radial gradients these produce repeated or mirrored rings. **Transparent Gradient End** sets the original background endpoint alpha to zero; **Reverse Gradient** then swaps both endpoint colors and alphas. Hold **Shift** while dragging or releasing to snap direction to 45° increments while preserving drag length. A zero-length gesture remains a no-op.

Selecting a normal tool returns to its regular input behavior; temporary hand, picker and zoom modifiers remain available. These GUI modes adapt gestures into public plugin actions and do not change the tool ABI. Attached clients can select their GUI mode using `ui.dispatch` with `id: "painting.mode"` and `value: "brush"`, `"fill"` or `"gradient"`.

## Preserve painted transparency and fill colors

Enable **Lock Layer Transparency** in the Layers panel to recolor existing marks while retaining each pixel's alpha. Brush, connected/global fill, feathered fill and gradients use source-atop color blending while locked. Fully transparent pixels retain their stored values. Layer opacity, visibility and blending controls continue to work. Erasing and clearing reject a locked layer; plugin edits and transforms that change its per-pixel alpha also fail without committing pixels.

The layer menu provides **Fill with Foreground** and **Fill with Background**. **Alt+Backspace** fills the current layer with the foreground color; **Ctrl+Backspace** uses the background color. Both use current brush opacity. In an unlocked layer this fills across the canvas; in a locked layer it recolors only existing coverage. Global shortcuts remain inactive while editing text. The public `layer.fill_color` action requires `color` (`#RRGGBB` or `#RRGGBBAA`) and accepts `opacity` from 0 to 1, defaulting to 1.

```json
{"action":"layer.set","args":{"alpha_locked":true}}
{"action":"layer.fill_color","args":{"color":"#ee7744","opacity":0.7}}
```

Lock state is part of the layer's document metadata, follows its stable ID through reorder/duplicate and supports undo, atomic groups, projects and verified replay. `canvas.state.layers` always reports `alpha_locked`. False values are omitted from recorded layer metadata, preserving older recording hashes. Locked painting requires a tool DLL advertising the optional alpha-lock capability; older DLLs continue to paint unlocked layers and fail explicitly for locked painting.

## Public actions

`layer.fill` and `layer.gradient` are supplied by the native painting plugin. They target the current layer, support expected revisions, workflow nodes, atomic pipelines, tiled undo and outcome replay. To retain a target across structural edits, atomically select its stable `layer_id` before the action. Hidden layers reject pixel edits. Algorithms never execute during verified replay.

```json
{"action":"layer.fill","args":{"x":10,"y":20,"color":"#ff6600","tolerance":8,"opacity":0.8,"expand":1,"feather":2}}
{"action":"layer.gradient","args":{"x0":0.5,"y0":0.5,"x1":100.5,"y1":0.5,"start_color":"#ff0000ff","end_color":"#0000ff00","kind":"linear","opacity":1}}
```

Fill requires integer `x`, `y` inside the canvas and `color`; tolerance defaults to zero and optional `contiguous` defaults to `true`. Optional integer `expand` is -32–32 and integer `feather` is 0–32, both defaulting to 0. Set it to `false` for whole-layer matching. Gradients require finite `x0`, `y0`, `x1`, `y1` between -4096 and 8192, and both colors. Endpoints closer than 0.001 pixels are rejected. `kind` defaults to `linear` and also accepts `radial`. Optional `spread` is `pad` (default), `repeat` or `reflect`; `reverse` defaults to `false` and swaps endpoint colors/alphas. Colors accept `#RRGGBB` or `#RRGGBBAA`; opacity defaults to 1. Pixel centers are at half-integer coordinates.

## Performance and limits

Contiguous fill uses iterative scanline spans, a one-bit-per-pixel visited map and a span stack. It avoids recursive call depth and per-pixel queue records. Spans contain three 16-bit values; total queued records are bounded by discovered runs. Whole-layer matching visits each pixel once without a visited bitmap or span stack. Gradients write in place without a separate gradient image. Both still use the host's existing full-region staging buffer, atomic checkpoint, touched-tile undo and journal validation; these costs are not eliminated. Maximum canvas dimensions remain 4096 per side, with existing document/history limits.

Nonzero edge controls build a one-bit-per-pixel region mask and reuse one byte per pixel for capped boundary distances. The temporary scanline stack is released before allocating distances. Expansion and feathering use linear scans independent of the chosen radius, with up to two distance passes. No additional full-color image is allocated. The default zero-edge path keeps the existing connected/global fill kernels. Repeating an identical translucent fill correctly accumulates alpha through source-over blending; identical opaque fills can still return early.

Default circular brush rasterization rejects pixels outside the dab circle before computing distance, and avoids distance evaluation inside solid hard dabs. Antialiased boundaries retain the previous distance formula. Analytic nondefault tips use their own bounded rotated regions and row-span culling; their coverage and pending measurement scope are described in the [tip guide](brush-tips.md). Legacy spacing, pressure, stabilization and the typed MOVE path remain unchanged at default options. No additional dependency is introduced. Exact DLL-versus-reference stroke tests protect existing pixels. A deterministic 100-pixel line at radius 5 uses 11 stamp callbacks at 100% spacing, versus more than 80 at default spacing; this measures work reduction for sparse brushes, rather than end-to-end input latency.

Regression checks include an independent breadth-first fill oracle, tolerance/connectivity/transparency fixtures, gradient endpoints and alpha fringes, gesture commit/cancel behavior, undo, atomic failure, replay parity and PNG comparisons across JSONL, MCP and GUI. Fill gap closing, merged-layer fill sampling, textured brush tips and arbitrary editable pressure curves remain future work. New stroke-opacity coverage is authored and pending CI, as detailed in its [guide](stroke-opacity.md#verification-status-and-measurement-scope).

Locked typed strokes reuse undo tile snapshots for alpha validation. Fully transparent locked regions are culled before capturing tiles. Unlocked strokes retain their existing pixel path; no region-sized copy or JSON call is added to typed MOVE. Region-based plugin actions use the existing staging buffer and validate alpha before committing.
