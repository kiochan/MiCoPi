# Agent Guide

These instructions apply throughout this repository. Explicit maintainer instructions take precedence.

## Start here

- Read [README](README.md), [Architecture](docs/architecture.md), [Philosophy](docs/philosophy.md) and [Contributing](CONTRIBUTING.md) before changing a subsystem.
- Use [ROADMAP](ROADMAP.md) for planned work and [CHANGELOG](CHANGELOG.md) for implemented behavior. Do not describe planned features as available.
- Inspect Git status before editing; preserve unrelated work. Keep changes focused and commit coherent features separately.
- For this maintainer's workflow, work directly on `main`. Do not create branches unless asked. Push only when explicitly requested; never force-push or rewrite published history.
- Use repository-local Git identity and credentials. Do not change global Git configuration or substitute another account. Verify the configured remote before pushing.

## Product direction

- Build a small, responsive native Rust painting studio for anime and manga illustrators. Prioritize drawing, clear tool feedback and a calm, configurable workspace. See [Interface Design](docs/interface-design.md).
- AI readiness means discoverable actions, schemas, public state and shared execution through plugins, MCP and headless clients. Keep document operations accessible through those contracts; presentation-only commands must explicitly report headless unavailability.
- Put implemented behavior in [CHANGELOG](CHANGELOG.md), intended work and acceptance criteria in [ROADMAP](ROADMAP.md), and module ownership in [Project Structure](docs/project-structure.md). Keep these guides consistent rather than treating plans as completed work.

## Repository map and ownership

| Location | Responsibility |
| --- | --- |
| `src/main.rs`, `src/bin/micopi-cli.rs` | Desktop and CLI/MCP entry points |
| `src/application/` | Session ownership, action schemas, validation, revisions, atomic execution, workflows and replay orchestration |
| `src/automation/` | JSONL, MCP and attachment transport; dispatch against Session |
| `src/engine/` | Document/layer structures, image composition, tiled history and replay encoding |
| `src/plugins/` | Host registration, DLL loading, lifecycle, contribution ownership and dependency planning |
| `crates/plugin-api/` | Independent C ABI SDK, capability contracts and C header |
| `plugins/` | Installable packages, manifests, native algorithm source and feature-owned UI adapters |
| `plugins/<feature>/panel.rs` | Feature UI source compiled into the desktop host |
| `src/desktop/` | Shared window lifecycle, docking, native viewports, input routing, themes and texture cache |
| `src/platform/` | Windows Ink, window chrome, DPI and native file integration |
| `src/ui/` | Shared command, shortcut and workspace models |
| `src/configuration.rs` | Window-free settings validation and persistence |
| `src/i18n.rs`, `src/i18n_external.rs`, `locales/` | Presentation lookup, owner-scoped external catalogs and background reload |
| `assets/` | Runtime fonts/icons and third-party notices |
| `docs/`, `docs/reference/`, `docs/screenshots/` | English guides, machine-readable references and reviewed application captures |
| `examples/` | Runnable automation, configuration, MCP and extension examples |
| `scripts/`, `tests/`, `.github/` | Build/smoke/repository tooling, shared fixtures/reference implementations and CI |
| `target/`, `dist/`, `artworks/` | Ignored local builds, test output, release artifacts and personal artwork |

Keep `src/plugins/` host integration separate from root `plugins/` feature packages. The legacy GDI frontend under `src/platform/windows/` remains a regression reference. Update this map and the detailed structure guide when moving responsibilities; avoid directory-only refactors mixed into feature commits.

## Architecture and performance

