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

## Exact geometry costs

Run `cargo test --release --locked -p micopi-painting-tools exact_geometry_cost -- --ignored --nocapture` with the portable target/environment arguments above when needed. On 2026-11-05 this development environment measured a clockwise quarter turn at 7.2 ms for 1024×1024 and 178.1 ms for 4096×4096. These are single-run kernel observations with preallocated source/output buffers and no native scratch allocation. They exclude host allocation, selection compression, structural history, journal commit, composition and GUI presentation; they are not pen-latency or cross-machine guarantees. No geometry callback runs on pen MOVE.

## Image resampling costs

Run `cargo test --release --locked -p micopi-painting-tools resample_cost -- --ignored --nocapture` with the portable environment/target arguments as needed. On 2026-11-05 this environment measured a constant 4096×4096 source: nearest to 2048×2048 10.2 ms, bilinear to 2048×2048 96.8 ms, bilinear to 4095×4095 417.2 ms. These single-run observations include axis planning and output-value assertions with preallocated image buffers; they exclude host allocation, mask encoding, history, journal, composition and GUI presentation. They are not pen-latency or cross-machine guarantees. Reference tests cover non-square, single-axis, up/downsample, strided storage, transparent edges, exact identity and invalid/aliased buffers. Session tests cover layer/mask identity, budgets, locks, inert nodes, project/replay, failed later outputs and atomic/partial-journal rollback.

## Area reduction costs

Run `cargo test --release --locked -p micopi-painting-tools area_cost -- --ignored --nocapture` with the portable environment/target arguments as needed. On 2026-11-05 this environment measured a deterministic opaque random-color 4096×4096 source: area to 1024×1024 77.2 ms, to 2731×2048 213.5 ms, to 4096×2048 261.3 ms and to 4095×4095 494.3 ms. These single-run kernel observations include plan/row allocations and output alpha assertions, with preallocated image buffers. They exclude host allocation, mask compression, history, journal, composition and GUI presentation, and are not directly interleaved comparisons against the earlier constant-image bilinear timings.

Independent direct 2-D integration checks noninteger, mixed-axis and strided images. Exact fixtures cover thin-line energy, transparent hidden color, fractional masks, full axis-plan weight sums and unchanged source/padding. Host tests cover optional-prefix/truncated-tail capability rejection, nearest/bilinear fallback availability, all-layer/mask outcomes, atomic/journal rollback and workflow execution. A texture regression exercises direct GUI Undo after shrinking, an equal-area quarter turn and subsequent normal edits. Localization smoke shrinks with area sampling and restores via GUI Undo before checking the unchanged final PNG. Default sampling is unchanged; an area box is not an ideal low-pass or Photoshop bicubic compatibility claim.

The development compatibility run also loaded the preceding prefix-only resampling DLL in the new host: nearest succeeded and area was explicitly rejected. The preceding host loaded the extended DLL and retained nearest behavior. Default table fixtures keep the base/truncated/full capability checks reproducible without tracking built libraries.

## Automatic trim costs

Run `cargo test --release --locked -p micopi-painting-tools trim_cost -- --ignored --nocapture` with the portable environment/target arguments as needed. On 2026-11-05 this host measured a transparent 4096×4096 image with a centered opaque 3072×3072 rectangle: alpha mode 18.9 ms, top-left color 23.2 ms, bottom-right color 23.7 ms. These single-run kernel observations include descriptor/assertion work, but exclude composition, mask/frame allocation, history, journal and GUI presentation. Native analysis allocates nothing; GUI bounds are cached until parameters change rather than rescanned every frame. These figures do not measure physical pen latency.

An independent row/column stripping reference checks every side combination, color/alpha modes, strided inputs and thresholds. Exact fixtures cover corner selection, hidden RGB, invalid/aliased storage and untouched outputs on errors. Session tests cover visible merged versus hidden/locked sources, fractional masks, no-ops, provisional read-only contacts, free graphs, inert nodes, projects, outcome replay and atomic/partial-journal rollback. Native loader fixtures reject missing/truncated tables and malformed output bounds. UI checks cover preview, cancel, draft sides, stale revisions, Apply, Undo and texture dimensions.

