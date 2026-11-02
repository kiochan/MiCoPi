# Localization

MiCoPi 0.19 uses English as its default and canonical source language. Supported display locales are en (English), zh-CN (Simplified Chinese), ja (Japanese) and ko (Korean).

![Display language setting](assets/localization-en.png)

## Select a language

Open Settings with Ctrl+, and search for Display Language or ui.language. Changes preview immediately; Save Settings persists the selection. Closing or canceling restores the last saved preferences. The dropdown uses each language's native name so it remains recognizable after switching.

```json
{"version":1,"ui":{"language":"en"}}
```

Configurations without language default to English. Unsupported codes and non-string values are rejected by settings validation. settings.update with an explicit settings path can also update ui.language in the live GUI. Keybindings and unknown root configuration fields are preserved.

## Scope and stable contracts

The workbench, menus, dialogs, settings descriptions, command search, shortcuts, tooltips, layer labels and node controls use embedded catalogs. Search accepts English IDs and translated names in all supported languages, independent of the active locale. Existing Chinese/pinyin search aliases remain available.

Action/state IDs, configuration keys, keyboard chord syntax, node type IDs, port names, schema types, file paths and user documents never change with display language. CLI/MCP contracts and technical diagnostics use English. Third-party text without a bundled translation remains as supplied; MiCoPi does not send plugin text to an online translation service. Existing bundled Chinese manifest titles are recognized for compatibility with customized installations.

Documentation and new canonical source messages are written in English. Historical screenshots may show earlier UI languages; they do not define a protocol contract.

## Catalogs

locales/en.json defines canonical English messages. zh-CN.json, ja.json and ko.json use the same keys. Catalogs are embedded in the executable, parsed once and shared immutably; no runtime downloads or locale files are required. Missing messages fall back to English/original text.

Static labels use lookup at presentation boundaries before layout. Rich text is translated before styles are applied. Formatted labels preserve the already formatted numeric/path arguments; translation never treats user data as format syntax. Keep every placeholder exactly, including names and precision specifiers. Avoid adjacent unseparated placeholders and escaped braces in translatable templates; use explicit static labels for ambiguous formatting. UI input text, file paths and editable JSON are not translated.

Windows fonts provide CJK fallback: Microsoft YaHei for Chinese, Yu Gothic/Meiryo for Japanese and Malgun Gothic for Korean, with existing installed alternatives. The application does not redistribute these fonts or enlarge the package with a font bundle. Missing optional system fonts use remaining font fallbacks; install the corresponding Windows language fonts if the OS cannot display those scripts.

## Add or change translations

1. Write an English source message and add the same key to every catalog.
2. Add natural translations for visible UI text. Keep technical IDs, paths and code unchanged.
3. Preserve all format placeholders. Translate before constructing styled RichText; use the shared desktop presentation helper for labels, buttons and hints.
4. Include translated names in search when adding new metadata-backed settings or nodes.
5. Run catalog/settings/search tests and inspect real GUI screenshots in all four languages, at narrow and wide sizes.

```powershell
cargo fmt --all -- --check
cargo test --locked
python scripts/smoke-localization.py
```

The tests enforce catalog key/placeholder parity, English defaults, locale validation, configuration preservation, localized command search and numeric/path formatting. The smoke test uses a disposable profile, verifies unchanged canvas PNG bytes while switching languages and closes only its own GUI process.
