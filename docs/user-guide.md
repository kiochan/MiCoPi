# User Guide

## Start drawing

Run `MiCoPi.exe` with the bundled `plugins/` directory beside it. Select Brush, choose a color and drag on the canvas. Eraser uses the same pressure and stabilization pipeline. Hover over tool buttons to see names and configured shortcuts. The cursor shows brush size and shape.

Use File > New Canvas, Open, Save Project or Export PNG. `.mcp` retains layers, image graphs and workflow graphs; PNG exports the composited result with transparency. Undo history is not saved. Image dimensions are limited to 4096 pixels per side, with an additional total document pixel budget.

The current release has one document per session. Opening or creating another document replaces it. The GUI asks before discarding unsaved artwork; automation with explicit file paths follows the documented action contract.

## Language and appearance

English is the default. Open Settings with **Ctrl+,**, search for `ui.language`, and select English, Simplified Chinese, Japanese or Korean. Changes preview immediately. Click Save Settings to retain them; Close or Esc cancels changes made since the last save. Language does not change documents, file paths or protocol identifiers.

All settings share one searchable window. Search by localized name, English setting ID, keywords or shortcut. `@modified` restricts results to values differing from defaults. Appearance includes themes, accent color, font sizes, item/section spacing, panel/window padding, toolbar/tab heights, corner radius and panel widths.

The JSON view edits the complete `settings.json`. Validate before applying or saving; invalid syntax, ranges or shortcut conflicts leave the file intact. Unknown root fields are preserved. JSON comments are not supported.

Search `selection.` to configure Wand/Grow/Similar defaults. Preview and cancel restore current tool controls; Save Settings persists the defaults. Temporary Tool Options choices remain separate from automation defaults. See [Selections](selections.md).

Settings live in `settings.json`; dock layout is saved separately in `workspace.egui.json`. Plugin manifests remain under `plugins/`. The older INI configuration is retained for legacy painting preferences, not as the primary desktop layout format.

## Pen and brush

Windows Ink supplies pressure. Pressure size and opacity can be enabled independently; the pressure curve and minimum radius control light-pressure response. Brush size, opacity, hardness and stabilization are available in the toolbar, brush panel and settings. Stabilization ranges from 0 to 1 and applies to subsequent strokes.

A stroke freezes its starting tool and parameters. Pen-up pressure uses the last contact sample to prevent disappearing stroke ends. The painting DLL processes pressure, stabilization, interpolation and rasterization. Disabling or removing that DLL makes painting unavailable; the host does not silently use another brush engine.

Windows Ink compatibility still depends on the actual tablet and driver. Wintab, tilt-specific brushes and a watercolor engine are not implemented.

## Shortcuts and command palette

Press **Ctrl+Shift+P** to search commands. Arrow keys select, Enter runs and Esc closes. Search supports localized names, English identifiers, keywords and existing pinyin aliases.

Defaults follow familiar Photoshop conventions: B for Brush, E for Eraser, I for Eyedropper, H for Hand, Z for Zoom; Space temporarily pans and Alt temporarily picks color. File operations use Ctrl+N/O/S and standard undo/redo commands. The shortcut editor is authoritative for all current bindings.

Layer consolidation uses **Ctrl+E** for Merge Down, **Ctrl+Shift+E** for Merge Visible and **Ctrl+Alt+Shift+E** for Stamp Visible. Flatten Image is available through Edit, layer menus and command search; it preserves canvas transparency. These operations support undo and verified replay; see [Layer editing](layer-editing.md) for visibility, lock and graph restrictions.

Open Keyboard Shortcuts to edit, record or clear a binding. Multiple alternatives are comma-separated. Modifier-only hold gestures and wheel combinations are supported. Conflicts are checked before application. Apply previews a binding; Save Settings persists it. Command and action IDs remain stable across languages.

## Panels and workspace

The Tools strip includes rectangular/elliptical marquees (**M**, **Shift+M**) and freehand/polygonal lassos (**L**, **Shift+L**). Their independent Tool Options bar selects New/Add/Subtract/Intersect; modifier hints are available on the keyboard icon. Escape cancels a preview, and Enter completes a polygon. Completion publishes one shared selection action, with undo and verified replay. See [Selections](selections.md) for constraints, combination modifiers and automation.

Drag tabs to reorder, split, group or float panels. Drag a group header to move the group. Dock on any side, resize splitters or floating borders, and use tab menus to reopen panels. Floating panels are constrained to the visible work area. Esc cancels an active drag.