- `application::Session` owns mutable document state. Desktop, JSONL, MCP and workflow clients share its validation and execution contracts.
- Put pixel algorithms in independent native plugins under `plugins/`. Keep host registration, validation and dispatch in `src/plugins/`; retain document/history ownership in the host.
- Feature `plugins/<feature>/panel.rs` files compile into the desktop host. Do not expose egui objects through the C ABI or imply that Rust UI code loads dynamically.
- Keep the typed pointer path free of per-MOVE JSON serialization, file I/O, registry lookup, instance creation and whole-image copying. Preserve cached instances, dirty rectangles, reused buffers and tiled history.
- Keep image composition and action workflows separate. Editing a workflow must not execute it.
- New document operations need schemas, validation, revision behavior, undo, atomic capability declarations and verified outcome replay. Test failure/rollback behavior where relevant. Replay restores outcomes without executing DLL commands or external effects.
- New single-layer pixel edits must honor pixel locks; geometric pixel edits also honor position locks. Preserve immutable reads, selection-only edits and document-wide geometry. Native actions declare `pixel_access` (`none`, `pixels`, `geometry`) or use owner-scoped `action_access` manifest overlays; undeclared opaque mutations are conservatively geometric. Gate every mutable host loan and keep typed permissions cached at contact start. See [Layer Editing](docs/layer-editing.md) and [Native Access](docs/native-plugins.md#layer-pixel-access).
- Keep layer multi-selection distinct from the active painting layer. Batch operations require explicit stable IDs, shared-handler validation, one bounded undo group/revision/outcome and full rollback; do not silently widen single-layer painting or use a general atomic pipeline as a substitute for grouped undo.
- Preserve supported project, recording and ABI compatibility. Extend optional ABI tails with size/capability checks; retain rejection fixtures and old recording hashes.
- Measure hot-path or dependency costs. Do not claim physical tablet compatibility from synthetic tests.

## Implementation conventions

- Register operations and their public contracts through the shared registry. Desktop controls, command search, shortcuts, nodes and automation should dispatch the same operations instead of introducing a second editing path.
- Keep appearance, shortcuts, painting defaults and plugin preferences in validated `settings.json`; keep docking/native geometry in `workspace.egui.json`. Preserve existing explicit preferences and supported unknown root settings fields when saving.
- Derive UI spacing and sizes from configured appearance metrics, use viewport DPI coordinates consistently, and check narrow/wide layouts and CJK labels. Keep schema editors and user document text independent of localization.
- Follow existing Rust module/style conventions and `cargo fmt`. Keep refactors focused, reuse existing helpers and justify dependency additions with measured costs. Keep `Cargo.lock` tracked and use the existing native build path.
- Use reference projects for feature and interaction ideas. For PhotoCraft-inspired work, independently implement suitable behavior within MiCoPi's architecture; the maintainer has excluded copying its source code. Retain provenance and required notices for any separately approved third-party assets or dependencies.

## Plugins and localization

- Use the shared registry, ownership information and dependency planner. Keep `plugins.plan` read-only; persist enable/disable drafts through settings. Plugin contribution and DLL changes require restart.
- Only English is embedded. Chinese, Japanese and Korean load from external JSON catalogs through the bounded background watcher; retain last-good translations on invalid edits.
- Apply localization at presentation boundaries. Never translate action/state IDs, node ports, config keys, file paths, editable JSON or user document content.
- Plugin catalogs are scoped by contribution owner. Use owner-aware presentation/search helpers and validate relative locale paths. Locale edits can hot-reload; manifest declarations and compiled UI changes require restart/rebuild.
- Technical docs, code comments and new source messages use English. Keep all four root READMEs aligned when changing the overview, and preserve catalog placeholders. See [Localization](docs/localization.md) and [Plugin System](docs/plugin-system.md).

## Verification

Use the existing Windows build scripts; do not install a replacement toolchain unnecessarily. If using the project-local portable environment, initialize the generated `target/cargo-env.ps1` in that shell. See [Development](docs/development.md) for setup and exact build paths.

Choose checks that exercise the changed behavior:

| Change | Verification |
| --- | --- |
| Documentation, examples, links | `python scripts/check-repository.py` and `git diff --check` |
| Rust code | `cargo fmt --all -- --check`, relevant tests; full desktop/headless tests for shared contracts |
| DLL algorithms or ABI | Build/stage matching DLLs first; run native lifecycle, compatibility and pixel/reference checks |
| Actions, transports or settings | Relevant tests and `python scripts/smoke-runtime.py` |
| Localization or translated UI | Catalog/search tests and `python scripts/smoke-localization.py`; inspect English and CJK screenshots |
| Nodes, replay or windows | Corresponding `scripts/smoke-*.py` and targeted regression tests |

Use locked dependencies (`--locked`); use `--offline` when the local cache suffices. Smoke scripts accept `--cli` and `--gui` to select built executables. Use isolated profiles, keep captures under `target/test-output/`, and close only processes created by the check. Report what ran and any remaining limitations; do not rerun an entire suite for documentation-only edits.

## Change summaries and commits

- Every authored commit must update the relevant summary under `CHANGELOG.md` > `Unreleased`, including documentation, maintenance and formatting commits. Describe the concrete change and its effect in English; consolidate overlapping notes and remove superseded Unreleased descriptions. Keep released-version history intact and planned work in `ROADMAP.md`.
- Give each commit a descriptive subject and an English body summarizing what changed, why, checks actually run and any remaining limitations. Before committing, verify that the summary matches the staged diff. After committing, report the hash, a brief change summary and validation to the maintainer; do not report a push unless one occurred.
- Follow the concrete message example in [Contributing](CONTRIBUTING.md). Documentation-only work needs repository/link and whitespace checks, without an application rebuild. Confirm the current branch and working-tree state again before staging; include only the intended change.

## Repository hygiene

- Track source, manifests, locale catalogs, SDK headers, examples, notices and `Cargo.lock`.
- Keep binaries, DLLs, release archives, downloaded toolchains, personal credentials/settings, session endpoints and artwork out of Git.
- Update relevant docs, schemas, examples and changelog with behavior changes. Add root guides to portable packaging when distributed docs link to them.
- Promote only selected, reviewed screenshots to `docs/screenshots/`. Do not replace current screenshots with mocks or unrelated captures.
- Do not bump versions or create release tags for routine documentation updates.
