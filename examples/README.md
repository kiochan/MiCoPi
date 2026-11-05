# Examples

- [Selection commands](automation/selection.jsonl): native shape/combine geometry, pressure strokes, clipped filters and verified outcomes.

[Canvas framing](automation/canvas-framing.jsonl) demonstrates transparent canvas expansion, exact crop, undo/redo and verified replay. Run it from the repository root with the CLI and loaded native plugins; it exports `target/canvas-framing.png`.

Run commands from the repository root. Build the host and default plugin DLLs first; the portable distribution already includes them. JSONL files use stable action IDs and document-pixel coordinates.

```powershell
New-Item -ItemType Directory -Force target/example-output | Out-Null
.\dist\micopi-cli.exe run examples/automation/draw.jsonl
```

[Document setup](automation/document-setup.jsonl) creates an editable background, changes physical resolution without resampling and verifies replay.

[Physical creation](automation/physical-creation.jsonl) plans A4 without allocating it, creates a small millimeter-sized page and exports a pressure stroke with verified outcomes. Create `target/example-output/` first.

[Physical Image Size](automation/physical-image-size.jsonl) plans and resamples every layer and the selection using millimeters at the retained density, then checks undo/redo and exports a PNG. Create `target/example-output/` first.

[Layer organization](automation/layer-organization.jsonl) protects an ink layer, labels layers, queries metadata and binds a found stable ID to a later atomic property action. Create `target/example-output/` first.

[PNG density](automation/png-density.jsonl) compares implicit defaults and explicit physical resolution through snapshots without resampling pixels.

[Export sizing](automation/export-sizing.jsonl) plans and writes a composed PNG without resizing the document. Create `target/example-output/` first.

## Automation

| File | Demonstrates |
| --- | --- |
| [atomic.jsonl](automation/atomic.jsonl) | One atomic cross-plugin drawing group, project persistence and sealed replay export |
| [replay.jsonl](automation/replay.jsonl) | Verified history, sealed archives, project persistence and PNG time-lapse frames; use a fresh frames directory |
| [draw.jsonl](automation/draw.jsonl) | Pressure-sensitive, stabilized drawing and project/PNG export |
| [native-actions.jsonl](automation/native-actions.jsonl) | Actual DLL checkerboard and region processing |
| [ai-pipeline.jsonl](automation/ai-pipeline.jsonl) | Discovery, state, cross-plugin calls and typed pipeline execution |
| [nodes.jsonl](automation/nodes.jsonl) | Non-destructive image graph and PNG export |
| [linked-layers.jsonl](automation/linked-layers.jsonl) | Per-layer filters following duplication/reorder, synchronized blend modes and verified outcomes |
| [trim.jsonl](automation/trim.jsonl) | Read-only trim analysis, transparent/corner-color margins, selected sides, undo and verified outcomes |
| [image-size.jsonl](automation/image-size.jsonl) | Aspect-locked and independent-axis image scaling, nearest/bilinear/area masks, undo and replay verification |
| [geometry.jsonl](automation/geometry.jsonl) | Exact document rotations/flips, centered clipped layer turns, masks, undo and replay verification |
| [layer-merge.jsonl](automation/layer-merge.jsonl) | Merge Down, Merge Visible, Stamp Visible, transparent flattening, undo and replay verification |
| [workflow.jsonl](automation/workflow.jsonl) | State/action nodes, patching, validation, dry run and execution |

Generated artwork belongs in ignored `target/example-output/`, not beside source files. Separate CLI invocations create separate Sessions; use GUI attachment when you want to modify a running desktop document.

## Configuration and connections

- [settings.json](config/settings.json): sample shortcut overrides and English display language.
- [settings-ui.json](config/settings-ui.json): appearance and spacing configuration.
- [Settings schema](../docs/reference/settings.schema.json): supported configuration fields.
- [Headless MCP configuration](mcp/headless.json): a client that owns an independent canvas.
- [GUI MCP configuration](mcp/gui.json): a client attached to an existing session endpoint.

MCP configurations contain example paths; adjust executable/profile paths for your installation. Save personal configuration outside this directory. See [Localization](../docs/localization.md) and [Automation](../docs/plugins-and-actions.md).

## Plugins and historical configurations

[Quick tools](plugins/quick-tools/manifest.json) is a small installable package with a preset, macro action and button panel. Copy the package into your chosen plugin directory and restart.

Its `locales/` directory demonstrates Chinese, Japanese and Korean plugin translations. Edit these JSON files while the desktop is running to update package titles, buttons and command search. English messages remain in the manifest; plugin translations are scoped to this package. See [Plugin system](../docs/plugin-system.md).

The INI files under [legacy](legacy/README.md) describe earlier workspaces. The current desktop uses `settings.json` and `workspace.egui.json`; do not copy the old INI files over current JSON settings.

[Session clipboard](automation/clipboard.jsonl) demonstrates internal copy/merged copy, cut, centered/explicit paste, undo and replay verification. It does not access the system clipboard.

[Selection to layer](automation/selection-to-layer.jsonl) demonstrates native Layer via Copy/Cut, single-entry undo/redo and unchanged clipboard state.
