# Read-only color histograms

`histogram.query` exposes encoded-byte color frequencies through the shared
Session, command search, schema-derived workflow nodes, JSONL and MCP. It uses
the optional native compositor histogram capability. This source increment
awaits complete CI verification; shared artist-facing charts are implemented in
source. Fenced node input/output queries and explicit chart source controls are
implemented in source; complete runtime and reviewed visual verification remain pending.
The [performance guide](histogram-performance.md) records the successful backend
CI baseline separately from pending node-picker and callback-cost verification.

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
Selections are not implicit inputs. These two scopes do not describe an effect's
input; use explicit node scope below for a published graph image port.

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

Typed [HSL nodes and adjustments](hue-saturation.md) now preserve separate HSL
parameter and LUT indices during projection, using the negotiated HSL sampler.
Mixed-table, source-input/output and parameter-storage regressions await CI.

## Published node inputs and outputs

Use `source=node` with explicit `graph_view` (`source` or `actual`), `node_id`,
`port` (`input` or `output`), `document_identity` and `document_revision`.
Input queries require `input_index`; output queries forbid it. `layer_id` is
forbidden, and node selectors are forbidden in composite/raw-layer scopes.
Discover graphs and fences together through `graph.read.histogram_views`.
`graph.read` and `graph.catalog` report `renderer_histogram_plan`.

- **source** evaluates the committed Source Layer Filters/free graph declarations,
  even if disabled. Local layer visibility/opacity apply, while generated owned
  masks, adjustments and ancestor gates are absent. This is a declaration
  observation, not an assertion that the effect is currently visible.
- **actual** evaluates the current composition graph, including group gates,
  Pass Through, owned effective A8 masks and filter bypass. A disabled flat source
  materializes its plain layer stack. Hidden/bypassed nodes omitted by generation
  are unavailable; do not reuse their source-view IDs in this view.

IDs belong to the selected graph at the captured document/revision. Stale fences
reject with `histogram_stale`; absent native sampling reports
`histogram_plan_unavailable`. Missing nodes/connections and coverage ports report
`histogram_node`. A node's input query evaluates its connected predecessor;
coverage inputs/outputs cannot be silently interpreted as color. Disconnected
incomplete branches reject when selected. Existing full declaration/capability
admission remains in place, including disabled definitions.

Results echo the requested view/node/port, optional input index,
`resolved_node_id`, original `graph_enabled` and `evaluation`
(`source_declaration` or `actual_plan`), alongside normal counts and fences.
Queries redirect a private output and borrow immutable full-canvas planes. Source
storage order keeps LUT indices stable. Matching intent borrows existing prepared
tables; source/actual differences may prepare bounded private tables. Actual mask
queries may refresh existing derived coverage caches. No RGBA surface or row
buffer is allocated, no document graph/cache is replaced, and no Undo/replay
outcome or revision is published. Active contacts remain rejected.

The [tiny example](../examples/automation/node-histograms.jsonl) reads identity and
revision in a pipeline, then binds them to before/after queries. Schema-generated
workflow nodes can wire the same `graph.read` outputs. Editing a workflow does
not execute it. These are published-graph observations, never local curve drafts.
Session, masks/Pass Through, stable-cache, stale/port/contact and JSONL/MCP tests
are authored; complete runtime verification remains pending.

## Artist-facing inspection

The [prepared-plan native sampler](histogram-plan.md) serves node-scope API queries
and the inspector's **Published Graph Node** source.

Expand **Histogram** above the image graph or in built-in Levels/Curves adjustment
properties. Choose **Visible Composite** or **Raw Active Layer**, set Maximum
Samples, then click **Refresh Histogram**. The raw-layer choice captures the
current raster's explicit stable ID; it does not expand links or selected layers.
Exact/Sampled, source, counts and revision identify the observation. Hover the
counts to inspect ignored transparent pixels.

For an effect's published input or output, select **Published Graph Node**, then
choose **Source Graph** or **Actual Composition**, a node and its **Image Port**.
Input observations also require a connected image input index. Coverage sources
are omitted and coverage inputs are never offered as color ports. Node labels
use plugin-owned presentation titles or unchanged artist-owned layer/group names.
Refresh captures the selected view, node, port, document identity and revision
through the same registered action. The chart caption keeps Source/Actual and
Input/Output explicit; its tooltip includes the full resolved scope.

Choices cache graph metadata for the current revision. Unchanged frames reuse
that cache without graph discovery, pixel reads or sampling. Pixel/parameter/name
revisions preserve a structurally unchanged selection while invalidating its old
histogram. Structural identities and edges are compared before retaining node IDs;
rewiring, structural edits, document replacement and view changes require selection
again. View preferences remain temporary. Generated group/adjustment gate identities
include their stable owner so reused generated IDs cannot silently retarget a chart.

One bounded shared snapshot serves the inspector, the Levels histogram band and
the Curves plot background. Both editors display the selected R/G/B channel, or
the RGB aggregate for Master RGB. Bars normalize to that channel's highest count; narrow
plots group adjacent bins by their maximum, without changing the native counts.
The aggregate is not a luminance histogram.

Refreshing dispatches the same registered `histogram.query` action. Idle frames,
curve drags and pointer samples never refresh automatically. A changed revision,
source, budget, document or raw active raster invalidates the chart; refresh is
explicit. During a contact, Refresh is disabled and cached plot overlays are
hidden without closing or flushing the contact. The inspector may retain labeled
old metadata with a refresh warning. Capability failure is explicit.

Snapshots describe the explicitly selected published scope, **not a local curve
or Levels draft**. Chart placement never implicitly selects an effect's input.
Source/exactness captions remain visible; hover for full source ID, count and
revision. A graph Apply/Live Preview invalidates the previous snapshot.

The snapshot uses four fixed 256-bin arrays behind one shared `Arc`, with no
retained image copy. Source and budget are temporary inspection controls, not
persisted painting defaults. UI tests cover actual refresh clicks, unchanged
revision/history/outcomes, shared idle cache identity, invalidation, contact
preservation, node input/output clicks, structural reselection, scope validation
and four-language narrow/wide/large-metric layouts including long node IDs. These new tests
await CI; no new screenshots or tablet measurements are claimed.
