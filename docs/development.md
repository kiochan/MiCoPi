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

On the maintainer's interactive desktop, keep one build/test worker active at
a time, below normal priority and restricted to at most two logical CPUs. Use
Cargo `-j 1` and test `--test-threads=1`. Keep existing profiles/compiler flags
to reuse caches. Source review/editing may run concurrently; large-image stress
and benchmark runs remain separate, necessary checks rather than background
work. CI is independent of these local responsiveness limits.

Unit tests cover document/history, codec, schemas, actions, public state, pipelines, workflow graphs, settings, shortcuts, pen routing, docking, Windows ABI and offscreen presentation. Build plugin DLLs first. Explicitly ignored tests cover the independent DLL fixture and optional benchmarks; build.ps1 -Test runs the DLL lifecycle fixture separately.

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

Builds stage dependency notices through the offline license collector; its source guide is `scripts/licenses/README.md` in the repository. The collector checks the exact Cargo registry archives against `Cargo.lock`, retains nested notices and verifies pinned upstream supplements for crates whose archives omit complete license texts. The four embedded font notices are required separately from the egui source terms. Every workspace package carries the project MIT text. A checksummed `licenses/LICENSES.json` records coverage and provenance; the packager rejects a stale lock digest or missing, modified or extra license files before writing a ZIP. It also checks relative Markdown links against the actual archive entries, so a source-only guide cannot become a broken portable link. Run `python scripts/test-collect-licenses.py` and `python scripts/test-package.py` after changing this tooling or its pins. Dependency upgrades need a fresh notice review rather than inheriting an older package's coverage.

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

## Native A8 prerequisite checks

Build and stage the matching painting/compositor DLLs before host tests. Run `cargo test --lib --locked -p micopi-plugin-api -p micopi-painting-tools -p micopi-compositing`, `python scripts/check-native-abi.py` and `python scripts/smoke-native-masks.py`. The header check compiles C11/C++17 fixtures with the existing portable compiler or explicit `--cc`; it creates no binaries. The actual-DLL check uses synthetic scoped A8/selection loans, independent coverage arithmetic, dirty stride/guard fixtures and legacy prefix parity. Optional `--old-painting` and `--old-compositor` paths add preceding-DLL comparisons. Native crate tests and the conformance script also run in CI.

These checks cover primitives, not application mask history or physical tablet compatibility. The document/event foundation is checked separately below; remaining editing integration is specified in [Raster Masks Design](raster-masks-design.md). Native failures on UP/CANCEL, including the final stabilizer tail, must propagate to the caller; malformed A8 metadata and aliases reject before writes. Host tests retain old-prefix negotiation, actual load/dispatch and all existing document/replay behavior. Run the ignored DLL lifecycle fixture against the built incompatible DLL as in the standard build script.

After builds and smoke processes finish, the ignored Release `a8_raster_cost` and `masked_region_cost` native tests measure prepared typed-painting and coverage-composition workloads. Keep native kernel/loan costs separate from Session/history/journal, encoded output, GUI and hardware input latency. Their checks and fixture preparation remain outside timers; no CI timing threshold is applied.

On 2026-11-06, Rust 1.90 GNU Release in this Windows x64 environment (12 logical processors; reported AMD64 Family 23 Model 113) measured five interleaved native test-binary medians of 25.97 ms A8 and 35.32 ms opaque grayscale RGBA for 5,120 prepared pen samples/80 strokes on 512×384, with pressure dynamics, stabilization and fractional selection. Both used 33,840 scoped loans and matched every output pixel. These different storage/sink workloads do not establish an application speedup. Timers include configure, samples and native raster/loans; allocation, resets, reference checks, Session/history/journal, DLL loading, encoding, I/O, GUI and physical pen input are excluded.

One native test-binary observation of a five-instruction 1024-square MaskedMix with two full-range ARGB sources and a full-range A8 plane measured 19.36/31.71/29.63 ms at opacity bytes 0/128/255. Native validation and composition are included; source/output allocation, fixture generation and full independent reference comparisons are outside the timers, as are Session/history/journal, encoding and presentation. The kernel uses fixed per-pixel scratch and no image intermediates.

