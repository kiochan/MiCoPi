# Direct Hue/Saturation editing

The separate [native composition contract](native-hsl.md) supplies optional HSL
render/observation callbacks and cached negotiation. Typed source nodes and
adjustment layers are now implemented in source, pending CI. The direct command
retains its existing semantics.

`layer.hue_saturation` is a native DLL action over the **active raster layer**.
It is available through command search and its parameter form, configurable
bindings, schema-derived action/workflow nodes, pipelines, JSONL and MCP. This
source increment and its native/Session/transport regressions await CI. It is
the direct pixel-editing slice; see the non-destructive path below. A dedicated
desktop draft is described below; Colorize remains planned.

## Desktop color draft

Choose **Hue/Saturation: Active Layer** in command search, or assign its
registered `layer.hue_saturation` command a shortcut in Settings. The desktop
opens local Hue Shift, Saturation Adjustment and Lightness Adjustment controls
with Reset, Preview, Apply and Cancel. Spacing, control heights and window width
derive from configured appearance metrics; the shared HSL controls wrap in
English, Chinese, Japanese and Korean. This UI increment awaits CI and reviewed
captures; no default Ctrl+U binding is implied.

Preview overlays the Canvas with a separate texture and keeps Navigator and
document observations on the committed surface. Slider/numeric changes settle
for 150 milliseconds before one preview attempt. Unchanged parameters reuse the
texture; a neutral draft needs no extra render/upload. Preview can be toggled
off or explicitly refreshed. A failed attempt is not retried every idle frame.
Closing, Cancel or Escape drops the draft and shows the committed Canvas again.

The draft captures document identity, revision and active raster ID/name. An
external edit hides its texture and disables Apply until **Refresh Preview
Source** explicitly captures the current source. An active native contact blocks
preview/Apply without completing input. Opening and cancelling the form retain
the shared command admission/synchronization guards. Apply dispatches the
existing revision-guarded native action and closes only on success: one changed
edit uses one revision, Undo group and recorded outcome.

Automation can open these same controls with
`layer.hue_saturation.open`, for example
`{"parameters":{"hue":120,"saturation":15,"lightness":10}}`.
Omitted parameters default to zero. The action queues only a presentation
request, captures document/revision/raster identity and does not run a pixel
operation, end an active contact or increment the document revision. A stale
queued request is rejected by the desktop instead of silently opening on a
different source. Queue capacity is 128; document-atomic pipelines reject this
presentation effect. Headless callers receive `ui_unavailable` and should use
the direct action or read-only PNG preview instead. Opening controls is not
equivalent to Apply, nor does an accepted queued response certify a rendered
window. The localization smoke requests a reviewed-capture candidate through
this action and checks committed PNG/revision invariance in all four languages.

The feature-owned `plugins/native-paint/hsl_panel.rs` compiles into the desktop
host; no egui object crosses the DLL ABI. It calls the same typed read-only
preview service as the public PNG action below, then uploads the private texture
without PNG/base64 encoding. Pixel buffers and display/GPU texture storage are
separate costs; there is no claim of measured end-to-end latency or peak memory.

## Read-only direct preview contract

`layer.hue_saturation.preview` now supplies the shared preview primitive for
command/workflow, MCP and headless clients. This command returns a PNG, rather
than replacing the visible canvas or opening a dialog. Desktop controls use its
same typed service with the local cache described above. New runtime regressions
await CI.

Provide complete `hsl` parameters and explicit `document_identity`,
`document_revision` and active `layer_id`. Discover document identity/revision
from `graph.read` and stable layer IDs from `canvas.state`. The
[preview example](../examples/automation/hue-saturation-preview.jsonl) resolves
those fences inside a pipeline. Stale documents/revisions, another active raster,
pixel locks, a mask painting target, unavailable native HSL and active contacts
reject before pixel staging. Position locks permit this color-only operation.

