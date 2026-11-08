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

For the maintainer's continuous-development workflow, write tests alongside each
feature and batch related implementation before verification. Keep local checks
focused; complete builds, desktop/headless regressions and transport smoke tests
run in CI. Commit and push coherent increments, continue independent work while
CI runs, and inspect and repair failures against the exact pushed commit. Pending
CI is not a passing result. Do not repeat full local suites after every small
edit or remove checks to make a failing feature appear complete.

The per-workflow/ref concurrency group retains its running CI validation when a
new increment is pushed. GitHub's default single-pending policy keeps the latest
waiting run and can replace an older pending run; it does not promise a separate
completed run for every intermediate commit. Continue implementing and pushing
authorized coherent increments while CI runs, then inspect evidence for the
exact tested head. See [GitHub concurrency rules](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency).

English is the source of truth for technical documentation, code comments and new source messages. The root README also has Chinese, Japanese and Korean editions; update them together when the overview changes. Add UI translations to the four catalogs in `locales/`, preserve format placeholders, and avoid translating protocol identifiers or user content. See [Localization](docs/localization.md).

Only English is embedded; other catalogs and declared plugin catalogs load externally. Use contribution ownership when translating plugin titles and search terms. Keep malformed-file fallback and hot reload behavior covered when changing the loader. Package activation and native DLL changes still require restart; see [Plugin system](docs/plugin-system.md).

For issues, include the MiCoPi and Windows versions, reproduction steps, expected behavior and actual behavior. For pen problems, include the device, driver and Windows Ink configuration.

Keep each PR focused. Describe the problem, resulting behavior and validation. Exclude `target/`, `dist/`, downloaded toolchains, personal artwork and local workspace settings.

## Git workflow

Keep `main` reviewable. Follow explicit maintainer instructions: current maintainer-directed agent work commits directly to `main` without creating branches. External contributions normally use focused branches and PRs. Commit a coherent change after its relevant checks pass; use short, descriptive messages such as `docs: explain the execution model` or `fix: preserve pen pressure on release`. Use repository-local identity and credentials when projects use different accounts.

### Change summaries for every commit

Before each authored commit, update the relevant category under [CHANGELOG.md](CHANGELOG.md) > **Unreleased**, including documentation, maintenance and formatting work. Summarize the concrete result and its effect in English. Consolidate overlapping entries rather than repeating the same feature; replace superseded Unreleased descriptions with the current behavior. Keep released-version notes intact and planned features in [ROADMAP.md](ROADMAP.md). Git history retains the individual commits.

Use a short subject and an English commit body explaining what changed, why, checks actually run and remaining limitations. State when a check was not run or is not applicable. Match the summary to the staged diff; never claim a test result from an unrelated change.

For example:

```text
fix: keep the selected panel tab visible

Changes:
- Scroll the tab strip when selection changes so the active tab stays visible.
- Update the Unreleased workbench summary.

Validation:
- Describe the targeted checks actually run and their results.
- State any remaining interactive or hardware checks.
```

After committing, report the commit hash, a brief change summary and validation to the maintainer. Distinguish a local commit from a push; publishing still requires an explicit request.

Do not rewrite published history or commit built DLLs, release archives, personal settings or generated test output. Keep `Cargo.lock`, SDK headers, source assets and selected documentation screenshots tracked. Annotated `vX.Y.Z` tags identify reviewed application versions. Commit and tag locally before an explicitly requested push; creating a local version does not publish it.

Run `python scripts/check-repository.py` after moving examples, documentation or screenshots. The directory ownership and artifact rules are documented in [Project Structure](docs/project-structure.md).
