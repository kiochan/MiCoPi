# Changelog

## Unreleased

Implemented changes since 0.21.0, grouped by their current behavior. Git history retains the individual commits and intermediate design iterations.

### Painting and document editing

- Add shared PNG export plans and native composed-frame sizing in pixels, millimeters, inches or percent with aspect locking, area/bilinear/nearest methods and output-only density overrides. Keep previews read-only and source layers, masks, metadata, file targets and undo intact; identity output bypasses sampling. Add Canvas-owned, revision-guarded options with configurable spacing and explicit cached previews before the save dialog. Publish PNGs through unique temporary siblings, protect native projects and bind successful artifact hashes to compact external-effect events without replaying writes. Distinguish post-publication recording failures, extend snapshot/node/MCP/headless contracts and add four-language controls, English guides, a reviewed Release capture, examples and regressions. New Canvas/Image Size physical-unit input and additional export formats remain planned.

- Preserve exact PNG physical/aspect density through bounded full-stream validation, document metadata, projects and shared live/recorded-state PNG output. Keep zero/unspecified axes explicit, expose known per-axis PPI/physical sizes through shared schemas/state/nodes, and let resolution commands replace raw density without resampling. Swap raw axes with document quarter turns under one undo, including unchanged square pixels. Preserve legacy default encoding/hashes and fast compression (only a 21-byte density chunk changes), support deliberate 72 PPI output, reuse cached dependencies and keep pen MOVE unchanged. Add translated import warnings, English contracts, a reviewed Release capture, measured cold costs and density/rollback/transport regressions. New Canvas/Image Size physical-unit input remains planned.

- Add configurable illustration/square/A4/B5 creation presets, editable initial background pixels and bounded physical-resolution metadata. Move New Canvas into the Canvas UI package with appearance-derived spacing, scrolling controls, a visible footer and identity/revision guards; keep drafts and cancellation inert. Share document-properties actions/state/nodes with MCP and headless, preserve metadata through project/undo/atomic outcome replay, omit legacy defaults and restore a failed new-document journal append. Keep preset collections in validated searchable settings and retain user names/unknown root fields. Add four-language presentation, English contracts, a reviewed Release capture, automation and regression checks. New Canvas/Image Size physical-unit input remains planned.

- Add bounded four-corner perspective through an independent optional projective DLL table and shared Free Transform actions/plans. Preserve the current frame when converting affine controls to corner editing; support provisional mouse/pen corners, edges, axis-constrained moves, numeric coordinates and reset without per-MOVE rasterization. Require idle contacts for corner-mode mutation/projective repeat, and validate convexity, determinant, coefficient limits and source horizons before staging; reuse native alpha/coverage loops with no affine ABI prefix change. Persist an optional denominator row for repeat/copy, projects, one undo/outcome and plugin-independent replay while retaining default-row recording hashes. Add header/callback/partial-output rejection checks, analytic pixel/mask references, GUI/transport regressions, four-language presentation, English contracts, a reviewed Release capture and an automation example. Frame clipping, bilinear minification and physical-device validation boundaries remain explicit.

- Extend Free Transform with ordered local X/Y skew angles, numeric controls and reversible mouse/pen edge diamonds with 5° Shift snapping. Unskew corner coordinates before scale drags, preserve pivots and validate combined coefficients before plans/sampling. Reuse the unchanged affine DLL/recipe contracts for selected pixels/masks, repeat/copy, undo, atomic rollback and verified replay; omit zero skew fields when serializing old-style drafts. Keep Apply/Cancel below a separately scrolling controls region. Add native lattice-shear references, core/UI failure and gesture checks, transport/replay/four-language smokes, English contracts, a reviewed Release capture and an example.

- Add document-owned, validated last-transform recipes with exact matrix repeat and whole-layer/selected copy-repeat. Share PS-style configurable commands, Edit menu entries, four-language titles, public state and schemas across nodes, MCP and headless. Stage copies with one structural undo/outcome, preserve source pixels and duplicate filter mapping, and restore recipes through project loading, undo/redo, atomic rollback and verified replay. Clear stale recipes on changed whole-canvas geometry; identity transforms retain the prior recipe. Reuse existing native samplers/transfer kernels without an ABI or dependency change; keep absent recipe metadata out of legacy hashes. Add regression, transport smoke and automation coverage plus English contracts.

