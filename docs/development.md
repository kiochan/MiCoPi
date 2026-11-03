# Development and Verification

## Standard Windows build

Requires Rust 1.88+, Rust 2021, Visual Studio C++ Build Tools and the Windows SDK. Default desktop dependencies are eframe/egui 0.33, egui_dock 0.18 and egui-snarl 0.9 with Glow. No WebView or wgpu is enabled.

```powershell
cargo fmt --all -- --check
cargo check --all-targets --locked
.\scripts\build.ps1 -PluginsOnly
cargo test --locked
cargo run --release --locked
.\scripts\build.ps1 -Test
```

Build scripts produce dist/MiCoPi.exe, a versioned executable, CLI, plugin DLLs and dependency licenses. The default build root is target/build; override with -BuildRoot. An in-use executable is preserved and a versioned/build-suffixed output is produced; the script does not close user applications.

## Project-local portable toolchain

Python 3.12+ prepares the local Rust/GNU linker toolchain under ignored target/ directories. Bootstrap downloads need network access and do not install a system toolchain.

```powershell
python scripts/bootstrap-portable.py
python scripts/bootstrap-egui.py
.\scripts\build.ps1 -Portable -Test
.\scripts\build.ps1 -Portable -PluginsOnly
.\scripts\build.ps1 -Portable -Headless -Test -BuildRoot target/headless
```

The headless build excludes desktop/OpenGL. Runtime releases need their plugin directory and licenses, not Rust or downloaded build tools. Existing dist/plugin customization is retained; packages distributed to others should use the current default manifests.

## Checks

Unit tests cover document/history, codec, schemas, actions, public state, pipelines, workflow graphs, settings, shortcuts, pen routing, docking, Windows ABI and offscreen presentation. Build plugin DLLs first. Four explicitly ignored tests cover the independent DLL fixture and optional benchmarks; build.ps1 -Test runs the DLL lifecycle fixture separately.

```powershell
python scripts/smoke-runtime.py
python scripts/smoke-workflows.py
python scripts/smoke-linked-graphs.py
python scripts/smoke-localization.py
```

Runtime smoke compares JSONL, MCP and GUI PNG bytes and tests streaming input, settings and plugin preferences. Workflow smoke tests DLL-contributed nodes, dry runs, patching, project persistence and inert editing. Localization smoke checks English defaults, four-language switching/persistence, captures settings and node UIs, and confirms unchanged canvas pixels. Each script uses its own profile and closes only the GUI process it created.

Use --cli and --gui to select debug binaries for quick UI iteration. Outputs are retained under target/test-output/. Visually inspect screenshots after changes, including English and CJK text at narrow and wide window sizes. Translation catalog tests validate key parity and format placeholders. Documentation and new source messages must be English.

Actual tablet driver compatibility requires hardware testing. Existing tests exercise Windows Ink ABI and sample behavior, not every physical device.

## Performance

Keep typed pen input separate from JSON orchestration. Preserve dirty-region textures and compositing, cached plugin instances, reused buffers and 64×64 tile history. Measure changed hot paths rather than asserting machine-specific timing in CI.

```powershell
.\scripts\build.ps1 -Portable -Test -Benchmark
```

The DLL/reference benchmark interleaves five runs of 80 strokes and 5,120 varying-pressure samples, compares full pixels/history and reports medians. It includes Session/input/raster/history work but excludes GUI presentation. Timing is a local observation, not a cross-machine performance guarantee.

## Repository and release packaging

Documentation is indexed in [docs/README.md](README.md); runnable scripts and configuration examples are indexed in [Examples](../examples/README.md). Run `python scripts/check-repository.py` after reorganizing either. Build artifacts remain ignored.

After building, `python scripts/package.py` creates the portable ZIP from versioned binaries, source-controlled default manifests, current documentation and examples. It excludes personal configuration and historical screenshots. Archive entries use stable ordering and timestamps. This packages locally and does not publish a release.

## Source and artifacts

Keep Cargo.lock. Exclude target/, dist/, downloaded toolchains, local settings, session endpoints and personal artwork. Main structure and invariants are in [Architecture](architecture.md). DLL ownership and independent build instructions are in [Native Plugins](native-plugins.md). Language catalog contributions are documented in [Localization](localization.md).

Replay end-to-end check: `python scripts/smoke-replay.py`. It isolates its profile and process, compares recorded and live PNG bytes, tests embedded history and bounded frame export, and plays archives through MCP without painting/compositor plugins. See [the replay contract](replay.md) for integrity and performance limits.

The existing release `dll_brush_benchmark` includes recording in the DLL path. On this development environment, 5,120 samples / 80 strokes took a median 262 ms with recording versus 71 ms for the in-process raster-only reference (five interleaved runs, 512×384 canvas). This includes outcome capture, composition, compression and journal writes at stroke boundaries; it is not a per-MOVE latency measurement or a hardware-independent target. Recording has a real cost even though individual MOVE calls avoid serialization and disk I/O.

The 0.21 development benchmark (including atomic contact checkpoints and compact events) measured 268 ms for the same 5,120 inputs / 80 strokes, versus 67 ms for the raster-only reference. These figures are environment-specific and include commit costs, not just MOVE latency. Atomic rollback and compression regression tests include deterministic partial-write injection. The original v1 fixture is preserved under `tests/fixtures/replay/v1.mcreplay`.

## Plugin window regression

Run `python scripts/smoke-windows.py` after staging the portable build. The check validates owned-window stacking after activation, whole-group moves, settled native resize stability, independent top-level HWNDs, native mouse input, monitor movement where displays are available, restart persistence, command discovery and dock/float/tab/hide/show behavior, explicit headless presentation results, unchanged PNG pixels, project persistence and replay verification in an isolated GUI/profile. Captures remain under ignored `target/test-output/window-smoke/`.