The trim compatibility run used isolated matching manifest sets: the preceding painting DLL loaded in the new host and retained area sampling while explicitly rejecting absent trim; the preceding host loaded the extended DLL and retained nearest sampling. The discovery response also reported bounds as read-only, trim as document-atomic and the typed provisional-drawing field. A GUI area-rectangle regression checks configured right spacing and an unobscured image center.

## Document-setup checks

Run `python scripts/smoke-document-setup.py --cli <built-cli> --gui <built-desktop>` after a host rebuild. It compares JSONL/MCP/desktop PNG bytes, verifies metadata-only resolution undo/redo, atomic rollback, schema-derived state/action nodes, saved projects and outcome replay, and captures New Canvas in four languages. Use `--skip-gui` for the transport-only subset. Captures stay in `target/test-output/document-setup-smoke/`; promote only reviewed application captures. Library regressions also inject a partial append into a staged new-document journal and confirm the old recording, pixels and undo history survive. No physical tablet claim follows from its synthetic pen fixture.

## PNG-density checks

Run `python scripts/smoke-png-density.py --cli <built-cli> --gui <built-desktop>` for signature-based input, exact asymmetric/unknown/zero density, cross-transport PNG parity, project and plugin-independent recording restoration, malformed late-chunk rejection and four-language resolution forms. `scripts/smoke-document-setup.py` now compares decoded pixels across differing resolutions while checking the expected density chunks. Run `png_io_cost` as an ignored Release library test to measure its deterministic cold encode/scan/decode workload. See [PNG Density](png-density.md) for supported limits and costs.

## Export-sizing checks

Run `python scripts/smoke-export.py --cli <built-cli> --gui <built-desktop>` for independent native sampling references, PNG/density parity across JSONL/MCP/desktop, unchanged source state, actual file digests, guarded project destinations and outcome playback without recreating files. UI captures cover English, Chinese, Japanese and Korean. The ignored Release `export_preview_cost` test times a cached 2048-square source through dispatch, native sampling, PNG and Base64/JSON, with source preparation, recording, file I/O and reference checking outside the timers. Library tests also inject recording failure after publication and verify the explicit file receipt. See [PNG Export Sizing](export-sizing.md).

## Physical creation checks

Run `python scripts/smoke-physical-creation.py --cli <built-cli> --gui <built-desktop>` for A4 planning without raster allocation, equivalent millimeter/inch creation, explicit-default versus legacy density, project persistence, transport parity, verified replay and four-language New Canvas cancellation captures. Use `--skip-gui` for transport-only checks. Library regressions cover rounding/bounds, active contacts, inert schema-derived workflow nodes and failed candidate-journal publication: restore the old document and retain its verified recording prefix plus a rejected-action audit. Desktop tests cover unit switching, PPI behavior, stale drafts and narrow CJK layouts. These are synthetic input checks, not tablet-driver hardware validation.

## Physical Image Size checks

Run `python scripts/smoke-physical-resample.py --cli <built-cli> --gui <built-desktop>` for exact-axis physical/pixel equivalence, unknown-density handling, source metadata retention, native output parity across transports, project/undo/replay checks and four-language Image Size cancellation captures. Use `--skip-gui` for transport-only checks. Library tests cover read-only plans during contacts, rejected active mutations, all-layer/mask estimates and budgets, static preflight versus evolving pipeline context, native/encoding/journal rollback and inert workflow editing. Desktop regressions cover unit switching, external density/revision changes and narrow CJK footers. No DLL ABI or pen-MOVE path change is required.

## Color blend checks

Build and stage the matching compositor before running `python scripts/smoke-blend-modes.py --cli <built-cli> --gui <built-desktop>`. Its 186-action fixture compares all 26 layer/free-graph/group modes across JSONL/MCP/GUI, schema/workflow availability, batch Undo, disabled filters, projects and DLL-independent replay, with four-language captures. `--legacy-compositor <previous-dll>` additionally checks rejection before replacing hidden-mode properties or opening extended-mode projects. Native Release tests retain exact old-mode hashes, independent full-byte alpha references, all 65,536 Soft Light/Hard Mix channel pairs and explicit rational half-byte boundaries. Host tests cover unsupported/missing/partial capabilities, active pressure contacts, baking, undo and render/atomic/journal rollback. See [Color Blend Modes](blend-modes.md).

