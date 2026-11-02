# Native Painting Tools

Independent cdylib for brush, eraser, eyedropper, hand and zoom. Handler IDs are 1, 2, 3, 4 and 5 respectively. The host never links this crate.

src/lib.rs manages contact state and frozen parameters; src/input.rs implements pressure and stabilization; src/raster.rs handles interpolation, dabs and blending. The host lends layer pixels synchronously, records undo tiles and validates view/color changes.

```powershell
cargo build --release --locked -p micopi-painting-tools
.\scripts\build.ps1 -Portable -PluginsOnly
```

Install the manifest and DLL under plugins/painting/ beside the executable and restart. Presets remain editable without compiling. SDK micopi_tools_v1 is separate from the action interface. Respect stride, region bounds and borrowed pointer lifetimes; never reenter the host from pixel callbacks. See [Native Plugins](../../docs/native-plugins.md).

Input filtering tests run in this crate. Host tests compare the actual DLL against a reference implementation for pixels, pressure, stabilization, history and failure recovery. The build script optionally runs a release benchmark.
