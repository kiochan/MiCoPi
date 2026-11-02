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
| check-repository.py | Validate local Markdown links, example JSON and tracked-file boundaries |

```powershell
python scripts/check-repository.py
.\scripts\build.ps1 -Portable -Test
python scripts/smoke-runtime.py
python scripts/smoke-workflows.py
python scripts/smoke-localization.py
python scripts/package.py
```

The three GUI smoke checks create their own isolated profiles and close only their own test process. Use `--cli` and `--gui` to select debug binaries when iterating. `package.py` packages already built binaries; it does not download dependencies, rebuild the application or publish anything.

See [Development](../docs/development.md) for prerequisites and [Project Structure](../docs/project-structure.md) for ownership rules.
