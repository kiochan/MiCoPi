# Threshold and Posterize

The native painting plugin registers two direct active-raster edits:

```json
{"action":"layer.threshold","args":{"level":128}}
{"action":"layer.posterize","args":{"levels":4}}
```

Command search, schema parameter forms, configurable shortcuts, action workflow
nodes, JSONL, MCP and headless all use the same Session/native execution path.
Typed source nodes and affect-below adjustment layers now share this native
arithmetic, with local controls, masks, history and verified persistence. New
integration checks await CI. Direct-operation PNG previews and cached reversible
controls are now implemented in source with additional runtime/UI checks pending. Editing a workflow does not execute it.

## Byte policy

Threshold accepts integer `level` 0–255, default 128. It compares the unrounded
encoded weighted gray `(54R + 183G + 19B) / 256` against the level. Equality is
white; a lower value is black. The implementation compares integer weighted sums
against `256 * level`, without rounding or floating point per pixel. Level 0
makes every nontransparent pixel white; level 255 makes only exact white RGB
white. Pure red is black at level 54 and white at level 53: rounding its gray
first would change that boundary.

Posterize accepts integer `levels` 2–256 per channel, default 4. For byte `c` and
`n = levels - 1`, choose bucket `floor((c*n + 127)/255)`, then return
`floor((bucket*255 + floor(n/2))/n)`. This selects the nearest evenly spaced
unrounded bucket and rounds its output to the nearest byte, positive ties up.
Two levels produce black/white channels, four levels use 0/85/170/255, and 256
levels preserve all bytes. RGB channels are independent; this is not an indexed
palette or perceptual color quantizer. A 256-byte table is prepared once per
command; the pixel loop uses three lookups.

Both effects preserve alpha and the hidden RGB of alpha-zero pixels. For
`#20406080`, threshold 60 produces `#00000080` and four-level Posterize produces
`#00555580`. These operations use the project's encoded-byte policy, not
linear-light or ICC-managed luminance; no Photoshop pixel-equivalence is claimed.

## Editing contracts and costs

The registered actions are idle-only, document-atomic pixel operations. Selection
coverage is blended once by the existing host path. Pixel locks reject edits;
transparency and position locks permit these alpha-preserving color edits. Raw
A8 mask planes remain separate: switch the painting target to Pixels first.
Layer multi-selection and links do not widen the active raster target.

One changed operation has one outer revision, tiled Undo and verified outcome.
A failed atomic batch restores the completed edit and history. Replay restores
verified surfaces without executing the DLL. Rejected commands leave pen contacts
active; existing recording boundaries may flush raw samples and journal failure.
No schema lookup, JSON, instance creation or image copying is added to typed MOVE.

The kernels allocate no images. Posterize uses a cold 256-byte stack table;
Threshold uses one integer cutoff. The existing host owns its bounded temporary
RGBA region and history. The host region may reach 64 MiB for a 4096-square
canvas; this excludes source, history, compositor, PNG/GUI and process peak.
End-to-end timing and peak-memory measurements remain pending. Direct edits need
no new dependencies, ABI additions, legacy filter-mode IDs or recording formats;
previews use the separately negotiated native quantization-plan capability.

## Verification

Native references cover all 255 supported posterization level counts, all byte
values and four alpha values against an independent floating-point palette
reference. Threshold uses a bounded RGB lattice, exact boundary witnesses,
alpha/hidden-byte checks and independent weighted comparisons. Invalid native
parameters reject before pixel loans.

