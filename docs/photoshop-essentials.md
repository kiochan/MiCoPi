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
| Layers | Stable IDs, multi-selection/batches, isolated/Pass Through groups and baking, locks/labels, selected-only merge/flatten/stamp, public raster-mask painting/effects and document geometry | Remaining mask/merge/transfer integration, clipping and multi-layer align/distribute |
| Selection/editing | Document masks, gestures, color matching, edge processing, selected painting and internal transfers | System clipboard and selection transforms |
| Geometry | Anchored framing/crop, orthogonal geometry, Image Size/Trim, Move and affine Free Transform/repeat/skew | Off-canvas storage and richer project metadata |
| Composition | Linked layer/filter/blend graphs, explicit free graphs and generated layer/group raster-mask coverage gates | More adjustments, filters and clipping relationships |
| Files | PNG/JPEG/BMP import, PNG export, projects and verified replay | Rich export and layered interchange |
| Workspace | Independent/docked/tabbed windows, Tools/Options, searchable settings and shortcuts | Guides, snapping, view rotation and multiple documents |
| Automation | Schemas, public state, nodes, JSONL, MCP and GUI attachment | Extend the same contracts with every new operation |

## Execution order

The next slices prioritize illustration work. P1–P8 remain stable backlog labels;
they do not require finishing text or photographic interchange before improving
painting. Shared admission, ownership, history and performance gates apply to
every slice.

| Priority | Next usable slice | Dependency and delivery boundary |
| --- | --- | --- |
| 1 | Blend and mask behavior artists can trust | Build on the delivered 26 color blend modes. Dissolve remains pending with its separate spatial coverage contract; use delivered bounded linked/unlinked owner movement as the foundation for broader mask transforms. |
| 2 | Linked multi-layer editing | Persistent link sets, linked integer Move/nudge and exact-set affine Free Transform are implemented; runtime validation of the affine slice remains pending CI. Continue with align/distribute. Painting stays active-only. |
| 3 | Non-destructive adjustment layers | Define ordered affect-below composition, group/mask scope and transparent-pixel behavior before adding the first practical adjustment controls. |
| 4 | Brush quality and presets | Prioritize flow, sampled/textured tips, deterministic dynamics, preset editing and responsive stabilization, with measured contact and cache costs. |
| 5 | Liquify for illustration corrections | Start with one bounded native deformation tool, reversible previews and deterministic mesh/stroke input; retain one undo and verified outcome per committed edit. |

Editable text, complex photographic processing and layered interchange stay in
the backlog below for later delivery. Small file/workspace fixes can proceed
independently when they unblock painting; no pending item is removed or marked
complete by this priority change.

### P1 — Everyday layer consolidation

- [x] Merge Down with explicit visibility, blend/filter and transparency-lock behavior.
- [x] Merge Visible while retaining hidden layers and their identities/filters.
- [x] Flatten the rendered image with an explicit transparency/background contract.
- [x] Stamp visible content into a new layer without deleting source layers.
- [x] Expose layer-menu actions, configurable PS-style shortcuts, schemas and headless examples.

Delivered on 2026-11-04. See [Layer editing](layer-editing.md) for supported blend/free-graph behavior, transparent flattening and stamp opacity. Desktop/headless regression tests cover undo, rollback, native renderer failure, project persistence and verified replay; runtime smoke compares JSONL/MCP/GUI pixels. Grouped Merge Visible and Group Merge were subsequently implemented in P4.

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
- [x] Add shared read-only creation plans and editable New Canvas pixel/millimeter/inch dimensions, with bounded rounding and explicit density intent. Preserve canonical pixel presets, legacy defaults and the existing lifecycle rollback/replay contract.
- [x] Add editable Image Size pixel/millimeter/inch input and read-only all-layer/pixel-selection plans, retaining exact density, native sampling, one atomic undo and verified outcomes. Guard physical drafts against external resolution changes.

Acceptance: coordinate units, pivots, bounds, resampling and clipping are specified; right-angle operations retain exact stored pixels; preview never publishes provisional outcomes; structural identities, graph references, undo and replay remain valid.

### P4 — Layer organization and masks