On 2026-11-06, after this task's build and smoke processes had finished, five interleaved old/new DLL pairs per original mode measured the following medians on two prepared 1024-square full-range straight-ARGB sources with four instructions. Every pair compared the complete output bytes. Timers include one ctypes callback, native validation and composition; allocation, source generation, comparison/hash work, Session/history/journal, encoding, GUI and tablet input are excluded. These workload/environment-specific observations do not establish pen latency or a general speedup.

| Legacy mode | Previous DLL (ms) | Current DLL (ms) |
| --- | ---: | ---: |
| Normal | 30.10 | 30.06 |
| Multiply | 33.08 | 32.18 |
| Screen | 31.68 | 32.75 |
| Overlay | 35.39 | 35.59 |
| Darken | 30.21 | 30.09 |
| Lighten | 31.24 | 31.43 |
| Linear Dodge | 28.73 | 29.84 |
| Difference | 35.96 | 36.58 |

Keep the extended kernel out of line: otherwise compiler inlining can make legacy integer calls save floating registers. The final pairwise median differences ranged from -2.7% to +3.8%, with identical pixels. The ignored Release `extended_blend_cost` test observed one cold 1024-square native call per appended mode at 31.4–87.8 ms, including 87.8 ms Soft Light and 83.7 ms Hue. That separate fixture uses prepared opaque sources and checks sampled reference pixels outside its timers; it excludes allocations, Session, recording, encoding and I/O. Dirty-region callbacks, a 64-word per-pixel scratch array and borrowed source descriptors remain in use.

The tested desktop/CLI executables are 9,944,064/3,887,616 bytes, 60.5/30 KiB above the preceding group-baking preview. The matching compositor is 273,408 bytes, an increase of 6.5 KiB, with no added dependency. The previous host with its own unchanged manifests also produces identical eight-mode PNGs using the new DLL; own/cross-host outcome archives verify without the compositor. Old 16/24-byte prefixes and incomplete tails retain their legacy contract; a complete mode bitmap must retain all baseline bits.

## Layer organization checks

Run `python scripts/smoke-group-merge.py --cli <built-cli> --gui <built-desktop>` for subtree/visible-group baking, fresh raster output bindings, hidden locked content, single-step undo, exact transport PNGs, projects and DLL-independent replay. It opens/cancels the real Merge Group schema form in all four languages against a fresh nonempty target. `group_merge_tests` checks independent all-mode/alpha/opacity references, retained filters and buffer addresses, active Brush/Hand contacts, render/journal/atomic failure and final-group removal. Desktop checks exercise the real group menu, Layers pending dispatch and grouping/visible-merge shortcuts; recording boundaries retain pressure samples and leave native contacts active on rejection. The ignored Release `group_bake_cost` test measures the two complete Session commands on a prepared 1024-square, four-raster, two-group fixture, with source/reference preparation outside the timers.

On 2026-11-06, with this task's build and smoke processes finished, one Release observation measured Group Merge at 158.7 ms and grouped Merge Visible at 194.3 ms. Timers include shared validation, staged native baking/final composition, structural history, replay encoding, temporary-journal append and result JSON. Reference preparation, PNG encoding and transport are excluded. Both commands retained exact independent full-frame pixels and Undo source restoration. These are cold-command observations, not pen latency, peak memory, durable-disk latency or cross-machine guarantees. Each engine bake allocates one required output raster and borrows raw source descriptors; removed buffers move into undo ownership.

The group-baking desktop/CLI executables are 9,882,112/3,856,896 bytes, respectively, 34/32 KiB above the preceding group preview. Native DLLs and dependencies are unchanged. The preceding group host also reads the new baked project and action-labeled recording archive with identical project/replay PNG bytes and valid hashes; outcome replay does not require a handler for the new command name.

