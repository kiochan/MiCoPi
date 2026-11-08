# Direct Brightness/Contrast

`layer.brightness_contrast` edits the active raster through the independent
native painting DLL. Command search and schema-derived parameter forms,
configurable bindings, workflow nodes, pipelines, JSONL and MCP share the action.
This source increment and its runtime/transport regressions await CI. The desktop
entry now opens coupled sliders and numeric percent controls with Reset, cached
Preview, Apply and Cancel, sharing the HSL draft lifecycle.

```json
{"action":"layer.brightness_contrast","args":{"brightness":25,"contrast":100}}
```

Both optional numbers range from -100 to 100 and default to zero. Brightness is
applied first, then contrast, retaining the existing encoded-byte source-filter
arithmetic. Normalize parameters by 100 to binary32. Brightness maps byte `v` to
`v + brightness * 255`, then rounds/clamps to a byte. Contrast maps that byte to
`(v - 127.5) * (1 + contrast)^2 + 127.5`, then rounds/clamps again. Positive
halves round upward; output is bounded to 0–255.

This is the project's legacy contrast policy, not an ICC-managed, linear-light
or exact Photoshop adjustment. Minimum contrast produces midpoint 128. For
example, RGBA `#20406080` with brightness 25 and contrast 100 becomes
`#0282FF80`. Neutral parameters preserve all bytes. Alpha and hidden RGB under
zero alpha remain unchanged.

The DLL prepares one 256-byte lookup table per command. Its pixel pass performs
three byte lookups and preserves alpha, with no image allocations or floating
point contrast calculation per pixel. Both independently compiled DLLs share
the same pure filter source; the compositor's public function, mode IDs,
arithmetic and dirty-region behavior remain unchanged. This does not claim a
speedup for the existing non-destructive renderer. No dependency or ABI is added.

The action declares `pixel_access=pixels`, `requires_idle=true` and
`document_atomic=true`. Selection coverage is blended once by the existing native
selection kernel. Pixel/transparency locks are honored; position locks permit
color-only edits. A selected A8 painting target must be switched to Pixels
explicitly. Raw masks and unselected layers remain unchanged; multi-selection
and link sets do not widen painting scope.

One changed edit uses tiled Undo, one outer revision and verified outcomes.
Atomic failure restores pixels, history and revision. Replay restores surfaces
without invoking the DLL. Active contacts reject before instance dispatch and
remain active; recording can still flush raw samples and record rejection.
The operation never enters pointer MOVE.

The host stages one bounded RGBA region, up to 64 MiB for a 4096-square canvas.
The table and cold scalar work are separate from document/history/renderer
buffers and process peak. End-to-end latency and peak memory remain unmeasured.

For non-destructive work, use the existing ordered **Brightness** and **Contrast**
source filters or affect-below adjustment providers. Their normalized `amount`
values are percent divided by 100; brightness before contrast matches the direct
command at full strength. Independent adjustment opacities, masks and group
boundaries retain their own semantics and can produce different results.

Native references cover all 256 byte values, four alpha values, integer
endpoints, combined order and malformed parameters. Session checks cover the
independent RGBA witness, selection/locks/masks, contact continuity, Undo/Redo,
post-edit rollback, source-filter parity and verified outcomes. The existing
tiny `scripts/smoke-hue-saturation.py` also checks this command's independent
bins, Undo and workflow/JSONL/MCP parity, sharing its isolated process setup.
Complete CI, actual window captures, visual review and measurements remain
pending. See the [feature checklist](photoshop-essentials.md).

## Read-only preview

`layer.brightness_contrast.preview` accepts complete `parameters`, explicit
`document_identity`, `document_revision` and the active stable `layer_id`.
Discover scope through `graph.read`/`canvas.state`; the
[example](../examples/automation/brightness-contrast-preview.jsonl) binds these
values in a pipeline. Its PNG/cost/guarded Apply response follows the same
contract as [HSL previews](hue-saturation.md#read-only-direct-preview-contract).

HSL and tonal previews share one admitted immutable Session loan, source/lock
checks, bounded private staging, transparent hidden-byte restoration, once-only
selection clipping and borrowed document/mask composition. Tonal staging runs
the native legacy brightness-then-contrast plan; Apply uses the native precomputed
table with the same byte arithmetic. No HSL-specific ABI capability is required
for the tonal primitive, although the document's own effects still require their
declared capabilities. No pixel processing moves into the host.

Preview leaves document pixels, the committed surface, history, revision and
outcome events unchanged. Active contacts reject without sample flushing or
completion; stale scope and invalid parameters reject before staging. The
existing native HSL failure/output-preservation cases now exercise the same
private staging path. New tonal soft-selection/mask/position-lock, PNG/Apply/Undo,
fence/contact and JSONL/MCP parity checks await CI. New desktop control regressions,
reviewed screenshots and end-to-end costs remain pending.

## Desktop draft

Command search opens the coupled Brightness/Contrast controls. Editing parameters
changes only the local draft. Previews settle for 150 ms, reuse their private
Canvas texture and keep Navigator/automation on committed pixels. Neutral Reset
uses the existing Canvas texture without a native render. Cancel/close/Escape
discard the draft; Apply uses the existing native action once. External edits
hide the old texture and require explicit source refresh before Apply.

`layer.brightness_contrast.open` opens the same controls for attached desktop
clients, with optional `parameters` such as `{"brightness":25,"contrast":100}`.
The shared queued delivery validates captured document/revision/raster identity;
an accepted request is not proof of a rendered window. Active contacts remain
unfinished. Headless returns `ui_unavailable`; atomic document groups reject this
presentation effect. Sliders, numeric fields and the footer derive sizes/spacing
from appearance settings and use all four language catalogs.

The native capture cases in `scripts/smoke-localization.py` include both color
dialogs and committed PNG/revision invariance. Standard CI currently skips GUI
smokes and does not run this script. Actual screenshot production and review
require a compatible interactive Windows/OpenGL environment; passing egui unit
tests alone does not prove those native captures or physical-device behavior.
