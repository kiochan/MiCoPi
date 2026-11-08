# Direct weighted grayscale

`layer.grayscale` edits the active raster through the native painting DLL.
Command search/parameter forms, configurable bindings, workflow nodes, pipelines,
JSONL and MCP share its schema and Session execution. Fenced PNG previews and
cached desktop controls are implemented in source; new runtime/transport/UI
checks await CI. The existing non-destructive Grayscale source/adjustment filter
remains available.

```json
{"action":"layer.grayscale","args":{"amount":100}}
```

`amount` is optional, ranges from 0 to 100 percent and defaults to 100. Zero
preserves every byte. For nontransparent RGB, the existing encoded-byte policy
uses unrounded weighted gray `(54R + 183G + 19B) / 256`. Each channel interpolates
toward that value by `amount/100`, normalized to binary32, then rounds/clamps to
0–255. Positive halves round upward. Alpha and alpha-zero hidden RGB remain
unchanged. This is the project's legacy weighted-gray policy, not linear-light,
ICC-managed luminance or the HSL lightness-based desaturation command.

Full strength uses the exactly equivalent integer expression
`(54R + 183G + 19B + 128) >> 8`, with no floating-point conversion per pixel.
Partial strength reuses the unchanged native scalar filter. For example,
`#FF000080` becomes `#36363680` at 100%, and `#9A1B1B80` at 50%. Partial strength
mixes the unrounded gray, so it can differ from mixing an already rounded full
grayscale result. Existing image filter mode IDs/arithmetic remain unchanged.

The action declares idle-only, document-atomic pixel access. It honors pixel and
transparency locks while allowing position-locked color edits. Current selection
coverage is blended once through the existing native kernel. Raw A8 masks remain
separate; select Pixels explicitly before using this command on a mask painting
target. Multi-selection and links do not widen the active-raster scope.

One changed edit publishes one outer revision, tiled Undo and verified outcome.
Atomic failures restore pixels/history/revision. Replay restores recorded
outcomes without invoking the DLL. Active contacts reject before instance
dispatch and remain active; recording may flush pending raw samples and append
the rejection event. No parsing, color processing or registry lookup is added
to typed pointer MOVE.

The scalar kernel allocates no images or tables. The existing host stages one
bounded RGBA region, up to 64 MiB for a 4096-square frame; this excludes source,
history, renderer and process peak memory. Latency and process peak remain
unmeasured for this increment. No dependency, ABI or project/replay format changes.

Native tests compare a bounded RGB lattice/four alpha values against independent
integer weights and legacy full-strength pixels, with neutral/partial witnesses
and invalid-input rejection. Session references cover source-filter parity,
selection/locks/mask targets, one Undo/Redo/revision/outcome, actual post-edit
rollback and contact continuity. The existing tiny color transport smoke also
checks independent partial/full bins, Undo, default workflow parameters and
JSONL/MCP parity, including exact PNG equality between preview and Apply. Full
runtime, native screenshot production/review and physical-device validation
remain separate pending gates.

## Reversible desktop controls

Search for **Grayscale** to open the shared color draft. Strength uses a slider
and numeric percent field; default and Reset are 100%, while 0% is neutral and
skips native rendering. Changes settle for 150 ms before rendering. Repeated
frames reuse the texture; failures wait for a parameter/source change rather
than retrying every frame. Appearance metrics size the controls and footer.

Only Canvas displays the private preview; Navigator and automation retain the
committed document. Apply dispatches `layer.grayscale` once. Cancel, Escape or
closing the draft drops it without history or events. A document/revision/active
layer change hides the stale texture and disables Apply until an explicit
source refresh. An active pen contact blocks preview and Apply without finishing
pressure/stabilization input.

`layer.grayscale.open` queues the same controls, optionally with
`{"parameters":{"amount":50}}`. It captures the document identity, revision and
active layer; stale deliveries are rejected. This presentation command reports
`ui_unavailable` headlessly and is forbidden in atomic document batches.

## Read-only PNG preview

`layer.grayscale.preview` requires `document_identity`, `document_revision`,
`layer_id` and complete `parameters: {"amount":50}`. Read current scope through
`graph.read`; never reuse an old scope after editing. The result contains PNG
`data`, dimensions, scope, parameters, scratch `cost` and a revision-guarded
`apply` action. Actions, workflow nodes, JSONL and MCP share this schema.

Preview admission validates the eventual native edit, locks, selection and
renderer before staging. The shared immutable color pipeline renders the native
legacy grayscale filter, preserves hidden RGB, blends selection once and composes
through current graph/mask owners. It never publishes pixels, revisions, Undo or
recording events. It rejects active contacts without flushing pending samples.
Scratch admission separately bounds source, composite and mask-effect storage;
PNG/base64 encoding and GUI textures are excluded from that cost report. There
is no end-to-end latency or peak-memory measurement for this increment.
