# Unified plugin packages

All package types use the same manifest, dependency resolver and registry. Tools, actions, state providers, nodes and windows retain their specific execution interfaces. Typed brush/compositor callbacks remain separate from JSON orchestration.

## Package identity and compatibility

Native pixel access is part of action registration: optional `pixel_access` and package `action_access` overlays classify `none`, `pixels` or `geometry` access to existing layers. The shared registry validates ownership/conflicts and publishes effective native access. Old DLLs can be annotated through manifests; tables and binary descriptors need no change. See [Layer pixel access](native-plugins.md#layer-pixel-access) for conservative defaults and host enforcement.

Manifests may add `version`, `description`, `author` and `min_host_version`. Versions use numeric `major.minor.patch` release triples. Existing manifests without these fields remain valid. `api_version` still describes manifest/registration compatibility; native SDK table versions remain independent.

```json
{"api_version":1,"id":"example","name":"Example tools","version":"1.0.0","description":"A small tool package","author":"Example author","min_host_version":"0.21.0","requires":["painting","workbench"]}
```

The loader checks metadata and minimum host version before opening a DLL. Invalid dependency IDs, duplicates and self-dependencies are rejected. Registration remains package-atomic: a failed package contributes no tools, actions, states, nodes or windows. Independent valid packages continue loading.

## Inventory and lifecycle

`plugins.list` preserves its existing fields and adds `packages`, `disabled` and `lifecycle`. Each package record contains its manifest path, optional identity/version/author, dependencies, capabilities, contribution counts and current status:

| Status | Meaning |
| --- | --- |
| `loaded` | Successfully registered for this session |
| `disabled` | Disabled by manifest or saved preferences; DLL was not opened |
| `blocked` | Required packages did not load, or dependencies form a cycle |
| `failed` | Manifest, compatibility, native loading or contribution validation failed |

Malformed manifests also receive a path-based record with a null ID and a diagnostic. `blocked_by` lists requirements absent from the loaded registry. For loaded packages, contribution counts include DLL descriptors and generated window actions. Other records report declared contributions. Records distinguish the manifest enable flag from preference-based disabling; duplicate-ID failures retain their own manifest paths.

The current package lifecycle is **discover → validate → resolve dependencies → register → run → destroy**. Contribution changes and native library changes require restart. Existing module references and instances retain their current lifetime rules. This does not add runtime DLL replacement or dynamically compiled egui controls.

## Package language catalogs

A manifest can declare optional external translations for its own contribution titles and UI text:

```json
{"locales":{"zh-CN":"locales/zh-CN.json","ja":"locales/ja.json","ko":"locales/ko.json"}}
```

Each file is a UTF-8 JSON object mapping canonical English messages to translations, for example `{"Quick Tools":"快捷工具"}`. Only English is embedded in the host. English package messages remain in contribution metadata; an `en` locale declaration is rejected. Paths must be relative `.json` paths with forward slashes and normal components, contained within the package directory after resolving links.

Only loaded packages contribute catalogs. Missing translations fall back to host translations and then the original English message. Malformed or inaccessible files report a diagnostic and retain their last valid contents without disabling package actions. The shared background watcher checks these files every 500 ms. Editing a declared file takes effect without restart; adding declarations or enabling a package requires restart.

The registry exposes contribution owners for actions, tools, states, nodes and panels through `plugins.list.contribution_owners`. Presentation and command search use these owners to isolate package translations: a plugin's `File` entry cannot replace the host File menu or another plugin's entry. Translations never change action IDs, shortcuts, workflow ports or protocol values. Compiled plugin UI must use the owner's presentation helper for its own text; runtime loading of arbitrary egui code is not provided.

Profile catalogs may override one package entry using a qualified key such as `@plugin/quick/Quick Tools`. Host catalogs load first, package catalogs next and profile overrides last. Each file is limited to 2 MiB; all accepted external files together are limited to 16 MiB. See [Localization](localization.md) and the installable [Quick tools example](../examples/plugins/quick-tools/manifest.json).

## Dependency-aware enable/disable plans

Settings > Plugins uses a shared planner before changing the settings draft. Enabling a package enables all required packages. Disabling it also disables enabled packages that depend on it, transitively. Core, painting and workbench cannot be disabled through the planner, including indirectly through a dependency cascade. Missing requirements, cycles, ambiguous duplicate IDs and manifest-level disabling reject an enable plan without partial preference changes.

Pending checkboxes and **Restart Required** describe the draft; current package status continues to describe the running registry. Save Settings persists the draft; Cancel restores the previous preferences. No DLL is unloaded while drawing.

The same read-only planner is available to JSONL, MCP and workflow clients:

```json
{"action":"plugins.plan","args":{"id":"example","enabled":false}}
```

The result contains the complete `disabled` list, individual `changes` and `restart_required`. The optional `disabled` argument starts from a caller-provided draft; otherwise the planner uses the session's startup preferences. Unknown but syntactically valid disabled IDs are preserved, allowing preferences for temporarily uninstalled packages. The planner never writes files, changes document revision or modifies the live registry. Apply its resulting list with the existing `settings.update` action and an explicit settings path, then restart to activate it.