- [x] Add persistent pixel/position locks, color labels and read-only layer search/filter through shared actions/state/nodes, typed host permissions, one undo and verified replay.
- [x] Add stable-ID multi-layer selection and explicit property, duplicate, delete, block-order and exact pixel batch targeting, with shared actions/state/nodes, one Undo/Redo group/revision/outcome and rollback. Painting remains active-only; owner/linked affine transforms use explicit stable-ID scopes.
- [x] Add versioned stable-ID isolated groups with nested visibility/opacity/blending, typed subtree operations, cached actual composition, deterministic history/replay and shared nodes/MCP/headless contracts.
- [x] Add explicit Group Merge and group-aware Merge Visible with documented backdrop/hidden-content/filter behavior, one structural undo/outcome, shared typed nodes/MCP/headless execution and failure rollback. Retain the group's outer properties or the complete visible root result as appropriate; keep hidden branches/skeletons for Merge Visible.
- [x] Add selected-only merge for 2–16 explicit stable raster IDs, including noncontiguous/cross-parent targets. Project selected leaves and intermediate filter/group gates over transparency, retain the common parent/ancestor gates and place the result above the highest original targeted branch. Preserve unselected buffers/filter identities, every folder and selection masks; retain the top target ID/name/label/position flag and select the result alone. Share read-only plans, contextual Ctrl+E, optional revision-guarded review, one Undo/revision/outcome and atomic/render/journal rollback. Enforce the existing 64-instruction projection/final limits and keep group v1/v2 public schemas compatible. See [Selected-only merge](selected-merge.md).
- [x] Add opt-in Pass Through groups with explicit incoming-backdrop semantics, once-only integer premultiplied opacity, nested isolation, optional native capability checks and independent alpha/opacity references. Retain isolated defaults/bytes and verified outcomes; reject standalone baking of a Pass Through target. See [Pass Through](pass-through.md).
- [x] Expand shared layer/group/node color blending to 26 modes, including Soft/Hard Light, Color Dodge/Burn, Linear Burn and Hue/Saturation/Color/Luminosity. Preserve the original eight mode IDs/pixels, advertise optional native mode capabilities, reject unavailable modes before mutation and retain alpha/history/replay references. See [Color blend modes](blend-modes.md).
- [ ] Add deterministic Dissolve coverage with an explicit document-coordinate/seed contract, dirty-region equivalence, alpha/opacity references and verified replay. It is a spatial coverage operation rather than a color-channel equation.
- [x] Add native A8 painting and typed mask-composition prerequisites with compatible tool/compositor tails, grayscale/reveal-white sinks, once-only opacity/coverage, independent kernel references and actual-DLL conformance. This gate does not provide document mask ownership, history, persistence or UI; follow [Raster Masks Design](raster-masks-design.md) for the remaining integration.
- [x] Add bounded mask identity/ownership/raw A8 storage, byte-tiled history and rollback, strict MICOPI04 projects, separate verified byte outcomes and generated typed layer/group coverage gates. Keep default legacy bytes/hashes and DLL-free restoration; explicitly reject pending editing paths. See [Raster-mask storage](raster-mask-storage.md).
- [x] Add the public layer/group mask painting and effects subset: cached pressure/stabilized A8 Brush/Eraser, enable/invert/density/feather, selection conversion, raster-layer raw-alpha Apply, bounded sample queries, schema-derived actions/state/nodes and four-language thumbnails/target feedback. Preserve raw planes, selection clipping, atomic failure recovery, tiled Undo and verified replay. See [Raster mask editing](raster-mask-editing.md).
- [x] Add the independent native dense/uniform A8 sampler prerequisite for mask-aware reframe, orthogonal geometry and nearest/bilinear/area sampling, preserving existing tables and byte arithmetic.
- [x] Integrate document-wide canvas framing/crop, rotations/flips, Image Size and Trim with every raw layer/group mask, including hidden/disabled/unlinked masks. Add shared frame plans, separate raw/effective budgets, Brush/Hand contact admission, complete atomic staging, one Canvas Undo/outcome and same-size replay/cache invalidation. Retain white expansion and fixed numeric feather; see [Mask canvas geometry](mask-canvas-geometry.md).
- [x] Add plain single-layer owner copy/delete with fresh copied mask IDs, exact raw/property/filter retention, explicit paint-target rules, one compound Undo/outcome, idle contact admission and schema-derived result bindings. See [Mask owner editing](mask-owner-editing.md).
- [x] Add owner-preserving group creation, selected-raster grouping and hierarchy reparent/order on mask-bearing documents, with source/candidate admission, unchanged raw A8 planes/targets, one structural Undo/outcome and shared result-schema bindings. See [Mask group editing](mask-group-editing.md).
- [x] Add mask-bearing group/subtree deletion, including empty masked groups, complete consumed owner removal, explicit target/selection fallback, shared result arrays and one compound Undo/outcome with rollback. See [Mask group deletion](mask-group-delete.md).
- [x] Add mask-bearing group/subtree duplication, including empty groups and hidden/disabled/unlinked masks, with ordered fresh IDs, exact independent raw buffers, explicit target/selection mapping, required schema outputs and one compound Undo/outcome. See [Mask group duplication](mask-group-duplicate.md).
- [ ] Complete owner/group perspective, masked selected-pixel/selection-outline affine, owner Copy Repeat, group-mask movement, selected-pixel transfers, ungrouping and batch structural edits, Layer via Copy/Cut, merges/flatten/clipboard mutations and group-mask application before marking the full mask feature complete.
- [ ] Add clipping masks and mask-aware image graph ports/composition.
- [x] Add persistent stable-ID raster link sets and explicit linked Move/nudge, with localized controls, lock/mask/idle admission, checked aggregate staging, one grouped Undo/revision/outcome, strict structural persistence and shared state/node/MCP/headless contracts. Keep painting distinct from selection and membership; see [Layer Links](layer-links.md).
- [x] Implement owner/exact-set linked affine Free Transform and captured stable-ID Repeat, sharing one raw union/pivot/matrix, linked mask sampling, bounded isolated previews, guarded desktop drafts and one Undo/outcome with nodes/MCP/headless. Preserve old unmasked perspective and default recipe bytes. Tests are authored and pending CI; masked selection/outline, group-mask and perspective/Copy Repeat boundaries remain guarded. See [Owner affine](owner-affine.md).
- [ ] Verify multi-layer align/distribute with explicit captured targets and one Undo/outcome per committed edit.
  - [Layer arrangement](layer-arrangement.md) now implements raw-ink edge/center alignment, center distribution and horizontal/vertical equal edge spacing, with linked masks, read-only plans, guarded desktop controls and node/MCP/headless commands. Unequal sizes, negative gaps, signed rounding, exact target order, Undo/rollback/replay and tiny transport cases are authored. Full runtime and reviewed native captures remain pending; keep this verification gate unchecked.
