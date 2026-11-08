# Direct weighted grayscale

`layer.grayscale` edits the active raster through the native painting DLL.
Command search/parameter forms, configurable bindings, workflow nodes, pipelines,
JSONL and MCP share its schema and Session execution. This source increment and
its runtime/transport checks await CI. Dedicated cached grayscale controls and
read-only PNG previews remain planned; the existing non-destructive Grayscale
source/adjustment filter remains available.

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
JSONL/MCP parity. Full runtime, dedicated preview controls, native screenshot
production/review and physical-device validation remain separate pending gates.
