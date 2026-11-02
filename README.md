# MiCoPi

[English](README.md) · [简体中文](README.zh.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

A small Rust painting application focused on responsive pen input and an extensible, AI-ready execution core.

MiCoPi runs on **Windows 10/11 x64**. The desktop workbench uses **egui + egui_dock** with OpenGL; pen input uses Windows Ink. The project is in early development.

**0.21.0:** [Atomic document groups](docs/atomic-operations.md) roll back failed batches and publish one compact event. MCRPLY02 adds metadata/XOR deltas and tile references while retaining v1 reading. [Verified event replay](docs/replay.md) records document operations, pressure input and plugin outcomes. Preview the drawing process, verify its SHA-256 chain, save history inside projects, or export a replay archive and PNG time-lapse frames. English is the default language, with Chinese, Japanese and Korean available.

![Painting workbench](docs/screenshots/workbench.png)

Connected fill with tolerance, linear/radial gradient gestures and inline pressure dynamics are available in the Brush panel. See [Painting tools](docs/painting-tools.md).

## Why MiCoPi

**Painting first. Plugins carry behavior. One execution core serves artists and automation.**

Typed pen input protects the drawing path; schema-described actions and public state make the same application inspectable from plugins, MCP and headless clients. Image composition stays non-destructive, and workflow execution remains explicit. AI-ready means a documented integration contract, not a required cloud assistant.

Read the illustrated [design philosophy](docs/philosophy.md), follow the [roadmap](ROADMAP.md), browse the [documentation index](docs/README.md), or see the [directory and ownership guide](docs/project-structure.md).

## Features

- DLL-based brush, eraser, eyedropper, hand and zoom tools.
- Pen pressure, pressure curves, stroke stabilization and brush outline cursors.
- HSV wheel, RGB controls, swatches and brush presets.
- Layers, visibility, opacity, blend modes and tiled undo/redo.
- Dockable, floating and tabbed panels; configurable spacing, sizing, themes and shortcuts.
- Searchable settings, JSON editing, command palette and shortcut recording.
- Transparent PNG export, PNG/JPEG/BMP import and `.mcp` projects.
- Non-destructive [image graphs](docs/node-editor.md) for filters and blending.
- [Plugin workflow graphs](docs/workflow-nodes.md) generated from action and state schemas.
- Independent native DLL plugins, headless JSONL, MCP stdio and attachment to a running GUI.
- [Atomic document groups](docs/atomic-operations.md), shared-tile rollback and compact verified events.
- Verified [event replay](docs/replay.md), time-lapse preview and PNG frame sequences.

Layers support renaming, duplication (**Ctrl+J**), reordering and deletion, plus exact flips, 180° rotation and integer offsets. See [Layer editing](docs/layer-editing.md).

Current limits: one document per session; no Wintab, selections, free transforms, layer groups/merging or watercolor engine. Native plugin DLLs execute inside the host process.

## Build and run

Requires Rust 1.88+, Visual Studio C++ Build Tools and the Windows SDK.

```powershell
.\scripts\build.ps1 -PluginsOnly
cargo run --release --locked
.\scripts\build.ps1 -Test
```

Run `dist/MiCoPi.exe`. Keep `plugins/` and `licenses/` beside the executable. End users do not need Rust. A project-local portable toolchain is available; see [Development](docs/development.md).

```powershell
New-Item -ItemType Directory -Force target/example-output | Out-Null
.\dist\micopi-cli.exe run examples/automation/draw.jsonl
.\dist\micopi-cli.exe actions
.\dist\micopi-cli.exe mcp
cargo build --release --no-default-features --bin micopi-cli --locked
```

MCP normally owns an independent canvas. Use `mcp --attach <profile>/session.json` to operate on an open GUI. See [Plugins and Automation](docs/plugins-and-actions.md), [Native SDK](docs/native-plugins.md) and [AI Pipeline](docs/ai-pipeline.md).

## Display language

Open Settings with **Ctrl+,**, search for **Display Language** or `ui.language`, choose a language and save. Changes preview immediately; closing without saving restores the previous settings.

```json
{"version":1,"ui":{"language":"en"}}
```

Supported codes: `en`, `zh-CN`, `ja`, `ko`. Existing configurations without a language use English. Language selection does not change action IDs, key names, node ports, document pixels or file paths.

## Source layout

| Path | Responsibility |
| --- | --- |
| `src/application/` | Session, validation, pipeline, workflow compilation, codec |
| `src/plugins/` | Host handlers, registry, DLL loading and typed tool routing |
| `plugins/` | Manifests and independent native plugin source |
| `crates/plugin-api/` | Dependency-free C ABI SDK and C header |
| `src/automation/` | JSONL, MCP and GUI attachment |
| `src/desktop/` | Workbench, canvas presentation, settings and node editors |
| `src/engine/` | Document, image graph, tiled history and input models |
| `src/configuration.rs` | Validated settings and atomic persistence |
| `src/i18n.rs`, `locales/` | Embedded English-first translations |
| `src/platform/` | Windows Ink, DWM, native dialogs and file replacement |
| `docs/` | English guides, reference schema and curated screenshots |
| `examples/` | Automation, configuration, MCP and plugin examples |
| `scripts/` | Reproducible build, packaging, smoke and repository checks |

See [Architecture](docs/architecture.md), [User Guide](docs/user-guide.md), [Contributing](CONTRIBUTING.md) and [Changelog](CHANGELOG.md). The legacy GDI workbench remains a regression reference and is not the production desktop entry point.

`Cargo.lock` is retained. Build outputs, downloaded toolchains and personal settings are ignored by Git.

## Contributing and version control

See [Contributing](CONTRIBUTING.md). Start with the [development guide](docs/development.md), use a focused branch and commit coherent, verified changes. Release tags use `vX.Y.Z`. Runtime binaries and personal files stay outside source history; selected documentation screenshots stay versioned.

```powershell
python scripts/check-repository.py
cargo fmt --all -- --check
```

## License

Application source: [MIT](LICENSE). Embedded Google Material icons: [Apache 2.0](assets/material-symbols/LICENSE), with [notices](assets/material-symbols/NOTICE.md). Build outputs include third-party dependency licenses under `dist/licenses/`. Downloaded toolchains retain their upstream licenses.