Actual-DLL legacy rendering retained complete bytes for all 26 modes across five interleaved old/new 1024-square pairs per mode. Initial median changes ranged from −9.2% to +13.4%, including a +13.4% Linear Burn result. An 11-pair follow-up interleaved Normal/Linear Burn/Soft Light, reversed DLL loading, used ABBA callback order and swapped output allocations each repeat: medians were 28.08/28.75, 32.56/30.63 and 82.08/78.67 ms respectively (old/new); Normal's median paired ratio was +0.14%. The initial slowdown did not reproduce consistently. Broad sample variation and changing directions do not prove unchanged throughput or a general speedup. These timers include ctypes/native validation/composition and exclude allocation, preparation, comparisons/hashing, Session/history, GUI and hardware input.

The four old/new host and painting/compositor combinations also ran the same 57-action fixture. Original RGBA planes including hidden RGB, composition, PNGs, fractional selection, groups, filters, selected-only merge, Undo/Redo and reopened projects matched exactly. One shared generated save/open path kept file-context metadata equal, allowing identical state/surface hashes across runs. Old/new core/replay-only readers verified the same archive's head/state/surface hashes without algorithm DLLs. Existing preview files and manifest sets remained unchanged.

The native-prerequisite desktop/CLI executables were 10,089,472/3,963,392 bytes (1/1.5 KiB above the selected-merge preview). Painting was 356,864 bytes (+1 KiB) and compositing 278,016 bytes (+4 KiB); no dependency was added. These are individual binary sizes rather than package or peak-memory costs. That prerequisite build had no document mask target or public coverage nodes and did not alter project or recording formats.

## Raster-mask document foundation checks

The measurements and original delivery results in this subsection describe the
historical foundation stage, before public A8 editing. Current public editing
checks and native effects measurements are documented in the next subsection.

Run `python scripts/smoke-raster-masks.py --cli <built-cli> --gui <built-desktop>` with matching native DLLs. Its independent MICOPI04 writer and integer references check raw A8 ownership, filtered layer coverage, isolated and Pass Through group gates, project/save signature protection, pressure/stabilizer RGBA contact continuity and explicit rejection of pending mask-unaware edits. JSONL, MCP and GUI use the shared Session contracts. The script also checks byte-outcome compaction, DLL-free replay, admission failures without the mask renderer or with out-of-range density/feather metadata, legacy projects and recording hashes, and four-language Actual Composition captures. Use `--skip-gui` for transport-only CI; pass `--baseline-cli <preceding-host>` for old-reader rejection. An absent preceding executable is explicitly skipped rather than downloaded.

The 97 by 65 fixture owns three planes: 18,915 logical A8 bytes, represented by two 6,305-byte dense planes and one uniform byte. Its sample payload is 12,611 bytes and the MICOPI04 suffix excluding recording is 12,662 bytes, including 51 bytes of count/entry/recording-length framing. These are deterministic file-storage costs, not peak memory, application throughput or pen latency. Cold-save tests separately verify complete-file equivalence between constant dense and uniform storage while retaining the live pointer, history and dirty state. Raw undo/lease/rollback, malformed storage, byte references/XOR/markers, legacy hashes, graph domains/budgets, once-only opacity and target/preflight boundaries have focused engine/application/UI regressions.

One final captured run produced a 200,970-byte, 17-event MCP archive and a 200,537-byte embedded archive. All twelve A8 tile changes belonged to genesis; later RGBA painting, history, property and raster-count changes did not repeat unchanged mask planes. Archive bytes vary with recorded paths/timestamps and include the other document outcomes; this is not an isolated mask compression ratio or an A8 painting throughput measurement.

At the foundation stage, public mask painting was still pending, and its engine
lease/native sink tests did not prove desktop A8 routing. The public editing
subset is checked separately below; complete structural/geometry integration
remains pending. Neither stage's synthetic contact checks establish physical
tablet compatibility. See [Raster-mask storage](raster-mask-storage.md) for
storage contracts and the retained temporary boundaries.

