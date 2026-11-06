# Project Structure and Ownership

The repository separates executable source, plugin packages, examples, documentation and local output. Paths that participate in the runtime contract remain stable; documentation and examples are grouped for navigation.

```text
micopi/
├── README.md                 English project overview
├── README.zh.md              Chinese overview
├── README.ja.md              Japanese overview
├── README.ko.md              Korean overview
├── ROADMAP.md               Priorities, milestones and acceptance criteria
├── AGENTS.md                Repository instructions for coding agents
├── Cargo.toml / Cargo.lock   Host package, workspace and locked dependencies
├── src/
│   ├── main.rs              Desktop entry point
│   ├── bin/micopi-cli.rs    CLI and MCP entry point
│   ├── application/         Session, action and pipeline contracts
│   ├── automation/          JSONL, MCP and GUI attachment
│   ├── desktop/             Shared window host, input routing and texture cache
│   ├── engine/              Document, composition, tiled history and replay codec
│   ├── plugins/             Host registry, DLL loader and adapters
│   ├── platform/            Windows input, chrome and file integration
│   ├── ui/                  Commands, shortcuts and workspace models
│   ├── configuration.rs     Validated settings persistence
│   ├── i18n.rs              Embedded English and owner-aware presentation lookup
│   └── i18n_external.rs     Bounded external catalog watcher and snapshots
├── crates/plugin-api/       Independent C ABI SDK and C header
├── plugins/                 Feature UI adapters, manifests and DLL source
├── locales/                 English source and three external UI catalogs
├── assets/                  Runtime assets and upstream notices
├── docs/
│   ├── README.md            Guide index
│   ├── philosophy.md        Product and engineering principles
│   ├── reference/           Machine-readable reference files
│   └── screenshots/         Current captures and a historical archive
├── examples/
│   ├── automation/          Runnable JSONL programs
│   ├── config/              Current settings examples
│   ├── mcp/                 Client connection examples
│   ├── plugins/             Small extension examples
│   └── legacy/              Historical INI examples
├── scripts/                 Build, bootstrap, smoke and repository checks
├── tests/                   ABI/replay fixtures and reference implementations
├── .github/                 CI and contribution templates
├── target/                  Local builds, toolchains and test output [ignored]
├── dist/                    Local release artifacts and licenses [ignored]
└── artworks/                Personal artwork [ignored]
```

## Runtime ownership

`src/plugins/` and root `plugins/` have different jobs. The former is host integration: registration, validation, lifecycle and dispatch. The latter is the installable extension boundary: manifests and independent DLL implementations. Feature UI adapters are source-owned in `plugins/<feature>/panel.rs` and compiled into the desktop host. Shared window lifecycle lives in `src/desktop/windows.rs`; native viewport presentation lives in `src/desktop/native_windows.rs` and topology-preserving group moves in `src/desktop/window_groups.rs`; the unified grip/tab/button row lives in `src/desktop/group_bar.rs`; cross-viewport drag/drop resolution lives in `src/desktop/group_drop.rs`; the document texture cache lives in `src/desktop/document_texture.rs`. This source boundary does not expose egui objects through the native C ABI. See [Plugin Windows](plugin-windows.md). Keep new pixel algorithms in the appropriate plugin, while the host retains document and history ownership.

The `engine` directory contains shared document structures and the image graph model. The legacy GDI UI under `src/platform/windows/` remains a regression reference. It is not the production desktop entry point; moving it requires its own measured refactor, not a cosmetic directory change.

`src/plugins/packages.rs` owns package metadata validation, lifecycle inventory, contribution ownership and dependency planning. Loaded packages declare optional external language files in their manifests; `src/i18n_external.rs` merges those owner-scoped files with host catalogs and profile overrides. Only English is compiled into the executable. See [Plugin System](plugin-system.md) and [Localization](localization.md).

## Document execution modules

