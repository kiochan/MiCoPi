# Documentation

[Direct Hue/Saturation](hue-saturation.md) describes the native active-raster
command, selection/lock/idle admission and shared workflow/MCP/headless path.
Typed source/adjustment HSL integration and shared controls are now implemented
in source, with new runtime verification pending.
The [native HSL prerequisite](native-hsl.md) adds independent optional render/
observation callbacks and cached negotiation; the document integration stores
separate parameter slots and shares verified persistence.

See [Histograms](histograms.md) for bounded native color queries through Session,
nodes and MCP, with shared manual chart overlays and explicit Source/Actual node
input/output selection. Backend runtime CI passed; new picker tests remain pending.

The optional [prepared-plan histogram foundation](histogram-plan.md) evaluates a
bounded native sample grid without an image copy. The [cost probe](histogram-performance.md)
separates verified backend behavior from pending callback measurements and UI checks.

See [Levels](levels.md) for typed source effects, affect-below adjustments and
cached native rendering.

[Curves](curves.md) source nodes and affect-below adjustments share typed
master/channel intent, dedicated local controls and bounded native rendering.

The optional [Channel Levels native foundation](channel-levels.md) adds negotiated
preparation/rendering and shared source/adjustment channel controls; the new
integration references await CI.

The technical documentation is maintained in English. The project overview is also available in [Chinese](../README.zh.md), [Japanese](../README.ja.md) and [Korean](../README.ko.md).

| Start here | What it explains |
| --- | --- |
| [Philosophy](philosophy.md) | Why MiCoPi prioritizes painting, small native components and one execution core |
| [Roadmap](../ROADMAP.md) | Priorities, milestone dependencies and acceptance criteria |
| [Photoshop-common Editing Plan](photoshop-essentials.md) | Independent functional adoption checklist, dependencies and acceptance checks |
| [User Guide](user-guide.md) | Drawing, pen input, settings, shortcuts and panels |
| [Interface Design](interface-design.md) | Visual hierarchy, configurable spacing and layout behavior |
| [PNG Export Sizing](export-sizing.md) | Shared pixel/physical/percentage sizing, explicit preview, artifact hashes and file safety |
| [PNG Density](png-density.md) | Exact physical/aspect density, unknown dimensions, bounded import and replay output |
| [Document Setup](document-setup.md) | New-canvas presets, editable background initialization, physical resolution and shared actions |
| [Canvas Editing](canvas-editing.md) | Framing/crop, rotation/flips, resampling/trim, Move/nudge, Free Transform, history and public actions |
| [Layer Editing](layer-editing.md) | Stable identities, structural edits, exact transforms and reference behavior |
| [Selected-only Merge](selected-merge.md) | Explicit raster targets, common-parent projection, placement/visibility plans, optional review and atomic baking |
| [Layer Groups](layer-groups.md) | Nested compositing policies, typed entries, hierarchy commands, node views and native capability compatibility |
| [Pass Through](pass-through.md) | Parent-backdrop composition, once-only group opacity, Mix nodes, baking boundaries and versioned compatibility |
| [Raster Mask Storage](raster-mask-storage.md) | A8 ownership/history/composition, MICOPI04 and verified replay foundation |
| [Raster Mask Editing](raster-mask-editing.md) | Public mask painting/actions/effects/UI subset, raw-alpha Apply and remaining owner/structural boundaries |
| [Raster Mask Canvas Geometry](mask-canvas-geometry.md) | All-plane framing, rotations/flips, Image Size/Trim, shared plans, budgets and atomic history/replay |
| [Raster Mask Owner Editing](mask-owner-editing.md) | Plain single-layer copy/delete, fresh IDs, paint-target rules, compound Undo/outcomes and remaining structural boundaries |
| [Raster Mask Group Editing](mask-group-editing.md) | Group creation and hierarchy moves with retained mask owners/data, explicit targets and reversible composition |
| [Raster Mask Group Deletion](mask-group-delete.md) | Nested owner deletion, shared multi-mask structural history, explicit result IDs and compound Undo/replay |
| [Raster Mask Group Duplication](mask-group-duplicate.md) | Independent copied owner buffers, distinct fresh-ID orders, target/selection mapping, compound history and checkpoint isolation |
| [Color Blend Modes](blend-modes.md) | The 26-mode catalog, RGB/alpha semantics, family menus, availability and legacy DLL compatibility |
| [Clipboard](clipboard.md) | Internal transfers, Windows PNG/DIB adapter, limits, external effects and native verification status |
| [Selections](selections.md) | Marquee/lasso gestures, color commands, native clipping, history and limits |
| [Painting Tools](painting-tools.md) | Pressure controls, connected fill, gradient gestures and performance limits |
| [Brush Opacity and Flow](stroke-opacity.md) | Independent Flow, opt-in per-stroke influence limits, public settings, live original-tile loans and bounded native storage |
| [Analytic Brush Tips](brush-tips.md) | Round/elliptical and square/rectangular tips, fixed clockwise angle, nominal cursor contours, optional native negotiation and legacy-default compatibility |
| [User Brush Presets](brush-presets.md) | Versioned scalar Brush/Eraser library, both-color preservation, exact providers, cached state and bounded data-only import/export |
| [Command Contact Admission](command-contact-admission.md) | Private cached frontend policy, shared execution, argument-form synchronization fences and native/local input continuity |
| [Raster Owner Movement](raster-owner-movement.md) | Linked layer/mask integer Move, independent stable-mask targets, outline admission, compound history and exact clipping/fill rules |
| [Layer Links](layer-links.md) | Persistent raster sets, explicit multi-owner Move/nudge, active-only painting, grouped history and shared action/state nodes |
| [Owner and Linked Free Transform](owner-affine.md) | Exact captured raster/mask scopes, union bounds, one affine mapping, grouped recipe/history, repeat policies and separate preview costs |
| [Adjustment Layers](adjustment-layers.md) | Real affect-below stack items, registered scalar filters, group/A8 scope, separate inspection, strict compatibility floors and shared history/outcomes |
| [Layer Alignment and Distribution](layer-arrangement.md) | Explicit ink-bound targets, linked masks, integer placement and shared plans |
| [Atomic Operations](atomic-operations.md) | Rollback, plugin capabilities, contact commits and compact event storage |
| [Verified Replay](replay.md) | Event journal, integrity checks, time-lapse preview and frame export |
| [Project Structure](project-structure.md) | Directory responsibilities, generated files and Git boundaries |
| [Architecture](architecture.md) | Session ownership, execution, rendering and persistence |
| [Development](development.md) | Toolchains, builds, tests and performance checks |
| [Agent Guide](../AGENTS.md) | Repository instructions, architecture invariants and verification by change type |

