# Photoshop-common editing plan

This is the implementation checklist for the maintainer's request to bring applicable PhotoCraft/Photoshop-common editing features to MiCoPi. Work proceeds continuously in dependency order, with small, verified commits on `main`. Unchecked items are planned, not available features.

## Reference and boundaries

The functional references reviewed on 2026-11-04 are [PhotoCraft's public feature overview](https://github.com/storytold/photocraft#features) and its [target-app parity assessment](https://github.com/storytold/photocraft/blob/main/docs/target-app-parity.md). They are feature/interaction references, not implementation templates or evidence that MiCoPi already supports a feature. No PhotoCraft source, shaders or artwork are imported. Algorithms and UI are independently implemented within this repository.

MiCoPi retains its own Session, native plugins, image graphs, action workflows, typed pen input, tiled history and verified outcome journal. Native pixel algorithms stay in plugins. Host-owned structural edits use shared document contracts. New dependencies require a concrete cost and compatibility review.

The target is the common editing workflow of an anime/manga illustrator. The checklist includes everyday painting, selections, layers, transforms, adjustments, text/shapes, interchange and workspace tools. PhotoCraft's specialist photographic, scientific, printing, cloud, generative and full Photoshop-compatibility surface requires separate evaluation; those features are not silently included as completed work.

## Existing foundation

| Area | Available foundation | Main gap |
| --- | --- | --- |
| Drawing | Native DLL tools, pressure, stabilization, spacing, pressure curve, size/shape cursor | Rich tips/dynamics and additional painting tools |
| Layers | Stable IDs, rename/duplicate/reorder/delete, opacity, visibility, alpha lock, merge/flatten/stamp | Masks, clipping, groups and multi-selection |
| Selection/editing | Document masks, gestures, color matching, edge processing, selected painting and internal transfers | System clipboard and selection transforms |
| Geometry | Anchored framing/crop, orthogonal geometry, Image Size/Trim, Move and affine Free Transform/repeat/skew | Off-canvas storage and richer project metadata |
| Composition | Linked layer/filter/blend graphs and explicit free graphs | More adjustments, filters and mask-aware composition |
| Files | PNG/JPEG/BMP import, PNG export, projects and verified replay | Rich export and layered interchange |
| Workspace | Independent/docked/tabbed windows, Tools/Options, searchable settings and shortcuts | Guides, snapping, view rotation and multiple documents |
| Automation | Schemas, public state, nodes, JSONL, MCP and GUI attachment | Extend the same contracts with every new operation |

## Execution order

### P1 — Everyday layer consolidation

- [x] Merge Down with explicit visibility, blend/filter and transparency-lock behavior.
- [x] Merge Visible while retaining hidden layers and their identities/filters.
- [x] Flatten the rendered image with an explicit transparency/background contract.
- [x] Stamp visible content into a new layer without deleting source layers.
- [x] Expose layer-menu actions, configurable PS-style shortcuts, schemas and headless examples.

Delivered on 2026-11-04. See [Layer editing](layer-editing.md) for supported blend/free-graph behavior, transparent flattening and stamp opacity. Desktop/headless regression tests cover undo, rollback, native renderer failure, project persistence and verified replay; runtime smoke compares JSONL/MCP/GUI pixels. P2 is the next stage.

Acceptance: source buffers and metadata return on undo; redo restores the merged state; filters are baked once; free graphs have an explicit supported path or fail before mutation; atomic failure restores document/history; saved projects and outcome replay reproduce the result. Test missing compositors and stale IDs/revisions.

### P2 — Selection and clipboard foundation

Implemented: bounded masks, geometry/combinations, marquee/lasso gestures, color matching, persistent sampling defaults, native edge processing, selected painting/actions and cached outlines. Undo, rollback and outcome replay share the document contracts. Internal copy/cut/paste and public clipboard state are now implemented; the Windows adapter is implemented, with native interoperability verification still pending. See [Selections](selections.md).

- [x] Add bounded document selection data, persistence, undo, atomic rollback and outcome replay.
- [x] Add rectangular/elliptical marquees, freehand/polygonal lassos and replace/add/subtract/intersect modes.
- [x] Add Select All, Deselect, Reselect and Invert with PS-style configurable bindings.
- [x] Add direct Magic Wand activation, tolerance/source/contiguous controls and Grow/Similar through shared commands.
- [x] Add persisted selection defaults to searchable settings while retaining explicit action arguments.
- [x] Add native selection feather, exact disk expansion/contraction and binary smoothing, with explicit radius/edge contracts.
- [x] Clip brush, eraser, clear, fill, gradient and applicable direct filters to the selection through a cached typed path (including the connected-fill traversal contract below).
- [x] Restrict connected fill traversal to selected coverage so excluded pixels cannot bridge separate selected islands, including expansion/feathering.
- [x] Add internal copy, copy merged, cut, centered paste and paste in place with bounded public state, atomic rollback and verified document outcomes. See [Session clipboard](clipboard.md).
- [x] Add Layer via Copy/Cut with selection-aware Ctrl+J/Ctrl+Shift+J, one structural/tiled undo, native transfers and unchanged clipboards. Preserve explicit older bindings.
- [x] Add an explicit Windows image clipboard adapter, bounded PNG/DIB codecs, local checkpoints, external-effect declarations and configurable PS bindings.
- [ ] Verify isolated native PNG/DIBV5/DIB roundtrips and cross-application clipboard interoperability on an unrestricted Windows host. This host denies private window-station creation; codec/memory-adapter tests and format-availability queries do not close this gate.

Acceptance: preview/cancel leaves pixels untouched; selection is visible and behaves identically through mouse, pen and actions; the mask is cached at contact boundaries rather than serialized/rebuilt per MOVE; overlapping selection modes and empty/full selections are tested; clipboard payloads have dimension/memory bounds and explicit external-effect semantics.

### P3 — Geometry, move and transform

- [x] Add exact document 90° clockwise/counterclockwise/180° rotations and flips, preserving all layers and selection masks. Add centered active-layer quarter turns with selection/alpha-lock guards and explicit frame clipping; off-canvas storage remains future work. See [Canvas editing](canvas-editing.md#exact-rotation-and-flips).
- [x] Add Image Size with aspect locking and measured nearest/bilinear resampling through DLL actions, menus, nodes and MCP/headless; retain masks, structural identities, undo and verified outcomes.
- [x] Add measured area averaging for substantial downscaling, with exact footprint weights, premultiplied alpha, fractional masks and bounded streaming rows. Enlarging axes use bilinear; this is a box filter, not Photoshop bicubic/Lanczos parity.
- [x] Add transparent/corner-color trim with tolerance, selected sides, cached read-only preview and shared atomic document actions. Analyze visible merged artwork; crop every raw layer and both selection masks with undo/replay.
- [ ] Add Reveal All after bounded off-canvas layer storage is available; clipped historical pixels are not live off-canvas content.
- [x] Add Move/nudge for active layers, selected pixels or selection outlines, with validated searchable steps, configurable V/arrow/Shift-arrow commands and a reversible outline drag. Publish one atomic pixel/mask outcome on release; clip beyond the frame and retain undo. Off-canvas storage remains planned. See [Move and nudge](canvas-editing.md#move-and-nudge).
- [x] Add native Free Transform translation, scale, clockwise rotation and flips, with explicit pivots, nearest/bilinear sampling, mouse/pen handles, reversible idle raster previews and apply/cancel. Retain atomic undo/replay and clip to the current frame. See [Free Transform](canvas-editing.md#free-transform).
- [x] Transform selected raw content and its current mask together, or transform only the selection outline; preserve the previous Reselect mask.
- [x] Add persisted, undoable document-space transform repeat and selected/whole-layer copy-repeat, shared by configurable PS commands, public state, nodes, MCP and headless.
- [x] Add bounded ordered local X/Y skew with numeric controls, reversible mouse/pen edge handles, skew-aware scaling and the existing native matrix/repeat/history contracts.
- [x] Add bounded finite convex perspective through an independent native sampler, interactive four-corner/edge previews, persistent world homographies and shared atomic/node/MCP/headless contracts. Preserve old affine/recording prefixes; explicitly reject crossed, degenerate and horizon-crossing source bounds.
- [x] Add validated central new-document presets, background initialization and persistent physical-resolution metadata, with a revision-guarded dialog, shared schemas/state/nodes, undo and verified replay. See [Document Setup](document-setup.md).
- [x] Preserve exact PNG density axes/units through import, export, projects and recorded-state PNG output, with bounded strict chunk validation, explicit unknown physical sizes and document-rotation axis semantics. See [PNG Density](png-density.md).
- [x] Add read-only PNG output plans, pixel/physical/percentage export sizing, density overrides and explicit cached desktop previews without changing source layers/history. Bind successful external files to compact event digests and share actions/nodes/MCP/headless execution. See [PNG Export Sizing](export-sizing.md).
- [ ] Add editable physical-unit inputs to New Canvas and Image Size.

Acceptance: coordinate units, pivots, bounds, resampling and clipping are specified; right-angle operations retain exact stored pixels; preview never publishes provisional outcomes; structural identities, graph references, undo and replay remain valid.

### P4 — Layer organization and masks

- [ ] Add general pixel/position locks, layer color labels, search/filter and multi-layer selection.
- [ ] Add groups with stable IDs, nested visibility/opacity/blending and deterministic structural history.
- [ ] Add raster masks, mask editing/enable/invert/density/feather and selection-to-mask conversion.
- [ ] Add clipping masks and mask-aware image graph ports/composition.
- [ ] Add multi-layer align/distribute/link and group/selection merge behavior.
- [ ] Add solid/gradient fill layers; evaluate pattern fills alongside the pattern library.

Acceptance: mask/group changes have explicit schema and persistence versions; old projects still load; partial invalid graphs fail safely; hidden/locked source content is retained; mask painting shares pressure/stabilization and the typed native path.

### P5 — Common adjustments and filters

- [ ] Add Levels and Curves with composite/channel controls and histogram feedback.
- [ ] Add Hue/Saturation, brightness/contrast, exposure/gamma, vibrance and color balance.
- [ ] Add grayscale/black-and-white, threshold/posterize and gradient map.
- [ ] Add Gaussian blur, sharpen/unsharp mask, noise and a small practical pixelate set.
- [ ] Expose adjustments as non-destructive plugin nodes and explicit direct pixel operations.
- [ ] Add live reversible filter previews, selection/mask support and repeat-last-filter.
- [ ] Evaluate common layer effects: outline, shadow, glow and color/gradient overlay.

Acceptance: parameter units/ranges and alpha/color behavior are documented; deterministic reference images cover transparent and edge pixels; large kernels use bounded working memory; preview cancellation preserves source pixels; no filter runs while merely editing a workflow.

### P6 — Illustrator painting tools

- [ ] Add brush flow, sampled/textured tips, tip shape/rotation and deterministic spacing/size/opacity/color dynamics.
- [ ] Add brush preset editing/import/export within a bounded, versioned MiCoPi format.
- [ ] Add straight-line/angle-assisted strokes, symmetry and useful smoothing modes.
- [ ] Add clone stamp and measured blur/sharpen/smudge tools through native plugins.
- [ ] Surface tilt/rotation/device capability data with explicit unsupported-device fallbacks.
- [ ] Evaluate external brush/palette/pattern formats individually with compatibility fixtures.

Acceptance: seeded dynamics replay exactly; cursor previews match configured tips; legacy typed-tool prefixes remain readable; benchmark pen input, commit cost, allocations and preset/texture cache bounds. Physical driver support needs named hardware evidence.

### P7 — Text, shapes and layout aids

- [ ] Add editable point/paragraph text with font, size, color, alignment, line spacing and CJK rendering.
- [ ] Add rectangle, ellipse, polygon and line tools with fill/stroke controls.
- [ ] Add a path/shape data contract before pen-path editing, rasterization and vector masks.
- [ ] Add rulers, guides, a configurable grid and snapping.
- [ ] Add non-destructive view rotation, mirrored view and fit/reset commands.

Acceptance: text/shape content remains independent of UI language; fonts and fallback behavior are explicit; snapping units and zoom/DPI transforms agree across viewports; text/vector rasterization is isolated behind a documented plugin or rendering contract.

### P8 — Files, export and practical workspace

- [ ] Add explicit JPEG/WebP export with quality, scale, alpha/background policy and preview.
- [ ] Evaluate TIFF and OpenRaster layered interchange with read/write limits and round-trip fixtures.
- [ ] Implement common PSD import from published format specifications, with explicit unsupported-content diagnostics.
- [ ] Implement a tested layered PSD export subset; retain the native project/replay format as the full-fidelity source.
- [ ] Add ICC/profile-aware import/export and document color-space metadata before broader depth/model support.
- [ ] Add multiple document tabs with separate Session/history/replay and explicit automation document targets.
- [ ] Add named workspace presets and session/layout recovery under bounded cache rules.

Acceptance: malformed files are bounded before allocation; unsupported data is reported rather than silently advertised as supported; export never overwrites a native project accidentally; separate document state cannot leak between AI/MCP targets. Full Photoshop round-trip fidelity is a separate measured claim.

## Delivery rules

For each increment: implement a complete usable slice, add schemas/public state and configurable commands, preserve undo/atomic/replay compatibility, update English technical docs and relevant UI catalogs, run the corresponding regression/smoke checks, update this checklist and CHANGELOG, and commit with an explanation and actual validation results. Push only on an explicit maintainer request.

Update this plan when investigation reveals a dependency or a narrower supported contract. Keep unchecked requirements visible. Never mark a command as implemented solely because a menu item dispatches it.
