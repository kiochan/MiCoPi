# Contributing

Issues, improvements and pull requests are welcome. Describe reproducible behavior and keep discussion respectful.

Read [Development](docs/development.md) and [Architecture](docs/architecture.md). Rust 2021 and Rust 1.88+ are required. Build the plugin DLLs before running host tests.

Coding agents must also follow [AGENTS.md](AGENTS.md), which records repository boundaries, the maintainer's Git workflow and checks appropriate to each change.

```powershell
cargo fmt --all -- --check
.\scripts\build.ps1 -PluginsOnly
cargo test --locked
cargo build --release --locked
```

Keep document ownership and execution in Session, painting algorithms in independent DLLs, presentation in desktop, and Windows APIs in platform. Preserve dirty-region rendering, typed pen input and tiled history. Explain memory, execution and binary-size costs when changing hot paths or adding dependencies.

Validate behavior changes at the appropriate level. Check UI screenshots for layout changes, including English and CJK locales. Report the actual device and driver used for hardware pen testing. Keep examples and documentation aligned with implemented behavior.

English is the source of truth for technical documentation, code comments and new source messages. The root README also has Chinese, Japanese and Korean editions; update them together when the overview changes. Add UI translations to the four catalogs in `locales/`, preserve format placeholders, and avoid translating protocol identifiers or user content. See [Localization](docs/localization.md).

Only English is embedded; other catalogs and declared plugin catalogs load externally. Use contribution ownership when translating plugin titles and search terms. Keep malformed-file fallback and hot reload behavior covered when changing the loader. Package activation and native DLL changes still require restart; see [Plugin system](docs/plugin-system.md).

For issues, include the MiCoPi and Windows versions, reproduction steps, expected behavior and actual behavior. For pen problems, include the device, driver and Windows Ink configuration.

Keep each PR focused. Describe the problem, resulting behavior and validation, and update [CHANGELOG.md](CHANGELOG.md) for user-visible changes. Exclude `target/`, `dist/`, downloaded toolchains, personal artwork and local workspace settings.

## Git workflow

Keep `main` reviewable. Follow explicit maintainer instructions: current maintainer-directed agent work commits directly to `main` without creating branches. External contributions normally use focused branches and PRs. Commit a coherent change after its relevant checks pass; use short, descriptive messages such as `docs: explain the execution model` or `fix: preserve pen pressure on release`. Use repository-local identity and credentials when projects use different accounts.

Do not rewrite published history or commit built DLLs, release archives, personal settings or generated test output. Keep `Cargo.lock`, SDK headers, source assets and selected documentation screenshots tracked. Annotated `vX.Y.Z` tags identify reviewed application versions. Commit and tag locally before an explicitly requested push; creating a local version does not publish it.

Run `python scripts/check-repository.py` after moving examples, documentation or screenshots. The directory ownership and artifact rules are documented in [Project Structure](docs/project-structure.md).
