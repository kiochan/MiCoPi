# Painting-first requirements

This document replaces exhaustive Photoshop/PhotoCraft feature parity as the
product acceptance baseline. On 2026-11-08 the maintainer cancelled that goal
and specified five illustration-editing priorities. The former
[comparison checklist](photoshop-essentials.md) remains a reference with useful
implementation history; its unchecked items are not a requirement to ship them
all. Existing work stays in Git and must retain its compatibility contracts.
This is requirements planning, not a declaration that these features are ready.

## Artist outcomes and priorities

| Priority | Requirement | Artist outcome |
| --- | --- | --- |
| 1 | Multi-layer Liquify | Correct a face, pose or silhouette across ink, base colors and shadows together, retaining every layer |
| 2 | A shared adjustment bound to multiple layers | Edit one parameter set and update the explicitly bound layers without repeating configuration |
| 3 | Convenient grouping | Quickly organize, move, select, name and recover related layers/groups |
| 4 | Layer search | Find relevant layers in a large illustration and act on an explicit result selection |
| 5 | Symmetry editing tools | Apply supported drawing and correction gestures consistently around visible document axes |

Priority 2 currently means applying one shared adjustment independently to each
bound source layer, preserving each layer's editable pixels and blend context.
Affect-below stack adjustment and a shared-target adjustment have distinct
semantics and must be named and exposed separately. This interpretation is
explicit for maintainer review; compositing the target set first would be a
different operation and must not be silently substituted.

## R1 — Multi-layer Liquify

- [ ] Capture explicit raster IDs, with reviewed expansion of selected groups or
  persistent links. Deduplicate once and show the exact target set before editing.
- [ ] Apply one document-space deformation field to every captured owner while
  preserving separate raster buffers, IDs, order, groups, opacity and blend modes.
- [ ] Define linked/unlinked raw mask behavior, selection coverage, protected
  owners, alpha/transparent edges, canvas clipping and interpolation before release.
  Validate every target before contact; an unsupported owner rejects the whole edit.
- [ ] Provide brush size/strength feedback and reversible cached previews with
  Apply/Cancel. A stale document/target revision requires an explicit refresh.
- [ ] Publish one Undo/Redo group, revision and compact verified outcome for the
  committed edit, including all targets. Failure restores the complete source.
- [ ] Keep immutable-source deformation and bounded reusable/tiled buffers in
  native plugins. Define contact, target, mesh/field and preview budgets; avoid
  per-MOVE serialization, whole-image copies and repeated graph/registry discovery.
- [ ] Expose the same target/field contract through actions, schemas, workflow
  nodes, MCP and headless. Replay restores outcomes without running Liquify again.

Acceptance: deform ink, base color and shadow together; cancel, Undo, Redo, save/
reopen and DLL-free replay preserve all raw owners and their alignment. Cover
masked/grouped/hidden/locked and failed/stale cases. Report latency, peak memory,
dirty upload area and event growth on recorded small and representative artwork
profiles. Synthetic tests and a native-only prototype do not prove artist readiness.

## R2 — Shared targeted adjustments

- [ ] Create one stable adjustment controller with explicit, inspectable target IDs
  and one parameter set. Bind/unbind multiple layers through normal layer UI.
- [ ] Define per-source evaluation order relative to source filters, masks, group
  gates and layer blending; expose the result in the generated composition view.
- [ ] Apply edits to the controller once, retaining independent target pixels and
  preserving all unbound layers. Avoid accidental double application.
- [ ] Define rename, reorder, duplicate, group, delete, missing-target and cycle
  behavior. Target IDs remain stable; selection/active layer changes do not retarget.
- [ ] Share preview/cancel, one Undo, atomic rollback, projects, verified replay,
  schemas/public state and node/MCP/headless execution with Session.
- [ ] Reuse a small set of useful existing color controls first. Add effects only
  when required by an illustration task, rather than to complete a filter catalog.

