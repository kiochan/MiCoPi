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

Runtime smoke compares JSONL, MCP and GUI PNG bytes and tests streaming input, settings and plugin preferences. Workflow smoke tests DLL-contributed nodes, dry runs, patching, project persistence and inert editing. Localization smoke checks English defaults, four-language switching/persistence, captures settings, nodes and plugin panels, exercises valid/invalid/restored host and plugin language files, and confirms unchanged canvas pixels and workflow graphs. Each script uses its own profile and closes only the GUI process it created.

Package metadata, compatibility, lifecycle and dependency planning have focused tests under `plugins::packages::tests`. External catalog tests under `i18n::external::tests` cover scope isolation, translated command search, profile precedence, path containment and file budgets. Build matching native DLLs before running tests that load the default plugin set; otherwise loader failures can hide the behavior being tested.

Use --cli and --gui to select debug binaries for quick UI iteration. Outputs are retained under target/test-output/. Visually inspect screenshots after changes, including English and CJK text at narrow and wide window sizes. Translation catalog tests validate key parity and format placeholders. Documentation and new source messages must be English.

Actual tablet driver compatibility requires hardware testing. Existing tests exercise Windows Ink ABI and sample behavior, not every physical device.

## Performance

Keep typed pen input separate from JSON orchestration. Preserve dirty-region textures and compositing, cached plugin instances, reused buffers and 64×64 tile history. Measure changed hot paths rather than asserting machine-specific timing in CI.

```powershell
.\scripts\build.ps1 -Portable -Test -Benchmark
```

The DLL/reference benchmark interleaves five runs of 80 strokes and 5,120 varying-pressure samples, compares full pixels/history and reports medians. It includes Session/input/raster/history work but excludes GUI presentation. Timing is a local observation, not a cross-machine performance guarantee.

The selection increment adds a third interleaved run using a full-canvas frozen mask and verifies exact pixel parity with unrestricted drawing. On this development environment, the 2026-11-04 release run measured 371 ms unrestricted DLL, 349 ms full-selection DLL and 94 ms raster-only reference per 5,120 samples/80 strokes (five-run medians, 512×384). The selection/unrestricted ratio was 0.939; scheduling/cache variation and commit/recording work are included, so this is not evidence that selection makes painting faster or a measurement of physical pen latency. No mask bytes are copied or compressed on MOVE; the new per-dab work is a coverage read and opacity multiplication.

## Repository and release packaging

Documentation is indexed in [docs/README.md](README.md); runnable scripts and configuration examples are indexed in [Examples](../examples/README.md). Run `python scripts/check-repository.py` after reorganizing either. Build artifacts remain ignored.

For documentation-only changes, run the repository checker and `git diff --check`; do not rebuild the application solely to validate prose. Keep root README translations, roadmap checkboxes and the [Agent guide](../AGENTS.md) consistent with implemented behavior. The portable package includes these root guides so relative links remain valid after extraction.

After building, `python scripts/package.py` creates the portable ZIP from versioned binaries, source-controlled default manifests, current documentation and examples. It excludes personal configuration and historical screenshots. Archive entries use stable ordering and timestamps. This packages locally and does not publish a release.

## Source and artifacts

Keep Cargo.lock. Exclude target/, dist/, downloaded toolchains, local settings, session endpoints and personal artwork. Main structure and invariants are in [Architecture](architecture.md). DLL ownership and independent build instructions are in [Native Plugins](native-plugins.md). Language catalog contributions are documented in [Localization](localization.md).

Replay end-to-end check: `python scripts/smoke-replay.py`. It isolates its profile and process, compares recorded and live PNG bytes, tests embedded history and bounded frame export, and plays archives through MCP without painting/compositor plugins. See [the replay contract](replay.md) for integrity and performance limits.

The existing release `dll_brush_benchmark` includes recording in the DLL path. On this development environment, 5,120 samples / 80 strokes took a median 262 ms with recording versus 71 ms for the in-process raster-only reference (five interleaved runs, 512×384 canvas). This includes outcome capture, composition, compression and journal writes at stroke boundaries; it is not a per-MOVE latency measurement or a hardware-independent target. Recording has a real cost even though individual MOVE calls avoid serialization and disk I/O.

