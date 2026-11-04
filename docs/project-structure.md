# Project Structure and Ownership

The repository separates executable source, plugin packages, examples, documentation and local output. Paths that participate in the runtime contract remain stable; documentation and examples are grouped for navigation.

```text
micopi/
├── README.md                 English project overview
├── README.zh.md              Chinese overview
├── README.ja.md              Japanese overview
├── README.ko.md              Korean overview
├── ROADMAP.md               Priorities, milestones and acceptance criteria
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
│   └── i18n.rs              Embedded translation lookup
├── crates/plugin-api/       Independent C ABI SDK and C header
├── plugins/                 Feature UI adapters, manifests and DLL source
├── locales/                 Four versioned UI catalogs
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

## Document execution modules

| Path | Responsibility |
| --- | --- |
| `src/application/session.rs` | Single document owner, validation and execution boundaries |
| `src/application/atomic.rs` | Shared-tile rollback checkpoints and atomic action capability checks |
| `src/application/replay.rs` | Recording boundaries, project persistence and replay actions |
| `src/engine/document.rs` | Pixels, composition and copy-on-write undo tiles |
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

The repository uses `main` as the baseline branch. Use focused work branches and coherent commits, and annotate reviewed release versions with `vX.Y.Z`. Keep the existing release version when a change only organizes documentation. Never manufacture historical commits for versions that were not previously recorded.

Run `python scripts/check-repository.py` after moving documentation or examples. Run the relevant build and smoke checks when executable paths, runtime contracts or source behavior change. See [Contributing](../CONTRIBUTING.md) for the Git workflow.
