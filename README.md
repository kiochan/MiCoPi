# MiCoPi

[Direct weighted grayscale](docs/grayscale.md) adds native active-layer strength controls with preserved alpha, selection/locks, Undo and verified replay. Runtime validation and dedicated preview controls remain pending.

[Direct Brightness/Contrast](docs/brightness-contrast.md) now provides native active-layer editing with a prepared byte table, selection/locks, Undo and verified replay. Read-only PNG previews now share the validated HSL staging pipeline. Runtime checks await CI; coupled desktop sliders, cached previews and Apply/Cancel are implemented in source, with new UI validation and native captures pending.

A fenced read-only HSL preview command now returns PNG and a revision-guarded Apply action through nodes, MCP and headless clients; runtime validation is pending. Desktop color drafts now add debounced cached Canvas previews and guarded Apply/Cancel controls; UI/runtime and reviewed captures await validation.

[Hue/Saturation](docs/hue-saturation.md) now has a native direct pixel command, typed source nodes and affect-below adjustment layers. Shared controls, masks, cached parameter slots, Undo, projects, verified outcomes and explicit node observations use the same execution core. New document/UI/transport checks await CI; Colorize remains planned.

[Curves source nodes](docs/curves.md) add master/R/G/B control points and cached native rendering to linked/free graphs, Undo, projects and verified outcomes. CI is pending; Curves adjustment layers share those contracts; shared point controls support dragging and numeric edits; [histogram queries](docs/histograms.md) expose bounded composite/raw statistics; shared manually refreshed charts now show explicit published composite/raw scope in Levels/Curves editors. Fenced published-node input/output queries now serve commands, workflows, MCP and headless clients. Shared charts now offer explicit Source/Actual node input/output selection with cached metadata and structural reselection. New UI runtime validation and reviewed captures remain pending.

[Levels source nodes](docs/levels.md) now share linked/free graphs, cached native
rendering, Undo, projects and verified outcomes. CI validation is pending;
Source nodes and affect-below adjustment layers share Master/R/G/B input/output controls, cached native channel tables and owned-mask support. Channel integration tests await CI.

