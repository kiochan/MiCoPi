# User Guide

## Start drawing

Run `MiCoPi.exe` with the bundled `plugins/` directory beside it. Select Brush, choose a color and drag on the canvas. Eraser uses the same pressure and stabilization pipeline. Hover over tool buttons to see names and configured shortcuts. The cursor shows brush size and shape.

Use File > New Canvas, Open, Save Project or Export PNG. `.mcp` retains layers, image graphs and workflow graphs; PNG exports the composited result with transparency. Undo history is not saved. Image dimensions are limited to 4096 pixels per side, with an additional total document pixel budget.

The current release has one document per session. Opening or creating another document replaces it. The GUI asks before discarding unsaved artwork; automation with explicit file paths follows the documented action contract.

## Language and appearance

English is the default. Open Settings with **Ctrl+,**, search for `ui.language`, and select English, Simplified Chinese, Japanese or Korean. Changes preview immediately. Click Save Settings to retain them; Close or Esc cancels changes made since the last save. Language does not change documents, file paths or protocol identifiers.

All settings share one searchable window. Search by localized name, English setting ID, keywords or shortcut. `@modified` restricts results to values differing from defaults. Appearance includes themes, accent color, font sizes, item/section spacing, panel/window padding, toolbar/tab heights, corner radius and panel widths.

The JSON view edits the complete `settings.json`. Validate before applying or saving; invalid syntax, ranges or shortcut conflicts leave the file intact. Unknown root fields are preserved. JSON comments are not supported.

Settings live in `settings.json`; dock layout is saved separately in `workspace.egui.json`. Plugin manifests remain under `plugins/`. The older INI configuration is retained for legacy painting preferences, not as the primary desktop layout format.

## Pen and brush

Windows Ink supplies pressure. Pressure size and opacity can be enabled independently; the pressure curve and minimum radius control light-pressure response. Brush size, opacity, hardness and stabilization are available in the toolbar, brush panel and settings. Stabilization ranges from 0 to 1 and applies to subsequent strokes.

A stroke freezes its starting tool and parameters. Pen-up pressure uses the last contact sample to prevent disappearing stroke ends. The painting DLL processes pressure, stabilization, interpolation and rasterization. Disabling or removing that DLL makes painting unavailable; the host does not silently use another brush engine.

Windows Ink compatibility still depends on the actual tablet and driver. Wintab, tilt-specific brushes and a watercolor engine are not implemented.

## Shortcuts and command palette

Press **Ctrl+Shift+P** to search commands. Arrow keys select, Enter runs and Esc closes. Search supports localized names, English identifiers, keywords and existing pinyin aliases.

Defaults follow familiar Photoshop conventions: B for Brush, E for Eraser, I for Eyedropper, H for Hand, Z for Zoom; Space temporarily pans and Alt temporarily picks color. File operations use Ctrl+N/O/S and standard undo/redo commands. The shortcut editor is authoritative for all current bindings.

Open Keyboard Shortcuts to edit, record or clear a binding. Multiple alternatives are comma-separated. Modifier-only hold gestures and wheel combinations are supported. Conflicts are checked before application. Apply previews a binding; Save Settings persists it. Command and action IDs remain stable across languages.

## Panels and workspace

Drag tabs to reorder, split, group or float panels. Drag a group header to move the group. Dock on any side, resize splitters or floating borders, and use tab menus to reopen panels. Floating panels are constrained to the visible work area. Esc cancels an active drag.

Workspace commands reset layout or group Color with Brush and Navigator with Layers. Closing a panel does not delete its underlying document data. Layout is persisted independently of appearance settings; layout reset is not reverted by canceling a settings preview.

## Layers and nodes

Layers provide visibility, opacity, blend mode and thumbnails. A hidden layer cannot receive a painting stroke. Undo and redo are recorded by touched pixel tiles, rather than full-document snapshots.

Open View > Node Editor. The **Image Compositing** tab provides layer, filter, blend and output nodes. Applying a graph is non-destructive. Optional live preview waits for parameter/connection changes to settle; moving a node alone does not force full-image processing. See [Image Graphs](node-editor.md).

The **Plugin Workflow** tab searches action/state nodes, exposes schema-driven parameters and connects typed values or execution-order ports. Editing does not execute actions. Save Workflow retains the graph; Run All Nodes executes every node in topological order. See [Workflow Nodes](workflow-nodes.md).

## Plugins and automation

Install each plugin directory beside the executable, with its manifest and DLL when required, then restart. Plugin enable/disable preferences take effect on restart and preserve the manifests. The settings page shows diagnostics and loaded contributions.

[JSONL, MCP and GUI attachment](plugins-and-actions.md) use the same Session as the desktop. For file automation, supply explicit paths to document and export actions. Headless UI actions return effects without opening windows.

Current omissions include selections, transforms, layer reordering/merging, Wintab, multi-document tabs and watercolor blending. The UI does not present placeholders for these features.

## Organize and transform layers

Use **Layer Actions** or right-click a layer to duplicate, move up/down, delete, flip or rotate it. **Ctrl+J** duplicates the current layer. Edit **Layer Name** and press Enter or leave the field to commit; Escape cancels. **Offset Layer** accepts signed integer X/Y offsets; positive X moves right and positive Y moves down. Exposed pixels become transparent and pixels outside the canvas are discarded. Undo restores them. See [Layer editing](layer-editing.md) for action schemas, reference rules and limits.

## Fill, gradients and pressure

The Brush panel contains **Flood Fill**, **Gradient** and expandable **Pressure Dynamics** controls. Click to fill a connected area or drag to define a gradient. See [Painting tools](painting-tools.md) for tolerance, alpha, gesture cancellation and the shared automation actions.
