# Changelog

## Unreleased

Implemented changes since 0.21.0, grouped by their current behavior. Git history retains the individual commits and intermediate design iterations.

### Painting and document editing

- Add compositor-backed Merge Down, Merge Visible, transparent-canvas flattening and Stamp Visible, with stable target IDs, hidden-layer preservation, single-entry undo, atomic rollback and verified outcomes. Expose four-language menus, configurable PS shortcuts and shared headless/workflow actions; explicitly reject unsupported free-graph/backdrop-dependent merges and locked destructive targets.
- Add stable layer identities, names, duplication, reordering and deletion, plus native flips, 180° rotation and integer offsets. Structural edits retain undo and graph references.
- Add persistent, undoable transparency locking and foreground/background fills, including Alt+Backspace and Ctrl+Backspace shortcuts. Keep draft layer names when toggling the lock.
- Extend connected fill with whole-layer color matching, tolerance, expansion/contraction and feathering. Correct repeated translucent fills so source-over alpha accumulates.
- Add linear/radial gradient gestures with repeat/reflection, reversed endpoints, transparent endpoints and Shift constraints.
- Add anchored canvas resizing and exact all-layer crop with Edit dialogs and an on-canvas guide. Preserve layer/node identities, dimension-aware history, atomic rollback and verified replay.
- Expose pressure dynamics with a curve preview, dab spacing and minimum opacity in the Brush panel and Painting settings. Freeze typed options at contact start and preserve the default painting pixels.
- Make new documents use linked layer compositing: layer insertion, duplication, reorder and deletion keep per-layer filters and blend nodes synchronized. Retain explicit free graphs for advanced and legacy documents.

### Workbench and windows

- Give anime and manga illustrators a configurable workbench with plum, paper and ivory palettes, lavender accents, tapered brush previews and aligned property controls. Panels use rounded cards, hairline outlines and soft shadows around a recessed canvas stage.
- Split Tools into an icon strip and an independent Tool Options window; reconcile the new panel into existing workspaces once. Configure icon visibility and order through Settings, `ui.tool_order` and `ui.hidden_tools`.
- Use browser-style tabs on a tinted group row with a group grip, maximize/restore and close-group controls. Keep the active tab visible; support tab ordering, detachment and leaf-scoped group commands.
- Float Canvas, Tools and property groups as native windows across monitors. Shared themes, textures and per-window Windows Ink queues retain pressure and stabilization; owned palettes stay above the application without system-wide topmost flags.
- Dock native groups by dragging onto split/tab guides, with previews and Escape cancellation. Preserve active tabs and nested split topology; expose the same targeting through `window.drop` and registered window commands.
- Separate requested placement from observed geometry to avoid resize/DPI feedback. Add splitter grab handles and persist normal geometry without restoring obsolete sizes during a live resize.
- Move zoom, document status, pen pressure and history memory into a thin strip beneath the canvas, with a logarithmic slider and editable percentage; remove the separate workspace status bar.
- Tighten default spacing to 4 pt item gaps, 8 pt panel padding, 6 pt workspace edges, 26 pt controls and 30 pt tabs, with a denser Compact preset. Preserve explicit user settings.

### Plugins and localization

- Unify package version/author/host compatibility metadata, path-based lifecycle inventory, contribution ownership and dependency-aware enable/disable plans. Settings and the read-only `plugins.plan` action share the planner; activation and DLL changes require restart.
- Embed only English and load Chinese, Japanese and Korean from external JSON catalogs, with profile overrides and bounded background hot reload. Keep the last valid translations after malformed edits and keep locale I/O off the drawing thread.
- Add owner-scoped plugin catalogs and translated command search, including a multilingual Quick tools example. A package's translations cannot replace another owner's labels.
- Keep feature UI source in plugin packages compiled into the desktop host, with shared window lifecycle commands and searchable, configurable actions. Keep workflow and image-composition editors separate.

### Reliability, performance and compatibility

- Share provisional contact recording and storage-failure rollback between GUI input and `canvas.pointer`; preserve revisions after failed atomic native results and refresh replay textures when an external client replaces an archive.
- Make layer creation undoable, restore the previous active layer and invalidate stale redo branches. Recognize project headers independently of filename extensions and retain the opened save path.
- Bound GUI pipe reads/writes and acknowledge complete responses without an unbounded flush. Coalesce settled workspace gestures into layout commands and preserve fractional metadata for replay verification.
- Optimize brush dab distance checks and reuse a capped byte-distance buffer for fill edges. Keep zero-edge fill kernels, dirty-region updates, cached tool instances and the normal-layer composition fast path.
- Extend optional typed stroke options with size/capability checks for pressure settings and alpha lock. Retain old v1 prefixes, unlocked legacy DLL compatibility, project readers and recorded hashes; explicitly reject unsupported options.
- Pin configured themes before applying styles and preserve native activation handling. Make resize checks honor effective client bounds and desktop tracking limits; synthetic checks do not establish physical tablet compatibility.