- Add independently implemented affine Free Transform through native pixel/mask callbacks, typed actions, nodes, MCP and headless. Support bounded translation, positive scale factors, flips, clockwise rotation, content-aware layer bounds, explicit/centered pivots and nearest/bilinear sampling with transparent outer edges. Preserve raw words on aligned permutations, selected cut/source-over semantics, previous Reselect masks, stable graph/layer identities and one atomic tiled undo/outcome; stage before lock checks and restore journal/batch failures. Add a registered Ctrl+T preview with mouse/pen corner/rotation/move handles, numeric controls, revision guards and cancel/apply. Cache idle raster previews using borrowed source layers and the existing compositor, with no provisional document/history mutation or per-MOVE pixel work; share cached mask contours. Expose read-only geometry plans and desktop-only open/configure requests explicitly, preserve older shortcut preferences and add four-language controls, English contracts/examples, parity/rollback/replay regressions and reviewed captures. Frame clipping remains explicit; off-canvas storage remains planned.

- Add a V-activated Move tool, integer drag and configurable arrow/Shift-arrow nudges for active layers, selected pixels and selection outlines. Cache source bounds and preview only the offset outline during dragging; release submits one shared atomic action and Escape, lost focus, modal changes or stale sources cancel. Stage immutable-source native pixel/mask outputs before one tiled undo/outcome; retain fractional cut/source-over semantics, transparency locks, stable layer/graph IDs and the previous Reselect mask. Expose schemas and effective/default movement state to workflows, MCP and headless clients; persist validated target/step settings with searchable preview/cancel/save controls and preserve older explicit shortcuts. Add four-language controls, transport/rollback/replay/UI regressions, English contracts and an automation example. Off-frame pixels are clipped and recoverable through undo; live pixel previews and off-canvas storage remain planned.

- Add transparent/top-left/bottom-right color Trim with alpha/RGBA tolerance and selectable sides. Analyze visible merged artwork through an allocation-free DLL extension; expose read-only bounds and atomic trimming as typed actions/nodes for MCP/headless and a draggable four-language Image menu dialog placed beside the central artwork. Cache previews until parameters change, reject stale revisions, retain all-matching/full-frame no-ops and crop all raw layers plus current/Reselect masks in one undo/outcome event. Preserve hidden/locked layer and graph identities, validate native bounds before publication and restore atomic/journal failures. Add independent edge/reference checks, UI preview/cancel/apply regressions, English contracts and an automation example. Reveal All remains planned pending off-canvas storage.

- Add Image Size with aspect-locked dimensions, native nearest/bilinear/area sampling and a configurable Ctrl+Alt+I dialog. Resample all raw layers and current/Reselect masks; interpolate premultiplied alpha to avoid transparent-color fringes. Area integrates exact shrinking footprints with two cached source rows and one accumulator, retaining fine-line coverage without a full intermediate image; enlarging axes use bilinear. Stage outputs before one structural undo/event, retain identities and restore atomic/journal failures. Share schemas with nodes and MCP/headless, preserve older shortcuts and require a complete optional area-capability tail before dispatch. Add reference/rejection tests, four-language controls, English contracts/examples, reviewed captures and measured kernel costs; default sampling remains bilinear. Make the shared texture cache detect unsynchronized dimension changes before partial uploads, preventing resize-undo bounds failures while retaining ordinary stroke updates.

- Add independently implemented DLL document quarter/half turns and flips plus centered active-layer quarter turns. Share seven schema-described actions across menus, configurable commands, nodes, MCP and headless; retain layer/node identities, fractional current/Reselect masks, one undo entry and verified outcomes. Stage bounded outputs before publication and restore atomic/journal failures; retain optional-ABI compatibility. Layer turns explicitly clip to the current canvas and honor destination selection/alpha lock. Add four-language titles, intrinsic menu label/keycap spacing, contract/reference tests and a runnable example. Off-canvas storage remains planned.

