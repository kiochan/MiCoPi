# Node Compositor DLL

Independent cdylib using the dependency-free ABI SDK. Filter and blend pixel algorithms live here, outside the host.

SDK 1.2 adds optional micopi_compositor_v1; action/tool ABI v1 remain unchanged. The host validates topology and passes one dirty rectangle, immutable source layers, typed instructions and output storage. Render the complete region without per-pixel DLL calls or full-size allocation per node.

Instruction kinds: 0=layer (a=index), 1=filter (a=previous slot, mode=filter), 2=blend (a=background, b=foreground, amount=opacity), 3=output (a=previous slot). Limits: 64 instructions, 16 layers. The final slot is output; an empty instruction list means ordinary layer stacking.

Blend modes 0..7: normal, multiply, screen, overlay, darken, lighten, linear dodge, difference. Filters 0..4: invert, grayscale, brightness, contrast, saturation. See graph.catalog for ranges. Pixels are straight-alpha ARGB; transparent composited output is zero. Source layers remain immutable.

```powershell
cargo build --release -p micopi-compositing --locked
.\scripts\build.ps1 -Portable -PluginsOnly
```

Install DLL and manifest under plugins/compositing/ and restart. Schema node definitions in the manifest register aliases for existing kernel operations; a replacement renderer must preserve the ABI and semantics. See [Image Graphs](../../docs/node-editor.md) and [Plugin Nodes](../../docs/workflow-nodes.md).
