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
| Layers | Stable IDs, rename/duplicate/reorder/delete, opacity, visibility, alpha lock | Merge/stamp, masks, clipping, groups and multi-selection |
| Selection/editing | Fill color matching and edge processing | Document selection, clipboard and selection-aware operations |
| Geometry | Anchored canvas size, exact crop, layer flips/180°/offset | Image resampling, quarter turns and free transform |
| Composition | Linked layer/filter/blend graphs and explicit free graphs | More adjustments, filters and mask-aware composition |
| Files | PNG/JPEG/BMP import, PNG export, projects and verified replay | Rich export and layered interchange |
| Workspace | Independent/docked/tabbed windows, Tools/Options, searchable settings and shortcuts | Guides, snapping, view rotation and multiple documents |
| Automation | Schemas, public state, nodes, JSONL, MCP and GUI attachment | Extend the same contracts with every new operation |

## Execution order

### P1 — Everyday layer consolidation

- [ ] Merge Down with explicit visibility, blend/filter and transparency-lock behavior.
- [ ] Merge Visible while retaining hidden layers and their identities/filters.
- [ ] Flatten the rendered image with an explicit transparency/background contract.
- [ ] Stamp visible content into a new layer without deleting source layers.
- [ ] Expose layer-menu actions, configurable PS-style shortcuts, schemas and headless examples.

Acceptance: source buffers and metadata return on undo; redo restores the merged state; filters are baked once; free graphs have an explicit supported path or fail before mutation; atomic failure restores document/history; saved projects and outcome replay reproduce the result. Test missing compositors and stale IDs/revisions.

### P2 — Selection and clipboard foundation

- [ ] Add bounded document selection data, persistence, undo, atomic rollback and outcome replay.
- [ ] Add rectangular/elliptical marquees, freehand/polygonal lassos and replace/add/subtract/intersect modes.
- [ ] Add Select All, Deselect, Reselect and Invert with PS-style configurable bindings.
- [ ] Add tolerance-based Magic Wand, connected/global color matching and Grow/Similar.
- [ ] Add selection feather, expansion/contraction and smoothing.
- [ ] Clip brush, eraser, clear, fill, gradient and applicable filters to the selection through a cached typed path.
- [ ] Add copy, copy merged, cut, paste, paste in place, duplicate selection to layer and selection-to-layer cut.
- [ ] Add internal clipboard state for headless execution and an explicit desktop system-clipboard adapter.

Acceptance: preview/cancel leaves pixels untouched; selection is visible and behaves identically through mouse, pen and actions; the mask is cached at contact boundaries rather than serialized/rebuilt per MOVE; overlapping selection modes and empty/full selections are tested; clipboard payloads have dimension/memory bounds and explicit external-effect semantics.

### P3 — Geometry, move and transform

- [ ] Add exact image/layer 90° clockwise/counterclockwise rotations and document flips.
- [ ] Add Image Size with aspect locking and nearest/bilinear resampling, followed by a measured higher-quality sampler.
- [ ] Add transparent/edge-color trim and Reveal All for supported off-canvas content.
- [ ] Add Move/nudge with explicit active-layer/selection targets and configurable modifier steps.
- [ ] Add Free Transform translate, scale, rotate and flip with a reversible preview and explicit commit/cancel.
- [ ] Add transform repeat, selection-aware transforms and subsequent skew/perspective where the data model supports them.
- [ ] Add new-document presets, background/resolution metadata and useful export sizing.

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