- Implement an explicit Windows image clipboard adapter and registered Copy/Merged/Cut/Paste/In-Place commands, public format state and configurable PS bindings. Reuse native selection extraction/placement, keep headless Session transfers separate, prepare movable buffers before publication and preserve local state on busy/codec/journal failures. Add bounded PNG and packed DIB codecs, checked origin hints, partial-publication status feedback and compact external-effect audit context; exclude system mutations from atomic groups and replay. Add safe codec/failure tests and an opt-in private-window-station fixture. Native write/read and cross-application verification remain pending: the current host denies private station creation with Access Denied; status-only desktop checks do not prove roundtrip interoperability.

- Add Layer via Copy/Cut through shared actions, Edit/Layers menus, workflow nodes and MCP/headless. Extract selected raw pixels with the existing native transfer ABI; preserve clipboards and source filter identities, deselect with Reselect support, and combine source clearing plus layer insertion into one tiled structural undo entry. Keep whole-layer duplication when no selection is active, reject locked cuts/empty masks/unsupported free graphs, and roll back atomic/journal failures. Default Ctrl+J/Ctrl+Shift+J bindings yield to explicit older preferences. Update the plan, four-language titles/overviews and English contracts/examples with a reviewed application capture.

- Add bounded Session clipboard copy, merged copy, cut, centered paste and paste-in-place through shared schemas, command search, configurable bindings, workflow nodes and MCP/headless. Native extraction/placement preserves selected alpha; paste inserts above the active layer and deselects in one undo entry. Share clipboard payloads for atomic rollback, publish public digest summaries, and record compact transfer context plus verified document outcomes without replaying DLLs or replacing the live clipboard. Preserve old ABI prefixes and recording hashes; reconcile new core public resources into portable packages while retaining custom definitions. The Windows adapter is implemented; native interoperability verification remains pending.
- Add DLL selection Expand/Contract with exact disk coverage, fixed-point Gaussian-approximate Feather and binary Smooth, bounded radius/edge policies and constant-mask fast paths. Expose four-language menu/parameter commands and schemas to workflows/MCP/headless; retain fractional morphology, source pixels, undo, atomic rollback and one verified event per operation. Size-check the optional edge tail and reject unsupported/oversized outcomes. Synchronize pending local view state before forms while retaining external revision guards, and make Cancel close command forms. Add typed desktop-only workspace capture that records the observation request without changing state/revision or invalidating a parameter form.
- Add native Wand, Grow and Similar color-selection commands with bounded tolerance, connected/global matching, active/merged sampling and exact original-palette semantics. Expose schemas to command forms, workflows and MCP/headless clients; preserve fractional coverage, source pixels, undo, atomic rollback and recorded outcomes. Size-check the optional DLL color tail, reject unavailable capabilities and retain old selection prefixes. Add a W-activated canvas Wand, size-bounded vector icon and session-local sampling controls with a Grow/Similar menu; publish effective controls to public state and reject desktop-only controls in headless mode. Preserve older explicit W bindings. Tool Options wraps and scrolls vertically instead of hiding controls off the horizontal edge. Persist tolerance/connectivity/source defaults under validated `settings.json` selection fields, with searchable reset/preview/cancel/save controls. Expose defaults separately from temporary controls; load profiles in desktop/CLI, honor omitted action arguments and preserve explicit arguments, older workflow literals and unrelated custom settings. Reject invalid values/write failures before changing runtime defaults, and keep other-profile writes inactive.
- Add mouse/pen rectangle and ellipse marquees plus freehand and polygonal lassos, with bounded reversible previews, Shift/Alt combination modes, square/circle constraints and configurable M/L shortcuts. Put selection options in the independent Tool Options bar; submit one shared selection action on completion and discard stale/cancelled previews. Preserve existing custom M/L bindings and append missing portable Toolbox commands without overwriting custom definitions. GUI activation commands explicitly reject headless execution, where geometry actions remain available.
- Restrict native connected fill traversal to selected coverage, including expanded/feathered fills, so excluded pixels cannot bridge independent selected islands. Add a size-checked immutable action-entry selection snapshot to the host ABI, preserve the unselected fast path and apply fractional selection coverage once at commit.
- Add DLL-generated rectangle/ellipse/polygon selection commands, combine modes and configurable PS selection shortcuts. Clip typed pressure/stabilized brushes, erasers, clear and native pixel actions; show a cached outline and expose masks to MCP/headless/workflow clients. Persist shared compressed masks with undo, atomic rollback and verified replay. Reject oversized project metadata before replacing saved files.
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
