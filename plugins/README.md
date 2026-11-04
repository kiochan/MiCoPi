# Plugin Packages

This directory is the installable extension boundary. It contains manifests and independent native implementation crates. `src/plugins/` at the repository root contains host-side loading, validation and dispatch instead.

| Package | Responsibility |
| --- | --- |
| core | Document, layers, history, image, configuration and discovery actions |
| painting | Typed brush, eraser, eyedropper, hand and zoom DLL, with presets |
| workbench | Panels, view commands and UI effects |
| native-paint | Native action examples, pixel regions, statistics and a node alias |
| ai-assistant | Cross-plugin public state and pipeline example |
| compositing | Typed region compositor and image node definitions |
| replay | Host-owned outcome journal, integrity checks, preview and frame export APIs |
| workflows | Action/state graph discovery, editing, validation and execution |

Build native implementations with `scripts/build.ps1 -PluginsOnly` or the corresponding Cargo package. DLLs are build output and remain ignored. Distributions install each DLL with its manifest beside the application; plugin source stays versioned here.

See [Plugin Guide](../docs/plugins-and-actions.md), [Native ABI](../docs/native-plugins.md), [Workflow Nodes](../docs/workflow-nodes.md) and the [Quick Tools example](../examples/plugins/quick-tools/manifest.json). New pixel behavior belongs in a suitable plugin; the host retains document ownership, history and validation.

## Feature windows

`canvas`, `toolbox`, `color`, `brush`, `navigator` and `layers` independently register window contributions. Compositing and workflows own separate editors. UI adapters live in each feature package (`panel.rs`), with settings and command forms under `workbench/`. They are compiled into the desktop host; drawing algorithms remain DLLs. The host supplies common layout, Session dispatch and a shared document texture. See [Plugin Windows](../docs/plugin-windows.md) for commands, schema arguments, state and runtime limits.
