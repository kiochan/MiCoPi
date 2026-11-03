# Changelog

## Unreleased

- Make new documents use linked layer compositing: graph branches follow layer insertion, duplication, reorder and deletion; per-layer filters retain identity and copy with their layer.
- Synchronize linked blend-node modes with layer properties, apply opacity once, protect stack wiring and provide connected plugin filter insertion/removal, source controls and automatic clean-view refresh.
- Preserve legacy/free graph semantics with an explicit advanced mode; validate linked topology, retain atomic undo/replay contracts and keep unfiltered stacks on the existing fast path.

- Add dab spacing and minimum pressure opacity in the Brush panel and persistent Painting settings. Freeze typed options at DOWN through an optional extension while preserving default pixels and legacy recording hashes.
- Reject custom options explicitly for DLLs without the extension, retaining existing tool tables and default compatibility.

- Surface pressure dynamics with a curve preview; add click-to-fill and drag-to-gradient desktop modes backed by shared native actions, undo, atomic rollback and verified replay.
- Add scanline four-connected fill with tolerance, premultiplied-alpha linear/radial gradients and guarded gesture cancellation.
- Skip unnecessary distance calculations outside brush dabs and inside solid hard tips while retaining reference stroke pixels and the existing tool ABI.
- Pin the configured UI theme before applying styles so first-frame system theme events cannot mix default light controls with the dark workbench.

- Add document-scoped stable layer IDs, rename/duplicate/reorder/delete actions, undoable structural edits and identity-preserving image-node remapping.
- Add exact flips, 180° rotation and clipped integer translation in the native painting DLL; expose layer menus, editable names and Ctrl+J in the four-language desktop.
- Move layer buffers during reorder/delete history and transform pixels in place; preserve legacy project/replay loading and recorded hashes.

- Make native resize tests honor effective client bounds and cover desktop tracking limits, avoiding false CI failures on small hosted displays.

- Share provisional contact recording and storage-failure rollback between GUI input and canvas.pointer actions.
- Preserve revisions after failed atomic native results and refresh replay textures when external clients replace an archive.
- Make layer creation undoable, restore its active layer on undo/redo and discard stale redo branches.
- Recognize project file headers independently of extensions and retain the opened project's save path.
- Bound GUI pipe reads and writes; acknowledge complete responses before disconnecting without an unbounded flush.

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