- [ ] Add solid/gradient fill layers; evaluate pattern fills alongside the pattern library.

Acceptance: group/mask changes have explicit schema and persistence versions; old projects still load; partial invalid graphs fail safely. Implemented selected-only merge reports placement and globally hidden targets, preserves unselected storage and surviving filters, rejects protected targets before rendering and restores consumed hidden content on Undo. Its projected forest may change outside-backdrop/interleaved-stack appearance; the plan does not promise equality to the original full composite. The public raster-mask subset shares pressure/stabilization, native effects, bounded storage, Undo and verified outcomes. Full mask acceptance still requires every structural/geometry/transfer rule in the design; clipping masks remain planned. Raw-alpha Apply retains source filters and layer properties and explicitly documents its 8-bit rounding behavior.

### P5 — Common adjustments and filters

- [ ] Add Levels and Curves with composite/channel controls and histogram feedback.
  - Source/affect-below Curves and per-channel Levels now share typed intent, controls, cached tables, masks, history and verified persistence. [Histogram queries](histograms.md) cover explicit composite/raw and fenced Source/Actual image ports. CI for `4ffa90d` passed desktop/headless/native and all configured transport checks. New published-node chart controls and extended four-language layouts remain pending runtime verification. The ignored Release [cost probe](histogram-performance.md) now measures bounded raw/plan callbacks with independent bins and allocation counts; results await CI. Reviewed visual captures, end-to-end Refresh measurements and physical-device validation remain open.
