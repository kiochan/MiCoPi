# MiCoPi Roadmap

MiCoPi aims to be a small, responsive painting application whose document operations are inspectable, replayable and accessible to artists, plugins and AI clients through one execution core.

This roadmap starts from the **0.21.0 implementation and subsequent work on main**. Checked items are implemented; unchecked items describe intended work, not release dates or promises that a feature already exists. The [Changelog](CHANGELOG.md) records completed work; the [design philosophy](docs/philosophy.md) explains the principles behind these priorities.

## Implemented baseline

- Native Rust desktop with independent plugin-owned control bar and canvas windows, dock/float/tab commands, searchable settings, configurable shortcuts and four display languages.
- DLL tools with pressure and stabilization, layers, tiled undo/redo and non-destructive image composition.
- Schema-described actions, public plugin state, workflow nodes, JSONL, MCP and GUI attachment.
- Verified outcome replay, compact MCRPLY02 events, atomic document groups and legacy recording support.
- Linked layer composition with per-layer plugin filters, synchronized structural edits and an explicit legacy/free graph mode.
- Brush spacing and minimum pressure opacity; connected/whole-layer color fill with expansion/contraction and feathering; repeated/reflected gradients.
- Stable layer IDs, exact layer transforms, transparency locking, color fills, anchored framing/crop, Image Size/Trim, Move/nudge and affine Free Transform with cancellable previews, history and outcome replay.
- Selected-only merge across noncontiguous/grouped rasters, explicit placement/consumption plans, contextual Ctrl+E and an optional guarded desktop review, with one structural undo/outcome.
- Unified package metadata, host compatibility checks, lifecycle diagnostics and dependency-aware enable/disable plans shared by Settings and automation.
- English-only embedding; external Chinese, Japanese and Korean catalogs with profile overrides, owner-scoped plugin catalogs and background hot reload.

Current boundaries include one document per Session, in-process native plugins, no crash-recovery workflow, and no verified system-clipboard interoperability or off-canvas layer storage. Raster masks support public painting/effects, document-wide geometry, plain single-layer owner copy/delete, owner-preserving group creation/hierarchy movement and mask-aware subtree copying/deletion; integer raster-owner movement and owner/exact-set affine Free Transform are supported, while owner/group perspective, masked selection/outline affine, owner Copy Repeat, group-mask movement and selected-pixel transfers, ungrouping, batches, merge/clipboard integration and clipping masks remain incomplete. Atomic document execution does not roll back arbitrary file, network or plugin-private effects. See [Architecture](docs/architecture.md), [Replay](docs/replay.md) and [Atomic Operations](docs/atomic-operations.md) for the exact contracts.

## Delivery order

| Milestone | Priority | Depends on | Outcome |
| --- | --- | --- | --- |
| M1 — Reliable document foundation | Next | Current baseline | Recoverable, bounded and consistently atomic editing |
| M2 — Essential painting and editing | High | M1 transaction/history contracts | Complete a small illustration without leaving the application |
| M3 — Plugin, node and AI authoring | High | M1 stable execution contracts | An independent author or AI client can build a useful extension |
| M4 — Brush and tablet refinement | High | M1 measurement infrastructure; M2 tool primitives | Better stroke quality across tested devices |
| M5 — Multiple documents and workspace | Later | M1; stable document/layer identifiers from M2 | Several canvases with explicit automation targets |
| M6 — Replay production and release readiness | Later | M1; M2–M4 daily-use validation | Shareable time-lapses and a dependable public alpha |

M2, M3 and M4 can progress independently after their foundation requirements are met. Every new editing feature must support actions, schemas, undo, recording and headless execution when introduced; AI integration is not deferred to a later rewrite.

