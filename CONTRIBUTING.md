# Contributing

Issues, improvements and pull requests are welcome. Describe reproducible behavior and keep discussion respectful.

Read [Development](docs/development.md) and [Architecture](docs/architecture.md). Rust 2021 and Rust 1.88+ are required. Build the plugin DLLs before running host tests.

```powershell
cargo fmt --all -- --check
.\scripts\build.ps1 -PluginsOnly
cargo test --locked
cargo build --release --locked
```

Keep document ownership and execution in Session, painting algorithms in independent DLLs, presentation in desktop, and Windows APIs in platform. Preserve dirty-region rendering, typed pen input and tiled history. Explain memory, execution and binary-size costs when changing hot paths or adding dependencies.

Validate behavior changes at the appropriate level. Check UI screenshots for layout changes, including English and CJK locales. Report the actual device and driver used for hardware pen testing. Keep examples and documentation aligned with implemented behavior.

Use English for documentation, code comments and new source messages. Add translations to the four catalogs in `locales/`, preserve format placeholders, and avoid translating protocol identifiers or user content. See [Localization](docs/localization.md).

For issues, include the MiCoPi and Windows versions, reproduction steps, expected behavior and actual behavior. For pen problems, include the device, driver and Windows Ink configuration.

Keep each PR focused. Describe the problem, resulting behavior and validation, and update [CHANGELOG.md](CHANGELOG.md) for user-visible changes. Exclude `target/`, `dist/`, downloaded toolchains, personal artwork and local workspace settings.
