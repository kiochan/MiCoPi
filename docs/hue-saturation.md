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
reversible direct-operation preview and Colorize remain planned.

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
dedicated artist-facing preview remain separate delivery gates.

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
pixel; end-to-end dirty-region and large-image cost measurements remain open.

Authored regressions cover mixed unused HSL/Curves slots, exact primary-color
outputs, source observations, partial adjustment patches, Pass Through and masks,
Undo/Redo, atomic failure, projects and DLL-free outcomes. A recording renderer
checks parameter-pointer reuse during synthetic painting and observation. The
tiny JSONL/MCP smoke includes both direct and non-destructive paths. These checks
remain pending CI, separately from reviewed captures and physical-device evidence.