### Documentation and contribution workflow

- Add the phased Photoshop-common editing plan and current capability/gap matrix, using PhotoCraft's public feature documentation as a functional reference while retaining MiCoPi's architecture and independent implementations.
- Refresh all four READMEs, architecture/development guides, feature references and roadmap progress. Include repository agent instructions in portable documentation.
- Consolidate Unreleased notes by current behavior while retaining released-version history. Expand the agent guide with product direction, directory ownership and implementation conventions; require a matching changelog summary, explanatory commit body and validation report for every authored commit.

## 0.21.0

- Add document action rollback, atomic pipeline/workflow groups and a default-enabled atomic workflow UI control.
- Add opt-in document_atomic native action capability and validate effects before executing atomic pipelines.
- Keep pen segments provisional until contact completion; rewind the whole contact on journal storage failure.
- Publish journal heads only after complete writes; reserve log records before replacing projects and roll reservations back on save failure.
- Compress MCRPLY02 events with metadata deltas, tile references and sparse XOR blocks; retain MCRPLY01 decoding.
- Cover partial-write failures, nested/output-schema rollback, redo restoration, old recordings and bounded event growth.

## 0.20.0

- Add a host-owned append-only document outcome journal with SHA-256 event/state chains and sealed archive verification.
- Preserve pressure/stabilization and typed DLL painting; batch raw pointer recording at stroke or 256-sample boundaries.
- Add a four-language time-lapse viewer, stepping/seeking, replay archives and bounded PNG frame export.
- Persist verified history in MICOPI03 projects; retain MICOPI01/02 reading and independent undo history.
- Expose replay actions/state through plugin schemas, workflow nodes, MCP and headless execution; never reexecute DLLs or file writes during playback.

## 0.19.0

- Organize examples by purpose, separate current screenshots from historical captures, and group reference schemas.
- Add an illustrated English design philosophy, documentation/directory indexes and Chinese, Japanese and Korean README editions.
- Establish Git source history, artifact boundaries, repository checks and local portable packaging.

- Default to English, with Simplified Chinese, Japanese and Korean display languages.
- Embed translation catalogs for workbench, settings, commands, shortcuts, tooltips and node controls; preserve protocol IDs and painting behavior.
- Add searchable ui.language with immediate preview, validated persistence and cancellation; support localized command/setting/node search.
- Select installed Windows CJK font fallbacks without bundling font packs. Recognize earlier bundled Chinese manifest titles.
- Rewrite project documentation, contribution guidance and issue templates in English; add localization tests and GUI captures.

## 0.18.0

- Register nodes through manifests or DLL descriptors; adapt all actions and public state using parameter/input/output schemas.
- Add a searchable plugin workflow editor, typed data/order connections and atomic graph edits.
- Expose node.catalog and workflow read/set/patch/validate/run/state/open through GUI, MCP and headless; compile to the existing pipeline.
- Persist graphs with undo, enforce types/cycles/revision/output checks and report partial execution. Editing never runs actions.
- Preserve pressure/stabilization and incremental image compositing; cache history metadata size.

## 0.17.0

- Add a dockable image node editor for layer sources, five filters, eight blend modes and a unique canvas output.
- Move compositing into an independent DLL using the optional SDK 1.2 typed region interface.
- Preserve source pixels, dirty-region rendering and the ordinary normal-layer fast path.
- Persist graph metadata and positions in MICOPI02 projects, retain MICOPI01 reading, and expose graph actions/resources for automation.

## 0.16.0

- Add app.describe, context.read and pipeline.run as a unified discovery/state/execution contract.
- Register schema-checked public state; enforce read-only scopes and isolate provider failures.
- Support cross-DLL calls, bounded depth/call budgets, typed JSON Pointer references, dry runs, revision guards and partial failure propagation.
- Publish MCP state resources and an independent AI-assistant DLL example; retain the typed pen hot path.

## 0.15.0

- Move brush, eraser, eyedropper, hand and zoom into an independent painting DLL.
- Add SDK 1.1 typed tool API, freezing stroke parameters and retaining pressure, stabilization, interpolation and rasterization.
- Lend strided pixels with tile history instead of per-point JSON or region copies; cache instances/contact state.
- Roll back the current stroke on tool failure. Add actual-DLL pixel/history parity and reproducible release benchmarks.

## 0.14.0

