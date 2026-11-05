# MiCoPi

[English](README.md) · [简体中文](README.zh.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

A small Rust painting studio for anime and manga illustrators, focused on responsive pen input and an extensible, AI-ready execution core.

MiCoPi runs on **Windows 10/11 x64**. The desktop workbench uses **egui + egui_dock** with OpenGL; pen input uses Windows Ink. The project is in early development.

**0.21.0:** [Atomic document groups](docs/atomic-operations.md) roll back failed batches and publish one compact event. MCRPLY02 adds metadata/XOR deltas and tile references while retaining v1 reading. [Verified event replay](docs/replay.md) records document operations, pressure input and plugin outcomes. Preview the drawing process, verify its SHA-256 chain, save history inside projects, or export a replay archive and PNG time-lapse frames. English is the default language, with Chinese, Japanese and Korean available.

![Painting workbench](docs/screenshots/workbench.png)

The independent Tools bar and Canvas can float as native OS windows across monitors, dock on any side or form tab groups. Feature packages own their window contributions and UI source; every registered action is searchable and bindable. See [Plugin Windows](docs/plugin-windows.md).

Connected or whole-layer color fill with tolerance and edge controls, linear/radial gradients with repeat and reflection and inline pressure dynamics are available in the Brush panel. See [Painting tools](docs/painting-tools.md).

Canvas size, exact all-layer crop, document quarter/half turns and flips preserve layer/node identities, selection masks, undo and verified replay. Active-layer quarter turns keep the canvas size and clip beyond its frame. **Image Size (Ctrl+Alt+I)** scales all layers and masks from pixel, millimeter or inch inputs with aspect locking and nearest/bilinear/area sampling. Read-only plans expose resolved dimensions and buffer costs while preserving exact density. Area averaging retains thin-line coverage during reductions. Trim previews transparent or corner-color margins and crops selected sides across all layers with undo. See [Canvas editing](docs/canvas-editing.md).

**Move (V)** drags an outline and commits on release. Move the active layer, selected pixels or only the selection outline; arrow/Shift-arrow steps are configurable in searchable Settings. Off-frame pixels are clipped and restored by undo. Actions also run through nodes, MCP and headless. See [Move and nudge](docs/canvas-editing.md#move-and-nudge).

**Free Transform (Ctrl+T)** supports numeric and mouse/pen move, scale, skew, rotation, flips and pivots, with cancellable cached pixel previews and one undo/event. The same native action runs through nodes, MCP and headless. Pixels outside the current frame clip; **Ctrl+Shift+T** repeats the saved matrix; **Ctrl+Alt+Shift+T** creates a transformed copy. Recipes persist with projects, undo and replay. Edge diamonds adjust skew; perspective corner editing maps a convex quadrilateral with the same undo, repeat and replay contracts. See [Free Transform](docs/canvas-editing.md#free-transform).

**New Canvas (Ctrl+N)** offers configurable illustration, square, A4 and B5 presets, editable background pixels, pixel/millimeter/inch dimensions and physical resolution. A shared read-only plan resolves the raster before creation. Resolution metadata persists with projects, undo and replay; it does not resample pixels. See [Document Setup](docs/document-setup.md).

The [unified plugin system](docs/plugin-system.md) exposes package versions, compatibility, dependencies and lifecycle status in Settings and automation. Enable/disable plans resolve dependencies before saving; activation takes effect after restart. Plugins can provide isolated language catalogs that hot-reload while the app runs.

[PNG density](docs/png-density.md) preserves exact X/Y counts and units through import/export, projects and replay PNG output. Unknown physical sizes stay explicit; setting resolution replaces imported density without resampling.

[PNG Export Sizing](docs/export-sizing.md) supports pixels, millimeters, inches and percentage output, explicit previews and output-only PPI overrides. Native sampling runs on the composed frame, preserving source layers/history; file digests are bound to export events.

## Why MiCoPi

**Painting first. Plugins carry behavior. One execution core serves artists and automation.**

Typed pen input protects the drawing path; schema-described actions and public state make the same application inspectable from plugins, MCP and headless clients. Image composition stays non-destructive, and workflow execution remains explicit. AI-ready means a documented integration contract, not a required cloud assistant.

Read the illustrated [design philosophy](docs/philosophy.md), follow the [roadmap](ROADMAP.md), browse the [documentation index](docs/README.md), or see the [directory and ownership guide](docs/project-structure.md).

## Features

- DLL-based brush, eraser, eyedropper, hand and zoom tools.
- Pen pressure, pressure curves, stroke stabilization and brush outline cursors.
- HSV wheel, RGB controls, swatches and brush presets.
- Layers, visibility, opacity, transparency locking, color fills, blend modes and tiled undo/redo.
- Dockable, floating and tabbed panels; configurable spacing, sizing, themes and shortcuts.
- Searchable settings, JSON editing, command palette and shortcut recording.
- Transparent PNG export, PNG/JPEG/BMP import and `.mcp` projects.
- Non-destructive [image graphs](docs/node-editor.md) with linked layer stacks, per-layer filters and explicit free compositing.
- [Plugin workflow graphs](docs/workflow-nodes.md) generated from action and state schemas.
- Independent native DLL plugins, headless JSONL, MCP stdio and attachment to a running GUI.
- Shared plugin inventory, dependency-aware settings and owner-scoped language packs.
- [Atomic document groups](docs/atomic-operations.md), shared-tile rollback and compact verified events.
- Verified [event replay](docs/replay.md), time-lapse preview and PNG frame sequences.

Layers support pixel/position locks, color labels and shared read-only name/visibility/lock filters, with undo, projects and verified replay. Native write permissions retain read tools and protect locked targets.

Layers support renaming, reordering, deletion and selection-aware **Layer via Copy (Ctrl+J)** / **Layer via Cut (Ctrl+Shift+J)**. Without a selection, Ctrl+J duplicates the whole layer. Layers also support Merge Down (**Ctrl+E**), Merge Visible (**Ctrl+Shift+E**), Stamp Visible (**Ctrl+Alt+Shift+E**) and transparent-canvas flattening. Exact flips, 180° rotation and integer offsets are also available. See [Layer editing](docs/layer-editing.md).

Current limits: one document per session; no Wintab, off-canvas layer storage, layer groups or watercolor engine. Native plugin DLLs execute inside the host process.

Schema-described rectangle, ellipse and polygon [selections](docs/selections.md) support combinations, native drawing/action clipping, undo and replay. The Tools strip offers rectangle/ellipse marquees and freehand/polygonal lassos with reversible previews. PS-style selection shortcuts are configurable; [Session clipboard](docs/clipboard.md) copy/cut/paste commands are available; Windows PNG/DIB clipboard commands and PS copy/cut/paste bindings are implemented; native roundtrip interoperability validation remains pending on this restricted test host.

Magic Wand (**W**) offers tolerance, contiguous and merged sampling controls, plus Grow/Similar. The same color-selection commands are available through parameter forms and automation. Selection defaults are configurable in the searchable settings center and `settings.json`. Selections also provide exact disk expansion/contraction, feathering and smoothing through native actions.

## Build and run

Requires Rust 1.88+, Visual Studio C++ Build Tools and the Windows SDK.

```powershell
.\scripts\build.ps1 -PluginsOnly
cargo run --release --locked
.\scripts\build.ps1 -Test
```

Run `dist/MiCoPi.exe`. Keep `plugins/`, `locales/` and `licenses/` beside the executable. End users do not need Rust. A project-local portable toolchain is available; see [Development](docs/development.md).

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

Only English is embedded. Other languages load from `<executable-directory>/locales/`, with overrides in `<config-directory>/locales/`. Editing a JSON catalog updates the desktop without rebuilding or restarting; invalid edits retain the last valid translations. Use `--locales-dir <directory>` to choose a different base catalog directory. See [Localization](docs/localization.md) for precedence and plugin catalogs.

## Extending MiCoPi

Start with the installable [Quick tools example](examples/plugins/quick-tools/manifest.json): a brush preset, macro and translated button panel. Copy the whole package into your plugin directory and restart. Inspect `plugins.list` for package status, diagnostics and contribution owners; `plugins.plan` previews dependency changes without modifying the running registry.

Native pixel algorithms use the [C ABI SDK](docs/native-plugins.md). Feature UI adapters compile with the desktop host; DLL replacement and contribution changes require restart. The host supplies shared execution, document history and window lifecycle. See [Plugin system](docs/plugin-system.md) and [Plugin windows](docs/plugin-windows.md).

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
| `src/i18n.rs`, `locales/` | Embedded English fallback and external, hot-reloadable translations |
| `src/platform/` | Windows Ink, DWM, native dialogs and file replacement |
| `docs/` | English guides, reference schema and curated screenshots |
| `examples/` | Automation, configuration, MCP and plugin examples |
| `scripts/` | Reproducible build, packaging, smoke and repository checks |

See [Architecture](docs/architecture.md), [User Guide](docs/user-guide.md), [Contributing](CONTRIBUTING.md) and [Changelog](CHANGELOG.md). The legacy GDI workbench remains a regression reference and is not the production desktop entry point.

`Cargo.lock` is retained. Build outputs, downloaded toolchains and personal settings are ignored by Git.

## Contributing and version control

See [Contributing](CONTRIBUTING.md) and the repository [Agent guide](AGENTS.md). Start with the [development guide](docs/development.md) and commit coherent, verified changes. Follow the maintainer's requested branch workflow; current maintainer-directed work commits directly to `main`. Release tags use `vX.Y.Z`. Runtime binaries and personal files stay outside source history; selected documentation screenshots stay versioned.

```powershell
python scripts/check-repository.py
cargo fmt --all -- --check
```

## License

Application source: [MIT](LICENSE). Embedded Google Material icons: [Apache 2.0](assets/material-symbols/LICENSE), with [notices](assets/material-symbols/NOTICE.md). Build outputs include third-party dependency licenses under `dist/licenses/`. Downloaded toolchains retain their upstream licenses.