## Extending and automating MiCoPi

| Guide | Scope |
| --- | --- |
| [Plugins and Actions](plugins-and-actions.md) | Packages, manifests, JSONL, MCP and GUI attachment |
| [Unified Plugin System](plugin-system.md) | Package metadata, compatibility, dependency plans, contribution ownership and hot-reloadable language packs |
| [Plugin Windows](plugin-windows.md) | Independent control bar, canvas, feature adapters and registered layout commands |
| [Capture Requests](capture-requests.md) | Asynchronous workspace/native PNG capture, request IDs, bounded queues, diagnostic polling and file/renderer boundaries |
| [Native Plugins](native-plugins.md) | DLL lifecycle, C ABI, borrowed pixels and independent builds |
| [Native Raw-A8 Affine](native-a8-affine.md) | Independent scalar sampler and optional neutral-support bounds, explicit borders and borrowed output for owner geometry |
| [AI Pipeline](ai-pipeline.md) | Discovery, public state, validated execution and partial failures |
| [Image Nodes](node-editor.md) | Non-destructive filters and layer blending |
| [Workflow Nodes](workflow-nodes.md) | Schema-driven action/state nodes and AI-editable workflows |
| [Localization](localization.md) | Language settings, catalogs, fallback and contributions |
| [Settings schema](reference/settings.schema.json) | Machine-readable configuration reference |

## Implementation designs

| Design | Status |
| --- | --- |
| [Paintable Raster Masks](raster-masks-design.md) | Foundations, painting/actions/effects/UI, document geometry, owner copy/delete/affine, group creation/movement and subtree copying/deletion; group-mask geometry, ungrouping, batches, merges and selected-pixel transfers remain planned |

Runnable material is indexed in [Examples](../examples/README.md); actual application captures are indexed in [Screenshots](screenshots/README.md). See [Contributing](../CONTRIBUTING.md) and [Changelog](../CHANGELOG.md) for the collaboration process and release history.
