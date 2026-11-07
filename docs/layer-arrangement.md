# Layer alignment and distribution

This source increment adds explicit raster alignment and center distribution
through the shared command pipeline. Runtime verification is pending CI.
It reuses the existing native bounds and integer movement callbacks, with no
new dependency or ABI requirement.

## Arrange ink, preserve painting ownership

`layer.arrange.plan` inspects a captured list of one to sixteen unique stable
raster IDs. `layer.arrange` applies the same operation while input is idle.
Neither action expands persistent layer links or changes the active painting
layer, selected rasters, pixel selection, mask painting target, adjustment
definitions, source filters or saved Free Transform recipe.

Bounds come from each raster's raw positive alpha, before opacity, filters and
masks. Every target must contain ink. A fully transparent target rejects the
whole request; it does not silently acquire canvas bounds or disappear from the
list. Masks follow their owner without steering its alignment box.

The six alignment modes are `left`, `horizontal_center`, `right`, `top`,
`vertical_center` and `bottom`. `reference: "targets"` uses the union of the
captured ink boxes and requires at least two rasters. `reference: "canvas"`
uses the current frame and permits a single raster. Omitting `reference` means
`targets`.

`distribute_horizontal_centers` and `distribute_vertical_centers` require at
least three targets and a `targets` reference. They distribute centers, rather
than gaps between edges. Canvas distribution and key-layer references are not
part of this contract.

## Integer placement

All boxes use exclusive right/bottom coordinates `[x0,y0,x1,y1]`. Alignment
translates whole pixel rows/columns without resampling. Center deltas round to
the nearest integer, with exact half ties away from zero. Different box parity
can leave a half-pixel center difference; the plan reports the actual integer
translation instead of implying fractional alignment.

Distribution sorts by doubled center coordinate, then stable raster ID. The
first and last centers stay fixed. For an interior item at sorted index `i`,
with `D = count - 1`, the translation is:

```text
round_away_at_half((first_center2 * D
                    + (last_center2 - first_center2) * i
                    - item_center2 * D) / (2 * D))
```

The other axis stays fixed. Input order does not break center ties; when all
centers coincide, every translation is a deterministic no-op. Plans retain the caller's
target order even when distribution uses a separate sorted ordering.

## Masks, admission and history

Every owned linked raw A8 mask travels with its raster, including disabled
masks. Unlinked and unrelated masks stay unchanged. Moved RGBA exterior is
transparent; moved raw-mask exterior reveals white. Pixels and raw mask support
outside the current frame are clipped and recoverable through Undo. Density and
feather remain metadata, with effective coverage recomputed as needed.

Preflight checks every explicit target, including no-op targets: identities,
visibility, pixel/position locks, complete composition definitions, native
callbacks and a conservative full-set staging ceiling of 192 MiB. This bounds
staged RGBA/A8 outputs; it is not a process-wide memory measurement. Planning
copies bounded descriptors, not image frames, and preserves active Brush, Hand
and mask contacts. A plan's `can_apply` is false during an active contact.
It reports idle eligibility after static preflight; staged-output checks such
as transparency locking and later native/render failures can still reject Apply.

Apply requires idle input. All changed owners are staged before publication
and committed through the existing raw owner history group. One operation
publishes one revision and verified outcome, with one Undo step when content
changes. Native, renderer, journal or later atomic-pipeline failures restore the
document and history. No-op requests retain shared command revision semantics.
Replay restores verified surfaces and tiles without rerunning the arrangement
algorithm or loading its DLLs.

## Public commands

```json
{
  "action": "layer.arrange.plan",
  "args": {
    "layer_ids": [1, 3, 5],
    "mode": "distribute_horizontal_centers",
    "reference": "targets"
  }
}
```

The result includes the captured IDs, source reference box and per-owner source
box, linked mask IDs, integer offsets and moved boxes. `staged_bytes` is the
conservative full-set output ceiling, including no-op targets. Apply can bind
the plan's revision through the shared `expected_revision` action field.

Actions expose typed schemas and output ports to workflow nodes, JSONL and MCP.
Editing a workflow remains separate from running it. Successful standalone
events also retain the resolved source boxes and offsets from the handler's
captured result; recording does not recompute bounds after moving pixels.

The Layers panel's Arrange menu captures selected raster IDs and its source
revision once. Reopen a stale menu after another edit. Command search and
configurable shortcuts expose the same registered operations. All spacing uses
the configured appearance metrics.
