# Agent Guide

These instructions apply throughout this repository. Explicit maintainer instructions take precedence.

## Start here

- Read [README](README.md), [Architecture](docs/architecture.md), [Philosophy](docs/philosophy.md) and [Contributing](CONTRIBUTING.md) before changing a subsystem.
- Use [ROADMAP](ROADMAP.md) for planned work and [CHANGELOG](CHANGELOG.md) for implemented behavior. Do not describe planned features as available.
- Inspect Git status before editing; preserve unrelated work. Keep changes focused and commit coherent features separately.
- For this maintainer's workflow, work directly on `main`. Do not create branches unless asked. Push only when explicitly requested; never force-push or rewrite published history.
- Use repository-local Git identity and credentials. Do not change global Git configuration or substitute another account. Verify the configured remote before pushing.

## Architecture and performance

- `application::Session` owns mutable document state. Desktop, JSONL, MCP and workflow clients share its validation and execution contracts.
- Put pixel algorithms in independent native plugins under `plugins/`. Keep host registration, validation and dispatch in `src/plugins/`; retain document/history ownership in the host.
- Feature `plugins/<feature>/panel.rs` files compile into the desktop host. Do not expose egui objects through the C ABI or imply that Rust UI code loads dynamically.
- Keep the typed pointer path free of per-MOVE JSON serialization, file I/O, registry lookup, instance creation and whole-image copying. Preserve cached instances, dirty rectangles, reused buffers and tiled history.
- Keep image composition and action workflows separate. Editing a workflow must not execute it.
- New document operations need schemas, validation, revision behavior, undo, atomic capability declarations and verified outcome replay. Test failure/rollback behavior where relevant. Replay restores outcomes without executing DLL commands or external effects.
- Preserve supported project, recording and ABI compatibility. Extend optional ABI tails with size/capability checks; retain rejection fixtures and old recording hashes.
- Measure hot-path or dependency costs. Do not claim physical tablet compatibility from synthetic tests.

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

## Repository hygiene

- Track source, manifests, locale catalogs, SDK headers, examples, notices and `Cargo.lock`.
- Keep binaries, DLLs, release archives, downloaded toolchains, personal credentials/settings, session endpoints and artwork out of Git.
- Update relevant docs, schemas, examples and changelog with behavior changes. Add root guides to portable packaging when distributed docs link to them.
- Promote only selected, reviewed screenshots to `docs/screenshots/`. Do not replace current screenshots with mocks or unrelated captures.
- Do not bump versions or create release tags for routine documentation updates.
