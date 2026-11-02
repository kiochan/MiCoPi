# AI-ready Action Plugin

Independent cdylib with no host or GUI dependency. Requires host 0.16+.

- assistant.context queries the active document, open files, input and another DLL's native_paint.stats.
- assistant.checkerboard submits the unified pipeline, invokes another DLL and reads the result.
- ai_assistant.context exposes the query as public state for plugins, MCP and headless.

Build with cargo build --release -p micopi-ai-assistant --locked or scripts/build.ps1 -Portable -PluginsOnly. Install the manifest and DLL beside the executable under plugins/ai-assistant/ and restart.

To create a plugin, copy this directory and change package name, manifest identity/library, action/state IDs and implementation. Declare dependencies, discover schemas, preserve host pointer lifetimes, prevent unwinding across C ABI and handle partial failure. Do not call back into the same instance. See [Pipeline](../../docs/ai-pipeline.md) and [Native ABI](../../docs/native-plugins.md).
