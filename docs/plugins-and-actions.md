# Plugins, Actions, MCP and Headless

All entry points operate through the same Session: GUI buttons, typed pen input, JSONL, MCP, GUI attachment and compiled workflow graphs. The GUI owns presentation and coordinate conversion; document mutation belongs to Session and plugins.

## Plugin packages

See [Unified plugin packages](plugin-system.md) for package versions, compatibility checks, per-package lifecycle states and dependency-aware enable/disable plans.

Plugins are loaded from `plugins/` beside the executable, or from `--plugins <directory>`. Packages contain `manifest.json`; first-level JSON manifests are also supported. A missing directory is initialized from the bundled definitions. Existing files and personal customizations are not overwritten.

`requires` establishes dependency order. `enabled:false` disables a package; `settings.json` also supports `plugins.disabled`. Changes take effect after restart. Registration is atomic: an invalid package contributes nothing and does not block other valid packages. Inspect `plugins.list` for duplicate IDs, bad schemas, missing dependencies or DLL errors.

| Field | Contribution |
| --- | --- |
| api_version, id, name, requires, enabled | Package identity and dependencies |
| library | DLL filename inside the package directory |
| tools | ID, English title, kind, parameters, optional preset and numeric native handler |
| actions | Exactly one of operation, steps or a native handler; input/output schemas |
| states | Namespaced read-only providers with output schemas |
| nodes | Action/state aliases or typed image node definitions |
| panels | Built-in adapters or titled groups of action buttons |

The default packages are core, painting, workbench, native-paint, ai-assistant, compositing and workflows. The painting DLL is required for brush input; missing tools report an explicit error instead of falling back to host rasterization.

[Quick tools](../examples/plugins/quick-tools/manifest.json) demonstrates a preset, macro action and button panel. Restart after installing a package. Arbitrary egui controls are not exposed through the native ABI.

## Actions and schemas

```json
{"action":"canvas.new","args":{"width":640,"height":480}}
{"action":"tool.select","args":{"tool":"brush"}}
{"action":"brush.set","args":{"color":"#4477DD","radius":12,"stabilizer":0.35}}
{"action":"canvas.stroke","args":{"points":[{"x":40,"y":60,"pen":true,"pressure":0.3},{"x":180,"y":120,"pen":true,"pressure":0.9}]}}
{"action":"canvas.export","args":{"path":"drawing.png"}}
```

Coordinates are document pixels, independent of view zoom. Color is `#RRGGBB`; opacity and pressure range from 0 to 1. Pressure mapping requires `pen:true`. Time is a millisecond timestamp; batch strokes supply an 8 ms interval if omitted. Pointer phases are down/move/up/cancel. Strokes freeze the starting tool and parameters; temporary tool overrides do not change the selected tool.

Common groups: document (`canvas.new`, `document.open/save`, `canvas.export/snapshot`), tools (`tool.select`, `brush.set`, `canvas.pointer/stroke`, `color.pick`), layers/history (`layer.add/select/set/clear`, `history.undo/redo`), state (`canvas.state`, `plugins.list`, `events.list`), configuration (`settings.read/update`), UI (`view.set`, `ui.dispatch`) and [graphs](workflow-nodes.md).

Use `app.describe`, `actions` or MCP tools/list for current schemas. The supported JSON Schema subset is object/array/string/boolean/integer/number, properties, required, additionalProperties, enum, minimum/maximum, minItems/maxItems, minLength, items, description, title and default. General schema defaults are metadata, not automatic argument insertion. Native validation always applies.

Macros use `steps` with existing action envelopes; `{"$arg":"field"}` substitutes an input value without expression evaluation. Expansion is validated before execution and is bounded. Semantics or I/O can still fail at runtime; earlier completed actions remain. Custom plugin actions do not automatically acquire bindings in the fixed command shortcut editor; invoke them from plugin panels, graphs, JSONL or MCP.

## JSONL and MCP

```powershell
micopi-cli.exe actions
New-Item -ItemType Directory -Force target/example-output | Out-Null
micopi-cli.exe run examples/automation/draw.jsonl
micopi-cli.exe call canvas.state '{}'
micopi-cli.exe mcp
```

JSONL runs one Session, returns one JSON result per line and stops at the first failure with exit code 1. Separate standalone call processes have separate sessions. Machine IDs and schemas do not vary with display language; CLI diagnostics use English.

MCP uses newline-delimited JSON-RPC over stdio, negotiates initialization, exposes registered actions as tools and public state as resources. Protocol errors are separate from action errors; failed actions mark `isError`. Stdout contains protocol data only. Examples for clients are in [headless MCP configuration](../examples/mcp/headless.json) and [GUI MCP configuration](../examples/mcp/gui.json).

## Attach to the GUI

```powershell
micopi-cli.exe call canvas.state '{}' --attach <profile>/session.json
micopi-cli.exe mcp --attach <profile>/session.json
```

A running GUI publishes its local endpoint in the profile's `session.json`. Attachment uses a local Windows named pipe, not a network listener. Work is queued to the GUI thread. A timeout does not prove that a mutation failed; query state before retrying.

`--config-dir <directory>` selects an isolated profile. `settings.update` requires an explicit path and validates/preserves other configuration fields. Updating `ui.language` changes the live display after the settings reload effect.

`ui.dispatch` supports command effects, settings.search, settings.reload, settings.close, nodes.open, asynchronous screenshot (PNG path), and close. `close:true` explicitly discards unsaved changes. Headless UI effects do not create dialogs or windows; use explicit paths for document operations and `canvas.snapshot/export` for artwork images.

Action revision guards reject stale plans. The event buffer holds only the latest 512 diagnostic events. Save JSONL or workflow graphs for durable automation. See [AI Pipeline](ai-pipeline.md), [Native Plugins](native-plugins.md) and [Workflow Nodes](workflow-nodes.md).
