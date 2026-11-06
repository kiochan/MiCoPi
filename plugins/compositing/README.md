# Node Compositor DLL

Independent cdylib using the dependency-free ABI SDK. Filter and blend pixel algorithms live here, outside the host.

SDK 1.2 adds optional micopi_compositor_v1; action/tool ABI v1 remain unchanged. The host validates topology and passes one dirty rectangle, immutable source layers, typed instructions and output storage. Render the complete region without per-pixel DLL calls or full-size allocation per node.

Instruction kinds: 0=layer (a=index), 1=filter (a=previous slot, mode=filter), 2=blend (a=background, b=foreground, amount=opacity), 3=output (a=previous slot), and optional 4=transparent zero. Kind 4 has no inputs and requires `a=b=mode=0` and `amount=0`. Limits remain 64 instructions and 16 real layers. The final slot is output; an empty instruction list means ordinary layer stacking.

`micopi_compositor_v1` retains the original `MicopiCompositorApi` prefix. Its `struct_size` covers the complete `MicopiCompositorApiFeatures` tail with a `uint64_t capabilities` field, followed by the optional `MicopiCompositorApiModes` supported-mode extension. Hosts read the tail only after checking its size and require `MICOPI_COMPOSITOR_TRANSPARENT` before submitting kind 4. Existing hosts can continue to call the prefix with their original instructions; a host using an older compositor must reject unsupported transparent instructions instead of flattening their meaning.

The plugin registers `compositing.transparent` as an image node with no input ports or parameters. One zero slot can seed all isolated group subtrees. Composite each group's children first, then blend that completed value into its parent with the group's own opacity and blend mode once. Applying group opacity to every child changes overlap alpha. Two overlapping opaque children under 50% group opacity yield alpha 128, rather than alpha 192. Nested opacity gates retain the same 8-bit rounding rules as ordinary blends.

The kernel supports 26 [color blend modes](../../docs/blend-modes.md). IDs 0..7 remain normal, multiply, screen, overlay, darken, lighten, linear dodge (`add`) and difference with unchanged arithmetic. IDs 8..25 append Color Burn, Linear Burn, Darker Color, Color Dodge, Lighter Color, Soft Light, Hard Light, Vivid Light, Linear Light, Pin Light, Hard Mix, Exclusion, Subtract, Divide, Hue, Saturation, Color and Luminosity. Filters 0..4 remain invert, grayscale, brightness, contrast and saturation. Pixels are straight-alpha ARGB; transparent composited output is zero. Source layers remain immutable.

The complete `CompositorApiModes` / `MicopiCompositorApiModes` table is 32 bytes on x64, with a 64-bit supported-mode bitmap at offset 24. Prefix-only or 24-byte Features tables imply modes 0..7. A complete bitmap must include mandatory legacy mask `0xff`, and may advertise a subset of appended modes. The bundled DLL advertises all 26. Hosts read only complete advertised tails and reject unsupported modes before native dispatch, including hidden/disabled content. Old hosts can continue using their unchanged prefix and original modes. No callback signature changes.

`graph.catalog.blend_modes` lists recognized names; `graph.read.available_blend_modes`, `graph.catalog.available_blend_modes` and public `compositing.graph` report current availability, including the CPU Normal fallback. Layer/group/node controls use one family-ordered, bounded scrolling menu and keep unsupported choices visibly disabled.

The kernel evaluates a dirty rectangle with a fixed 64-word scratch array, reusing the transparent slot without an extra source layer or image intermediate. Output storage must be disjoint from source pixels, instruction/descriptor arrays and its own descriptor. Dimension, span, alias and instruction validation completes before output writes; strided row padding is preserved. Group compilation must count generated instructions against the native limit before publication.

Kernel tests compare all 26 blend modes and alpha boundaries with independent normalized references and protect original eight-mode output hashes. High-precision Decimal/Fraction goldens cover every Soft Light and Hard Mix byte pair, with explicit rational half-byte rounding cases. They also cover once-only and nested group opacity, hidden/empty groups, all 16 source layers, strided dirty rectangles, unchanged original plans, and rejection before writes for invalid instructions, aliases or bounds. These synthetic tests establish pixel and memory-contract behavior, not physical tablet latency.

```powershell
cargo build --release -p micopi-compositing --locked
.\scripts\build.ps1 -Portable -PluginsOnly
```

Install DLL and manifest under plugins/compositing/ and restart. Schema node definitions in the manifest register aliases for existing kernel operations; a replacement renderer must preserve the ABI and semantics. See [Image Graphs](../../docs/node-editor.md) and [Plugin Nodes](../../docs/workflow-nodes.md).