The final 2026-11-06 GNU Release foundation desktop/CLI executables were 10,266,624/4,146,688 bytes, 173/179 KiB above the native-prerequisite build. Matching painting/compositor DLLs remained 356,864/278,016 bytes with identical hashes, and no dependency was added. These are historical executable sizes, not current editing-build sizes, portable-package size, peak memory, input latency or a throughput measurement.

The mask-specific JSONL/MCP/GUI check passed, including four reviewed language captures. Separate runtime, workflow, linked-graph, replay, group, selected-merge and Pass Through GUI checks passed. Subsequent capture stabilization adds request/viewport correlation and retained callback acknowledgement; use the capture-aware checks below when validating current builds rather than treating file existence as completion.

## Mask-aware canvas geometry checks

Build/stage the painting DLL with the independent A8 sampler, then run
`python scripts/smoke-mask-geometry.py --cli <built-cli> --gui <built-desktop>`.
Use `--skip-gui` for JSONL/MCP only. The optional `--legacy-painting` path selects
the preceding A8 painting DLL without the sampler table; absence is explicitly
reported. Small independently encoded MICOPI04 fixtures compare all nine expand
and shrink anchors, crop, five orthogonal transforms, three sampling methods
and Trim against exact raw RGBA/A8/selection references. The check verifies
one-step undo, retained IDs/properties, exact density, independent composed
pixels, transport parity, project roundtrips and DLL-free outcome playback.
The full run also captures real framing/Image Size/Trim dialogs and narrow/wide
native Layers panels in English, Chinese, Japanese and Korean.

Engine `mask_geometry` tests cover complete bundle admission, semantic uniform
equality, same-size raw-only changes and cache/tile invalidation. Session
`mask_canvas` regressions additionally exercise Brush/Hand contact continuity,
logical budgets without large allocations, native sampling failure, renderer
and partial-journal failure, atomic rollback, plan bindings and inert workflow
editing/dry runs. UI tests check owned and unrelated releases, stale drafts,
menu/shortcut adapters, configurable sizing and visible short-window footers.
Journal fault injection is tested in Rust, not through a public transport hook.
See [Mask Canvas Geometry](mask-canvas-geometry.md) for fixed-pixel feather
rounding and the remaining owner-editing boundaries. These synthetic checks
do not establish physical tablet compatibility.

The broad library runs passed 746 desktop and 557 headless tests, each with
9 ignored. A preceding single-process GNU libtest attempt completed 199 tests
before aborting with
`out of TLS indexes`; its cause remains unproven. Both complete binaries were
therefore enumerated and checked in fresh batches of 16 exact test names, one
process/test thread at a time, with all 755 desktop and 566 headless names
accounted for. This is test isolation, not a demonstrated fix for a
shipping-runtime defect. Workers stayed
below normal priority on at most two logical CPUs; no large-image benchmark was
run for this change.

A subsequent desktop-only cleanup reconciles local pen/hand flags after a
successful legacy owner turn without ending a contact on protected rejection.
Its four command tests passed, including both tools, directions, static/search
dispatch and masked/position-locked targets; the 15 canvas-dialog regressions
and two owned/external contact regressions also passed after that change. The
headless execution core is unchanged by this localized presentation correction.

The final Release geometry smoke passed 31 independently referenced cases:
931 JSONL commands, 1,006 MCP commands including 35 remaining owner-edit guards,
and matching attached-GUI results. It retained all five hidden/disabled/unlinked
layer/group masks, selections, exact density, properties, targets and one-step
Undo/Redo, verified project roundtrips and restored every outcome archive without
DLLs. The preceding painting DLL retained identity requests and safely rejected
nonidentity raw-mask sampling. Twenty-four request-correlated root/native PNGs
totaled 11,245,177 bytes. Reviewed captures include all four Image Size languages,
Japanese framing, Korean Trim and narrow English Layers; one English Image Size
capture is promoted to the guide. The independent 7-by-5 fixture has 175 logical
raw-A8 bytes. Rust regressions, rather than a public fault-injection API, cover
native/renderer/partial-journal failures.

