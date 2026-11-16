# Painting, layout and video workspace boundaries

Status: design decomposition requested by the maintainer. None of the new
workspace switching, editable text, media references or video facilities below
is implemented by this document. The existing
[painting-first requirements](painting-requirements.md) remain the immediate
delivery baseline; this plan does not restart Photoshop parity.

## Product boundary

Keep one MiCoPi desktop application with task-oriented workspaces. A workspace
selects panels, view state and command context; it does not create another editing
engine, copy a document or change its meaning. Reuse the existing native docking
and plugin-window foundations rather than embedding a second application.

| Workspace | Artist task | Owned presentation | Document dependency |
| --- | --- | --- | --- |
| Painting | Draw and correct illustrations, including multi-layer Liquify and symmetry | Canvas, tools, colors, layers and correction drafts | Existing painting document and Session |
| Layout | Prepare Sovia covers, title cards and reusable visual assets | Canvas, typography, alignment and template controls | Painting document initially; editable text requires a separately specified document extension |
| Video | Arrange visuals, music, timed lyrics and simple motion | Preview, timeline, media list and clip properties | Future versioned video project referencing immutable media |

Painting remains the default. Layout starts with a static cover workflow; advanced
typesetting is deferred. The first video slice is one visual, one music track,
timed lyrics and transform/opacity animation. General multitrack footage editing,
multicam, tracking, advanced audio and professional grading are outside that slice.
Separate executables are not needed for the first slice. An optional media worker
is an execution boundary, not another user-facing editor.

## Shared infrastructure and separate state

| Concern | Owner and rule |
| --- | --- |
| Mutable painting pixels, masks and layer metadata | Existing `application::Session` and engine history; desktop and media workers never own a second writable canvas |
| Static cover editing | The same painting document; switching Painting/Layout preserves IDs, raw pixels, history and unsaved state |
| Workspace layouts | Desktop-owned named dock/view snapshots in a versioned extension of `workspace.egui.json`; preserve the current layout during migration |
| Appearance, shortcuts and tool defaults | Existing validated `settings.json`; workspace switching cannot silently replace explicit preferences |
| Video timeline and asset bindings | Future host-owned video project state admitted through the shared application boundary, with its own revision, history and recovery contract |
| Preview, decoding and export | Optional bounded workers reading immutable snapshots and returning source-tagged results; project mutation stays in the host |
| Action discovery and automation | Existing registry, schemas, validation, nodes, JSONL and MCP; explicit document/project targets before multiple editable resources are exposed |

Do not put a timeline into layer composition metadata, an A8 mask or the painting
replay journal. Image graphs, action workflows and time-dependent video graphs
have different semantics. A workspace name is not a document or project ID.

Painting Undo applies to painting edits; future video Undo applies to timeline
edits and reference replacement. Merely switching workspaces creates no document
Undo entry and changes no document revision. Existing presentation-event behavior
must be preserved and documented separately. Cross-resource atomic editing is
unsupported until explicitly designed; registry reuse is not a promise of a
transaction spanning a painting document, video project and external files.

## Painting-to-video asset contract

Start with a published PNG snapshot plus a manifest, rather than live access to
mutable painting buffers. Whole-composite PNG publication is the initial scope;
separate owner exports require explicit IDs and a later export contract.

A proposed manifest binds an asset ID, format version, source document identity,
source revision, content digest, pixel dimensions, alpha/color interpretation and
relative artifact location. These are design fields, not registered APIs. A video
project stores the selected immutable asset version and its source relationship;
a pathname alone does not identify a version.

1. Publish from an idle, revision-fenced painting snapshot through the existing
   action boundary. Stage the artifact and manifest, validate their digests, then
   expose the complete version; a failed publication leaves no usable partial
   version and preserves the source document.
2. Import pins that version. Opening the source for editing never retargets
   painting based on the current video selection or changes the pinned artifact.
3. A later publication may advertise an update. The first implementation requires
   explicit replacement, retaining the old version for video Undo. A save or file
   watcher cannot silently alter a finished cut.
4. Export captures exact video revision and asset digests at job start. Subsequent
   edits cannot change that job's inputs. Missing or digest-mismatched media is
   reported explicitly; the user can relink only after identity validation.

Preview caches may be discarded; referenced published versions may not be deleted
as ordinary cache cleanup. Moving/copying a project needs relative-path resolution,
dependency inspection and a collect-assets operation before portable delivery is
claimed. Export cancellation removes only job-owned temporary output. A successful
external file write is not reversible through document Undo; replay restores
recorded document/project outcomes without rerunning encoders or external effects.

