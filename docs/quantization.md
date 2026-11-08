# Threshold and Posterize

The native painting plugin registers two direct active-raster edits:

```json
{"action":"layer.threshold","args":{"level":128}}
{"action":"layer.posterize","args":{"levels":4}}
```

Command search, schema parameter forms, configurable shortcuts, action workflow
nodes, JSONL, MCP and headless all use the same Session/native execution path.
This increment is implemented in source; runtime verification awaits CI. Cached
reversible previews and non-destructive source/adjustment nodes for these effects
remain planned. Editing a workflow does not execute it.

## Byte policy

Threshold accepts integer `level` 0–255, default 128. It compares the unrounded
encoded weighted gray `(54R + 183G + 19B) / 256` against the level. Equality is
white; a lower value is black. The implementation compares integer weighted sums
against `256 * level`, without rounding or floating point per pixel. Level 0
makes every nontransparent pixel white; level 255 makes only exact white RGB
white. Pure red is black at level 54 and white at level 53: rounding its gray
first would change that boundary.

Posterize accepts integer `levels` 2–256 per channel, default 4. For byte `c` and
`n = levels - 1`, choose bucket `floor((c*n + 127)/255)`, then return
`floor((bucket*255 + floor(n/2))/n)`. This selects the nearest evenly spaced
unrounded bucket and rounds its output to the nearest byte, positive ties up.
Two levels produce black/white channels, four levels use 0/85/170/255, and 256
levels preserve all bytes. RGB channels are independent; this is not an indexed
palette or perceptual color quantizer. A 256-byte table is prepared once per
command; the pixel loop uses three lookups.

Both effects preserve alpha and the hidden RGB of alpha-zero pixels. For
`#20406080`, threshold 60 produces `#00000080` and four-level Posterize produces
`#00555580`. These operations use the project's encoded-byte policy, not
linear-light or ICC-managed luminance; no Photoshop pixel-equivalence is claimed.

## Editing contracts and costs

The registered actions are idle-only, document-atomic pixel operations. Selection
coverage is blended once by the existing host path. Pixel locks reject edits;
transparency and position locks permit these alpha-preserving color edits. Raw
A8 mask planes remain separate: switch the painting target to Pixels first.
Layer multi-selection and links do not widen the active raster target.

One changed operation has one outer revision, tiled Undo and verified outcome.
A failed atomic batch restores the completed edit and history. Replay restores
verified surfaces without executing the DLL. Rejected commands leave pen contacts
active; existing recording boundaries may flush raw samples and journal failure.
No schema lookup, JSON, instance creation or image copying is added to typed MOVE.

The kernels allocate no images. Posterize uses a cold 256-byte stack table;
Threshold uses one integer cutoff. The existing host owns its bounded temporary
RGBA region and history. The host region may reach 64 MiB for a 4096-square
canvas; this excludes source, history, compositor, PNG/GUI and process peak.
End-to-end timing and peak-memory measurements remain pending. No dependencies,
ABI additions, legacy filter-mode IDs or project/recording format changes.

## Verification

Native references cover all 255 supported posterization level counts, all byte
values and four alpha values against an independent floating-point palette
reference. Threshold uses a bounded RGB lattice, exact boundary witnesses,
alpha/hidden-byte checks and independent weighted comparisons. Invalid native
parameters reject before pixel loans.

Session tests cover selection, locks, masks, one revision/Undo/Redo/outcome,
actual post-edit atomic rollback, default workflow parameters, continuing pressure
contacts and verified replay. The existing small color smoke adds independent
histogram bins, Undo, default workflows and JSONL/MCP parity for both operations.
These tests are authored, with runtime results pending CI. Native screenshot
review and physical-device validation remain separate pending gates.