- Add Windows DLL loading, versioned C ABI, independent instances and balanced lifecycle handling.
- Provide a dependency-free Rust SDK/C header and native painting action examples.
- Support atomic pixel-region callbacks, action dispatch, plugin diagnostics and atomic contribution registration.
- Stage DLLs through build scripts and verify actual loading, invalid ABI rejection and JSONL/MCP/GUI parity.

## 0.13.1

- Fix real-window drag painting by forwarding activation to winit while suppressing native title repaint with WM_NCACTIVATE lParam=-1.
- Add native message-chain regression coverage and real mouse-drag verification.

## 0.13.0

- Consolidate appearance, layout, pen, shortcuts and plugins into searchable settings with modified filtering.
- Add resizable categories, JSON validation/editing, live preview, reset, persistence and cancellation.
- Store painting defaults and plugin preferences in settings.json; add shared --config-dir and settings UI effects.

## 0.12.0

- Share a window-free Session/Action core across GUI, JSONL and MCP.
- Load declarative presets, macros and panels at startup; separate host handlers and typed pointer routing.
- Add CLI, local GUI attachment, PNG snapshots, bounded events and revision guards.
- Reconcile plugin panels without reopening intentionally closed panels. Remove the alternate legacy execution entry point.

## 0.11.0

- Move the production workbench to egui/egui_dock and Glow/OpenGL.
- Add configurable appearance metrics, JSON settings and independent dock layout persistence.
- Preserve native pen history samples, partial textures, themes and command/shortcut behavior.
- Raise MSRV to 1.88 and provide project-local build tools and dependency license staging.

## 0.10.1

- Correct maximized work-area bounds, support negative monitor coordinates and add DPI-scaled outer padding.
- Align mouse/pen, cursor, panels and menus with the corrected client geometry.

## 0.10.0

- Adopt a small embedded Google Material Symbols Rounded subset for tools, menus and shortcut keycaps.
- Add configured shortcut tooltips, glyph/DPI checks and third-party notices.

## 0.9.0

- Add docking on all four sides and tab grouping, ordering, tear-off and group movement.
- Improve floating headers, shadows, borders and resize grips; persist groups and sizes.
- Add grouping commands and regression checks across docking, visibility and DPI.

## 0.8.0

- Separate Color, Brush, Navigator and Layers into dockable/floating panels.
- Add target previews, resizing, cancellation, persistence and input isolation from the canvas.

## 0.7.2

- Suppress native non-client title/frame flashes while retaining title metadata, DWM shadows and window visibility.
- Avoid redundant title updates and add hidden-window metadata/activation regressions.

## 0.7.1

- Restore DWM composition and native shadows for custom chrome.
- Request small Windows 11 corners when restored and square corners when maximized; match theme borders.

## 0.7.0

- Adopt an editor-style activity bar, sidebars, document toolbar, command center and status bar.
- Preserve configurable sidebar visibility, sizing, swapping, focus mode and themes.

## 0.6.2

- Theme dropdown menus and submenus, align shortcut columns and preserve keyboard navigation.
- Own menu drawing resources and handle synchronous native callbacks without reentrant state borrowing.

## 0.6.1

- Replace native title chrome with themed custom controls, drag/maximize behavior and dirty document titles.
- Retain resize borders, system cursors, save confirmation and monitor work-area bounds.

## 0.6.0

- Add a unified configurable command registry for keyboard, hold gestures and wheel combinations.
- Add command search and shortcut editing/recording with conflict validation and atomic settings.json persistence.
- Support Chinese/English/pinyin search, IME input and DPI-aware native controls.

## 0.5.0

- Adopt Photoshop-style bindings, hand/zoom gestures, opacity digits, hardness steps and foreground/background color operations.
- Generate shortcut references and hints from the command registry.

## 0.4.1

- Fix continuous resize layout updates and fit/manual view behavior.
- Add appropriate resize/move cursors and consistent monitor-DPI coordinates.

## 0.4.0

- Improve themes, hierarchy, rounded controls, sliders, spacing and small-window clipping.
- Reduce transparency checker contrast and add visual regression captures.

## 0.3.0

- Add compact painting workbench, HSV/RGB controls, six presets, navigator and layer thumbnails.
- Add menus, project tabs and three themes.

## 0.2.1

- Add brush outline cursor responsive to size, zoom and pressure, with an independent clipped overlay.

## 0.2.0

- Add Windows Ink pressure/coalesced samples/eraser support, stabilization and pressure interpolation.
- Add workspace preferences, resizable/swappable sidebars and themes.

## 0.1.0

- Initial native Rust Windows painting application with brush, eraser, picker, layers and history.
- Add zoom, pan, project files and PNG import/export, initially without third-party Rust dependencies.
