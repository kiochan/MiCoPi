# Multi-layer Liquify delivery design

R1 in [Painting-first Requirements](painting-requirements.md) is the acceptance
baseline. Source now connects the bounded native field and RGBA/A8 region samplers to
captured multi-owner Session actions, private previews, one grouped Undo/outcome
and desktop canvas controls. New runtime/transport/UI tests await CI. Performance
measurements, reviewed native captures and the wider R1 artist matrix remain
open; source integration is not a completed acceptance claim.

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

## Shared execution and artist controls

The painting package registers `canvas.liquify.plan` (read-only),
`canvas.liquify.preview` (read-only PNG), `canvas.liquify` (atomic edit) and
`canvas.liquify.open` (desktop-only controls). Generated action nodes, command
search, key bindings, JSONL, MCP and headless clients share their schemas.
The [tiny JSONL example](../examples/automation/liquify.jsonl) demonstrates the flow.
Read `graph.read` for `document_identity` and `revision`; capture these along
with one through sixteen exact `layer_ids`. A request also carries `spacing`
(default 4), `method` (default bilinear) and at most 8192 ordered `steps`, each
containing one through four joint `{x,y,dx,dy,radius,strength}` arms. The PNG
response supplies a revision-guarded Apply action. Inspect plans without finishing
live contacts; preview, Apply and opening controls require idle admission.

Targets are whole visible rasters and their currently linked raw masks, including
disabled linked masks. Preserve unlinked masks, ancestor/group/adjustment masks,
all hierarchy, source filters, selection/Reselect and the active painting layer.
Pixel selection does not clip this explicit whole-owner operation. Reject pixel
or position locks; staged output validation also enforces alpha locks. There is
no implicit link/group expansion, group-mask deformation, mask-only Liquify or
off-canvas retained storage in this first contract. Existing ancestor composition
remains in place. Future group expansion needs a reviewed explicit owner policy.

A draft caches one DLL field and initialized owner outputs. Accepted typed pushes
record compact bounded intent and accumulate up to four dirty unions; a packet
batch updates the field first, partitions the exact rectangle union into disjoint
row bands/runs and samples each covered owner pixel once. Fixed stack storage
handles at most seven bands with four intervals per band, coalescing adjacent
identical runs; distant symmetry arms never force one enclosing image span. Sources stay
immutable. Headless execution likewise accumulates the full requested sequence
before owner sampling. No per-MOVE JSON, DLL instance creation, registry lookup,
file I/O or full-source copying occurs. One through four arms are stored inline
per intent step, preserving the existing JSON array-of-arrays contract. Reserve
8192 inline slots once when opening a draft; accepted points and Reset reuse that
transcript without per-step heap arrays. `transcript_bytes` reports the requested
payload as `8192 * size_of::<Step>()` (819,200 bytes with the current 100-byte
layout), separate from allocator overhead and JSON transport storage. Clearing
or a native no-op never appends an empty step. Reset clears and refills existing buffers;
Cancel drops the private draft. Failed native updates/sampling poison the draft
until reset/cancel; no partial owner set is published.

Before publication, revalidate the source fence, exact owner/mask pairings and
all outputs. Publish changed raw RGBA/A8 owners under one bounded history group,
without replacing the independent affine Repeat recipe. Session owns atomic
checkpoint/rollback, one revision and the verified outcome. Replay restores tiles
and verified surfaces without invoking Liquify or loading its original DLL. No
journal event is emitted per draft pointer point. Intent has a hard step bound;
existing tiled/compressed outcome persistence remains the recording format.

Select rasters in Layers and search **Multi-layer Liquify…** in the command
palette. Drag on the ordinary canvas. Controls expose radius, strength, optional
pressure strength and Reset/Cancel/Apply. The captured target list shows artist
layer names in a bounded scroll area and explains linked/unlinked mask behavior. The adapter uses native packet pressure,
existing stabilization intent and document/view reflection. Captured symmetry
axes reflect both centers and displacement vectors, with joint native evaluation.
The normal Brush/Eraser path is unchanged. A circular canvas cursor shows radius.
UI spacing derives from appearance metrics and labels have four-language catalogs.

Full preview composition remains synchronous and is throttled to at most one
attempt per 100 ms. Native no-op input, including zero strength, does not schedule
another composition. Reset requests an immediate fresh source preview. Owner
sampling uses disjoint ROI writes; the compositor still processes a complete
bounded frame, including source filters and nonlocal effects.

The desktop retains one converted CPU `ColorImage` with at most 64 MiB of pixel
payload. Initial/full uploads share its immutable Arc rather than cloning it.
Compare each fresh composed frame after checker conversion: unchanged visible
pixels upload nothing; otherwise upload only the exact enclosing changed display
rectangle and patch a uniquely held CPU cache in place. This comparison is over
the complete composed image, not just deformation bounds, so filter spread and
ancestor composition cannot leave stale texture pixels. If an earlier upload
still shares the image, replace/upload a full new immutable image; never mutate
an image retained by an outstanding renderer delta or trigger copy-on-write.

The retained CPU display image is additional desktop storage, separate from
`preview_cost` (compositor/effective-mask payload), field/transcript and owner
staging. A fresh composed RGBA frame and an upload patch can coexist with it;
each is bounded by a full-frame pixel payload, and the full-size GPU texture plus
driver/framework overhead is separate again. These ceilings are not measured
process peak memory. Warm uniquely owned partial updates reuse CPU frame storage,
but composition, patches and framework uploads are not allocation-free. Exact
comparison and lower upload area do not establish lower end-to-end latency until
measured. Changing the source revision invalidates the draft rather than rebasing edits.
Apply drops the private native/output cache before executing the shared action,
avoiding simultaneous duplicate owner staging. Field/workspace has a 64 MiB cap,
owner outputs a 192 MiB cap; preview uses existing separate composite/effect caps.
These payload ceilings do not describe process peak memory or graphics overhead.

## Verification and remaining acceptance

Authored CI gates cover independent native scalar/region/exterior references,
SDK prefix/layout/lifecycle, owner preview/PNG versus Apply, selection/unlinked
mask preservation, unchanged affine recipe, one Undo/Redo, missing capabilities,
late native failure and post-publication renderer rollback. Typed synthetic pen
input covers pressure, mirrored mapping and an idle committed canvas; desktop
controls cover texture reuse, exact changed upload bounds, immutable shared-image
fallback, no-op suppression, immediate Reset and stale-source behavior. Grouped
masked project round-trips and imported DLL-free raw outcomes are authored too. Tiny JSONL/MCP smoke exercises
preview/Apply parity, generated nodes and verified outcomes. These new gates have
not yet returned a passing CI result. Synthetic input does not certify devices.

- [x] Connect field, samplers, captured owners, private previews and grouped
  publication in source, with registered contracts and desktop controls.
- [ ] Pass exact-head native, desktop, headless and JSONL/MCP/replay gates.
- [ ] Review actual English/CJK captures, narrow/wide metrics and multiple native
  viewports; no new screenshots have been promoted as evidence.
- [ ] Measure first/warm pushes, sampler/commit cost, full preview composition
  and uploads, staging/history/journal growth and representative owner memory.
- [ ] Verify the required group/mask/adjustment/alpha-lock artist matrix, undo
  after project round-trips and DLL-free replay; add named tablet evidence.

Keep these gates open in ROADMAP until supported by the corresponding evidence.
