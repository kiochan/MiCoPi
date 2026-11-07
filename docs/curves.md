# Curves source nodes

`compositing.curves` is a registered source effect for linked Layer Filters and
free image graphs. It preserves source pixels and alpha while mapping encoded
RGB. This source increment awaits CI. Affect-below Curves adjustments, dedicated
curve/histogram controls and reviewed screenshots remain planned.

Insert **Curves** through a source node's **Insert Filter** menu or a free graph's
node menu. The existing schema editor exposes the parameters. Editing changes
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