For isolated groups, run `python scripts/smoke-groups.py --cli <built-cli> --gui <built-desktop>` after building/staging the matching compositor DLL. It compares exact alpha/transport outcomes, subtree operations, source-filter behavior, project/replay and four-language hierarchy captures. `cargo test -p micopi-compositing --locked` exercises independent blend references, once-only nested opacity, hidden/empty/16-source groups, dirty stride/padding and prewrite rejection. SDK and host tests cover the optional compositor tail, legacy flat plans, missing capabilities, tree/version/depth/budget validation, contact/cache behavior, buffers, workflow and rollback. See [Layer Groups](layer-groups.md) for supported boundaries.

The 2026-11-06 group build was also checked with the preceding portable host and prefix-only compositor: all three compatible host/DLL combinations produced identical four-kind filtered/blended PNGs and valid replay. The prefix-only DLL rejected group creation, a transparent-node graph and a successfully saved/reopened grouped project with the expected capability errors, preserving the current document.

After build and smoke processes had finished, the existing Release `dll_brush_benchmark` measured 234.4 ms unrestricted DLL, 240.8 ms full pixel-selection DLL and 68.3 ms raster-only reference per 5,120 samples/80 strokes (five-run medians, 512×384), with exact full-pixel parity. This active-layer workload includes Session/input/raster/history/recording, excludes GUI and physical tablet latency, and does not isolate nested-composition overhead or establish a speedup. Group rendering retains one dirty-region callback, a 64-word scratch array and cached inherited visibility; no per-MOVE JSON or hierarchy walk is added.

The tested group desktop/CLI executables are 9,847,296/3,824,128 bytes, respectively, 536.5/290 KiB above the preceding multi-selection preview. The matching compositor DLL remains 266,752 bytes. No dependency is added; these are executable sizes, not total portable-package or memory costs.

For stable-ID multi-selection and explicit batch operations, run `python scripts/smoke-layer-batch.py --cli <built-cli> --gui <built-desktop>`. It checks selected public state, grouped property/structural/pixel edits, undo/redo, transport parity, projects and plugin-independent replay, and captures the Layers controls in all four languages. Library regressions also cover all-target preflight, locked sources, capacity, masks, dynamic bindings, malformed persistence and partial journal failure. Pure selection has no pixel undo entry; an explicit batch has one undo group and one revision/outcome. These checks use synthetic input, not physical tablet hardware.

After the multi-selection/history-group change, the final 2026-11-06 Release DLL/reference benchmark measured 250.2 ms unrestricted DLL, 256.6 ms full pixel-selection DLL and 69.8 ms raster-only reference per 5,120 samples/80 strokes (five-run medians, 512×384), with exact full-pixel parity. Build jobs and smoke GUIs had finished before this run. This existing active-layer workload includes Session/input/raster/history/recording, excludes GUI and physical tablet latency, and does not isolate grouping overhead or prove a speedup. The tested desktop/CLI executables are 9,297,920/3,527,168 bytes, respectively, 109/87.5 KiB above the preceding layer-organization preview with no added dependency or DLL change.

Run `python scripts/smoke-layer-organization.py --cli <built-cli> --gui <built-desktop>` for lock/label/query parity, copies, project persistence, public workflow state and plugin-independent replay, with four-language Layers captures. `layer_organization` library tests cover filtered Unicode names, combined undo, active pressure continuity, write-failure rollback, protected edits and exact double-locked document geometry. Native/registry checks validate legacy access defaults, ownership/conflict rejection, nested target changes and denied mutable loans from read tools. Desktop tests cover filter purity/cache invalidation, pending names, property history and narrow CJK/long-name layout. Use the ignored Release `dll_brush_benchmark` for deterministic DLL/reference pixel parity and throughput; this does not establish tablet latency or driver compatibility.

On 2026-11-05, after layer-access enforcement, the same Release headless benchmark measured 268.5 ms unrestricted DLL, 266.5 ms full-selection DLL and 82.3 ms raster-only reference per 5,120 samples/80 strokes (five-run medians, 512×384). Full pixels matched the independent reference. DLL timings include Session/input/raster/history and outcome recording; GUI presentation and physical tablet latency are excluded. These workload/environment-specific observations do not isolate lock-check overhead or establish a speedup over earlier runs.