Workspace commands reset layout or group Color with Brush and Navigator with Layers. Closing a panel does not delete its underlying document data. Layout is persisted independently of appearance settings; layout reset is not reverted by canceling a settings preview.

## Layers and nodes

Layers provide visibility, opacity, blend mode and thumbnails. A hidden layer cannot receive a painting stroke. Undo and redo are recorded by touched pixel tiles, rather than full-document snapshots.

Open View > Node Editor. The **Image Compositing** window provides layer, filter, blend and output nodes. Applying a graph is non-destructive. Optional live preview waits for parameter/connection changes to settle; moving a node alone does not force full-image processing. See [Image Graphs](node-editor.md).

The independent **Plugin Workflow** window searches action/state nodes, exposes schema-driven parameters and connects typed values or execution-order ports. Editing does not execute actions. Save Workflow retains the graph; Run All Nodes executes every node in topological order. See [Workflow Nodes](workflow-nodes.md).

## Plugins and automation

Install each plugin directory beside the executable, with its manifest and DLL when required, then restart. Plugin enable/disable preferences take effect on restart and preserve the manifests. The settings page shows diagnostics and loaded contributions.

[JSONL, MCP and GUI attachment](plugins-and-actions.md) use the same Session as the desktop. For file automation, supply explicit paths to document and export actions. Generic UI intent actions can return stored effects without opening windows. Desktop selection gesture/window commands explicitly report headless unavailability; use document actions such as `selection.shape` for headless editing.

Current omissions include clipboard, free transforms, layer groups, Wintab, multi-document tabs and watercolor blending. The UI does not present placeholders for these features.

## Organize and transform layers

Use **Layer Actions** or right-click a layer to duplicate, move up/down, delete, flip or rotate it. **Ctrl+J** copies the active selection to a new layer, or duplicates the whole layer when no selection is active. **Ctrl+Shift+J** cuts selected pixels to a new layer in one undo step; the clipboard stays unchanged. Explicit older shortcut preferences are retained. Edit **Layer Name** and press Enter or leave the field to commit; Escape cancels. **Offset Layer** accepts signed integer X/Y offsets; positive X moves right and positive Y moves down. Exposed pixels become transparent and pixels outside the canvas are discarded. Undo restores them. See [Layer editing](layer-editing.md) for action schemas, reference rules and limits.

## Canvas size and crop

Use **Edit > Canvas Size…** (**Ctrl+Alt+C**) to reframe all layers around an anchor, or **Edit > Crop Canvas…** to enter an exact pixel rectangle. Added space is transparent. Both support undo, retain layer/node identities and share the public actions described in [Canvas editing](canvas-editing.md).

## Tools and tool options

The **Tools** window holds one icon per tool, including **Flood Fill** and **Gradient**. **Tool Options** beside it shows the active tool and its size, opacity and stabilizer. Each window can be moved, grouped, floated or hidden on its own. To change the icon order or hide icons, open Settings and search for **Tools Icons**. You can also list IDs in `ui.tool_order` and `ui.hidden_tools`, for example `"tool_order": ["brush", "fill", "eraser"]` and `"hidden_tools": ["zoom"]`.

## Fill, gradients and pressure

The Brush panel contains **Flood Fill**, **Gradient** and expandable **Pressure Dynamics** controls. Click to fill a connected area or drag to define a gradient. See [Painting tools](painting-tools.md) for tolerance, alpha, gesture cancellation and the shared automation actions.

Clear **Contiguous Fill** to recolor matching pixels across the current layer. Gradient controls provide extend/repeat/reflect modes, reverse and a transparent endpoint; hold **Shift** during a gradient drag to constrain its angle to 45° steps. These operations retain the same undo, atomic and replay behavior.

**Fill Expansion** grows a region with positive values and contracts it with negative values (up to 32 pixels). **Fill Feather** softens the final boundary. Values default to zero. Expansion can paint onto adjacent colors on the current layer; use undo to restore the previous pixels.

Enable **Lock Layer Transparency** in Layers to recolor painted pixels while retaining their alpha. Use the layer menu or **Alt+Backspace** / **Ctrl+Backspace** for foreground/background fills. These fills respect brush opacity and the layer transparency lock.
See [Plugin Windows](plugin-windows.md) for the independent control bar, floating canvas, window commands and workspace migration.