## Source ownership and dependencies

This is a logical split, not an instruction to move current files or create empty
crates. Existing responsibilities remain in the
[structure guide](project-structure.md). Suggested new locations below are reserved
design names, not directories that already exist.

| Work package | Existing boundary to reuse | Proposed additions when implemented |
| --- | --- | --- |
| Workspace shell | `src/desktop/mod.rs`, native windows and `workspace.layout.apply` | `src/desktop/workspaces.rs` for named layout lifecycle; presentation-only registry adapters |
| Cover/layout | Current engine, canvas, layer arrangement and native plugins | A feature-owned `plugins/layout/` UI package; host text persistence/commands only after a text contract is accepted |
| Published assets | Session validation, PNG export and file publication conventions | `src/application/assets.rs` for source fences/manifests; bounded artifact storage outside document metadata |
| Video project | Shared action registry and application admission | `src/engine/video/` for versioned timeline data; `src/application/video.rs` for validation/history/jobs; `plugins/video/` for compiled UI |
| Media execution | Host worker/job boundary and optional capability discovery | An optional media backend for decoding/encoding, bounded messages and cancellation; no codec requirement for painting-only startup |

Pixel algorithms remain in native plugins. Host registration/dispatch remains in
`src/plugins/`; feature UI is compiled into the desktop, without egui across the
C ABI. Do not create a competing Session or history implementation to avoid
shared-file coordination. Future video history must be explicitly integrated
through host admission, not implemented privately by its UI plugin.

Long decode/export jobs must not block the painting Session or access its mutable
state from another thread. Workers return job ID, source identity/revision and
bounded results; stale/cancelled results cannot overwrite newer previews or jobs.
Specify cache, queue, frame and process-memory limits before implementation.
Measure painting-only startup, idle memory and contact costs against a named
baseline; this design makes no performance claim or codec dependency choice.

## Ordered implementation packages and acceptance

Every package needs a bounded assignment with exact file owners, base commit,
contracts and validation under the [parallel development protocol](../AGENTS.md#parallel-agent-development)
before parallel implementation. This document does not launch parallel tasks.

| Order | Package | Prerequisites | Acceptance gate |
| --- | --- | --- | --- |
| 0 | Preserve painting delivery | Current requirements | Continue exact-head CI repair/verification for Liquify, bound adjustments, groups/search and symmetry; keep their open gates visible |
| 1 | Named workspace shell | Existing docking, explicit source targeting rules | Switch between two useful layouts and restore after restart; preserve hidden/floating/plugin windows, pixels/history, drafts and preferences; migrate old layout losslessly |
| 2 | Static Sovia cover workflow | Shell plus separately accepted text contract | Make a real cover with editable title, save/reopen and export required sizes; verify font fallback, missing fonts, CJK/Cyrillic, alpha, Undo and GUI/headless parity |
| 3 | Versioned asset publication | Source identity/revision fences and PNG export | Publish/import/replace/Undo; handle stale sources, interrupted publication, missing media and project relocation without changing pinned versions |
| 4 | Minimal lyric-video project | Asset contract, explicit project targeting and versioned project/history specification | Load one visual, music and timed lyrics; seek deterministically; apply transform/opacity keyframes; save/reopen, Undo, cancel and export a pinned-input clip |
| 5 | Broader editing evaluation | One reviewed end-to-end Sovia production | Add multiple clips or audio features only for demonstrated work; record benefits and costs before expanding scope |

Workspace switching during an active pen contact must defer or reject cleanly;
it cannot finish the contact to bypass idle guards. Switching while a draft exists
preserves its source fence and Apply/Cancel lifecycle, with a visible way back to
it; it never applies implicitly. Shell commands are discoverable presentation
actions and explicitly unavailable headlessly. Document/layout/video editing
actions must expose their supported headless contracts through the shared registry.

Before package 4, specify integer/rational time bases, clip intervals, audio-master
clock, frame/lyric boundary rules, font resolution, alpha/color conversion and
decoder capability/version handling. Validate saved timing and independent
rendered-frame/audio-duration references, failure rollback, worker cancellation,
project compatibility and DLL/encoder-free outcome restoration. Input digests alone
do not promise byte-identical output from different encoder versions.

Package 1 does not expose an empty Video workspace as a supported feature.
Packages 2-4 are follow-up proposals, not accepted runtime capabilities or a new
autonomous goal. Keep implementation progress in the
[roadmap](../ROADMAP.md#workspace-and-sovia-production-boundaries) and delivered
changes in the [changelog](../CHANGELOG.md).