The 0.21 development benchmark (including atomic contact checkpoints and compact events) measured 268 ms for the same 5,120 inputs / 80 strokes, versus 67 ms for the raster-only reference. These figures are environment-specific and include commit costs, not just MOVE latency. Atomic rollback and compression regression tests include deterministic partial-write injection. The original v1 fixture is preserved under `tests/fixtures/replay/v1.mcreplay`.

## Plugin window regression

Run `python scripts/smoke-windows.py` after staging the portable build. The check validates owned-window stacking after activation, whole-group moves, settled native resize stability, independent top-level HWNDs, native mouse input, monitor movement where displays are available, restart persistence, command discovery and dock/float/tab/hide/show behavior, explicit headless presentation results, unchanged PNG pixels, project persistence and replay verification in an isolated GUI/profile. Captures remain under ignored `target/test-output/window-smoke/`.

## Color-selection checks and costs

The painting DLL tests compare color-tree queries to exhaustive RGBA search and strided Wand connectivity to a breadth-first reference, including fractional combinations, alpha-zero hidden RGB and invalid-buffer rejection. Session tests use staged DLLs for raw-versus-merged sampling, undo/redo, stale arguments/revisions, atomic failure, saved projects and outcome restoration without DLL execution. Optional-table fixtures cover legacy, complete and truncated color tails.

Run the explicit release measurement with `cargo test --release --locked -p micopi-painting-tools color_selection_cost -- --ignored --nocapture` (add the portable target/environment arguments from above when needed). On 2026-11-05 this development environment measured a 2048×2048 deterministic random-color source with half-canvas original coverage and tolerance 4: seeded contiguous Wand 8 ms, Grow 2477 ms and Similar 2363 ms. These single-run observations include test fixture copying/assertions but exclude host mask compression, history and journal commit. The large unique reference palette makes Grow/Similar cold commands visibly expensive; this is not pen latency, and no tree construction or color sampling is added to MOVE. Smaller repeated-color artwork generally has a smaller palette; these figures are not a performance guarantee.

## Selection edge costs

Run `cargo test --release --locked -p micopi-painting-tools selection_edges_cost -- --ignored --nocapture` with the portable target/environment arguments above when needed. Exhaustive small-image disk references cover binary/fractional, strided and border cases; naive fixed-point box convolutions independently check feather/smooth. Fixtures cover the exact radius-256 boundary, uniform/empty masks and invalid buffers. Actual-DLL Session checks cover unchanged source pixels, one event per operation, project/undo/replay, workflow dry runs, stale arguments/revisions, missing/truncated tails, oversized results and atomic failure.

On 2026-11-05 this environment measured radius 32 on a centered rectangle: 4096×4096 binary Expand 442 ms, Contract 436 ms, Feather 500 ms, Smooth 500 ms; 1024×1024 fractional Expand 488 ms. These single-run release observations include output/workspace allocation and source classification, but exclude host mask compression, history, journal commit, rendering and GUI latency. Fractional morphology scales with radius and pixel count; do not extrapolate the binary result to a large feathered mask.

Binary transforms use one u32 image workspace (64 MiB at 4096²) plus dimension-sized reused line/envelope buffers. Feather/Smooth use two u16 image buffers (64 MiB total at 4096²) and a reused width-sized sum buffer. Fractional disk processing reuses a bounded row deque instead of another image. These are additional to host output/current/previous masks, encoding, layers and history; they are not total-process memory figures. Constant-result cases skip image scratch. None of this work runs on pointer MOVE.

### System clipboard verification

Default clipboard tests use pure codecs and an in-memory external-resource adapter. Real Win32 writes are confined to the ignored, station-guarded fixture launched by `scripts/smoke-system-clipboard.py --test-binary <library-test-exe>`; see [Clipboard](clipboard.md#safe-native-fixture). The localization smoke queries only format availability and never reads or replaces clipboard image data. If Windows denies private window-station creation, report the native roundtrip as unverified instead of running destructive tests against WinSta0.