[English](README.md) · [简体中文](README.zh.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

A small Rust painting studio for anime and manga illustrators, focused on responsive pen input and an extensible, AI-ready execution core.

MiCoPi runs on **Windows 10/11 x64**. The desktop workbench uses **egui + egui_dock** with OpenGL; pen input uses Windows Ink. The project is in early development.

**0.21.0:** [Atomic document groups](docs/atomic-operations.md) roll back failed batches and publish one compact event. MCRPLY02 adds metadata/XOR deltas and tile references while retaining v1 reading. [Verified event replay](docs/replay.md) records document operations, pressure input and plugin outcomes. Preview the drawing process, verify its SHA-256 chain, save history inside projects, or export a replay archive and PNG time-lapse frames. English is the default language, with Chinese, Japanese and Korean available.

![Painting workbench](docs/screenshots/workbench.png)

The independent Tools bar and Canvas can float as native OS windows across monitors, dock on any side or form tab groups. Feature packages own their window contributions and UI source; every registered action is searchable and bindable. See [Plugin Windows](docs/plugin-windows.md).

Connected or whole-layer color fill with tolerance and edge controls, linear/radial gradients with repeat and reflection and inline pressure dynamics are available in the Brush panel. See [Painting tools](docs/painting-tools.md).

Brush/Eraser support independent **Flow** and an opt-in **per-stroke opacity limit**, including a Glazing Marker preset. Default Flow 100% / Build Up preserves existing painting. Searchable settings, nodes, MCP and headless share these parameters. See [Brush opacity and Flow](docs/stroke-opacity.md) for scope and validation status.

[Analytic brush tips](docs/brush-tips.md) add round/elliptical and square/rectangular shapes, 25–100% roundness and a fixed clockwise angle, with matching nominal cursor contours. Brush/Eraser, searchable defaults, presets, nodes, MCP and headless share these values. Default circles preserve legacy pixels and recordings. This source increment and its bounded synthetic cost probe await CI; sampled/textured tips and tilt dynamics remain planned.

[User brush presets](docs/brush-presets.md) save complete scalar Brush/Eraser setups while preserving both color swatches. My Brushes provides search, save/update, rename, delete and bounded data-only import/export; exact provider checks and cached state are shared with workflows, MCP and headless. Built-in presets stay immutable. This source increment is awaiting CI validation; sampled tips and textures remain planned.

Canvas size, exact all-layer crop, document quarter/half turns and flips preserve layer/node identities, selection masks, undo and verified replay. Active-layer quarter turns keep the canvas size and clip beyond its frame. **Image Size (Ctrl+Alt+I)** scales all layers and current/Reselect pixel selections from pixel, millimeter or inch inputs with aspect locking and nearest/bilinear/area sampling. Read-only plans expose resolved dimensions and buffer costs while preserving exact density. Area averaging retains thin-line coverage during reductions. Trim previews transparent or corner-color margins and crops selected sides across all layers with undo. See [Canvas editing](docs/canvas-editing.md).

**Move (V)** drags an outline and commits on release. Move the active raster with its linked mask, an independent raw raster mask, selected pixels on unmasked documents or only the selection outline. Auto prioritizes the selected painting mask; explicit `mask_id` binds a stable owner independently of active layers. Arrow/Shift-arrow steps are configurable in searchable Settings. Off-frame content is clipped and restored by one Undo. Actions also run through nodes, MCP and headless. See [Move and nudge](docs/canvas-editing.md#move-and-nudge) and [Raster owner movement](docs/raster-owner-movement.md).

**Free Transform (Ctrl+T)** supports numeric and mouse/pen move, scale, skew, rotation, flips and pivots, with cancellable cached pixel previews and one undo/event. The same native action runs through nodes, MCP and headless. Pixels outside the current frame clip; **Ctrl+Shift+T** repeats the saved matrix; **Ctrl+Alt+Shift+T** creates a transformed copy. Recipes persist with projects, undo and replay. Edge diamonds adjust skew; perspective corner editing maps a convex quadrilateral with the same undo, repeat and replay contracts. These legacy copy/perspective shortcuts retain their single-target contracts; [owner/linked affine scopes](docs/owner-affine.md) carry linked raw masks, preserve selections and repeat exact captured IDs. See [Free Transform](docs/canvas-editing.md#free-transform).

**New Canvas (Ctrl+N)** offers configurable illustration, square, A4 and B5 presets, editable background pixels, pixel/millimeter/inch dimensions and physical resolution. A shared read-only plan resolves the raster before creation. Resolution metadata persists with projects, undo and replay; it does not resample pixels. See [Document Setup](docs/document-setup.md).

The [unified plugin system](docs/plugin-system.md) exposes package versions, compatibility, dependencies and lifecycle status in Settings and automation. Enable/disable plans resolve dependencies before saving; activation takes effect after restart. Plugins can provide isolated language catalogs that hot-reload while the app runs.

[PNG density](docs/png-density.md) preserves exact X/Y counts and units through import/export, projects and replay PNG output. Unknown physical sizes stay explicit; setting resolution replaces imported density without resampling.

[PNG Export Sizing](docs/export-sizing.md) supports pixels, millimeters, inches and percentage output, explicit previews and output-only PPI overrides. Native sampling runs on the composed frame, preserving source layers/history; file digests are bound to export events.

[Color blend modes](docs/blend-modes.md) provide 26 modes for layers, isolated groups and Blend nodes, including burn/dodge, contrast and Hue/Saturation/Color/Luminosity families. Scrolling menus follow Photoshop family order; unsupported modes remain visible and disabled. The original eight mode IDs and pixel results are preserved, and shared APIs expose current renderer availability.

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

Ctrl/Shift layer selection is separate from the active painting layer. Explicit stable-ID batches apply properties, duplicate, delete, reorder or run exact pixel edits with one Undo/Redo group and one verified outcome. The same actions and selected state are available to nodes, MCP and headless clients.

[Layer Links](docs/layer-links.md) persistently join related ink/color rasters for whole-layer Move and nudges. Link/Unlink controls and chain indicators retain active-only painting. Exact target arrays carry owned linked masks under one Undo/outcome and a checked staging limit; actions and state nodes also serve MCP/headless clients. [Linked affine Free Transform](docs/owner-affine.md) captures one exact set and matrix; Repeat retains its IDs, while Copy Repeat and perspective remain restricted to the supported legacy domains. Align/distribute remain planned; current owner-affine runtime validation is pending CI.

Layers support renaming, reordering, deletion and selection-aware **Layer via Copy (Ctrl+J)** / **Layer via Cut (Ctrl+Shift+J)**. Without a selection, Ctrl+J duplicates the whole layer. **Merge Layers (Ctrl+E)** merges multiple selected rasters or uses Merge Down for a singleton. Merge Visible (**Ctrl+Shift+E**), Stamp Visible (**Ctrl+Alt+Shift+E**), transparent-canvas flattening and exact flips/180° rotation/offsets remain available. See [Layer editing](docs/layer-editing.md).

[Selected-only merge](docs/selected-merge.md) supports noncontiguous and cross-parent targets while preserving unselected buffers and all folders. It bakes the selected forest over transparency, so outside-backdrop and stacking interactions can change. Read-only plans and an optional nonmodal review show placement and hidden targets to be removed; explicit stable-ID execution shares one Undo/outcome with nodes, MCP and headless. Ctrl+E remains immediate, and explicit older Merge Down bindings are preserved.

[Layer Groups](docs/layer-groups.md) provide nested visibility, opacity, Isolated or [Pass Through](docs/pass-through.md) composition, subtree operations and an actual-composition node view. Pass Through blends children against the parent backdrop and applies group opacity to the combined change; the isolated blend mode is retained. Ctrl+G groups selected siblings; painting remains on the active raster. Merge Group accepts isolated targets, including nested Pass Through children; grouped Merge Visible preserves hidden branches and folders. Groups share commands, projects, atomic history and verified replay with MCP/headless.

Current limits: one document per session; no Wintab, off-canvas layer storage, clipping masks or watercolor engine. Native plugin DLLs execute inside the host process.

[Adjustment layers](docs/adjustment-layers.md) are real non-raster affect-below items with five registered filters, Normal alpha-preserving mixing, opacity and owned A8 masks. Root, Isolated and Pass Through scopes share actions, Undo and verified outcomes; inspecting properties keeps the raster painting target separate. Group copy/delete carries adjustment owners. Levels and Curves are supported; broader blending remains planned. This source increment is awaiting CI validation.

[Layer alignment and distribution](docs/layer-arrangement.md) uses exact captured raster IDs for edge/center alignment and center distribution, with optional canvas alignment. Raw ink bounds determine placement; linked masks follow under one Undo/outcome. Read-only plans preserve contacts, and the same commands serve nodes, MCP and headless. Runtime verification is pending CI.

[Raster mask editing](docs/raster-mask-editing.md) supports layer/group A8 masks, pressure-sensitive Brush/Eraser with stabilization, non-destructive density/feather, selection conversion and raw-alpha application to raster layers. Eleven mask actions and public state share the desktop, workflow nodes, MCP and headless paths, with tiled Undo and verified persistence. [Document-wide geometry](docs/mask-canvas-geometry.md) reframes, rotates, flips, resamples and trims every raw mask with the canvas, including hidden, disabled and unlinked masks. Shared read-only plans report separate mask costs and contact/capability state; one Apply publishes one reversible edit. [Single-layer owner copy/delete](docs/mask-owner-editing.md) carries exact raw masks through fresh IDs, explicit paint-target rules and one compound Undo/outcome. [Group creation and hierarchy movement](docs/mask-group-editing.md) retain every existing owner, raw plane and paint target, with one structural Undo/outcome and shared schema bindings. [Group-and-contents deletion](docs/mask-group-delete.md) removes only consumed owner masks, with explicit target/selection fallback, required result-ID arrays and one compound Undo/outcome. [Group/subtree duplication](docs/mask-group-duplicate.md) copies nested owners under fresh ordered IDs and independent raw buffers, with mapped paint targets, typed result arrays and one compound Undo/outcome. [Raster-owner integer Move](docs/raster-owner-movement.md) carries linked masks or moves a raw raster mask alone in one edit. [Owner affine Free Transform](docs/owner-affine.md) adds exact raster sets and independent raw masks. Group-mask movement, selected-pixel transfers, selection-outline affine transforms on masked documents (integer outline Move remains available), ungrouping, batch edits, Layer via Copy/Cut, merges, clipboard mutations and group-mask Apply remain guarded; complete mask integration is still planned.

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