Acceptance: bind ink/color/shadow targets, change one setting and inspect the
per-target effect; rebind, undo, persist and replay it. Test noncontiguous and
cross-group targets, alpha, masks and a failing/unsupported target. Existing
ordered affect-below adjustment layers do not satisfy this binding requirement.

## R3 — Convenient groups

- [ ] Review actual interactions for multi-select Group, rename, nested drag/move,
  collapse/expand, selecting contents and group-wide visibility/opacity.
- [ ] Complete safe ungroup/reparent behavior with masks and adjustments; retain
  link sets, identities, target bindings and clear selection/active-layer feedback.
- [ ] Keep shortcuts searchable/configurable and make drop targets and invalid
  destinations visible. Each structural gesture has one Undo and full rollback.

Acceptance: organize a real illustration without JSON or node editing, then undo,
reopen and replay without losing owners, masks, binding relationships or ordering.
Existing group APIs are foundations; usability and missing ownership paths remain
open requirements rather than grounds to replace the implementation wholesale.

## R4 — Layer search

- [ ] Review existing name/visibility/lock/label filters against actual large-layer
  workflows; retain results independent of the active painting layer.
- [ ] Make hierarchical matches and their ancestors understandable, show a result
  count and offer explicit multi-selection/grouping/binding from the results.
- [ ] Keep typing read-only and responsive. Clearing a query restores the normal
  tree; search never silently hides document content or expands mutation targets.

Acceptance: locate and select a named ink/color set, group it or bind an adjustment,
clear the query and retain predictable selection. Validate CJK names and deep trees.
Measure large-list interaction without rebuilding images or composition per keystroke.

## R5 — Symmetry editing

- [ ] Make document axes, mode, positions and participating tools explicit, with
  nominal cursor/gesture feedback consistent with mirrored Canvas views.
- [ ] Verify the current native/Session Brush and Eraser symmetry source increment
  before extending it. Pressure, stabilization, Flow, overlap limits, masks and
  selection retain their documented behavior.
- [ ] Extend symmetry to Liquify with correctly reflected displacement vectors and
  deduplicated axis contacts. Evaluate transform/selection gestures as separate
  declared tool capabilities with defined target and preview semantics.
- [ ] Share axis intent across supported tools without losing old brush presets or
  recordings. Each committed editing gesture remains one bounded Undo/outcome.

Acceptance: correct a symmetric face across several layers, inspect the reflected
feedback, cancel/apply and undo it. Unsupported tools show an explicit capability
boundary. A view flip or brush-only symmetry does not complete general symmetry editing.

## Delivery and evidence

1. Audit the exact implementation and CI status of existing ownership, grouping,
   search, adjustments and brush symmetry. Fix reliability gaps needed by R1/R2.
2. Deliver an end-to-end multi-owner Liquify slice: capture, bounded native field,
   reversible UI, commit/history/persistence/automation and independent references.
3. Deliver explicit shared-target adjustment binding with the existing useful
   adjustment controls and understandable layer feedback.
4. Improve grouping/search along those artist tasks, then extend symmetry correction.

Every increment identifies a named artist task, acceptance steps, current evidence
and open gates. Existing source, unit tests, transport parity, native screenshots,
manual workflow review, named tablet/driver evidence and measurements prove
different things; a CI pass alone does not certify the full painting experience.
Keep four-language presentation, configured spacing, small dependencies and one
Session/plugin/action execution core. Node editors and AI interfaces remain
available, while normal painting tasks work without opening them.

Use CI for heavy builds and regressions. Keep local verification light, under the
maintainer's CPU/priority limits. Benchmark profiles and numeric targets must be
recorded against named hardware; this plan invents no unmeasured latency guarantee.
Do not start a replacement autonomous goal until its scope is explicitly requested.

## Deferred reference backlog

Full Photoshop parity, broad photography filters, advanced text/typesetting,
printing/color-management expansion, layered-format parity and a plugin marketplace
are outside this acceptance baseline. They may be reconsidered for a concrete
painting workflow. Preserve the prior checklist and supported behavior rather
than deleting work or marking deferred items as completed.
