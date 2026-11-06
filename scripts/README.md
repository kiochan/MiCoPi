# Development Scripts

Run these tools from the repository root. Downloaded toolchains, builds, captures and release output remain in ignored directories.

| Script | Purpose |
| --- | --- |
| build.ps1 | Build/stage desktop or headless binaries, DLLs and licenses; optional tests/benchmarks |
| package.py | Package an existing build with current manifests, documentation and examples |
| bootstrap-portable.py | Prepare the project-local GNU linker toolchain |
| bootstrap-egui.py | Prepare the project-local Rust/desktop build environment |
| prepare-icons.py | Maintain the selected Material Symbols font subset |
| smoke-runtime.py | Compare JSONL, MCP and GUI pixels; exercise input/settings |
| smoke-workflows.py | Verify plugin nodes, workflow execution and project persistence |
| smoke-localization.py | Check four-language switching, settings and unchanged document data |
| smoke-captures.py | Verify concurrent root/native capture identities, exact PNG sizes, terminal failures and unchanged artwork |
| capture_checks.py | Shared owned-window readiness, capture-status and independent PNG checks for GUI smoke scripts |
| smoke-raster-masks.py | Verify A8 storage/composition, safe edit boundaries, legacy compatibility and DLL-free replay; optional GUI captures |
| smoke-mask-editing.py | Verify eleven public mask actions, native density/feather and typed painting, workflow bindings, guarded edits and raw/verified replay across JSONL/MCP/GUI; optional four-language captures |
| smoke-mask-geometry.py | Compare document framing, rotations/flips, image sampling and Trim against independent raw-mask references across JSONL/MCP/GUI, with undo, legacy capability rejection and DLL-free replay |
| smoke-mask-owners.py | Compare single-layer mask copies/deletions, independent storage, ID bindings, targets, grouped composition, undo, rollback and DLL-free replay across JSONL/MCP/GUI |
| smoke-mask-groups.py | Compare mask-preserving group creation/reparenting, raw ownership, explicit targets, composition, undo, rollback and DLL-free replay across JSONL/MCP/GUI |
| check-repository.py | Validate local Markdown links, example JSON and tracked-file boundaries |

```powershell
python scripts/check-repository.py
.\scripts\build.ps1 -Portable -Test
python scripts/smoke-runtime.py
python scripts/smoke-workflows.py
python scripts/smoke-localization.py
python scripts/smoke-mask-editing.py --cli <built-cli> --gui <built-desktop>
python scripts/package.py
```

GUI smoke checks create their own isolated profiles and close only their own test process. Use `--cli` and `--gui` to select debug binaries when iterating. `package.py` packages already built binaries; it does not download dependencies, rebuild the application or publish anything.

`smoke-mask-editing.py --skip-gui` runs the transport-only subset. Its optional
`--legacy-compositor` path selects a preserved masks-only 40-byte DLL for missing
effects checks; an absent baseline is reported as skipped. Build/stage matching
current DLLs before using the script. Captures remain under ignored
`target/test-output/mask-editing-smoke/`. See [Raster Mask Editing](../docs/raster-mask-editing.md)
for the implemented subset and the owner-editing paths that remain guarded.

`smoke-mask-geometry.py --skip-gui` checks document-wide geometry through JSONL
and MCP. Its optional `--legacy-painting` selects a preceding A8 painting DLL
without the independent sampler table; absence is reported explicitly. The full
run adds GUI transport parity and four-language dialog/native Layers captures
under `target/test-output/mask-geometry-smoke/`. See
[Mask Canvas Geometry](../docs/mask-canvas-geometry.md) for raw/effective coverage,
admission and remaining owner-editing boundaries.

`smoke-mask-owners.py --skip-gui` checks the single-raster ownership subset;
the full run adds attached-GUI parity and four native Layers captures. It uses
small independently written project fixtures and integral JSON-float selectors,
and keeps output under `target/test-output/mask-owners-smoke/`. See
[Mask Owner Editing](../docs/mask-owner-editing.md) for exact target/ID semantics
and the remaining group/batch/transfer boundaries.

`smoke-mask-groups.py --skip-gui` checks mask-preserving group creation and
hierarchy moves through JSONL/MCP; the full run adds GUI parity and four native
Layers captures. Tiny independent tree/raw/composition references cover typed
result bindings, targets, Undo, contacts, rollback and DLL-free outcomes. Output
stays under `target/test-output/mask-groups-smoke/`. See
[Mask Group Editing](../docs/mask-group-editing.md) for the supported subset and
remaining group copy/delete, ungroup, batch and geometry boundaries.

See [Development](../docs/development.md) for prerequisites and [Project Structure](../docs/project-structure.md) for ownership rules.
