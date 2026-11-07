# Curves

`compositing.curves` is a registered source effect for linked Layer Filters and
free image graphs. It preserves source pixels and alpha while mapping encoded
RGB. The same intent supports real affect-below Curves adjustment layers. Runtime
validation awaits CI. Shared curve controls and histogram queries are implemented;
plot overlays and reviewed screenshots remain planned.

Bounded [histogram queries](histograms.md) are available through shared commands;
plot overlays and per-node/input scope remain a subsequent UI/renderer increment.

Insert **Curves** through a source node's **Insert Filter** menu or a free graph's
node menu. Built-in nodes and adjustment properties share the curve editor;
custom aliases retain their schema editor. Editing changes
the local draft; Apply or enabled Live Preview dispatches `graph.set`. Live
Preview pauses during a contact. Explicit prepared-color graph changes reject
active contacts without ending them. Actual Composition stays read-only.

## Parameters and order

A Curves node has one image input and a `curves` object with four channels:
`master`, `red`, `green` and `blue`. Each contains `interpolation` (`linear` or
`smooth`) and 2–32 `points` with byte `input`/`output` values. Inputs strictly
increase; outputs may turn, flatten or reverse. Inputs outside the first/last
point clamp to the endpoint output. Identity is two points `(0,0)`, `(255,255)`.
Defaults and complete schemas are discoverable through the plugin node catalog.

MiCoPi applies **master first, then the corresponding channel curve**, with
rounded byte tables at each stage:

```text
red'   = red_table[master_table[red]]
green' = green_table[master_table[green]]
blue'  = blue_table[master_table[blue]]
alpha' = alpha
```

For example, a linear master with `(0,0)`, `(128,64)`, `(255,255)` followed by
linear red `(0,255)`, `(255,0)` and identity green/blue maps `#404040` to
`#DF2020`, retaining alpha. This is an encoded-byte painting operation, without
a Photoshop parity or color-management claim. See the
[native interpolation contract](native-plugins.md#curves-preparation-prerequisite).

## Shared execution, persistence and bounds

`graph.read` and `graph.catalog` expose `renderer_curves`. Validation covers
disconnected nodes, disabled graphs, all four channels and table capacity before
publication. Supported structural edits retain the source effect's branch and
identity. Owned raster masks apply after the source effect; selection coverage
is not an implicit composition mask.

Cold preparation compiles the master and three channel curves, combining them
into **three** final LUTs. Levels uses one LUT. A mixed plan remains bounded to
64 tables / 16 KiB, including disconnected declarations, alongside the existing
64-node limit. Table indices follow source storage order independently of render
order. Unchanged intent slots reuse their tables; failed rebuilds preserve the
old cache/provider. Painting borrows published tables without per-MOVE preparation
or per-pixel curve interpolation. These are source bounds, not latency measurements.

Undo/Redo, projects, structural projections and previews share prepared rendering.
Projects store intent only. Verified outcome replay restores surfaces and intent
without invoking a DLL. Legacy renderers reject channel instructions;
preparation-only Curves exports do not make document rendering available.

Authored regressions cover independent channel witnesses, alpha, raw masks,
mixed/disconnected indices, budgets, invalid/unsupported declarations, contacts,
cached pointer drawing, atomic failure, projects and DLL-free replay. Linked
smoke includes Levels/Curves across JSONL, MCP and GUI attachment with structural
and geometry changes and PNG parity. Runtime and visual validation are pending.
The broader [editing plan](photoshop-essentials.md) remains open.

## Affect-below adjustments

Use New Adjustment or `adjustment.create` with `node_type=compositing.curves`.
The same full channel schema is exposed by `adjustment.catalog`. Partial nested
patches retain defaults/current fields; arrays replace complete point lists.
Provider changes reset to complete defaults, clearing other effect tails.
Opacity, owned A8 masks, root/Isolated/Pass Through placement, compound Undo,
atomic rollback, project storage and DLL-free outcome replay use the existing
adjustment contracts. Painting remains on the active raster. Hidden/bypassed
Curves records still preflight native capability. See [Adjustment layers](adjustment-layers.md)
for shared actions and the strict optional metadata tail. Session and independent
transport references are authored and await CI; histograms remain planned.

## Local curve controls

Choose Master RGB, Red, Green or Blue, then Linear or Smooth Curve. Click empty
plot space to add a control point, drag a point to move it, or right-click a point
to remove it. Input movement clamps between adjacent inputs, retaining strict
order; output stays within 0–255. Two points are the minimum and 32 the maximum.
Use the Input/Output numeric fields for precision. Remove Point deletes the
selected point; Reset Curve affects only the selected channel, while Reset All
Curves restores all four channels. Channel/point selection is presentation state.

The chart uses theme surfaces and accent colors. Plot dimensions, hit targets,
point sizes and spacing derive from appearance metrics. Source-node fit accounts
for the full editor height. Changes remain in the local draft; existing Apply,
Cancel, conflict and live-preview rules decide document publication through shared
commands. Plugin aliases keep their own schema editor and validation constraints.

The line previews the selected channel's native byte table, not a second host
interpolator or the final document composite. Unchanged frames reuse the table;
active contacts pause native preparation. Missing/failed providers leave editable
points with an explicit unavailable indication. Hidden plots do not prepare.
A context-wide LRU retains at most 96 small editor/preview records across removed
nodes and documents. It holds no images or native callbacks. Package/DLL activation
still requires restart; preview keys separate document and provider identity.
No document pixels, history, registry queries or per-MOVE serialization are added.

Authored UI tests exercise actual click/drag/right-click/reset events, strict
point bounds, cache reuse/contact pause/provider changes, bounded LRU eviction
and four-language narrow/wide layouts with larger appearance metrics. Runtime,
reviewed application captures and physical mouse/pen checks are pending CI/device
validation; the existing workbench screenshots are not new curve-editor captures.