| Path | Responsibility |
| --- | --- |
| `src/application/session.rs` | Single document owner, validation and execution boundaries |
| `src/application/atomic.rs` | Shared-tile rollback checkpoints and atomic action capability checks |
| `src/application/replay.rs` | Recording boundaries, project persistence and replay actions |
| `src/engine/document.rs` | Pixels, composition and copy-on-write undo tiles |
| `src/engine/canvas_editing.rs` | Staged geometry, shared transform validation/publication and borrowed-source preview composition |
| `src/engine/layer_merge.rs` | Compositor-backed layer consolidation and structural buffer ownership |
| `src/engine/group_merge.rs`, `src/application/group_merge_tests.rs`, `scripts/smoke-group-merge.py` | Staged subtree/visible-group baking, retained hidden branches and shared action/history/replay verification |
| `src/engine/selected_merge.rs`, `src/plugins/selected_merge.rs` | Pure explicit-target/LCA projection plans, staged selected-only baking, shared merge dispatch and bounded presentation requests |
| `plugins/layers/merge_selected.rs` | Compiled nonmodal selected-merge review, captured-target/revision guards and shared plan/execute controls |
| `src/application/selected_merge_tests.rs`, `scripts/smoke-selected-merge.py` | Selected-only pixels, retained buffer/filter identities, graph budgets, contacts, history/rollback and cross-client replay verification |
| `plugins/compositing/src/blends.rs`, `plugins/compositing/src/blend_tests.rs` | Independent extended color equations, exact rounding and native/reference compatibility checks |
| `src/engine/blend_modes.rs`, `src/desktop/blend_modes.rs`, `src/application/blend_mode_tests.rs`, `scripts/smoke-blend-modes.py` | Shared renderer-capability preflight, bounded family menus and cross-client blend/history/replay checks |
| `src/engine/mix.rs`, `plugins/compositing/src/mix_tests.rs`, `src/application/pass_through_tests.rs`, `scripts/smoke-pass-through.py` | Backdrop-mix capability preflight, native interpolation references and Pass Through policy/history/transport verification |
| `src/application/layer_query.rs`, `src/application/layer_organization_tests.rs` | Read-only layer discovery, organization schema and lock/label/history/replay contracts |
| `src/engine/layer_selection.rs`, `src/plugins/layer_batch.rs` | Sparse stable-ID layer selection, bounded history grouping and explicit shared-handler batch execution |
| `src/engine/groups.rs`, `src/engine/group_editing.rs`, `src/plugins/groups.rs` | Versioned group trees, cached isolated composition/visibility, buffer-preserving hierarchy edits and shared public commands |
| `plugins/layers/groups.rs`, `plugins/compositing/groups_view.rs` | Compiled hierarchy/property inspection and the actual-composition node view |
| `src/application/group_tests.rs`, `scripts/smoke-groups.py` | Group composition, structural/history/persistence contracts and cross-transport parity |
| `src/application/layer_batch_tests.rs`, `scripts/smoke-layer-batch.py` | Batch selection, atomic/history/persistence contracts and transport parity |
| `plugins/layers/panel.rs` | Compiled layer properties, metadata-based filter cache, markers, names and responsive rows |
| `src/engine/selection.rs` | Immutable coverage storage, bounded encoding and shared selection snapshots |
| `src/application/movement_preferences.rs`, `src/application/movement_tests.rs` | Validated move targets/steps, public state and shared history/replay regressions |
| `src/engine/homography.rs`, `src/application/perspective_tests.rs` | Finite convex quad geometry, horizon validation and projective action/history/persistence contracts |
| `src/application/output_size.rs`, `src/application/export_tests.rs` | Pure output-unit/aspect planning and shared export/history/transport contracts |
| `src/application/units.rs`, `src/application/creation.rs`, `src/application/physical_creation_tests.rs` | Shared bounded physical-unit conversion, creation plans without raster allocation and lifecycle/persistence regressions |
| `src/application/image_size.rs`, `src/application/physical_resample_tests.rs` | Unit-aware all-layer/mask sizing plans, source-dependent limits and atomic resampling/replay contracts |
| `plugins/canvas/image_size.rs` | Compiled physical Image Size draft, density/revision guards, shared planning and responsive controls |
| `src/plugins/export.rs` | Borrowed composed-frame sampling, preview/file orchestration and compact artifact context |
| `plugins/canvas/export.rs` | Compiled export draft, explicit cached preview, source guards and responsive controls |
| `src/application/png_io.rs`, `src/application/png_density_tests.rs` | Bounded PNG scanning, shared density encoding, legacy-byte and persistence/transport regressions |
| `src/application/geometry_density_tests.rs` | Density axis semantics under document geometry and structural undo |
| `src/engine/document_info.rs`, `src/application/document_setup_tests.rs` | Bounded physical metadata, legacy omission and setup/history/replay contracts |
| `src/configuration/canvas.rs` | Validated central new-canvas defaults and artist preset collections |
| `plugins/canvas/new_document.rs` | Compiled preset dialog, revision-guarded creation, sizing and cancellation regressions |
| `src/engine/transform.rs`, `src/application/transform_repeat_tests.rs` | Bounded document-owned repeat recipes, persistence validation and repeat/copy history/replay contracts |
| `src/application/skew_tests.rs` | Local shear order, coefficient bounds and native action/history/rollback contracts |
| `src/plugins/affine.rs`, `src/application/affine_tests.rs` | Shared affine plans, staged outputs, borrowed-source preview composition and action/history/replay contracts |
| `plugins/painting/src/projective.rs` | Optional homography sampling export, independent references, rejection fixtures and kernel measurements |
| `plugins/painting/src/affine.rs` | Allocation-free inverse affine sampling with transparent edges and independent references |
| `plugins/canvas/transform_perspective.rs`, `plugins/canvas/transform_perspective_tests.rs` | Compiled four-corner controls, provisional gestures, affine-frame conversion and viewport checks |
| `plugins/canvas/transform.rs`, `plugins/canvas/transform_tests.rs` | Reversible numeric/handle drafts, idle textures, source guards and UI regressions |
| `src/plugins/movement.rs` | Atomic movement orchestration, resolved targets, staged masks/pixels and compact context |
| `plugins/painting/src/movement.rs` | Allocation-free integer layer/selected-pixel/mask kernels and native references |
| `plugins/canvas/movement.rs` | Cached outline drag, source guards, cancellation and Tool Options adapter |
| `src/application/selection_sampling.rs` | Configured selection defaults, session control overrides and compact public state |
| `src/plugins/layer_transfer.rs` | Selection-to-layer orchestration through native extraction/placement and one host-owned undo entry |
| `src/platform/clipboard/` | Bounded PNG/DIB image codecs, desktop resource adapter and Win32 memory/window ownership |
| `src/plugins/system_clipboard.rs` | Explicit system transfers, verified origin hints, public state and external-effect audit context |
| `src/plugins/resample.rs` | Image dimensions/aspect validation and staged all-layer/mask resampling publication |
| `plugins/painting/src/resample.rs` | Rational pixel-center nearest/bilinear sampling and bounded axis plans |
| `plugins/painting/src/resample/area.rs` | Exact shrinking footprints, per-axis linear enlargement and bounded streaming premultiplied sums |
| `plugins/canvas/geometry.rs` | Compiled framing, crop and aspect-locked image sizing dialogs |
| `plugins/canvas/trim.rs` | Compiled trim controls, revision guards and parameter-cached read-only bounds preview |
| `src/plugins/trim.rs` | Shared trim options, result metadata and all-layer frame publication |
| `plugins/painting/src/trim.rs` | Allocation-free alpha/corner bounds, also reused for raw-layer transform handles |
| `src/plugins/geometry.rs` | Shared exact-geometry orchestration, staged masks/layers and lock/selection guards |
| `plugins/painting/src/geometry.rs` | Allocation-free tiled orthogonal pixel and coverage permutations |
| `src/engine/canvas_editing.rs` | Dimensioned framing/geometry publication and shared-buffer structural history |
| `src/plugins/clipboard.rs` | Bounded Session clipboard ownership, transfer actions and public summaries |
| `plugins/painting/src/selection/transfer.rs` | Allocation-free native extraction and clipped placement kernels |
| `src/plugins/selection.rs` | Selection schema dispatch, history publication and DLL geometry calls |
| `plugins/painting/src/selection.rs` | Typed geometry and premultiplied selection blending kernels |
| `plugins/painting/src/selection/edges.rs` | Exact disk morphology, fixed-point feather/smoothing and independent references |
| `plugins/painting/src/selection/color.rs` | Immutable color sampling, connected selection and exact Grow/Similar palette kernels |
| `plugins/canvas/selection.rs` | Cached desktop selection outline presentation |
| `plugins/canvas/selection_gesture.rs` | Bounded marquee/lasso previews and single-action completion adapters |
| `src/engine/linked_graph.rs` | Validated layer-stack topology, per-layer filter branches and metadata-only synchronization |
| `src/engine/replay.rs` | Journal storage, compressed tile/metadata encoding and integrity verification |
| `plugins/replay/panel.rs` | Read-only time-lapse presentation |
| `plugins/replay/manifest.json` | Public action/state registration for automation and workflow nodes |

