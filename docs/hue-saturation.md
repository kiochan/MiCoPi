# Direct Hue/Saturation editing

The separate [native composition prerequisite](native-hsl.md) now defines optional
HSL render/observation callbacks and cached negotiation. Document node/adjustment
integration remains planned; this direct command retains its existing semantics.

`layer.hue_saturation` is a native DLL action over the **active raster layer**.
It is available through command search and its parameter form, configurable
bindings, schema-derived action/workflow nodes, pipelines, JSONL and MCP. This
source increment and its native/Session/transport regressions await CI. It is
the direct pixel-editing slice; non-destructive image nodes, affect-below
adjustment layers, a dedicated reversible preview and Colorize remain planned.

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
