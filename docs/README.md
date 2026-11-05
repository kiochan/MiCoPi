# Documentation

The technical documentation is maintained in English. The project overview is also available in [Chinese](../README.zh.md), [Japanese](../README.ja.md) and [Korean](../README.ko.md).

| Start here | What it explains |
| --- | --- |
| [Philosophy](philosophy.md) | Why MiCoPi prioritizes painting, small native components and one execution core |
| [Roadmap](../ROADMAP.md) | Priorities, milestone dependencies and acceptance criteria |
| [Photoshop-common Editing Plan](photoshop-essentials.md) | Independent functional adoption checklist, dependencies and acceptance checks |
| [User Guide](user-guide.md) | Drawing, pen input, settings, shortcuts and panels |
| [Interface Design](interface-design.md) | Visual hierarchy, configurable spacing and layout behavior |
| [Canvas Editing](canvas-editing.md) | Anchored framing, exact crop, all-layer history and public actions |
| [Layer Editing](layer-editing.md) | Stable identities, structural edits, exact transforms and reference behavior |
| [Selections](selections.md) | Marquee/lasso gestures, color commands, native clipping, history and limits |
| [Painting Tools](painting-tools.md) | Pressure controls, connected fill, gradient gestures and performance limits |
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
| [Native Plugins](native-plugins.md) | DLL lifecycle, C ABI, borrowed pixels and independent builds |
| [AI Pipeline](ai-pipeline.md) | Discovery, public state, validated execution and partial failures |
| [Image Nodes](node-editor.md) | Non-destructive filters and layer blending |
| [Workflow Nodes](workflow-nodes.md) | Schema-driven action/state nodes and AI-editable workflows |
| [Localization](localization.md) | Language settings, catalogs, fallback and contributions |
| [Settings schema](reference/settings.schema.json) | Machine-readable configuration reference |

Runnable material is indexed in [Examples](../examples/README.md); actual application captures are indexed in [Screenshots](screenshots/README.md). See [Contributing](../CONTRIBUTING.md) and [Changelog](../CHANGELOG.md) for the collaboration process and release history.