- [ ] Add Hue/Saturation, brightness/contrast, exposure/gamma, vibrance and color balance.
  - [Direct Brightness/Contrast](brightness-contrast.md) now shares the existing encoded-byte source-filter math through native cold 256-byte table preparation, active-raster selection/locks, atomic Undo/rollback and outcome replay. Commands, parameter forms, workflow nodes, JSONL and MCP use the registered action. Existing source/adjustment Brightness and Contrast remain non-destructive. Fenced read-only PNG previews now share admitted immutable HSL/tonal staging and return guarded Apply commands. New runtime/transport references await CI; coupled desktop sliders and a shared cached draft are now implemented in source; new UI runtime, native capture/review and measured costs remain pending.
  - [Direct Hue/Saturation](hue-saturation.md) now registers a native active-raster command with optional HSL parameters, selection coverage, pixel locks, alpha preservation, shared idle admission, Undo and verified outcomes. Schema-derived workflow nodes, pipelines, command forms, MCP and headless share execution. Native/Session/transport regressions await CI. Non-destructive HSL source nodes/adjustment layers now share typed intent, separate cached parameters, controls, masks, previews/bakes, Undo, projects and verified outcomes. New mixed-slot/cache/contact/persistence/layout/transport tests await CI; Colorize and remaining color adjustments stay planned. The optional [native HSL prerequisite](native-hsl.md) now supplies shared scalar math, complete render/observation callbacks, fixed parameter slots and cached startup capability discovery. SDK/native/host references await CI; the separate typed document integration is now implemented in source, pending runtime verification. The opt-in Release [HSL callback cost probe](histogram-performance.md#hsl-dirty-region-and-observation-probe) now covers padded 32/256-square regions and bounded sampling on 1024/4096-square sources with independent references and allocation counts; results await CI.
  - A registered fenced direct HSL preview now returns an immutable composed PNG and an existing revision-guarded Apply action through nodes/MCP/headless. New native/Session/transport references await CI; cached desktop drafts now add debounced private Canvas textures, explicit stale-source refresh and guarded Apply/Cancel. Runtime and reviewed captures remain pending.
- [ ] Add grayscale/black-and-white, threshold/posterize and gradient map.
  - [Threshold and Posterize](quantization.md) now register direct native active-raster commands with integer weighted comparisons or cold byte tables, alpha/hidden-RGB preservation and shared selection/locks/history/outcomes. Native all-level/byte references, Session rollback/contact/default-workflow tests and JSONL/MCP bins are authored; typed source nodes and affect-below adjustment layers now share controls, mask/stack composition, all-declaration capability admission, intent slots, projects, Undo and verified outcomes. Mixed HSL/Curves slots, failure/contact/provider changes, four-language controls and extended transport tests await CI; direct fenced PNG previews and cached Apply/Cancel controls now share the color draft, with new runtime/UI checks pending. The [optional native plan prerequisite](native-quantization.md) now shares integer kernels and bounded render/observation callbacks with cached capability discovery; new compatibility/reference checks await CI, and typed document integration is now implemented in source pending validation. A bounded Release [quantization callback probe](histogram-performance.md#quantization-dirty-region-and-observation-probe) now covers padded ROIs, sample grids, independent pixels/bins and allocation/output reuse; results await CI and do not establish end-to-end preview performance.
  - [Direct weighted grayscale](grayscale.md) now registers a native active-raster percent command with an exact integer full-strength path and unchanged legacy partial mixing. Selection, alpha, locks, masks, atomic Undo/rollback, outcomes and workflow/MCP/headless share Session. Fenced PNG previews and cached Strength/Reset/Apply/Cancel controls now share the color draft, with exact preview/Apply, stale/default/contact and four-language layout cases authored. Runtime and native screenshot review remain pending; configurable channel-mixer black-and-white remains planned.
- [ ] Add Gaussian blur, sharpen/unsharp mask, noise and a small practical pixelate set.
- [ ] Expose adjustments as non-destructive plugin nodes, real ordered adjustment layers that affect content below them, and explicit direct pixel operations. Define group/isolation/Pass Through and mask scope, opacity/blending, persistence and shared action/state schemas before publication; retain source pixels and one undo/outcome for stack or parameter edits.
- [ ] Add live reversible filter previews, selection/mask support and repeat-last-filter.
- [ ] Evaluate common layer effects: outline, shadow, glow and color/gradient overlay.
- [ ] Add native plugin Liquify with bounded deterministic mesh/stroke deformation, explicit interpolation/frame clipping, reversible cached previews and apply/cancel. Commit one undo/revision/verified outcome, restore without executing the plugin, and gate delivery on rollback, transparent-edge references, brush latency and peak-memory measurements.

Acceptance: parameter units/ranges and alpha/color behavior are documented; deterministic reference images cover transparent and edge pixels; large kernels use bounded working memory; preview cancellation preserves source pixels; no filter runs while merely editing a workflow. Adjustment layers participate in ordered image composition with explicit affect-below boundaries, group gates and masks. Liquify stages immutable-source results under bounded mesh, sample and preview-cache limits; it does not add whole-image copies or workflow execution to individual pen MOVE calls.

### P6 — Illustrator painting tools

- [x] Implement independent Flow and opt-in capped stroke opacity through cached native Brush/Eraser tools on RGBA and raw A8 targets, preserving pressure/stabilization, fractional selection, one Undo/outcome and DLL-free replay. Retain default per-dab pixels and bound coverage/origin storage. Tests and bounded contact measurements are wired to CI; runtime and physical-device evidence are recorded separately. See [Stroke opacity](stroke-opacity.md).
- [ ] Add sampled/textured tips, accurate tip shape/rotation feedback and deterministic spacing/size/opacity/color dynamics through cached native tools.
- [ ] Add brush preset editing/import/export within a bounded, versioned MiCoPi format.
- [ ] Add straight-line/angle-assisted strokes, symmetry and configurable smoothing/stabilization modes with explicit latency and pressure-continuity behavior.
  - [Brush symmetry](native-brush-symmetry.md) now implements an optional native tail and fixed-size reflected dabs after shared filtering/spacing, including destination selection, A8/RGBA, Flow and shared capped influence. Session parameters, settings/presets, public state/commands/nodes/MCP/headless, idle four-language controls and view-aware axis/tip feedback are implemented in source. SDK/native/Session/UI and tiny transport/replay cases are authored pending CI; reviewed captures and measured contact/cache/upload costs remain open. Keep this grouped item unchecked; radial/angled axes and the other assisted stroke modes remain planned.
- [ ] Add clone stamp and measured blur/sharpen/smudge tools through native plugins.
- [ ] Surface tilt/rotation/device capability data with explicit unsupported-device fallbacks.
- [ ] Evaluate external brush/palette/pattern formats individually with compatibility fixtures.

Acceptance: seeded dynamics replay exactly; cursor previews match configured tips; legacy typed-tool prefixes remain readable; benchmark pen input, commit cost, allocations and preset/texture cache bounds. Physical driver support needs named hardware evidence.

### P7 — Text, shapes and layout aids

Editable text and the larger path/vector model follow the painting-priority
slices above. Guides and view aids may ship independently when they improve the
drawing workflow.

- [ ] Add editable point/paragraph text with font, size, color, alignment, line spacing and CJK rendering.
- [ ] Add rectangle, ellipse, polygon and line tools with fill/stroke controls.
- [ ] Add a path/shape data contract before pen-path editing, rasterization and vector masks.
- [ ] Add rulers, guides, a configurable grid and snapping.
- [ ] Add non-destructive view rotation, mirrored view and fit/reset commands.
  - [Horizontal/vertical view mirroring](view-mirroring.md) now reflects Canvas textures, input and edit overlays through shared coordinates, with unchanged document pixels/export/history, desktop/contact guards, command discovery and four-language feedback. Source/UV/zoom/cursor/synthetic input and compatibility cases are authored; runtime/native capture/device validation remains pending. Both mirror axes and their combination share the same mapping and synthetic input checks. Arbitrary view rotation remains planned; keep this grouped item unchecked.

Acceptance: text/shape content remains independent of UI language; fonts and fallback behavior are explicit; snapping units and zoom/DPI transforms agree across viewports; text/vector rasterization is isolated behind a documented plugin or rendering contract.

### P8 — Files, export and practical workspace

Complex layered interchange and photographic color-management expansion follow
the painting-priority slices. Keep their compatibility and malformed-input gates
visible; ordinary save/export reliability remains part of daily painting.

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