The host owns transaction and verification rules. Plugins own drawing and processing algorithms; transports and the desktop dispatch through the same Session. See [Atomic Operations](atomic-operations.md) and [Verified Replay](replay.md) for the contracts.

## Tests and local output

Unit tests stay near their implementation. Shared support lives under `tests/reference/`, ABI rejection fixtures under `tests/fixtures/native-incompatible/`, and historical recording fixtures under `tests/fixtures/replay/`. Keep each binary fixture with its provenance and expected hashes. See [Development](development.md) for commands and `tests/README.md` for shared support.

Generated smoke captures, extracted packages, benchmarks and artwork from examples belong under ignored `target/test-output/` or `target/example-output/`. Only reviewed screenshots are promoted to `docs/screenshots/`; portable deliverables stay under ignored `dist/`.

## Documentation and screenshots

English technical guides live directly under `docs/`, with an index in `docs/README.md`. Translated root READMEs summarize the same capabilities and link to the English technical contracts. Current screenshots use stable, descriptive filenames. Older captures are retained under `docs/screenshots/archive/` and are not used to represent the current workbench.

Runtime assets belong in `assets/`; documentation screenshots do not. Avoid placing personal artwork, test output or raw captures in either directory. Promote only selected, reviewed captures from `target/test-output/` into documentation.

## Version-control boundaries

Track source, manifests, translation catalogs, SDK headers, reference examples, selected screenshots, notices and `Cargo.lock`. Ignore compiled executables/DLLs, all Cargo target directories, downloaded toolchains, release archives, session endpoints, personal settings, editor state and artwork.

The repository uses `main` as the baseline branch. Follow the maintainer's requested workflow: current maintainer-directed agent work commits coherent changes directly to `main`; external contributions normally use focused branches. Annotate reviewed release versions with `vX.Y.Z`. Keep the existing release version when a change only organizes documentation. Never manufacture historical commits for versions that were not previously recorded. Agent-specific instructions live in [AGENTS.md](../AGENTS.md).

Run `python scripts/check-repository.py` after moving documentation or examples. Run the relevant build and smoke checks when executable paths, runtime contracts or source behavior change. See [Contributing](../CONTRIBUTING.md) for the Git workflow.
