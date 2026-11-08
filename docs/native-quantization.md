# Native quantization composition prerequisite

This optional SDK/native capability is the prerequisite for non-destructive
Threshold/Posterize and immutable direct-operation previews. The direct
[document commands](quantization.md) already have separate registered contracts.
Typed source nodes and adjustment layers are now integrated in source, with
shared local controls and verified persistence contracts; runtime checks await CI.
Direct-operation fenced PNG previews and cached desktop controls now share the
immutable color pipeline; new runtime/UI verification remains pending.

## Independent negotiation

`micopi_quantization_v1` returns a complete version-one, 24-byte
`QuantizationApi`: header at offsets 0/4, render at 8 and histogram at 16.
Both callbacks are mandatory. Its fixed signatures and layout alias `HslApi`,
retaining immutable RGBA/A8 loans, LUTs, fixed HSL records and bounded ROI/output
contracts. It is an independently negotiated superset, not an extension claim
made by the old HSL symbol. Missing symbols remain compatible; advertised null,
short, incompatible or incomplete tables reject startup. Larger compatible
prefixes retain the known layout. Startup caches function pointers once.

Inventory reports native `filters.quantization_plan` and
`analysis.histogram_quantization`. These flags describe the native prerequisite;
document availability additionally requires registered typed node definitions. No symbol lookup occurs during
contacts or per-pixel evaluation. Other compositor tables retain their contracts
and reject the new instructions.

## Instructions and arithmetic

The new API accepts existing image, A8, blend/Mix, Levels, channel LUT and HSL
instructions, plus:

| Kind | Meaning | Fields |
| --- | --- | --- |
| 12 | Threshold | `a`: earlier image; `b=0`; `mode`: integer level 0–255; `amount=1` |
| 13 | Posterize | `a`: earlier image; `b=0`; `mode`: integer levels 2–256; `amount=1` |

These are prepared instruction kinds, not legacy filter-mode IDs. Every node,
including disconnected declarations, must validate before output publication.
Coverage cannot be used as an image input. Invalid parameters, image references,
unsupported fields, capacity or aliases leave caller output unchanged.

`plugins/native-paint/src/quantize_math.rs` is pure integer code independently
compiled into both DLLs. The direct JSON adapter remains in the painting plugin.
Threshold compares unrounded encoded weighted sums; Posterize prepares a byte
table and uses three lookups per pixel. See the direct-operation guide for exact
rounding. No host pixel algorithm, JSON parser or dependency enters the compositor.
Composition canonicalizes alpha-zero source pixels; direct editing retains their
raw hidden bytes, matching the existing color-effect distinction.

## Bounds and verification

At most 64 instructions each have a fixed prepared kernel slot. Preparation is
once per callback, uses bounded stack storage (under 17 KiB for quantization
slots), and allocates no image or row buffers. It does not yet persist tables
across callbacks; document cache integration remains planned. The caller owns
cropped/strided ROI output. The histogram callback validates the same complete
plan with one private pixel, then evaluates a centered sample grid bounded by
its existing budget. No pointers are retained.

SDK Rust/C11/C++17 fixtures assert stable aliases and offsets. Native cases cover
padded ROI sentinels, exact threshold boundaries, multiple Posterize levels,
independent render/histogram bins, HSL and A8 order, disconnected invalid nodes,
maximum invalid mode values, output aliases and old HSL rejection. Host fixtures
cover missing capability on the painting DLL, live cached callbacks, malformed
prefixes and future tails. The shared direct-kernel all-level/all-byte references
remain enabled after extracting the pure math source.

Only source/format/repository checks run locally; runtime/ABI checks await CI.
No latency, binary-size or process-peak measurements are claimed. Typed source/adjustment intent, all-declaration renderer admission, separate
intent slots, persistence/Undo/replay and shared controls are now implemented in
source. Fenced direct PNG previews now share the immutable color pipeline, pending
runtime validation. Persistent native prepared tables and measured costs remain
planned; keep corresponding feature checklist items unchecked pending all gates.

## Cost instrumentation

The opt-in Release [quantization cost probe](histogram-performance.md#quantization-dirty-region-and-observation-probe)
now covers Threshold and multiple Posterize tables on padded dirty regions and
bounded histogram grids. It uses exported callbacks, independent pixels/bins,
source immutability, output reuse and a Rust allocation counter. Per-call native
table preparation is included; Session/PNG/GPU, physical input and process peak
are excluded. This is instrumentation in source, with actual results awaiting CI.
