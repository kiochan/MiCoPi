# Read-only color histograms

`histogram.query` exposes encoded-byte color frequencies through the shared
Session, command search, schema-derived workflow nodes, JSONL and MCP. It uses
the optional native compositor histogram capability. This source increment
awaits complete CI verification; shared artist-facing charts are implemented in
source. Per-node/input histograms remain planned.

```json
{"action":"histogram.query","args":{"source":"composite","max_samples":65536}}
{"action":"histogram.query","args":{"source":"layer","layer_id":1,"max_samples":4096}}
```

## Scope and sampling

`source=composite` (the default) reads the current composed visible image.
`source=layer` requires an explicit stable raster ID and reads raw pixels,
ignoring that layer's visibility, opacity, filters and owned mask. A composite
request rejects `layer_id`; a raw request never defaults to the active layer or
expands links/multi-selection. Pixel/position locks permit this immutable read.
Selections are not implicit inputs. The query does not describe the image below
an adjustment or the input of an arbitrary source node.

The default and maximum budget is **65,536 pixels**. Smaller images are scanned
completely. Larger images sample a deterministic centered rectangular grid with
at most the requested number of points. For width `W`, height `H` and budget `B`,
choose `gw=clamp(floor(sqrt(B*W/H)),1,min(W,B))` and
`gh=min(H,max(1,floor(B/gw)))`. Grid point `(i,j)` samples
`(floor((2i+1)W/(2gw)),floor((2j+1)H/(2gh)))`. A full scan uses `gw=W, gh=H`.
Thin and uneven images retain the same bound. Approximate bins are sample counts,
not estimates multiplied to the complete canvas size.

Alpha zero contributes nothing. Every other alpha value contributes once,
without alpha weighting. `red`, `green` and `blue` each contain 256 integer bins;
`rgb` is their binwise aggregate and sums to three times the nontransparent
sample count. It is **not luminance**, a linear-light conversion or a color-managed
analysis. Composed alpha already reflects masks, opacity and blending; raw scope
deliberately does not.

The result includes document identity/revision, source and optional layer ID,
dimensions, total pixels, sampling budget/grid, visited/nontransparent/ignored
counts and `exact`. Exactness means every pixel was visited, including transparent
ones. Results are snapshots; consumers must check identity/revision rather than
displaying an old result as current.

## Execution and bounds

The handler rejects active drawing/panning contacts without finishing them or
flushing pending samples. Query only when idle; the painting path never invokes
it. Composite reads use the normal composition/error check; raw reads borrow the
explicit raster. Neither copies an image for histogram sampling.

The native implementation uses a fixed four-channel count table and retains no
pointers. The loader discovers/caches its independent ABI once, and inventory
reports `native[].analysis.histogram`. Missing capabilities reject explicitly.
The host validates dimensions/budget before calling and validates output headers,
grid bounds and all count relationships before publication. Failure/corruption
does not expose partial results.

The action is read-only: no document revision, undo group or replay event is
created. It adds no required project/recording metadata. Queries can run in an
explicit workflow; editing that workflow does not execute it. The normal expected
revision and request validation contracts still apply. Native plugins remain
trusted in-process code; a const source pointer is not process sandboxing.

Native, host and Session regressions cover independent exact/centered samples,
padding, transparency, thin grids, bounds, aliasing, malformed providers, locked
raw scope, contact rejection and unchanged document/history/events. The tiny
`scripts/smoke-histogram.py` compares JSONL/MCP and node discovery without a GUI or
stress workload. These runtime checks await CI. Sampling bounds are not latency
measurements; reviewed visual captures and physical-device checks remain separate.

## Artist-facing inspection

Expand **Histogram** above the image graph or in built-in Levels/Curves adjustment
properties. Choose **Visible Composite** or **Raw Active Layer**, set Maximum
Samples, then click **Refresh Histogram**. The raw-layer choice captures the
current raster's explicit stable ID; it does not expand links or selected layers.
Exact/Sampled, source, counts and revision identify the observation. Hover the
counts to inspect ignored transparent pixels.

One bounded shared snapshot serves the inspector, the Levels histogram band and
the Curves plot background. Curves displays the selected R/G/B channel, or the RGB
aggregate for Master RGB. Bars normalize to that channel's highest count; narrow
plots group adjacent bins by their maximum, without changing the native counts.
The aggregate is not a luminance histogram.

Refreshing dispatches the same registered `histogram.query` action. Idle frames,
curve drags and pointer samples never refresh automatically. A changed revision,
source, budget, document or raw active raster invalidates the chart; refresh is
explicit. During a contact, Refresh is disabled and cached plot overlays are
hidden without closing or flushing the contact. The inspector may retain labeled
old metadata with a refresh warning. Capability failure is explicit.

Snapshots describe the published composite or raw pixels, **not a local curve
or Levels draft**, nor the arbitrary input below that effect. Source/exactness
captions remain visible; hover for full source ID, count and revision. A graph
Apply/Live Preview invalidates the previous snapshot. Per-node/input scope is a
separate planned renderer contract, not inferred from node placement.

The snapshot uses four fixed 256-bin arrays behind one shared `Arc`, with no
retained image copy. Source and budget are temporary inspection controls, not
persisted painting defaults. UI tests cover actual refresh clicks, unchanged
revision/history/outcomes, shared idle cache identity, invalidation, contact
preservation and four-language narrow/wide/large-metric layouts. These new tests
await CI; no new screenshots or tablet measurements are claimed.