Public mask editing also passed its updated 387-command JSONL and 464-command MCP
transport subset, including 35 remaining guarded boundaries, old 40-byte effects
rejection and DLL-free replay. The storage regression passed legacy 01/02/03 and
old-reader rejection, contact continuity and separate raw outcomes. Its former
Image Size-plan rejection was replaced with idle/live read-only plan checks.
Runtime JSONL/MCP/GUI parity, streaming input and settings checks passed.
The workflow smoke passed cross-client pixel parity, inert dry runs/patching and
project roundtrips. Full localization switching/persistence and valid/invalid
host/plugin catalog reloads passed with unchanged PNG/workflow data.
The 24-command mask-geometry example ran with isolated output/profile paths,
produced an 88-by-112 PNG/project/archive and verified all 21 committed events
with no pending samples.

The final GNU Release desktop/CLI binaries are 10,579,968/4,357,632 bytes,
83/59 KiB above the public mask-editing build, including the earlier native-sampler
host prerequisite. The painting/compositor DLLs retain the prerequisite's
359,936/287,232-byte sizes and hashes. No Cargo dependency is added. Geometry
borrows raw sources and allocates bounded cold outputs; typed MOVE still uses
cached instances/loans and dirty regions. These size/scope observations do not
measure cold-operation peak memory or physical pen latency.

## Native raw-A8 sampling prerequisite