Session tests cover selection, locks, masks, one revision/Undo/Redo/outcome,
actual post-edit atomic rollback, default workflow parameters, continuing pressure
contacts and verified replay. The existing small color smoke adds independent
histogram bins, Undo, default workflows and JSONL/MCP parity for both operations.
CI at [`d27f03a`](https://github.com/kiochan/MiCoPi/actions/runs/42435215434)
passed direct native/Session/transport cases (1,232 desktop and 908 headless
tests, plus native SDK/kernel suites and all configured transport smokes). This
baseline excludes the later native quantization-plan and document-node increments.
New integration tests await CI. Native screenshot review and physical-device
validation remain separate pending gates.

## Non-destructive source nodes and adjustment layers

The compositing package registers `compositing.threshold` and
`compositing.posterize` as image nodes. Both use `kind: "quantization"` and a typed
`quantization` object: `{"effect":"threshold","value":128}` or
`{"effect":"posterize","value":4}`. Provider schemas restrict the effect name and
its integer range; generic document validation also rejects mismatched ranges.
The image has one input. Linked Layer Filters allow insertion/deletion alongside
existing filters; free graphs allow explicit wiring. Actual Composition remains
a generated read-only view, distinct from editable source filters.

Threshold Level / Posterize Levels use shared slider, numeric and Reset controls
in nodes and adjustment drafts. Reset uses 128 / 4. Drafts do not dispatch pixel
operations or execute workflows. Node fitting derives from appearance metrics;
four-language narrow/wide and large-metric reset cases are authored. Aliased
plugin providers retain their schema editors. Native screenshot review is pending.

```json
{"action":"adjustment.create","args":{"node_type":"compositing.threshold","parameters":{"quantization":{"value":128}}}}
{"action":"adjustment.set","args":{"adjustment_id":1,"parameters":{"quantization":{"value":64}}}}
```

Adjustment operations retain affect-below order, group/isolation/Pass Through
boundaries, Normal premultiplied opacity and optional owned A8 masks. Painting
stays on the active raster. Switching provider clears the previous typed family;
public parameter schemas expose the selected provider's actual contract.

Typed quantization records have separate intent/cache slots and consume no
Levels/Curves LUT or HSL parameter indices. Compiled instruction kinds 12/13 carry
the integer value inline. The renderer negotiates
[the independent native capability](native-quantization.md). Capability preflight
covers every declaration, including hidden, unused and disabled branches, before
mutation or project publication. Old DLLs can still render old documents; the host
rejects new quantization intent when the capability is absent. Outcome replay
restores verified pixels without renderer capability checks.

Version-three adjustment metadata adds an optional `quantization` tail, omitted
for older/scalar records. Exactly one typed family is permitted, with neutral
legacy anchors. Strict old readers reject the new tail; existing projects and
recording hashes remain supported. Source intent and adjustment metadata share
project persistence, Undo and verified outcomes; private tables remain derived.
Native tables are prepared per callback rather than persistently cached across
callbacks; the ordinary typed brush MOVE contract remains unchanged.

Source/adjustment, mixed HSL/Curves slot order, masks, actual post-edit rollback,
provider changes, contact guards, project round trips, DLL-free outcome restoration,
fenced node histograms and JSONL/MCP cases are authored. Runtime validation, reviewed
native captures and end-to-end measurements remain pending. The full common-filter
checklist is not complete. See [the automation example](../examples/automation/quantization-composition.jsonl).

## Direct PNG previews and cached controls

Search for the direct Threshold or Posterize command to open the shared color
draft. Integer slider/numeric/Reset controls share the source-node controls;
default/reset is 128 / 4. Parameters settle for 150 ms, then the Canvas uses a
private cached texture. Repeated idle frames reuse it; failed parameters do not
retry every frame. Posterize at 256 levels is identity and skips native rendering.
Threshold has no identity parameter. Apply dispatches the existing native command
once; Cancel, Escape or close discards the draft without Undo or recording events.

`layer.threshold.open` and `layer.posterize.open` queue the same controls with
optional `parameters: {"level":64}` or `parameters: {"levels":4}`. Queue/delivery
capture document identity, revision and the active raster. Stale deliveries reject;
changes after opening hide the old texture and disable Apply until explicit source
refresh. Active pen contacts block previews/Apply without finishing input. These
presentation commands report `ui_unavailable` headlessly and reject atomic batches.

`layer.threshold.preview` / `layer.posterize.preview` require `document_identity`,
`document_revision`, `layer_id` and a complete `parameters` object using the same
direct parameter names. Read scope from `graph.read`. Nodes, JSONL, MCP and headless
share these schemas. The result includes composed PNG `data`, dimensions, scope,
parameters, scratch `cost` and the existing revision-guarded `apply` action.
Preview never dispatches a mutating action or publishes history/revision/events.

Both effects use the shared admitted immutable color loan, native quantization
rendering, hidden-RGB restoration, once-only selection mixing and current graph/
mask composition. Locks and native edit admission remain shared with Apply. An old
compositor without the optional quantization API rejects before image staging.
Navigator and automation snapshots keep committed pixels while Canvas previews.
Source/composite/mask scratch bounds are reused; PNG/base64, GUI textures and process
peak are excluded from reported scratch costs.

Session/transport cases compare exact preview/Apply PNGs with soft selection and
raw masks, assert unchanged state/history, reject stale/capability/contact requests
and cover queued defaults/headless/atomic rules. Desktop cases cover texture reuse,
actual Reset/Apply/Cancel, Undo, stale delivery, identity skipping and four-language
narrow/wide/large-metric footers. The optional native capture script includes both
dialogs, but it has not run for this increment and is not part of standard CI.
Runtime validation, native screenshot review and measured end-to-end costs remain
pending. See [the read-only example](../examples/automation/quantization-preview.jsonl).

## Callback cost probe

The [bounded Release probe](histogram-performance.md#quantization-dirty-region-and-observation-probe)
measures native Threshold/Posterize ROI and histogram callbacks with independent
references and allocation counting. It includes per-call preparation but excludes
Session dispatch, full-frame preview allocation/encoding, GUI/GPU and process peak.
The test is opt-in locally and included by the existing CI measurement filter;
actual measurements remain pending. It does not certify end-to-end drawing latency.