The immutable preview renders a private raw-raster override using the same
native scalar HSL implementation, restores transparent hidden RGB, blends pixel
selection coverage once through the painting DLL, then borrows the document's
existing graph, layers and effective masks for composition. Source filters,
adjustments, group gates and raster masks remain part of the displayed result.
The document surface, raw pixels, masks, history, revision and outcome journal
remain unchanged, including on query failure. Dropping the result cancels it.

The response includes `mimeType`, base64 PNG `data`, dimensions, captured scope,
parameters, pixel scratch `cost` and an `apply` Action. That Action uses the
existing `layer.hue_saturation` command with `expected_revision`; execute it only
against the same Session/document and captured active layer. Apply recomputes
the direct edit through its normal validation/history path. Never copy the
preview composite into a raw layer. A changed Apply publishes one Undo/outcome;
preview requests publish none. MCP presents a standalone successful query as
image content with the same structured response.

This is an explicit cold operation, never a pointer-MOVE callback. Pixel staging
uses one full-canvas RGBA override (at most 64 MiB), one composed RGBA surface
(at most 64 MiB) and up to 64 MiB of separately bounded cold mask effects.
Unchanged layer/mask storage and prepared color tables are borrowed, and no
native command instance is created. `cost.pixel_scratch_bytes` includes these
pixel buffers, not PNG/base64 encoding, metadata, retained client surfaces or
process peak. Repeated automated queries are not free or implicitly cached;
desktop integration debounces/caches drafts and avoids per-frame PNG encoding.
Performance measurements and reviewed desktop captures remain separate gates.

```json
{"action":"layer.hue_saturation","args":{"hue":120,"saturation":0,"lightness":0}}
```

All three optional parameters default to zero:

| Parameter | Range | Meaning |
| --- | --- | --- |
| `hue` | -180 to 180 | Circular HSL hue shift in degrees |
| `saturation` | -100 to 100 | Negative moves saturation toward zero; positive moves chromatic colors toward full saturation |
| `lightness` | -100 to 100 | Negative moves HSL lightness toward black; positive moves toward white |

The independent implementation operates on encoded RGB bytes, with standard
HSL conversion and positive half-up rounding back to bytes. Negative saturation
uses `S * (1 + adjustment/100)`; positive saturation uses
`S + (1 - S) * adjustment/100` for chromatic colors. Achromatic colors remain
achromatic. Lightness uses the same toward-zero/toward-one policy. RGB edits
preserve alpha exactly and leave alpha-zero pixels, including hidden RGB bytes,
unchanged. Zero parameters preserve every byte. No ICC conversion, linear-light
processing or exact Photoshop algorithm equivalence is implied.

The host applies current pixel-selection coverage once, enforces pixel locks and
transparency locks, and retains document/history ownership. Position locks permit
this color-only edit. Linked layers and multi-selection do not expand its scope;
owned A8 masks remain separate and unchanged. Select Pixels before running this
command when an A8 painting target is active; the host rejects that target rather
than redirecting a color operation implicitly. The operation changes raw raster
pixels before source filters, composition masks and ancestor gates.

The action declares `pixel_access=pixels`, `document_atomic=true` and
`requires_idle=true`. Active pen or pan contacts reject before native instance
dispatch without finishing input. Recording may flush pending raw sample chunks and
append the failed-action event while the native contact remains active. The same admission applies to
headless/MCP callers; the frontend preserves input until shared admission runs.
Existing native actions keep their legacy contact policy unless they opt in.

One successful changed edit uses the existing tiled Undo and verified outcome
path; replay restores outcomes without rerunning the plugin. Atomic pipelines
roll back failed groups. The kernel visits each admitted region pixel once,
uses scalar stack state and allocates no images. The existing host edit-region
loan stages one bounded RGBA region, at most 64 MiB for a 4096-square canvas;
this is a logical buffer bound, not a measured process peak. The operation never
runs during pointer MOVE or idle painting frames. Large-image latency and a
reviewed artist-facing preview captures remain separate delivery gates.

