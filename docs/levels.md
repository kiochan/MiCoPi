# Levels Source Nodes

[Channel Levels](channel-levels.md) now extends source nodes and affect-below
adjustments with optional Red/Green/Blue ranges after the legacy master range.
Its source integration, tests and shared controls await CI validation.

The registered `compositing.levels` image node applies a master encoded-RGB
curve independently to R, G and B while preserving alpha. It works in linked
Layer Filters and free image graphs. This source increment awaits CI validation;
true affect-below Levels adjustment layers use the same typed intent and controls.

Right-click a layer or source effect in Layer Filters, choose **Insert Filter →
Levels**, and edit its Input Levels and Output Levels controls. The numeric
fields support dragging, the grayscale bars show the input/output ranges and
Choose Master RGB, Red, Green or Blue; Reset Channel restores only that range,
and Reset Levels restores all four. Controls edit the local draft only.
Plugin aliases with custom schemas retain the generic schema editor.
Changes remain in the graph
draft until Apply or explicitly enabled Live Preview. Live Preview pauses during
active contacts; explicit Levels graph edits reject without ending the contact.
Undo restores the previous graph and composite while raw layer pixels remain
unchanged. Actual Composition shows the generated Levels node read-only.

## Parameters and automation

| Field | Range | Identity |
| --- | --- | --- |
| `input_black` | integer 0–255, less than input white | 0 |
| `input_white` | integer 0–255, greater than input black | 255 |
| `gamma` | finite 0.1–10 | 1 |
| `output_black` | integer 0–255 | 0 |
| `output_white` | integer 0–255 | 255 |

Optional `channels` contains complete `red`, `green` and `blue` objects with the
same five fields. Each final table maps a master-rounded byte through that
channel's rounded table. Scalar intent still uses its original one-table path.
Adjustment patches may change a subset of channel fields: the first patch starts
from identity, later patches preserve neighbors. Send complete identity channel
ranges to clear prior channel effects; omission in a partial patch retains them.
Built-in Apply sends explicit identity channels after Reset Levels. Source graph
replacement instead serializes the complete intent, omitting identity channels.
Custom aliases continue obeying their own provider schema.

Gamma above 1 brightens midtones. Reversed output endpoints invert the curve.
The operation clamps input normalization, raises it to `1/gamma`, maps output
endpoints and rounds half-up. Alpha is unchanged. The native layer-source and
composition rules still determine how fully transparent source words appear.

For example, a node in a `graph.set` payload uses:

```json
{
  "id": 20,
  "node_type": "compositing.levels",
  "kind": "levels",
  "position": [260, 0],
  "parameters": {
    "input_black": 0,
    "input_white": 255,
    "gamma": 2,
    "output_black": 0,
    "output_white": 255
  }
}
```

Connect its single image input and output through normal graph edges. Plugin
catalogs expose its schema/defaults; command forms, nodes, MCP and headless use
the same `graph.set` validation and execution. `graph.read` and `graph.catalog`
report `renderer_levels` and `renderer_channel_levels`. All declared Levels parameters and capabilities are
validated, including disconnected nodes and disabled graphs.

## Rendering, persistence and limits

Document plans cache bounded native byte tables. Unchanged slots reuse native
results; changed tables prepare before publication, and failed preparation leaves
the previous document plan intact. Normal painting and mask composition borrow
these tables without per-pixel power calculations, per-MOVE registry lookup or
table preparation. Source effects follow layer branches through structural edits;
previews and supported merge paths use the corresponding prepared graph.

Projects serialize the five master parameters and optional nonidentity `channels`.
Identity channel ranges are omitted, retaining old five-field bytes and one-table
rendering. Nonidentity channels use three native tables with master-then-channel
byte rounding. Older scalar-only readers reject the tail. Undo/Redo rebuilds derived plans;
verified outcome replay restores recorded pixels without executing DLL code.
Legacy scalar Filter IDs/arithmetic and old default recording bytes are retained.
Older strict readers reject the new node kind rather than silently dropping it.

Authored Session regressions cover independent gamma witnesses, alpha, linked/free
graphs, disconnected table indexing, invalid/unsupported disabled declarations,
contact rejection, atomic rollback, projects and DLL-free replay. The existing
linked-graph smoke now includes Levels across JSONL, MCP and GUI attachment with
structural edits, geometry and byte-identical PNGs. Local lightweight checks are
separate from pending CI execution. Physical-tablet and reviewed screenshot
validation remain separate.

The shared `adjustment.create/set/get` path also supports `compositing.levels`,
owned masks, Normal effect strength, groups, Undo and verified outcomes. See
[Adjustment layers](adjustment-layers.md) for placement and partial parameter patches.
Shared channel controls select the edited range and the corresponding cached
[histogram](histograms.md) bins (RGB aggregate for Master). Source/exactness remain
explicit; the snapshot is
not the local Levels draft or the effect input. Bounded queries also serve automation;
[Curves](curves.md) supplies separate master/R/G/B source and adjustment intent. See [Native contracts](native-plugins.md),
[Node editor](node-editor.md) and [Roadmap](../ROADMAP.md).
