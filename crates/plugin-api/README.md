# MiCoPi native plugin SDK

`micopi-plugin-api` **1.3.0** is a dependency-free Rust/C SDK for MiCoPi's
synchronous Windows x64 native plugins. The ABI version remains **1**; SDK
versions and optional feature tails are separate from the host application
version. The [C header](include/micopi_plugin.h) and [Rust definitions](src/lib.rs)
describe the same layouts.

The SDK defines action, typed point-tool, compositor and image-kernel callbacks.
Keep mutable document/history ownership in the host, algorithms in the plugin,
and UI outside the C ABI. Every borrowed pointer is synchronous: never retain,
free, cross threads with, or reenter the host through a borrowed region. Native
DLLs run in process; this boundary provides no crash or permission isolation.

## SDK 1.3 additions

- `TOOL_OPTION_STROKE_OPACITY` (`0x8`) negotiates independent Flow and capped
  stroke opacity through a complete 64-byte `ToolOptionsStroke`. Flow lives at
  offset 56; reserved at 60 is zero. The configured bit selects Stroke mode;
  nondefault Flow alone also requires the tail.
- A complete 64-byte `ToolHostStroke` lends immutable Undo originals beside
  current RGBA/A8 storage, split at 64×64 tile intersections. Match the admitted
  target's origin capability, size and callback; preserve independent strides
  and padding. Original and writable spans must be disjoint.
- Existing `ToolParameters`, tool tables, `ToolHost`/`ToolHostA8` and option
  prefixes are unchanged. Default Flow 1 / Dab works through older prefixes;
  unsupported nondefaults reject before a contact writes pixels.

See [Native Plugins](../../docs/native-plugins.md#stroke-opacity-and-flow-sdk-13)
for layout/loan rules and [Brush opacity and Flow](../../docs/stroke-opacity.md)
for the bundled implementation's arithmetic, storage bounds and validation status.
Additional APIs remain independently capability/size-gated; a larger SDK version
does not authorize reading an unadvertised tail.

## Build an independent plugin

Use this crate as a path dependency, or the C header, without linking the host or
desktop crates. Feature package examples live under `plugins/painting`,
`plugins/native-paint`, `plugins/ai-assistant` and `plugins/compositing` in the
repository. See [Development](../../docs/development.md) and
[Contributing](../../CONTRIBUTING.md) for the existing locked build paths and
CI workflow. Runtime DLLs and local build output are not source artifacts.
