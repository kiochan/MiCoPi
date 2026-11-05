# Native Painting Tools

Independent cdylib for brush, eraser, eyedropper, hand and zoom. Handler IDs are 1, 2, 3, 4 and 5 respectively. The host never links this crate.

src/lib.rs manages contact state and frozen parameters; src/input.rs implements pressure and stabilization; src/raster.rs handles interpolation, dabs and blending. The host lends layer pixels synchronously, records undo tiles and validates view/color changes.

```powershell
cargo build --release --locked -p micopi-painting-tools
.\scripts\build.ps1 -Portable -PluginsOnly
```

Install the manifest and DLL under plugins/painting/ beside the executable and restart. Presets remain editable without compiling. SDK micopi_tools_v1 is separate from the action interface. Respect stride, region bounds and borrowed pointer lifetimes; never reenter the host from pixel callbacks. See [Native Plugins](../../docs/native-plugins.md).

Input filtering tests run in this crate. Host tests compare the actual DLL against a reference implementation for pixels, pressure, stabilization, history and failure recovery. The build script optionally runs a release benchmark.

The optional `micopi_tool_options_v1` extension configures diameter-based spacing and pressure opacity floor before DOWN. Each contact freezes its options; absent configuration retains legacy defaults. Tests validate headers/ranges, reject mid-contact configuration, preserve zero-pressure transparency and count spacing-dependent callbacks. See [Painting tools](../../docs/painting-tools.md).


`micopi_projective_v1` provides an optional finite homography sampler beside the unchanged affine table. It reuses validated immutable-source pixel/coverage loops, with no scratch allocation or tool-instance creation per sample. The host owns convex corner plans, source horizons, previews, staged publication, history and replay. See [Perspective Corners](../../docs/canvas-editing.md#perspective-corners) and the [SDK contract](../../docs/native-plugins.md#optional-projective-sampling-table). This extension does not change the typed pressure/stabilization path.
