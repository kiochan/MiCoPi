# Plugin windows and commands

Window contributions are owned by feature packages under `plugins/`: canvas, toolbox, color, brush, navigator and layers. The workbench provides shared layout, input routing, settings and command infrastructure. It does not own another painting document.

Each registered panel automatically contributes `window.<panel-id>.show`, `.hide`, `.focus`, `.float`, `.dock_left`, `.dock_right`, `.dock_top`, `.dock_bottom` and `.tab` actions. Tab grouping takes a `target` panel ID. `window.command` exposes the same operations with optional target, split fraction and floating bounds. Disabled plugins do not register their windows or commands. Native DLL panel contributions use the same registration path.

`window.list` and the `workbench.windows` public state report available contributions, the last request/result and any desktop-reported actual layout. A desktop command initially returns `queued: true, applied: false`; clients inspect state for presentation completion. A headless runtime returns `presentation_unavailable` and never creates or claims to have moved a window. Presentation commands cannot participate in atomic document groups.

Every action is available to the desktop command palette and shortcut editor under a `plugin.<action-id>` binding ID. These IDs are distinct from existing Photoshop-style UI shortcuts. Plugin commands default to unbound, retain configuration while their plugin is unloaded, and join ordinary conflict validation when registered. Invalid plugin shortcut conflicts disable plugin bindings with an explanatory status while keeping commands available. Parameterized commands open an argument editor; edits do not execute actions. Execution uses schema validation and a document revision guard for mutations.

GUI rendering uses feature-owned, host-compiled adapters; drawing algorithms continue to use DLLs and the typed pen ABI. The canvas, toolbox, color, brush, navigator, layers, compositing, workflow, replay and workbench adapters live alongside their feature packages and uses registered tools/actions. This is not an ABI that passes Rust or egui objects into arbitrary DLLs. Native plugins can contribute independent command-backed button panels through the existing descriptor API. Headless builds exclude egui rendering entirely.

## Workspace behavior

The default layout uses an independent Tools bar above a large Canvas window, with Navigator and property tabs on the right. Both Tools and Canvas can float, dock on any side, join a tab group or be hidden. These are **application-contained windows**, not separate operating-system windows. Hiding Canvas keeps the document, pixels, undo history and plugins alive. The Windows menu lists registered contributions; tab context menus expose docking and grouping commands.

![Independent control bar and canvas](screenshots/floating-windows.png)

One Session owns document execution. Windows share the existing dirty-region document texture; moving a window does not copy a painting surface. Pressure and stabilization still use the typed DLL input path. The root workbench handles layout and input routing; feature UI source lives in `plugins/<feature>/panel.rs`. Settings and the command argument editor live in `plugins/workbench/`. Native DLLs can register command-backed panels, but arbitrary egui widgets are not dynamically loaded across the C ABI.

## State and event contract

`window.list.actual` reports open, active, floating and logical-point bounds for each registered window. Missing bounds are null. Requests are deferred to the UI frame and `last_result.status` becomes `applied` or `failed`. A successful queue response is not proof of presentation completion. At most 128 pending presentation requests are accepted; overflow fails before mutating request state. Layouts allow up to 32 surfaces and 128 unique windows.

Drag, split and resize changes settle for 750 ms before dispatching `workspace.layout.apply` with the normalized layout. This coalesces pointer movement into a command instead of recording every frame. The same action accepts a workspace snapshot from automation. It is a desktop presentation command, excluded from atomic document groups, with explicit headless unavailability. Presentation commands and observed window metadata participate in the existing compressed outcome journal; floating-point JSON parsing preserves exact values for hash verification.

Legacy workbench panel contributions are adapted in memory to independent feature packages. The original manifest is not rewritten. Before migrating a legacy workspace, the application creates `workspace.before-plugin-windows.json` once. Existing floating and intentionally hidden windows are retained; the old main left color/brush area is consolidated with property tabs.

## Configuration and invocation

Search Settings for **Control Bar** to set floating width and preferred docked height. Existing UI padding, tab height and control metrics determine window spacing and minimum split sizes. All registered actions appear in command search and shortcut settings; parameterized actions expose schema-driven argument controls and require explicit Run.

Examples for an attached GUI (replace `session.json` with its endpoint path):

```powershell
micopi-cli call window.command '{"panel":"canvas","operation":"float","x":160,"y":180,"width":900,"height":620}' --attach session.json
micopi-cli call window.command '{"panel":"toolbox","operation":"dock_top","target":"canvas"}' --attach session.json
micopi-cli call window.color.tab '{"target":"brush"}' --attach session.json
micopi-cli call window.list '{}' --attach session.json
```

Check the queued request via `window.list` before issuing dependent layout operations. Use `app.describe` to discover complete schemas and `workbench.windows` for plugin-visible state.
