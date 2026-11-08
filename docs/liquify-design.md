# Multi-layer Liquify delivery design

R1 in [Painting-first Requirements](painting-requirements.md) is the acceptance
baseline. Source now includes the bounded inverse field, immutable-source RGBA/A8 region
samplers and optional cached DLL ABI. These prerequisites do not yet register
an editing command or provide a desktop tool. Runtime tests await CI; usable
multi-layer Liquify still requires the Session/preview/history integration below.

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
and stores finite f32 node displacements. RGBA/A8 interpolation, identity/hidden-RGB behavior and exterior fill are
defined by the sampler contract below.

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
field. No dependencies or existing API table layouts change. The sampler adds a separate
optional symbol without changing the tool or affine ABI.
Pointer/capacity assertions cover buffer reuse, not end-to-end latency or allocator
instrumentation. Native benchmarks and real tablet evidence remain pending.

## Optional native sampler contract

`micopi_liquify_v1` exposes a version-1 table: create, destroy, clear, joint push,
RGBA pixels and raw A8 mask sampling. The SDK mirrors Rust/C layouts and the
host negotiates a complete callback prefix once during DLL loading. A cached
handle retains its DLL and owns only the field/workspace. Serialize calls for
one live handle; destroy it exactly once. Callbacks retain no caller pointers.
An absent symbol remains compatible and reports unavailable when requested.

Sources are immutable complete frames; output is a nonempty document-coordinate
ROI with independent stride. Method 0 is nearest, method 1 uses Q16 bilinear
weights with encoded-color premultiplied RGBA arithmetic shared with the existing
affine sampler. Identity pixels copy their raw words, including hidden RGB.
RGBA exterior is transparent zero; raw scalar A8 exterior is reveal (255).
Dense and uniform raw masks share the same coordinates. Sampling applies neither
mask effects nor selection coverage; owner composition remains a host concern.

Validate dimensions, spans, strides, method, ROI and source/output/descriptor/
field-storage aliases before writing. Rejected pushes leave both the field and
update report unchanged. Sampling writes only ROI pixel cells, preserving output
padding. It performs no image allocation, registry lookup or deformation rebuild.
One handle samples every captured owner, preserving identical deformation across
layers. Private outputs must be discarded on callback failure; document publication
belongs to Session. Native DLLs are trusted process code, not a memory sandbox.

Rust/C/C++ layout assertions, host prefix/lifecycle checks, independent Q16 RGBA
and scalar A8 references, raw identity/clear, padding/ROI parity, explicit exterior,
malformed inputs and alias rejection are authored for CI. They have not yet passed
runtime validation; latency, allocation instrumentation and peak memory remain open.

## Remaining delivery stages

- [x] Implement bounded internal inverse-field update/interpolation in source.
- [ ] Verify its independent numeric, identity/partial-cell, compound, symmetry,
  duplicate, local-ROI, rejection/budget and workspace-reuse cases in CI.
- [x] Implement independent native RGBA and dense/uniform A8 region samplers, identity
  fast paths, alpha/exterior references, source/alias validation and optional ABI
  negotiation with C/Rust conformance tests in source. Runtime validation awaits CI;
  do not rerun deformation per target owner.
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
