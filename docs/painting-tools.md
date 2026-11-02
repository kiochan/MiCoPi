# Painting dynamics, fill and gradients

The Brush panel exposes pressure size/opacity, minimum size and pressure gamma under **Pressure Dynamics**. A response curve previews gamma; 1 is linear, smaller values respond more strongly to light pressure. Minimum size is a fraction of the selected radius. These controls use existing `brush.set` parameters and the typed native pen path. Mouse input ignores pressure; zero-pressure pen samples deposit no paint. Controls change the current Session; use Painting settings to save startup defaults.

**Dab Spacing** controls stamp distance as a fraction of brush diameter: 0.01–2 (1–200%), with the original 0.11 (11%) default. Larger spacing creates separated marks and reduces stamp callbacks, trading continuous coverage for a different brush appearance. **Minimum Opacity** under Pressure Dynamics retains a fraction of brush opacity at light pressure when pressure opacity is enabled. The response becomes `minimum + (1 - minimum) × pressure^gamma`; zero pressure still deposits nothing. Mouse strokes ignore this floor. Both controls support `brush.set`, project replay metadata and startup settings as `painting.spacing` and `painting.minimum_opacity`.

```json
{"action":"brush.set","args":{"spacing":0.25,"pressure_opacity":true,"minimum_opacity":0.2}}
```

Options are copied into the native contact at DOWN, without extra configuration calls or JSON on MOVE. The optional `micopi_tool_options_v1` extension leaves existing tool tables and parameter sizes unchanged. Older DLLs accept default settings; custom values fail explicitly when the extension is absent. New DLLs use legacy defaults with an older host. Default option fields are omitted from replay parameter metadata to preserve old recording hashes; public `canvas.state.parameters` always reports both values. Nondefault recordings require a host that understands the new parameters.

Choose **Flood Fill** in the Brush panel, set **Fill Tolerance** and click a pixel. The operation fills the four-connected region on the current visible layer, using the foreground color and brush opacity with source-over blending. It samples this layer, rather than the merged canvas. Tolerance is 0–255, compared with the seed in premultiplied 8-bit RGB and alpha; transparent pixels with different hidden RGB therefore belong to the same region at tolerance zero. Higher tolerance compares every candidate with the original seed, rather than accumulating differences along a path.

Choose **Gradient** and drag on the canvas. Release commits one edit across the current visible layer. Foreground/background colors define its ends; brush opacity scales both alphas. **Radial Gradient** uses the start as its center and drag length as its radius. Linear gradients project onto the drag direction. Both clamp outside their endpoints and blend in premultiplied alpha to avoid transparent color fringes; interpolation uses encoded RGB, without a linear-light color-management conversion. A line previews the drag direction. Escape, focus loss, pen cancellation or switching tools cancels the preview. A changed document, active layer or revision during a drag also prevents committing. Undo restores the previous pixels.

Selecting a normal tool returns to its regular input behavior; temporary hand, picker and zoom modifiers remain available. These GUI modes adapt gestures into public plugin actions and do not change the tool ABI. Attached clients can select their GUI mode using `ui.dispatch` with `id: "painting.mode"` and `value: "brush"`, `"fill"` or `"gradient"`.

## Public actions

`layer.fill` and `layer.gradient` are supplied by the native painting plugin. They target the current layer, support expected revisions, workflow nodes, atomic pipelines, tiled undo and outcome replay. To retain a target across structural edits, atomically select its stable `layer_id` before the action. Hidden layers reject pixel edits. Algorithms never execute during verified replay.

```json
{"action":"layer.fill","args":{"x":10,"y":20,"color":"#ff6600","tolerance":8,"opacity":0.8}}
{"action":"layer.gradient","args":{"x0":0.5,"y0":0.5,"x1":100.5,"y1":0.5,"start_color":"#ff0000ff","end_color":"#0000ff00","kind":"linear","opacity":1}}
```

Fill requires integer `x`, `y` inside the canvas and `color`; tolerance defaults to zero. Gradients require finite `x0`, `y0`, `x1`, `y1` between -4096 and 8192, and both colors. Endpoints closer than 0.001 pixels are rejected. `kind` defaults to `linear` and also accepts `radial`. Colors accept `#RRGGBB` or `#RRGGBBAA`; opacity defaults to 1. Pixel centers are at half-integer coordinates.

## Performance and limits

Fill uses iterative scanline spans, a one-bit-per-pixel visited map and a span stack. It avoids recursive call depth and per-pixel queue records. Spans contain three 16-bit values; total queued records are bounded by discovered runs. Gradients write in place without a separate gradient image. Both still use the host's existing full-region staging buffer, atomic checkpoint, touched-tile undo and journal validation; these costs are not eliminated. Maximum canvas dimensions remain 4096 per side, with existing document/history limits.

Brush rasterization rejects pixels outside the dab circle before computing distance, and avoids distance evaluation inside solid hard dabs. Antialiased boundaries retain the previous distance formula. Legacy spacing, pressure, stabilization and the typed MOVE path remain unchanged at default options. No additional dependency is introduced. Exact DLL-versus-reference stroke tests protect existing pixels. A deterministic 100-pixel line at radius 5 uses 11 stamp callbacks at 100% spacing, versus more than 80 at default spacing; this measures work reduction for sparse brushes, rather than end-to-end input latency.

Regression checks include an independent breadth-first fill oracle, tolerance/connectivity/transparency fixtures, gradient endpoints and alpha fringes, gesture commit/cancel behavior, undo, atomic failure, replay parity and PNG comparisons across JSONL, MCP and GUI. Selections, fill gap closing, merged-layer sampling, textured brush tips and arbitrary pressure curves remain future work.