The independent `micopi_a8_sampler_v1` table adds borrowed dense/uniform reframe,
orthogonal geometry and nearest/bilinear/area sampling. Build/stage the painting
DLL first, then run the native `a8_sampler` and host `a8_sampler` test filters;
the x64 C11/C++17 fixture includes the new requests, table offsets and signatures.
See [Native Plugins](native-plugins.md#optional-raw-a8-sampler) for the contract.
This native prerequisite does not remove the document editing guards by itself.

The isolated Release painting run passed 70 tests with 11 ignored; SDK passed
4 tests. Five host tests passed, including actual DLL borrowed strided/uniform
calls, prewrite rejection, strict table negotiation and the preserved preceding
painting DLL's absent sampler entry. The latter baseline already supports A8
painting; it is not the pre-A8 baseline expected by `smoke-native-masks.py`'s
optional `--old-painting` comparison. No larger timing benchmark was run.

The matching painting DLL is 359,936 bytes, 3 KiB above the 356,864-byte mask
editing build. Its SHA-256 is
`a7d3a6199ab307c401209c8992f26115367c3a11180156ff88c4c5624b657408`.
No Cargo dependency or legacy table size changes. These sizes and native tests
do not measure complete document operations, pen latency or hardware support.
Local native build/test and targeted host workers ran sequentially at below
normal priority on at most two logical CPUs, with one Cargo job/test thread.

## Public raster-mask editing checks

Build/stage the matching painting and compositor DLLs, then run
`python scripts/smoke-mask-editing.py --cli <built-cli> --gui <built-desktop>`.
Use `--skip-gui` for the JSONL/MCP/outcome subset. The optional
`--legacy-compositor <preserved-masks-only-dll>` checks that a preceding 40-byte
table rejects nondefault effects before mutation, including hidden/disabled
masks; an absent baseline is explicitly skipped. Output and captures stay under
`target/test-output/mask-editing-smoke/`. See [Raster Mask Editing](raster-mask-editing.md)
for all eleven action schemas, stable-ID/target semantics and the remaining
guarded owner geometry, structural, merge and transfer paths. The full raster-mask
roadmap item remains unchecked.

The current compositor effects tail is 72 bytes on x64, with capability `0x8`
and create/destroy/region/reserve callbacks after the preserved 40-byte Masks
prefix. Host tests reject truncated/missing/incorrect tails and retain old
prefix behavior. Native tests compare independent feather/density and raw-invert
references, strided/edge/ROI output, malformed descriptors, aliases and prewrite
failure. The cache-local implementation uses a reusable `u32` column-sum vector,
two A8 scratch buffers and exact three-box support `R` per axis. Logical native
scratch is bounded to 32 MiB plus 16 KiB; raw and derived host planes each have
separate 64 MiB logical budgets.

Uniform-mask DOWN reserves native capacity without zeroing/feathering a full
frame, then materializes constant raw/effective storage once. Typed MOVE uses
cached A8 leases, tool/effects instances, fractional selection and dirty regions;
no per-sample JSON, discovery, file I/O or whole-plane storage clone is added.
The demanded output and its halo can still span the frame for a large brush or
radius. Tests cover lease epochs, contact continuity, single-stroke Undo,
allocator/revision/raw-state rollback, guaranteed workflow result binding,
pixel-versus-mask targets and read-only snapshots. Apply tests retain filters
and layer properties and explicitly check the live-alpha 69 versus raw-bake 68
rounding case. Group-mask Apply remains unsupported.

Rollback retains an active checkpoint's cached lease, while a private monotonic
nonce prevents abandoned contacts from becoming valid again on the next DOWN.
Two regressions failed against the previous document-local epoch increment and
passed after this correction; the focused headless mask run passed 23 tests.
The nonce is not serialized, exposed as public state or incremented on MOVE.
Accepted pointer samples retain the existing per-sample Session revisions; one
completed contact uses one Undo entry and bounded provisional outcome segments.

The completed library runs passed 710 desktop and 531 headless tests, each with
9 ignored tests. The native effects run passed 33 compositor tests with 5 ignored,
and the SDK run passed 3 tests; the C11/C++17 ABI fixture check also passed.
These counts describe those recorded runs, not a physical-device claim or a
claim that every guarded mask feature is complete.

A Release public-editing smoke passed 387 JSONL commands and 475 MCP commands,
including 46 guarded geometry/boundary cases, locks, contacts and rollback. It
compared independent raw-A8/effective/composite references, rejected effects
through the previous 40-byte compositor, verified DLL-free playback and passed
GUI parity plus 20 exact-size English/Chinese/Japanese/Korean root/native captures.
Those PNGs totaled 4,759,101 bytes. Its 19 by 13, two-mask fixture had 494 logical
raw A8 bytes, a 248-byte sample payload and a 286-byte MICOPI04 suffix excluding
the recording. The exported recording was 90,782 bytes with 63 events, 21 A8 tile
changes and 51 mask-version events; the embedded recording was 90,348 bytes.
These concrete file observations include other outcomes and recorded paths;
they are not an isolated compression ratio or peak-memory measurement.

The separate runnable mask example completed 18 commands with valid 15-event
replay; its PNG/project/archive outputs were 15,263/804,486/94,167 bytes. Runtime's
125-command JSONL/MCP/GUI check, workflow transport/GUI parity and four-language
switching/persistence checks also passed, preserving canvas and workflow data.
The replay transport check passed with a 1,166,711-byte recording. Foundation
transport checks retained MICOPI01/02/03 compatibility,
preceding-host MICOPI04 rejection and a verified 204,344-byte, 17-event archive.
Windows CI runs the public-editing transport subset after staging the matching
native DLLs. Physical tablet and driver compatibility remain untested.

### Scoped native effects measurements

The ignored Release tests `mask_effects_native_4096_cold_and_warm_cost` and
`mask_effects_interleaved_dll_cache_local_cost` measure prepared 4096-square
inputs at density 203, radii 8 and 256. Run them with the existing portable
environment/target arguments:

```powershell
cargo test --release --offline --locked -p micopi-compositing mask_effects_native_4096_cold_and_warm_cost -- --ignored --nocapture
cargo test --release --offline --locked -p micopi-compositing mask_effects_interleaved_dll_cache_local_cost -- --ignored --nocapture
```

The paired fixture requires two preserved
**effects-capable** 72-byte DLLs; set `MICOPI_MASK_BASELINE_DLL` and
`MICOPI_MASK_CANDIDATE_DLL` to their paths. A masks-only 40-byte stable DLL cannot
serve as this benchmark baseline. These ignored tests apply no CI timing threshold.

Recorded opaque-DLL pairs alternated callback order and compared all output bytes
and ROI padding. Fresh full-frame medians used three pairs; warm influence-ROI
medians used 32 pairs and reused output allocations after capacity-only reserve.
The baseline here is an earlier effects implementation, not the previously
distributed masks-only application. For 32 by 32 raw damage, the host-demanded
output expands to 48 by 48 at `R=8` or 544 by 544 at `R=256`; the native source
halo expands again to 64 by 64 or 1056 by 1056 respectively.

| Source | Radius | Full-frame baseline / candidate (ms) | Influence-ROI baseline / candidate (ms) |
| --- | ---: | ---: | ---: |
| Dense white | 8 | 1548.03 / 518.44 | 0.1191 / 0.1189 |
| Dense white | 256 | 1474.85 / 515.66 | 33.9955 / 32.9740 |
| Deterministic dense pattern | 8 | 1581.11 / 506.45 | 0.1157 / 0.1148 |
| Deterministic dense pattern | 256 | 1474.18 / 489.74 | 31.8461 / 30.7866 |

A separate direct-callback run used 128 warm, fixed 32 by 32 output requests.
Candidate dense-pattern medians were 0.0644 ms at radius 8 and 8.4979 ms at
radius 256; these fixed outputs are smaller than the host-expanded influence
regions above and must not be substituted for their cost. Fresh capacity-only
reserve observations ranged from 0.0249 to 0.0413 ms, with zero scratch lengths
and 33,570,816 reserved logical bytes. Uniform descriptors avoided feather
scratch entirely unless reserve was explicitly requested. Pointers/capacities
remained reusable and independent center/edge references matched.

Region timers include the C callback, validation and kernel, with native scratch
allocation included for fresh full-frame work. Reserve timers include capacity
allocation but no pixel processing. DLL loading/instance creation, prepared
source/output allocation, host raw/effective caches, history/replay, composition,
encoding, I/O, reference comparisons, GUI and physical pen input are outside
these timers. The measurements are local observations, not application speedup,
frame-rate, allocation-residency or tablet-latency guarantees.

The previously delivered `MiCoPi-0.21.0-1b8fc9d-Windows-x64.zip` packages the
validated `eff8e54` runtime with later license/document-link fixes. It does not
contain this new editing subset or its effects DLL. Its 443 entries, 148-package
license manifest, 66 Markdown files and 43 examples were checked independently;
do not present the new source/benchmark results as validation of different
binaries, or imply that the earlier stable package exposes these new controls.

The final editing desktop/CLI sizes are 10,494,976/4,297,216 bytes,
199/122.5 KiB above the `eff8e54` capture-stabilized executables. Compositing is
287,232 bytes, 9 KiB above its preceding 278,016-byte DLL; painting remains
356,864 bytes. No Cargo dependency is added. These individual file sizes are
separate from package size, peak memory and latency. The final source-matched
GNU Release build completed after the private lease-nonce correction.

The final Release mask smoke repeated JSONL/MCP/GUI parity after that correction;
all twenty captured PNGs were byte-identical to the reviewed candidate captures.
The portable preflight contains 446 files and the unchanged 148-package license
manifest. Its extracted CLI and desktop each ran the 18-command example with
default plugin discovery and a Windows-system-only PATH, verified both masks and
the journal, and closed the owned GUI normally. Packaging uses the normalized
Git snapshot: exact lock/policy hashes can legitimately differ from a CRLF
working copy, and the strict license verifier is not bypassed. No DLL, executable,
personal settings or generated output is added to source history.

## Workspace and native capture checks

The capture contracts remain current. The recorded counts, executable sizes and
regression results in this subsection describe the earlier `eff8e54`
capture-stabilization delivery; they are not fresh results for the public mask
editing candidate documented above.

Run `python scripts/smoke-captures.py --cli <built-cli> --gui <built-desktop>` for concurrent root/native requests, multiple paths in one viewport, narrow/wide resizing, hidden/docked targets and file failures. It checks request-specific terminal records, independent PNG CRCs/RGBA and exact owned-process client pixels, plus unchanged artwork/composition and capture-only revisions. A successful final scenario saves nine captures and reports four explicit failures. Use `python scripts/smoke-localization.py --cli <built-cli> --gui <built-desktop>` for the full English/Chinese/Japanese/Korean form/settings/hot-reload workload with correlated capture checks. One complete run validates 106 PNGs: 90 root frames at 1700 by 1150, eight narrow floating frames at 350 by 375 and eight wide frames at 1250 by 188 in the tested 1.25-scale desktop. Those dimensions describe that environment, not fixed application sizes.

The capture helper waits for actual native readiness and gives each placement, readiness and capture stage its own existing deadline; terminal failures and wrong dimensions fail directly. Its DPI-aware `GetClientRect` checks target only the isolated GUI process. Root discovery follows the workbench title contract and excludes winit's zero-size event-target window. Capture-only revision tests first wait for stable geometry and the existing 750 ms quiet-period `workspace.layout.apply`, then retain strict admission/completion invariance. This separates an independent deferred layout command from screenshot side effects without hiding a failed capture.

The encoder starts on demand and has two reserved/queued image slots plus one active job, with a 16 Mi-pixel per-image bound. It uses existing image/PNG dependencies, Fast compression and atomic publication. The pointer/DLL path is unchanged, and UI callbacks perform framebuffer readback without PNG conversion or file writes. Counts and bounds are not a process-wide memory measurement, an input-latency guarantee or physical tablet validation. See [Capture Requests](capture-requests.md) for deadlines, external effects, diagnostic schemas and replay boundaries.

The capture-stabilization GNU Release desktop/CLI executables were 10,291,200/4,171,776 bytes, increases of 24/24.5 KiB over the mask document foundation. Painting/compositor DLLs retained their exact hashes and 356,864/278,016-byte sizes; Cargo dependencies were unchanged. Full library suites passed 669 desktop and 497 headless tests, with 9 ignored in each; the final capture subset passed 26 desktop and 6 headless checks. Focused capture and full localization checks each passed twice across that candidate/final pair, including independent client-size checks and reviewed captures. Runtime, workflow, replay, raster-mask and native-window checks also completed. One runtime attempt rejected an Eraser stroke against an alpha-locked layer; the unchanged 125-command diagnostic rerun passed, and its first tool-state cause could not be recovered. The runtime helper now preserves the failing command, state and events if that condition recurs; it is not reported as a fixed drawing bug.

## Selected-layer merge checks

Run `python scripts/smoke-selected-merge.py --cli <built-cli> --gui <built-desktop>` for explicit noncontiguous and cross-parent targets, independent partial-alpha references, shared JSONL/MCP/GUI results, typed workflow output bindings, one-step history, projects and DLL-independent replay. The optional review is opened and cancelled in four languages without changing the layers, graph or PNG. Use `--skip-gui` for the transport-only subset. See [Selected-only Merge](selected-merge.md) for placement, hidden consumption and the deliberate outside-backdrop boundary.

Library regressions cover all 26 blend modes, retained raw buffer/filter identities, local/intermediate/common-parent visibility, protected targets, source/projection/final budgets, active Brush/Hand continuity and atomic/render/journal rollback. Desktop regressions exercise actual menu/shortcut dispatch, captured IDs, document/revision guards, refresh/apply/Undo and strict footer clipping under narrow/short English and CJK metrics. Existing explicit Merge Down bindings remain supported. No native ABI, DLL or recording format is changed.

Run the ignored Release `selected_merge_cost` library test after builds and smoke GUIs finish. Its prepared 1024-square A/U/B/V fixture selects A and B; A is opaque, while U/B/V use full-range ARGB. Five read-only plan samples and one complete mutating `Session::apply` are timed separately. Source/reference preparation, action construction, recording-boundary flush, PNG encoding, transport, GUI and physical pen input are excluded. Full baked pixels, survivor pointers/attributes, one revision/event, Undo restoration and verified replay are checked outside the timers. This diagnostic has no CI timing threshold or peak-memory claim.

On 2026-11-06, after this task's builds and smoke GUIs finished, Rust 1.90 GNU Release on this Windows x64 environment (12 logical processors; reported AMD64 Family 23 Model 113) measured a 0.025 ms median plan and one 316.7 ms merge. Plan samples ranged from 0.0237 to 0.0502 ms. The merge timer includes shared validation, staged native baking, final composition, structural history, replay encoding/temporary-journal append and result JSON. These are workload-specific cold-command observations, without a previous-build comparison, durable-disk latency, physical tablet or speedup claim. The required output raster is 4 MiB; source rasters are borrowed and consumed buffers move into Undo, while existing history/recording costs remain separate from that output estimate.

The desktop/CLI executables are 10,088,448/3,961,856 bytes, respectively, 97/50 KiB above the preceding Pass Through preview. All native DLLs and dependencies are unchanged; these are executable sizes rather than total package or memory costs. The compositor retains SHA-256 `76bcba78f97ddb428d3ab485db23eb73f075d395db63664a0afbe271d93a70c1`.

The preceding Pass Through host, with its own unchanged manifests and DLLs, also opened the new cross-parent selected-merge version-2 project with identical full PNG and layer/group metadata. Old and new core/replay-only readers verified and restored the unknown merge action's outcomes without algorithm DLLs; head, state and surface hashes were identical, including a recording re-exported by the old host. This checks the existing format contract without registering the new handler in the old package.

## Pass Through checks

Run `python scripts/smoke-pass-through.py --cli <built-cli> --gui <built-desktop> --legacy-compositor <previous-dll>` after staging the matching compositor. Its 53-action fixture compares independent partial-alpha/mode references, policy changes and Undo, inert workflows, complete-output baking, projects and DLL-independent replay across JSONL/MCP/GUI. The legacy fixture checks rejection before hidden-policy edits or project replacement. The registered `group.inspect` command supplies real four-language inspector captures while keeping the painting layer, raw pixels and effective graph intact. See [Pass Through](pass-through.md).

Native debug/Release checks cover all 65,536 input-alpha pairs at six boundary weights, random colors, all byte-to-float weight roundtrips, endpoint words, unrounded-alpha division, dirty strides, aliases and rejection before writes. Host regressions cover all 26 full-opacity equivalences, nested policies, retained modes, version-1 defaults, version-2 opt-in/Undo, semantic merge eligibility, unsupported capabilities, contact continuity and render/atomic/journal rollback. Pass Through availability requires both transparent group bases and Mix; free Mix availability remains an independent capability. Desktop checks exercise actual policy/opacity controls and Mix editing under narrow English/CJK metrics, stale drafts and queued document-bound inspection.

On 2026-11-06, with this task's builds and smoke GUIs finished, one ignored Release `mix_region_cost` observation measured a 1024-square four-instruction Mix at 12.8 ms for weight 0, 27.0 ms for weight 128 and 12.6 ms for weight 255. Two full-range ARGB sources and reusable output are prepared outside timers. Validation and native composition are included; allocation, source generation, sampled reference checks, Session/history/journal, encoding, I/O and physical pen latency are excluded. A complete group graph contains additional instructions; these are kernel observations, not a scene frame-rate claim.

Five interleaved old/new DLL pairs per existing color mode compared complete 1024-square outputs using two prepared full-range ARGB sources and four instructions. All 26 modes retained exact bytes. Final median differences ranged from -7.5% to +6.9%; Normal measured 25.9/27.5 ms (+6.1%) and Soft Light 72.8/77.8 ms (+6.9%). This is observed overhead for that workload, not unchanged-throughput evidence. The equations and legacy kernel bodies remain unchanged; adding the Mix dispatcher changes generated layout, without establishing its precise microarchitectural cost. Keep the original wrapper structure rather than assuming a general inlining improvement. Timers exclude allocation, fixture/compare/hash work, host state/history, transport, GUI and physical pen input. Results are environment-specific and do not establish a speedup.

The preceding 26-mode host was also run with its own unchanged manifests and the new DLL: all new-host-saved version-1 isolated projects produced identical PNGs, including own/cross-host verified outcome playback. The old host rejects a valid version-2 Pass Through project before changing its current state or pixels. Outcome playback requires no new compositor callbacks.

The desktop/CLI executables are 9,989,120/3,910,656 bytes, respectively, 44/22.5 KiB above the preceding blend preview. The compositor is 273,920 bytes, an increase of 512 bytes. No dependency is added; native rendering still borrows descriptors and uses fixed scratch rather than image-sized node intermediates.

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
