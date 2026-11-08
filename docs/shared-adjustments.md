# Shared targeted adjustments

This source increment adds explicit per-source binding to the existing stable
adjustment controller and shared Session actions. It does not replace ordinary
affect-below adjustment layers. New runtime/transport/UI tests await CI; native
capture review, the broader artist matrix and measured large-document costs
remain open under [R2](painting-requirements.md#r2--shared-targeted-adjustments).

## Two explicit scopes

- **Affect below** retains the existing adjustment stack behavior and serialized
  bytes. Create without binding fields uses this scope.
- **Targets** applies one controller's parameter set independently to each named
  raster source. It never composites the target set first and never applies to
  the backdrop at the controller's stack row.

`adjustment.create` and `adjustment.set` accept `scope` (`affect_below` or
`targets`) and `target_layer_ids` (up to sixteen unique positive existing raster
IDs). Supplying an array implies `targets`. The empty array is an explicitly
unbound shared controller: it affects nothing, rather than falling back to the
backdrop. Omitted Set fields retain the current binding. Explicit
`scope: affect_below` converts to stack behavior and cannot be combined with
`target_layer_ids`. No nullable schema extension is required.

The internal optional `target_layer_ids` is omitted for legacy controllers,
preserving their default data. Existing strict readers reject the new field
instead of silently interpreting a shared controller as affect-below. Current
project, Undo and verified replay metadata retain the exact IDs and mode.

Query/read and create/set/move results expose `scope`, `target_layer_ids` and
`evaluation_order`. Shared controllers use `ascending_controller_id`; legacy
items use `stack_position`. Selection and active painting layer never retarget
an existing controller. Registered action nodes, MCP and headless use the same
schema, parameter validation and execution paths as the inspector.

## Evaluation and ownership

For each raster source, evaluate its editable source-filter chain, then every
bound shared controller in ascending stable controller-ID order, then its own
raw mask/opacity, hierarchy/group gates and ordinary blending. Controller own
visibility, opacity and optional document-space A8 mask mix the effect separately
into each bound source. Existing native Filter, Levels, Curves, HSL and
quantization kernels and prepared slots are reused; pixel algorithms do not move
into the host or UI. Neither raw raster nor raw mask samples are rewritten by
an adjustment change.

Shared controllers must remain at the root in this first contract. Their
presentation row position has no effect on shared evaluation order. Targets may
be noncontiguous, hidden, locked or in different groups: their existing source
visibility, locks, masks and blend context remain intact. Non-destructive binding
does not claim permission to write protected raw pixels. Empty/hidden/disabled
controllers retain parameter/provider validation and conservative generated
instruction budget admission before publication.

A shared controller has several per-source generated outputs, so its singular
`node_id` is absent/null in group rows. Inspect a concrete **Actual Composition**
node to observe one output. Generated composition is a read-only view; never
submit it as the editable source graph.

## Structural policy and limits

Rename, reorder and reparent of target rasters preserve stable bindings. A copied
raster gets a fresh ID and is not implicitly bound. A missing bound target rejects
a deletion/replacement candidate before publication; unbind it first. Moving a
shared controller below a group rejects. Duplicating a subtree containing a
shared controller is guarded rather than applying a copied controller twice to
its original targets. Existing unsupported mixed-stack merge/transfer boundaries
remain guarded. This first slice introduces no adjustment-to-adjustment targets,
implicit link/group expansion or cycles.

Keep the existing controller count (16), raster count and generated graph limit
(64 nodes). Two effect/mix gates per target can reach the graph limit before the
nominal target/controller limits. Full source and hidden declaration preflight
rejects the entire candidate; nothing is silently truncated or disabled. These
are bounded source contracts, not a promise that all combinations of sixteen
controllers and sixteen targets fit the generated graph.

## Reversible preview and UI

The adjustment inspector adds **Bind explicit source layers**, **Bind Selected
Layers**, individual target toggles, **Unbind All Layers**, **Select Bound Layers**
and **Move Controller to Root**. Selection is a separate explicit action. Normal
Apply publishes one metadata history edit/revision/outcome; Cancel discards local
parameters and bindings. Binding changes use the same ordinary adjustment
operation, not a second editing path.

Shared draft previews borrow immutable source frames and mask caches. They build
and validate a private candidate plan with its own prepared color slots and mask
ownership flags, then render into a bounded temporary output. They do not clone
Session, copy every source frame, change raw samples, warm source caches or
publish history. Desktop preview attempts debounce for 150 ms and reuse the GPU
texture; pending, cancelled, stale and applied drafts do not publish an invalid
cached preview. Full composition and upload remain synchronous/full-frame, under
the existing separate preview composite/effect budgets; no end-to-end latency or
allocation-free UI claim is made.

`adjustment.preview` exposes the same private candidate as a read-only PNG. Supply
`document_identity`, `document_revision` and `adjustment_id`, plus ordinary Set
patch fields. It requires idle and rejects a stale source fence. Its response
contains exact resolved scope/targets, dimensions, PNG, cost and a guarded
`adjustment.set` Apply action. Apply arguments exclude the preview-only source
fields and carry `expected_revision`. See the
[small JSONL example](../examples/automation/shared-adjustments.jsonl).

## Verification gates

Authored tests cover independent per-source color composition, noncontiguous and
cross-group targets, controller versus owner mask/opacity order, unchanged raw
owners/allocations, empty/legacy metadata, stable controller order, copy and
missing-target policy, hidden graph budgets, project persistence, candidate
color-family preview parity, one Undo/Redo, atomic rollback, DLL-free verified
outcomes and actual bind/preview/cancel/apply controls, plus private-preview and post-publication compositor failure recovery. Tiny JSONL/MCP smoke checks
explicit mode, PNG/Apply parity, binding Undo/Redo and generated action nodes.

Only source/format/repository/Python/JSON checks have run locally for this slice.
CI runtime results, CJK/narrow native captures and representative cold/warm preview/controller costs must be reviewed before
closing R2 artist acceptance. The integrator wires the cached preview into Canvas
and runs cross-feature coverage with Liquify on the combined head.