Native references cover primary rotations, all grayscale bytes, alpha/hidden RGB,
black/white endpoints and a 512-color independent integer half-turn lattice.
Session references cover selection scope, one revision/event, Undo/Redo, atomic
rollback, locks, invalid parameters and contact preservation. The tiny
`scripts/smoke-hue-saturation.py` checks independent primary-color bins and
workflow/JSONL/MCP parity. No reviewed screenshots or physical-tablet result is
claimed. See the [feature checklist](photoshop-essentials.md).

## Non-destructive source and adjustment effects

Insert **Hue/Saturation** from a layer source/filter node's context menu, or
choose its registered provider when creating an adjustment layer. The shared
controls edit local Hue Shift, Saturation Adjustment and Lightness Adjustment
drafts, with Reset and configured appearance metrics. Apply publishes through
`graph.set` or `adjustment.set`; editing a workflow never executes it. Aliases use
their provider-owned schemas. New UI/layout and document/transport tests await CI;
no new reviewed captures are claimed.

The image definition is `compositing.hsl`, binding operation `hsl`, with one image
input/output and a nested `hsl` object containing the three parameters. Source
graphs use `kind=hsl`; public complete graph objects provide all three values.
Adjustment creation and updates merge partial parameter patches with validated
provider defaults/current intent:

```json
{"action":"adjustment.create","args":{"node_type":"compositing.hsl","parameters":{"hsl":{"hue":120}}}}
{"action":"adjustment.set","args":{"adjustment_id":1,"parameters":{"hsl":{"lightness":50}}}}
```

The second operation retains the existing hue and saturation. Use the returned
adjustment ID in clients; the example assumes the first created ID is 1.
`graph.read`/`graph.catalog` publish `renderer_hsl` for the active renderer.
Known but unavailable native capability rejects before document publication,
including disabled and unused declarations. Source HSL edits and adjustment
changes reject live contacts through shared admission while preserving the
painting tool's pressure/stabilization state.

Source effects retain raw pixels. Adjustment HSL follows the existing ordered
affect-below Normal mixing, opacity, Isolated/Pass Through scopes and owned A8
mask contracts. Painting and raster multi-selection remain independent. Intent
participates in structural edits, tiled/metadata Undo, atomic rollback, project
storage and verified outcome replay; restoration does not execute DLL commands.
Optional absent adjustment HSL intent remains omitted from legacy serialization.

Cold prepared plans cache HSL parameter records separately from Levels/Curves
tables. Source-storage order fixes both independent domains, including unused
declarations, so redirected histogram outputs cannot shift table/parameter slots.
Unchanged intent/provider reuses parameter storage through pen contacts and node
observations. Composition, merge/bake paths and transform previews pass the same
borrowed parameters through the shared renderer dispatch. Source/Actual histogram
selection stays explicit and reads published intent, never local slider drafts.

At most 64 HSL records occupy 1,536 payload bytes in a prepared parameter vector;
native preparation uses another fixed 64-record scalar array. This is bounded
metadata, separate from image/history/preview storage and process peak memory.
No parameter preparation, registry discovery or JSON serialization is added to
typed pointer MOVE. Native HSL adds scalar color work for each rendered dirty
pixel. The ignored Release [HSL cost probe](histogram-performance.md#hsl-dirty-region-and-observation-probe)
now checks padded 32/256-square regions and bounded observations on 1024/4096-square
sources, including independent pixels/bins and callback allocations. Results
await CI; end-to-end dirty-region and large-image costs remain open.

Authored regressions cover mixed unused HSL/Curves slots, exact primary-color
outputs, source observations, partial adjustment patches, Pass Through and masks,
Undo/Redo, atomic failure, projects and DLL-free outcomes. A recording renderer
checks parameter-pointer reuse during synthetic painting and observation. The
tiny JSONL/MCP smoke includes both direct and non-destructive paths. These checks
remain pending CI, separately from reviewed captures and physical-device evidence.