Within that foundation, the next editing slices prioritize anime/manga painting:
reliable blend/mask behavior, linked multi-layer align/distribute,
non-destructive affect-below adjustment layers, brush flow/tips/dynamics/presets
and stabilization, then bounded Liquify corrections. The 26 color blend modes
are delivered; deterministic Dissolve remains pending in the
[editing checklist](docs/photoshop-essentials.md#p4--layer-organization-and-masks).
Editable text, complex photographic features and layered interchange remain
later backlog work. P1–P8 identify areas of that checklist rather than a strict
requirement to finish every earlier area first.

The next plugin increment should build on the existing registry: an authoring kit and conformance runner, clearer package diagnostics and upgrade guidance. Locale edits already hot-reload; package activation, contribution changes and DLL replacement still require restart. Do not present runtime DLL unloading or a plugin marketplace as available. Reliability measurement and selection contracts remain priorities alongside plugin authoring.

The maintainer's expanded [Photoshop-common editing plan](docs/photoshop-essentials.md) breaks the PhotoCraft-inspired functional work into P1–P8 increments. Follow its dependency order and acceptance checks while preserving the architecture and reliability gates above; unchecked entries remain planned work.

## M1 — Reliable document foundation

Layer organization now includes persistent pixel/position locks, color labels, read-only name/metadata filtering, stable-ID multi-selection and explicit shared-handler batches with one Undo/Redo group and verified outcome. These are shared with nodes/MCP/headless and retain host-enforced native write permissions. Raster masks support public painting/effects, document-wide geometry, plain single-layer owner copy/delete, owner-preserving group creation/hierarchy movement and mask-aware subtree copying/deletion; complete ungrouping and batch ownership integration, clipping masks and multi-layer align/distribute remain planned in the [editing checklist](docs/photoshop-essentials.md).

- [ ] Audit rollback across single actions, active-contact transitions, structural edits and project lifecycle operations. Make unsupported atomic effects explicit before execution.
- [x] Preserve shared contact admission through desktop commands, declared macros and runtime forms, including observations, draft synchronization, pending names and replacement external contacts. Keep public capabilities unchanged and frontend cache work out of production headless builds; see [Command Contact Admission](docs/command-contact-admission.md). The wider rollback/lifecycle audit remains open.
- [ ] Add durable recovery checkpoints and a recovery UI. Recover only validated committed outcomes; never rerun DLL commands or external side effects to restore a canvas.
- [ ] Stream archive verification/export and introduce bounded seek checkpoints so large histories do not require loading an entire archive into memory.
- [ ] Design segmented history with linked manifests and clear retention/export controls. Preserve verification anchors and compatibility; never silently drop operations or compress away committed intermediate results.
- [x] Align built-in state-bearing result revisions with their published outer response, retaining native/provider/settings ownership, historical nested reports and existing revision counts. Share optimistic follow-up guards across JSONL, MCP, GUI attachment and nodes; see [Action Results](docs/action-results.md).
- [ ] Expose committed versus provisional state consistently in the viewer, MCP and execution reports. Define how retries, interrupted contacts and storage failures are reported. A current response revision does not by itself certify a completed contact or durable save.
- [ ] Establish repeatable latency, peak-memory, binary-size and bytes-per-operation measurements for small strokes, large brushes, noisy images, long sessions and atomic batches.
- [x] Add request-correlated workspace/native capture FIFOs, physical framebuffer bounds, explicit terminal diagnostics and a lazy bounded PNG worker. Retain requests across discarded layout passes, reject stale/foreign outcomes and keep capture-only document revisions unchanged. Verify concurrent, hidden/docked, resized and four-language narrow/wide captures through independent PNG/client-size checks; keep unrelated quiet-period layout publication distinct from capture completion. See [Capture Requests](docs/capture-requests.md).

**Acceptance:** fault injection at prepare, append, save and recovery boundaries leaves either the previous committed state or the complete new state. Legacy fixtures still verify. Memory and recording growth have documented bounds, and artists can recover their last committed canvas after an interrupted session.

## M2 — Essential painting and editing

- [x] Introduce stable layer IDs, persist names/identities and remap image-node references for reorder, rename, duplication and deletion.
- [x] Add exact layer flips, 180° rotation and clipped integer offsets through native plugin actions.
- [x] Add connected fill with tolerance, linear/radial gradient gestures and inline pressure controls through shared native actions.
- [x] Add whole-layer color matching, fill expansion/contraction and feathering, repeated/reflected gradients, transparency locking and layer color fills.
- [x] Add anchored canvas resizing and exact all-layer crop with dimension-aware history, atomic rollback and outcome replay.
- [x] Add transparent/corner-color Trim with selected sides, read-only cached preview, atomic all-layer framing and verified replay. Reveal All remains dependent on off-canvas storage.
- [x] Add Image Size with aspect locking, native nearest/bilinear/area sampling, selection resampling, atomic undo and outcome replay. Area averages shrinking footprints with bounded row buffers.
- [x] Add DLL document quarter/half turns and flips, preserving layer identities and fractional selection masks; centered layer quarter turns explicitly clip to the current frame.
- [x] Add layer Merge Down, Merge Visible, Stamp Visible and transparent-canvas flattening with explicit graph/lock contracts, undo, atomic rollback and verified replay.
- [x] Add independent native perspective sampling, finite convex four-corner controls, persistent homogeneous repeat recipes, shared preview/node/MCP/headless execution and verified atomic outcomes.
- [x] Add ordered local X/Y skew through the shared affine action, numeric controls and reversible mouse/pen edge handles, preserving cached previews, repeat, undo and verified outcomes.
- [x] Add document-owned transform recipes, exact-matrix repeat and selected/whole-layer copy-repeat with configurable PS bindings, one undo/outcome, public state and shared nodes/MCP/headless execution. Clear recipes on changed canvas geometry and restore them with undo.
- [x] Add versioned stable-ID isolated groups, nested appearance, typed subtree operations, cached composition/visibility and deterministic undo/replay.
- [x] Add staged Group Merge and grouped Merge Visible, retaining outer appearance or hidden branches/group skeletons as appropriate, with one structural undo/outcome and shared schema/workflow/MCP/headless execution.
- [x] Add selected-only merge for explicit stable IDs across noncontiguous layers and parents. Preserve unselected buffers, filters, folders and masks; project intermediate gates over transparency while keeping the common parent/ancestors outside the bake. Share pure plans, contextual Ctrl+E, optional guarded review, one Undo/revision/outcome and failure rollback. Enforce source/projected/final graph budgets and expose deliberate stacking/backdrop changes. See [Selected-only merge](docs/selected-merge.md).
- [x] Expand shared layer/group/node color blending to 26 modes, with optional native capability discovery, preflight before mutation, bounded family menus and independent alpha/rounding references. Retain original mode IDs, pixels and supported recording hashes. See [Color blend modes](docs/blend-modes.md).
- [x] Add opt-in versioned Pass Through groups with seeded backdrop composition and once-only integer premultiplied group opacity, a capability-gated image Mix node, shared inspection/eligibility, undo and renderer-independent outcome replay. Keep isolated defaults and hashes unchanged; reject backdrop-dependent subtree baking. See [Pass Through](docs/pass-through.md).
- [x] Add native A8 tool loans and typed uniform/strided coverage composition as independently tested mask prerequisites, with compatible SDK tails, explicit capability negotiation and actual-DLL conformance.
- [x] Add the mask document/event foundation: bounded stable-ID A8 planes, byte-tiled history/rollback, strict MICOPI04 storage, separate verified byte outcomes and generated typed layer/Isolated/Pass Through coverage gates. Preserve default legacy writers/hashes and DLL-free restoration; reject unsupported edits before mutation. See [Raster-mask storage](docs/raster-mask-storage.md).
- [x] Expose the first layer/group mask editing subset: cached typed pressure/stabilized Brush/Eraser, selection conversion, enable/invert/density/feather, raster-layer raw-alpha application, bounded raw inspection, shared action/state/workflow contracts and four-language target/thumbnail controls. Retain source pixels, byte-tiled Undo, atomic rollback and verified outcomes. See [Raster mask editing](docs/raster-mask-editing.md).
- [x] Add an independent native raw-A8 reframe/permutation/resampling table with borrowed dense/uniform sources, explicit expansion fill, strict prewrite validation and preserved legacy kernels. Keep sampling independent from host-owned document publication/history.
- [x] Implement the independent optional raw-A8 affine prerequisite with explicit exterior bytes, borrowed dense/uniform sources, nearest/Q16 bilinear sampling, cached capability discovery and native/host/C ABI fixtures. Keep existing tables and kernels unchanged. Neutral-support bounds extend the optional table without changing its sampling prefix. This primitive is now consumed by the separate owner-affine contract; full runtime checks remain tracked in CI. See [Raw A8 affine](docs/native-a8-affine.md).
- [x] Integrate every raw layer/group mask with canvas framing/crop, document rotations/flips, Image Size and Trim, including hidden/disabled/unlinked planes. Share read-only frame plans, separate A8/effective costs, idle admission, complete all-plane staging, one Canvas Undo/outcome and cache/replay invalidation. Preserve white expansion, stable IDs/targets, exact physical-axis policy and fixed numeric feather. See [Mask canvas geometry](docs/mask-canvas-geometry.md).
- [x] Add plain single-layer duplicate/delete with owned dense/uniform masks, fresh result-ID schemas, explicit target retention/remapping, copied filter/opacity/flags, one compound Undo/outcome and idle admission through UI/nodes/MCP/headless. Preserve unrelated buffers/folders and exact atomic rollback; see [Mask owner editing](docs/mask-owner-editing.md).
- [x] Support mask-bearing `group.create`, `group.create_selected` and `group.move`, retaining owner IDs, raw A8 samples, target, active/selected raster IDs and source filter semantics. Preflight source/candidate capabilities and budgets, retain one structural Undo/revision/outcome and exact failure rollback, and expose guaranteed create-result `group_id` bindings without new action IDs. See [Mask group editing](docs/mask-group-editing.md).
- [x] Add mask-bearing Group Delete with complete descendant owner removal, original-order result-ID arrays, explicit target/selection fallback, one compound Undo/revision/outcome and exact failure recovery. Retain unrelated raw buffers/filter semantics, compact A8 damage and checkpoint-isolated resurrection; see [Mask group deletion](docs/mask-group-delete.md).
- [x] Add mask-bearing Group Duplicate with fresh breadth-first group, depth-first raster and source-ordered mask IDs; exact independent raw storage, mapped paint targets, explicit empty/nonempty selection and one compound Undo/revision/outcome. Preflight complete source/candidate limits before copying and preserve exact failure/checkpoint rollback; see [Mask group duplication](docs/mask-group-duplicate.md).
- [ ] Complete owner/group perspective, masked selection/outline affine, owner Copy Repeat, group-mask movement and selected-pixel transfers, ungrouping, structural batches, Layer via Copy/Cut, every merge/flatten/clipboard boundary and group-mask application. The full raster-mask feature remains unchecked until this matrix, public contracts and delivery gates pass. See [Raster Masks Design](docs/raster-masks-design.md).
- [x] Add whole raster-owner integer Move with linked-mask companions, independent stable-ID raw-mask Move/nudge and selection-outline movement. Retain unrelated planes, targets/selections/recipes, raw-only compound Undo, source/capability/idle admission and verified outcomes; keep group masks and selected-pixel transfers guarded. See [Raster owner movement](docs/raster-owner-movement.md).
- [x] Add persistent explicit stable-ID raster link sets, Link/Unlink controls and linked whole-layer Move/nudge. Capture exact IDs before dragging, preflight every owner/mask/lock and aggregate output cost, stage all results before one grouped Undo/revision/outcome and retain active-only painting. Expose shared commands/state/node bindings, structural projection, strict persistence and verified outcomes; see [Layer Links](docs/layer-links.md). Full CI and manual capture/device validation remain distinct evidence.
- [x] Implement exact-set linked and single-owner affine Free Transform with shared raw RGBA/A8 bounds, one pivot/matrix, linked-mask sampling, isolated cached previews and captured repeat scopes. Preserve active-only painting, masks/effects/filters and legacy unmasked perspective; publish one Undo/revision/outcome with rollback and DLL-free replay. Read plans preserve contacts, mutations require idle, and four-language desktop drafts capture source identities once. Source and transport/UI regressions are authored; runtime verification remains pending CI. See [Owner affine transforms](docs/owner-affine.md).
- [ ] Validate the multi-layer alignment/distribute-centers source increment: exact captured raster IDs, six align modes, Targets/Canvas alignment, deterministic integer center distribution, linked-mask carry, all-target admission and bounded staging with one Undo/outcome. Contact-safe plans, shared UI/actions/nodes/MCP/headless and independent references are implemented in source; full runtime validation is pending CI. See [Layer arrangement](docs/layer-arrangement.md). Keep key-layer references and edge-gap distribution separate.
- [ ] Complete non-destructive ordered adjustment layers alongside plugin nodes and direct pixel adjustments. The current [true adjustment-layer source increment](docs/adjustment-layers.md) adds five registered scalar filters, Normal alpha-preserving effect mixing, root/Isolated/Pass Through and owned-mask scope, separate inspection, strict persistence floors, actions and compound history/outcomes; runtime verification is pending CI. Levels/Curves, broader multi-parameter effects, additional adjustment blend/clipping semantics and full guarded-operation integration remain open; this umbrella is not complete.
  - Registered [Levels source nodes and adjustments](docs/levels.md) now share typed intent, cached native plans and dedicated Input/Output controls. Affect-below placement, Normal strength, owned masks, root/Isolated/Pass Through scopes, Undo, project metadata and verified outcomes are implemented in source. Session and independent JSONL/MCP/GUI smoke references are authored; runtime verification remains pending CI. Per-channel Levels and per-node/input histogram scope remain planned. Keep the broader adjustment feature unchecked until its complete contracts and delivery gates pass.
  - [Curves source nodes](docs/curves.md) now expose typed master/R/G/B intent and schemas through linked/free graphs, shared prepared rendering, contact guards, masks, structural projections and verified persistence. Native/Session/transport regressions await CI. Affect-below Curves now shares recursive parameter patches, strict optional intent, Normal opacity/owned masks, group scopes and verified outcomes; seven-case transport and adjustment regressions await CI. Shared local point controls now cover channel/interpolation selection, mouse gestures, numeric edits and cached native previews; UI/contact/cache/layout regressions await CI. Bounded read-only [histogram queries](docs/histograms.md) now expose explicit composite/raw scopes, exactness and sample grids through nodes/MCP/headless; native/host/transport regressions await CI. Manual cached histogram controls/overlays now serve source/adjustment editors with explicit published composite/raw scope, invalidation and contact guards; UI tests await CI. Next: per-node/input statistics, reviewed visual captures and measured performance. Keep the umbrella unchecked until those delivery gates pass.
- [ ] Add bounded native plugin Liquify with deterministic mesh/stroke deformation, immutable-source sampling, reversible cached previews and apply/cancel. Require explicit clipping/interpolation, one undo/revision/outcome, exact failure recovery, plugin-independent replay and measured contact latency/memory before delivery.
- [x] Make implemented structural/multi-layer edits undoable with buffer ownership and explicit batch undo groups, separately from atomic failure rollback.
- [x] Add bounded selection persistence, native shape/combine commands, PS selection shortcuts and drawing/action output clipping.
- [x] Add direct rectangle/ellipse marquee and freehand/polygonal lasso gestures, reversible previews and connected-fill selection traversal.
- [x] Add tolerance-based Wand, Grow and Similar commands with native active/merged sampling and shared automation contracts.
- [x] Add direct Magic Wand activation, session-local sampling controls and a Grow/Similar menu.
- [x] Add persisted searchable selection defaults, preview/cancel and shared profile validation.
- [x] Add native selection expansion/contraction, feathering and smoothing with shared actions/history.
- [x] Add bounded Session clipboard copy/cut/paste through shared actions.
- [x] Add selection-to-layer copy/cut with one undo entry and configurable PS shortcuts.
- [x] Add Windows PNG/DIB clipboard commands and configurable PS bindings, with explicit external-resource semantics.
- [ ] Verify native and cross-application clipboard roundtrips; private window-station creation is denied on the current test host.
- [ ] Add selection-aware move, scale and rotate.
- [x] Add central new-canvas presets, background choices and document-resolution metadata with revision-guarded creation and shared history/project/replay contracts.
- [x] Preserve exact PNG density in native document metadata, import/export and recorded-state output, including unknown/asymmetric axes and strict bounded validation.
- [x] Add shared PNG export sizing in pixels/physical units/percent, native composed-frame sampling, read-only previews, artifact digests and source-preserving desktop controls.
- [x] Add shared New Canvas plans and pixel/millimeter/inch input, preserving pixel presets, legacy metadata and verified creation lifecycle outcomes.
- [x] Add Image Size physical-unit input, all-layer/pixel-selection plans, exact-density conversion and revision-guarded desktop controls.
- [ ] Improve dirty-state and save/recovery feedback.

**Acceptance:** an artist can create, select, transform and organize a small illustration, save/reopen it, undo/redo its structural edits and replay the same visible results. GUI, MCP and headless actions produce matching pixels for shared fixtures. Graph references remain valid after layer changes. Selected-only merge preserves unselected storage and surviving filter identities, reports its explicit destination/hidden consumption, rejects unsupported topology before allocation and restores all consumed content on Undo; the selected-only result need not preserve outside-backdrop or interleaved-stack appearance.

## M3 — Plugin, node and AI authoring

- [x] Unify package metadata, host version validation, path-based status inventory and contribution ownership without replacing the existing C ABI.
- [x] Share dependency-aware enable/disable planning between Settings and `plugins.plan`; preserve running registry status until restart and protect required core packages.
- [x] Add isolated plugin language catalogs, profile overrides, translated command search and bounded background hot reload with last-good fallback.
- [ ] Publish a small, versioned plugin authoring kit with templates, schema examples, host compatibility rules and a reusable conformance runner.
- [ ] Improve schema validation and discovery for capabilities, units, defaults, errors, document targets and atomic scope. Keep display labels separate from machine identifiers.
- [ ] Improve node search, port compatibility feedback, parameter editing, execution progress and failed-node inspection. Keep image composition and action workflows as distinct typed domains.
- [ ] Let an AI client discover capabilities, read public state, propose/patch a graph, dry-run it, execute against an expected revision and inspect the committed result using the same public interfaces as the UI.
- [ ] Add package upgrade/migration guidance and diagnostics for failed locale files. Evaluate runtime activation only with explicit instance, document-reference and ABI lifetime rules; retain restart-based changes until those rules are implemented.
- [ ] Evaluate isolated plugin workers where crash containment is worth the overhead; retain the typed native path for latency-sensitive tools.

**Acceptance:** an independently built example registers an action, state and schema-derived node without host source changes. It passes conformance checks and runs through GUI, MCP and headless modes. Invalid schemas, stale revisions, unsupported atomic effects and incompatible plugins fail clearly without corrupting the document.

## M4 — Brush and tablet refinement

- [x] Add configurable dab spacing, minimum pressure opacity and inline pressure curve controls through the typed painting path.
- [x] Implement independent Flow and opt-in per-stroke opacity limits for native Brush/Eraser on RGBA and raw A8 targets, with pressure/stabilization, immutable origin-tile loans, sparse bounded coverage, shared settings/state/schema contracts and verified outcome replay. Preserve legacy defaults. Native/host/Session/UI/transport tests and a bounded synthetic contact measurement are authored and wired to CI; runtime results and physical-device validation are tracked separately. See [Stroke opacity](docs/stroke-opacity.md).
- [x] Add [analytic Brush/Eraser tips](docs/brush-tips.md): round/ellipse and square/rectangle shapes, 0.25–1 minor/major ratios, fixed clockwise angle, nominal cursor/preview contours and shared settings/actions/macros/state. Preserve pressure, stabilization, RGBA/A8 blending, legacy circular pixels/default recording bytes and twelve-field presets; negotiate nondefaults through an optional native tail. Session/native/UI/transport regressions and a bounded synthetic contact/candidate-work probe are authored and wired to CI; runtime results remain pending.
- [ ] Add sampled/textured tips, deterministic tip/size/opacity/color dynamics and bounded versioned brush preset editing/import/export through tool plugins, with accurate size/shape cursor previews. The [scalar user-preset source increment](docs/brush-presets.md) adds a cached 128-record Brush/Eraser library, twelve required scalar values plus optional analytic geometry preserving both swatches, exact provider checks and data-only import/export through shared actions; runtime verification is pending CI. Sampled tips/textures, additional dynamics and smoothing/stabilization refinement with measured latency, pressure continuity and cache bounds remain open; this umbrella is not complete.
- [ ] Surface tilt, rotation and device capabilities where the backend supports them, with explicit fallbacks for other devices and headless input.
- [ ] Build a Windows Ink device/driver test matrix covering hover, fast movement, focus changes, DPI, capture loss and interrupted contact.
- [ ] Profile before adding SIMD, parallel tile work or GPU painting. Preserve a lightweight CPU/headless path and document any rendering differences.
- [ ] Evaluate Wintab only against demonstrated compatibility gaps. Prototype watercolor/mixing separately until quality and cost are understood.

**Acceptance:** agreed stroke fixtures preserve pressure, stabilization, undo and replay. Device reports identify tested hardware and drivers. Compare input latency, commit latency and memory against recorded baselines; do not substitute a single average throughput number for responsiveness.

## M5 — Multiple documents and workspace

- [ ] Add document tabs with independent history, recovery, file state and view state, using bounded caches for inactive canvases.
- [ ] Extend action/state schemas with explicit document IDs while preserving a documented active-document default for existing clients.
- [ ] Make AI/MCP targets and revision guards unambiguous so an operation cannot accidentally edit another tab.
- [ ] Persist and restore workspace layouts and document sessions with clear handling of missing or changed files.

**Acceptance:** drawing in one tab does not change another tab's history or replay. Automation can select a document explicitly, and save/recovery flows handle several dirty canvases correctly. Cross-document transactions remain explicitly unsupported until a separate contract is implemented.

## M6 — Replay production and release readiness

- [ ] Extend replay with in/out ranges, background choices, configurable playback/export timing and export progress/cancellation.
- [ ] Provide an optional video-encoding integration for MP4 or other delivery formats without making a large encoder part of the basic painting runtime.
- [ ] Make exported clips traceable to a recording range and verification head. Keep integrity checking distinct from signatures or authorship claims.
- [ ] Expand CI, portable-package checks, migration fixtures, hardware feedback and long-session tests. Maintain English technical documentation and all four UI catalogs.
- [ ] Define public-alpha support boundaries, a reproducible release checklist and a practical bug-report workflow before calling a build stable.

**Acceptance:** selected replay ranges export without changing the artwork, cancellation leaves the source recording intact, and the portable package works from a fresh profile. Format upgrades preserve supported old projects and recordings. Published performance claims include workload, build and hardware context.

## Deliberately deferred

Editable text/path expansion, complex photographic processing and layered
PSD/TIFF/OpenRaster interchange remain visible later requirements in the
[editing checklist](docs/photoshop-essentials.md). They follow the current
painting, linked-layer, adjustment-layer and brush priorities rather than being
removed from scope. Ordinary file integrity and save/export fixes remain daily
workflow requirements.

Cloud accounts, mandatory online AI, real-time collaboration, a plugin marketplace, a general scripting runtime and full Photoshop compatibility are not near-term commitments. macOS/Linux support and untrusted-plugin sandboxing require separate platform and execution designs. These may be explored after the Windows editing foundation is dependable and their costs are measured.

## Rules for every milestone

1. Keep one document execution contract across the desktop, plugins, nodes, MCP and headless clients.
2. Keep JSON discovery, graph compilation and synchronous disk work out of individual pen MOVE calls.
3. Record committed operations losslessly, verify reconstructed outcomes and preserve documented format compatibility.
4. Use shared tiles, dirty regions and bounded caches; justify new dependencies with measured costs.
5. Specify transaction, undo, cancellation and external-effect behavior before implementing a new action.
6. Update schemas, examples, translations, documentation and meaningful regression checks with the feature.

Use a focused issue or proposal for a milestone item, including its public action/state contract, failure behavior, performance impact and acceptance evidence. Update this roadmap when priorities change; move completed work into [CHANGELOG.md](CHANGELOG.md). See [Contributing](CONTRIBUTING.md) for repository and review conventions.
