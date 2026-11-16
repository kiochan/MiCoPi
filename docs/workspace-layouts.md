# Named workspace layouts

Named layouts save and restore panel arrangement within the existing desktop.
They do not create new Painting/Layout/Video engines, documents or capabilities.
This source increment has focused automated checks; exact validation results are
reported with its commit. Native screenshots, physical-device interaction and
complete exact-head CI remain separate acceptance gates.

Local validation passed 10 targeted layout tests (including real menu clicks in
four languages at two widths), three language-catalog tests and the repaired
shared-controller state-schema regression. Matching native DLLs were built/staged
through the portable build script before the final runtime checks. The existing
runtime smoke passed JSONL/MCP PNG-byte parity and explicit headless named-layout
responses with `--skip-gui`. Formatting, repository links/example/artifact checks
and whitespace checks passed. The GUI branch of that smoke and
`smoke-localization.py` have not been run for this increment; offscreen controls
do not substitute for reviewed native captures or physical pen evidence.

## Desktop use

Open **Workspace**, enter a **Layout Name**, and choose **Save New Layout**.
Click a saved name to switch. **Update Active Layout** explicitly replaces the
selected snapshot; **Delete Saved Layout** removes that snapshot without changing
the displayed arrangement or artwork. Names are user text and are not translated.
The first normal startup creates a `Default` snapshot of the reconciled layout.

Up to 16 snapshots are supported. Names contain 1-64 Unicode characters, without
control characters or surrounding whitespace. Save with an existing name fails
unless replacement is explicitly requested. Deleting a snapshot does not delete
documents, plugins or files outside the workspace configuration.

The current working arrangement autosaves separately from named snapshots. Moving
a panel does not silently replace its saved snapshot. Save/Update before switching
if those changes should be available under the name later. Restart restores the
last current arrangement, including unsaved-to-snapshot panel changes, and the
selected name. The active name identifies the selected snapshot, not an assertion
that the current arrangement is identical to it.

Snapshots retain dock trees, selected tabs, floating geometry and intentional
hidden windows. Available plugin windows are reconciled using each snapshot's
known-panel inventory. Unavailable panels are removed from the displayed layout;
the saved snapshot remains intact until explicitly updated. New contributions may
be added through the existing reconciliation rules.

Switching checks both Session contacts and local pen/mouse/selection/movement or
window-drag state. It rejects while those interactions are active, without ending
them. The desktop rechecks after a queued request reaches presentation. Idle
transform/Liquify and other drafts keep their source fences and values: switching
does not reset input, apply drafts, change pixels, increment the Session revision
or add document Undo. Existing presentation events are still recorded. If a target
layout hides the relevant panel, use **Windows** to show Canvas or Layers again;
standalone draft windows retain their existing lifecycle.

## Commands and observation

All operations use the shared registry, searchable/bindable plugin commands and
schema-derived nodes. For an attached desktop:

```powershell
micopi-cli call workspace.layout.save '{"name":"Painting"}' --attach session.json
micopi-cli call workspace.layout.save '{"name":"Painting","replace":true}' --attach session.json
micopi-cli call workspace.layout.switch '{"name":"Painting"}' --attach session.json
micopi-cli call workspace.layout.list '{}' --attach session.json
micopi-cli call workspace.layout.delete '{"name":"Painting"}' --attach session.json
```

Mutating commands return `queued: true, applied: false`. Poll
`workspace.layout.list`: `actual` includes `active`, `names`, `writable`, `error`
and `limit`; `last_result` reports `queued`, `applied` or `failed`, the operation,
name and any error. Serialize dependent requests and wait for presentation before
checking results. `applied` means the persisted arrangement was accepted, not that
every native viewport has already created its HWND. Use `window.list` for native
window observations. The last result is not a per-request completion ledger.

Headless commands return `queued: false`, `applied: false` and
`status: "presentation_unavailable"`; the read-only list reports `available: false`.
They never create a layout file. Save/Switch/Delete are external presentation
effects and are rejected inside atomic document groups. List remains available
while the bounded presentation queue is full.

## Storage and compatibility

`workspace.egui.json` becomes a version-2 envelope with `current` (a version-1
snapshot), `active` (a name or null) and `layouts` (named version-1 snapshots).
The whole file is limited to 8 MiB; each snapshot retains the existing 1 MiB,
32-surface/128-window bounds. `settings.json` remains unchanged.

Legacy bare dock trees and version-1 workspace files are read without first
rewriting them. Their arrangement becomes the initial `Default` snapshot;
normal persistence writes the new envelope through the existing atomic file
replacement helper. Older MiCoPi versions cannot be assumed to read version 2.
The existing one-time pre-plugin-window migration backup is retained.

Malformed, unsupported-version, oversized or unreadable workspace files are
preserved. Startup uses an in-memory default with a visible error and marks layout
persistence unavailable; autosave and explicit changes cannot overwrite that file.
To recover, close MiCoPi, back up and move the invalid file, then restart. Candidate
save/switch/delete changes are validated and written before updating live state;
write failure retains the old inventory and displayed dock. Exporting document
replay does not re-run these external file writes.

See [Plugin windows](plugin-windows.md), the [domain plan](workspace-domains.md)
and [roadmap](../ROADMAP.md#workspace-and-sovia-production-boundaries).
