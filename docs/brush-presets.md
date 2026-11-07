# User Brush and Eraser Presets

User presets are bounded profile data, separate from registered plugin tools and
document history. They save a complete scalar Brush or Eraser setup without
changing either color swatch. This source increment is awaiting CI validation;
physical tablet testing and reviewed screenshots remain separate evidence.

## Using the library

The Brush panel's **My Brushes** section offers **Save Current Brush**, search,
Rename, **Update from Current Brush** and Delete. Its library menu imports,
exports or explicitly reloads saved brushes. User names remain untranslated.
Unavailable providers stay visible with a reason instead of being silently
replaced. JSON file pickers use the existing native dialog path; environments
without it open a prefilled command form.

Built-in presets remain immutable registered tool contributions. Selecting one
continues to apply its existing partial `tool.select` parameters. A user preset
instead stores all twelve current scalar/dynamics values:

`radius`, `opacity`, `hardness`, `stabilizer`, `pressure_size`,
`pressure_opacity`, `pressure_gamma`, `minimum_radius`, `spacing`,
`minimum_opacity`, `flow` and `opacity_mode`.

Both `color` and `background_color` are excluded. Applying a preset preserves
the artist's foreground and background swatches. These are scalar presets;
sampled tips, textures and additional dynamics are still planned.

## Shared commands and state

| Action | Arguments and effect |
| --- | --- |
| `brush.presets.query` | Contact-safe cached library, built-in/user availability and matching information; no file I/O |
| `brush.presets.upsert` | Complete `name`, `owner`, `tool_id`, `parameters`; omit `preset_id` to create, include it to replace an existing record |
| `brush.presets.delete` | Remove one explicit `preset_id` without recycling its identity |
| `brush.presets.apply` | Resolve one `preset_id`, finish an accepted contact through the checked pointer boundary, then select its real tool and apply all twelve values |
| `brush.presets.import` | Read a bounded bundle from `path` and assign fresh local IDs in file order |
| `brush.presets.export` | Write explicit `preset_ids` to `path`; report selected IDs, byte count and SHA-256 |
| `brush.presets.reload` | Explicitly reload the active profile's library without resetting painting defaults |

The read-only public state is `painting.presets`. Its query action and state are registered by the painting package; the Brush package owns the library mutation commands and compiled controls. Query returns `version`,
`library_token`, `library`, `user`, `builtin`, `matching_preset_ids` and
`profile_path`. Each user row includes its preset, availability, reason and
matching status. Other actions accept optional `expected_library_token` for
stale-library protection; this token is Session-local and is not a persisted ID
or document revision. Schemas describe the complete arguments for command
forms, workflow nodes, MCP and headless clients.

File and library edits require idle Brush/Hand contacts and reject through the
shared admission path. Apply checks the exact tool owner, Brush/Eraser kind,
native provider and required parameter capabilities before sending one checked
UP for an active contact. Invalid or unavailable presets leave that contact
untouched. Failures while committing an already accepted contact propagate;
there is no promise to resurrect physical pen input after an I/O failure.

CRUD, import, export and reload are external profile effects: they create no
canvas Undo and are rejected inside document-atomic execution. Apply uses the
existing tool/parameter transaction contract; idle atomic failure restores its
painting state. Do not confuse the library token with canvas Undo or a durable
contact-completion guarantee.

## Storage and interchange

The active `settings.json` owns an optional `brush_presets` object:

```json
{"version":1,"next_id":1,"items":[]}
```

At most 128 user records are stored. IDs are positive 32-bit values, stored in
ascending order; `next_id` can reach the exhausted sentinel 4294967296. Normal
library mutations never recycle deleted IDs; same-Session reload/settings updates
also reject a regressed counter. Loading a manually reset/replaced identity space
with a lower counter requires a restart or new Session; library tokens do not carry across
Sessions. A never-used default library stays omitted
unless the root field was explicit. Unknown supported settings root fields and
explicit preferences survive saves. Library changes validate and atomically
replace the profile before publishing the new cache. If another editor changed
the profile library, reload explicitly before retrying.

Each record contains only `id`, `name`, `owner`, `tool_id` and `parameters`.
Names must be nonblank, control-free and at most 512 UTF-8 bytes. Owner/tool IDs
use the registered contribution identifier syntax. Parameters are complete and
use the same raw numeric validation as ordinary painting settings, including
the canonical `spacing` boundary. Missing or differently owned providers may
be stored, but Apply reports them unavailable. The native fallback is the same
Brush/Eraser provider resolution used by painting, not a new preset engine.

Interchange is strict data-only JSON:

```json
{"format":"micopi.brush-presets","version":1,"items":[]}
```

Bundles are limited to 1 MiB and 128 records. Future versions, unknown fields,
duplicate source IDs, incomplete parameters and invalid values reject the
whole import. All records validate before publication; successful imports map
source IDs to fresh local IDs in input order. No actions, handlers, code or
assets are accepted. Exports accept explicit user IDs only and protect the
active settings file, including canonical path aliases. Import/export results
include the source/output byte count and SHA-256 digest.

## Ownership, replay and performance

The Session loads a typed library cache for both desktop and CLI. Only explicit
file operations read disk. A dedicated desktop cache-sync effect updates the
settings model without applying painting defaults; later settings saves retain
the latest library and supported unknown root fields. Cached UI cards rebuild
when the library token changes; matching compares the twelve values directly.
No registry injection, new dependency, native ABI or per-MOVE file/JSON work is
introduced.

Existing replay metadata records the resolved tool and painting parameters,
not a dependency on the preset file. Verified outcome playback therefore needs
neither the library nor its DLL. Successful preset source context records the
handler result; import/export context retains the reported digests. Replay does
not repeat profile writes or other external effects.

Storage/codec, Session, actual UI and tiny CLI/MCP regressions are authored and
wired for CI. They cover raw numeric boundaries, identity allocation, both
swatches, provider failures, contacts, cache synchronization, external effects,
atomic Apply and DLL-free replay. No throughput or hardware claim follows from
those synthetic fixtures.

See [Painting tools](painting-tools.md), [Stroke opacity](stroke-opacity.md),
[Command admission](command-contact-admission.md) and
[Settings schema](reference/settings.schema.json).
