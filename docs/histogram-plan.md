# Prepared-plan histogram foundation

This is a native prerequisite for node input/output observations. The public
`histogram.query` action and current controls still expose only visible composite
and explicit raw-layer scopes. Node selection, source/actual-view ID binding and
their command/UI integration remain planned. Do not infer those features from
the native capability alone.

## Optional negotiation

The bundled `micopi_histogram_v1` now advertises a complete 24-byte
`HistogramApiPlan`: the unchanged raw-sampling prefix, followed by `sample_plan`
at offset 16. Old 16-byte providers remain valid; partial tails or missing
advertised callbacks reject loading. Larger tables preserve known-prefix
semantics. Startup caches the callback with its DLL and inventory reports
`native[].analysis.histogram_plan`. No discovery occurs during drawing.

The callback borrows a `HistogramRequest`, prepared instructions, immutable layer
and A8 mask descriptors, native LUTs and a caller-owned `HistogramResult`. The
request uses dense document coordinates (`stride == width`), dimensions 1–4096
and a budget of 1–65,536 samples. Plans contain 1–64 instructions, 1–16 layers,
0–32 masks and 0–64 LUTs. Empty/coverage-output plans reject. These are trusted
in-process loans with the same lifetime/capacity guarantees as rendering.

## Evaluation and bounds

The native callback shares renderer instruction arithmetic, including scalar and
three-channel tables, masks, blend modes and premultiplied Mix. Existing legacy
delegation remains in place; the coverage/LUT evaluator is an explicitly inlined
helper used by rendering and observations. This avoids a second color algorithm.

Complete existing renderer validation evaluates one private pixel. The sampler
then visits only the deterministic centered-grid points from [Histograms](histograms.md),
with the same alpha-zero exclusion and unweighted nonzero-alpha counts. There
are at most **budget + 1** point evaluations, including validation, with at most
64 instructions per point. The extra private validation pixel never contributes
to counts. No full image, row output or coordinate list is allocated or copied.
Result and instruction slots use fixed stack storage, with no retained pointers.
These bounds are not latency measurements; CI's existing renderer/contact probes
remain applicable to the shared evaluator change.

Headers, capacities, graph types, declared instructions (including unused ones),
planes and output disjointness validate before result publication. Failed calls
leave the result unchanged. The output cannot alias request/plan/descriptor/LUT
storage or declared image/mask loans. Larger result records retain untouched
unknown tails. The host checks bounds before invoking the callback and checks
headers, the exact deterministic grid and all channel/count relationships before
publishing a successful result. This grid check also applies to raw sampling.

## Verification and remaining integration

Authored tests cover independent invert-bin witnesses, thin centered sampling,
masked mixed-table render/sampling agreement, invalid/unused declarations,
alias/future-prefix behavior, old/partial API tables, failed/corrupt providers,
strict grid validation and cached live-DLL discovery. SDK Rust/C/C++ layout
fixtures include the new tail. Runtime and cost checks await CI; no local heavy
build, device validation or reviewed UI capture is claimed.

The next Session integration must resolve explicit node IDs and image ports in
either source Layer Filters or generated Actual Composition, fence them by
document/revision, honor mask/group/Pass Through semantics and expose precise
scope in results. A source ID must not be mistaken for a generated node ID.
Reject missing nodes, coverage ports and active contacts without changing the
document. Reuse prepared tables, preserve read-only history/replay behavior and
keep local UI drafts distinct from published inputs. See the
[editing plan](photoshop-essentials.md) for open delivery gates.
