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
- Stable layer IDs, exact layer transforms, transparency locking, color fills, anchored canvas sizing and all-layer crop with history and outcome replay.
- Unified package metadata, host compatibility checks, lifecycle diagnostics and dependency-aware enable/disable plans shared by Settings and automation.
- English-only embedding; external Chinese, Japanese and Korean catalogs with profile overrides, owner-scoped plugin catalogs and background hot reload.

Current boundaries include one document per Session, in-process native plugins, no crash-recovery workflow, and no clipboard, free transforms or layer groups. Atomic document execution does not roll back arbitrary file, network or plugin-private effects. See [Architecture](docs/architecture.md), [Replay](docs/replay.md) and [Atomic Operations](docs/atomic-operations.md) for the exact contracts.

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

The next plugin increment should build on the existing registry: an authoring kit and conformance runner, clearer package diagnostics and upgrade guidance. Locale edits already hot-reload; package activation, contribution changes and DLL replacement still require restart. Do not present runtime DLL unloading or a plugin marketplace as available. Reliability measurement and selection contracts remain priorities alongside plugin authoring.

The maintainer's expanded [Photoshop-common editing plan](docs/photoshop-essentials.md) breaks the PhotoCraft-inspired functional work into P1–P8 increments. Follow its dependency order and acceptance checks while preserving the architecture and reliability gates above; unchecked entries remain planned work.

## M1 — Reliable document foundation

- [ ] Audit rollback across single actions, active-contact transitions, structural edits and project lifecycle operations. Make unsupported atomic effects explicit before execution.
- [ ] Add durable recovery checkpoints and a recovery UI. Recover only validated committed outcomes; never rerun DLL commands or external side effects to restore a canvas.
- [ ] Stream archive verification/export and introduce bounded seek checkpoints so large histories do not require loading an entire archive into memory.
- [ ] Design segmented history with linked manifests and clear retention/export controls. Preserve verification anchors and compatibility; never silently drop operations or compress away committed intermediate results.
- [ ] Expose committed versus provisional state consistently in the viewer, MCP and execution reports. Define how retries, interrupted contacts and storage failures are reported.
- [ ] Establish repeatable latency, peak-memory, binary-size and bytes-per-operation measurements for small strokes, large brushes, noisy images, long sessions and atomic batches.

**Acceptance:** fault injection at prepare, append, save and recovery boundaries leaves either the previous committed state or the complete new state. Legacy fixtures still verify. Memory and recording growth have documented bounds, and artists can recover their last committed canvas after an interrupted session.

## M2 — Essential painting and editing

- [x] Introduce stable layer IDs, persist names/identities and remap image-node references for reorder, rename, duplication and deletion.
- [x] Add exact layer flips, 180° rotation and clipped integer offsets through native plugin actions.
- [x] Add connected fill with tolerance, linear/radial gradient gestures and inline pressure controls through shared native actions.
- [x] Add whole-layer color matching, fill expansion/contraction and feathering, repeated/reflected gradients, transparency locking and layer color fills.
- [x] Add anchored canvas resizing and exact all-layer crop with dimension-aware history, atomic rollback and outcome replay.
- [x] Add layer Merge Down, Merge Visible, Stamp Visible and transparent-canvas flattening with explicit graph/lock contracts, undo, atomic rollback and verified replay.
- [ ] Add layer groups with explicit reference and history behavior.
- [ ] Make structural and multi-layer edits reliably undoable; define optional undo grouping separately from transaction rollback.
- [x] Add bounded selection persistence, native shape/combine commands, PS selection shortcuts and drawing/action output clipping.
- [x] Add direct rectangle/ellipse marquee and freehand/polygonal lasso gestures, reversible previews and connected-fill selection traversal.
- [x] Add tolerance-based Wand, Grow and Similar commands with native active/merged sampling and shared automation contracts.
- [x] Add direct Magic Wand activation, session-local sampling controls and a Grow/Similar menu.
- [x] Add persisted searchable selection defaults, preview/cancel and shared profile validation.
- [x] Add native selection expansion/contraction, feathering and smoothing with shared actions/history.
- [ ] Add clipboard operations.
- [ ] Add selection-aware move, scale and rotate.
- [ ] Add useful document controls: new-canvas presets, resolution/background choices, clearer dirty state and save/recovery feedback.

**Acceptance:** an artist can create, select, transform and organize a small illustration, save/reopen it, undo/redo its structural edits and replay the same visible results. GUI, MCP and headless actions produce matching pixels for shared fixtures. Graph references remain valid after layer changes.

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
- [ ] Add flow, textured tips and richer brush dynamics through tool plugins, with accurate size/shape cursor previews.
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

Cloud accounts, mandatory online AI, real-time collaboration, a plugin marketplace, a general scripting runtime and full Photoshop compatibility are not near-term commitments. macOS/Linux support and untrusted-plugin sandboxing require separate platform and execution designs. These may be explored after the Windows editing foundation is dependable and their costs are measured.

## Rules for every milestone

1. Keep one document execution contract across the desktop, plugins, nodes, MCP and headless clients.
2. Keep JSON discovery, graph compilation and synchronous disk work out of individual pen MOVE calls.
3. Record committed operations losslessly, verify reconstructed outcomes and preserve documented format compatibility.
4. Use shared tiles, dirty regions and bounded caches; justify new dependencies with measured costs.
5. Specify transaction, undo, cancellation and external-effect behavior before implementing a new action.
6. Update schemas, examples, translations, documentation and meaningful regression checks with the feature.

Use a focused issue or proposal for a milestone item, including its public action/state contract, failure behavior, performance impact and acceptance evidence. Update this roadmap when priorities change; move completed work into [CHANGELOG.md](CHANGELOG.md). See [Contributing](CONTRIBUTING.md) for repository and review conventions.
