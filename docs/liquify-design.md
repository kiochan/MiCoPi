# Multi-layer Liquify delivery design

R1 in [Painting-first Requirements](painting-requirements.md) is the acceptance
baseline. The first source increment is an internal native inverse-field
primitive; it does not export a Liquify ABI, register an editing command, deform
image pixels or provide a desktop tool yet. Its tests await CI. Do not infer
usable multi-layer Liquify from this prerequisite.

## Reuse the owner contract

Capture stable raster IDs once, with reviewed expansion of selected groups or
persistent links. Retain separate raw owners, masks, source filters, hierarchy
and blend context. Use the [owner transform](owner-affine.md) and private
[owner preview](project-structure.md) boundaries as foundations, with one grouped
history/outcome publication rather than using a generic pipeline as grouped Undo.

Every target is validated before a mutable contact/commit, including pixel and
position locks, current ownership and complete composition capabilities. Linked
raw masks follow their raster; unlinked masks remain explicit independent planes.
Group-mask behavior must be represented in captured scope before grouped editing
is exposed. Group selection must not silently truncate unsupported targets.
Planning preserves live Brush/Hand contacts. Preview and Apply require idle,
revision-fenced source ownership, and Cancel preserves the committed document.

## Native inverse field prerequisite

`plugins/painting/src/liquify_field.rs` owns a regular grid of inverse
**displacements** in document-edge pixel coordinates. Zero is identity. Grid
vertices include the exact final canvas edge, so partial final cells interpolate
with their actual extent. Outside the field the displacement is zero.

For a push centered at `c`, radius `r`, forward movement `d`, strength `s`, define:

```text
w(q) = s * max(0, 1 - squared_distance(q,c) / r^2)^2
H(q) = q - w(q) * d
F_new(q) = F_old(H(q))
```

The stored displacement is `F(q) - q`. Compose against the immutable old field;
adding offsets without resampling the old mapping would produce a different
result after multiple pushes. Bilinear field interpolation uses f64 intermediates
and stores finite f32 node displacements. Future RGBA/A8 samplers will separately
define pixel interpolation, identity/hidden-RGB behavior and exterior fill.

One update accepts one to four joint arms. Duplicate push records contribute once.
Symmetric corrections can reflect both centers and displacement vectors. Joint
influence sums weighted vectors, divided by `max(1, sum(weights))`, before the
single inverse composition. This bounds overlapping influence and avoids exposing
intermediate asymmetric fields. It is a declared brush rule, not PS pixel parity.

The primitive validates all input before modifying nodes. New node values are
calculated into a reused workspace and published only after successful range
validation. An invalid update leaves the deformation unchanged. Nodes outside the
union of at most four local arm boxes are not scanned; overlapping boxes are
visited once. Dirty output reports up to four separate conservative rectangles
with interpolation halos, rather than forcing a single span between distant arms.

### Bounds and memory

- Canvas dimensions: 1 through 16,384 per axis; integer cell spacing: 1 through 64.
- At most 2,097,152 nodes; the caller's requested buffer budget is capped at 64 MiB.
- The budget covers node payload plus full reusable `(index, displacement)`
  workspace, using checked native element sizes. Allocator/container/OS overhead
  and image/preview/history storage are separate; this is not process peak memory.
- On x64 payload is 24 bytes per node. A 4096-square canvas with 4-pixel spacing
  has 1,050,625 nodes and reserves 25,215,000 payload bytes at creation.
- Each push radius must be at least the grid spacing and at most 2048 pixels;
  strength is finite 0 through 1, and displacement length cannot exceed radius.
  Smaller brush corrections require a finer admitted grid, with visible quality
  and budget feedback in the eventual tool. Do not silently turn them into no-ops.
- Grid/workspace reservation is fallible and happens at creation. Accepted pushes
  reuse these buffers. Clearing resets the field while preserving allocations.

No image, host pointer, document owner, JSON transport or disk state lives in the
field. No dependency, SDK layout or public capability changes in this increment.
Pointer/capacity assertions cover buffer reuse, not end-to-end latency or allocator
instrumentation. Native benchmarks and real tablet evidence remain pending.

## Remaining delivery stages

- [x] Implement bounded internal inverse-field update/interpolation in source.
- [ ] Verify its independent numeric, identity/partial-cell, compound, symmetry,
  duplicate, local-ROI, rejection/budget and workspace-reuse cases in CI.
- [ ] Add independent native RGBA and dense/uniform A8 region samplers, identity
  fast paths, alpha/exterior references, source/alias validation and optional ABI
  negotiation with C/Rust conformance. Do not rerun deformation per target owner.
- [ ] Add captured multi-owner admission, selection/mask policy, cost plans, one
  transactional grouped history/outcome and failure rollback through Session.
- [ ] Add schema-described public actions/state, node/MCP/headless execution,
  strict project/replay data and DLL-free outcome restoration.
- [ ] Add cached reversible multi-layer previews and artist controls, preserving
  pressure, view coordinates, source fences and configured four-language spacing.
- [ ] Measure first/warm pushes, sampler and commit cost, preview uploads, raw
  staging/history/journal growth and representative multi-owner memory. Review
  native captures and named devices before closing the artist-workflow requirement.

Intermediate phases are reviewable commits, not replacements for the full R1
workflow. Preserve explicit scope and open gates in ROADMAP and CHANGELOG.
