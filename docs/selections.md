# Selections

Selections belong to the document, not the desktop tool. The current increment provides schema-described rectangle, ellipse and polygon geometry, combination modes, Select All/Deselect/Reselect/Invert, native painting/action clipping, persistence, undo and verified replay. Mouse/pen marquee and lasso gestures, feathering, Magic Wand and clipboard operations remain planned in [the editing checklist](photoshop-essentials.md).

![Actual workbench with a command-generated ellipse selection](screenshots/selection.png)

*A development build captured by the localization smoke check. The outline is a presentation overlay; the generated checkerboard and pressure strokes remain unchanged.*

## Commands and automation

| Action | Arguments | Default shortcut |
| --- | --- | --- |
| `selection.shape` | `kind`: `rectangle`, `ellipse` or `polygon`; optional `mode` | Unbound; parameter form in command search |
| `selection.all` | None | Ctrl+A |
| `selection.deselect` | None | Ctrl+D |
| `selection.reselect` | None | Ctrl+Shift+D |
| `selection.invert` | None | Ctrl+Shift+I |
| `selection.read` | None; read-only current/previous encoded masks | Unbound |

Rectangle/ellipse geometry requires positive `width` and `height`, with `x` and `y` defaulting to zero. Polygon geometry requires 3–4096 `[x,y]` pairs; it closes the final edge automatically and uses an even-odd fill rule. Coordinates are document pixels, bounded to ±16384; dimensions are at most 16384. Geometry samples pixel centers and currently produces binary coverage without antialiasing. Out-of-canvas portions are clipped. A completely outside shape creates an active empty selection.

`mode` defaults to `replace`. `add` uses maximum coverage, `subtract` uses saturating subtraction and `intersect` uses minimum coverage. An absent current mask is treated as empty for these combinations; use `selection.all` before subtracting from the whole canvas. Invert without a current mask selects the whole canvas. Deselect removes the editing restriction while retaining the last active mask for Reselect. Reselect without a previous mask fails. Commands that replace an active mask retain that mask as the previous selection.

The Edit menu and command search expose the four shortcuts; all bindings remain configurable. A cached static outline overlays the canvas without changing artwork or export pixels. It shows the 50% coverage boundary, with a 1024-pixel longest-edge preview limit. It is not an animated marching-ants or full-resolution mask editor. `canvas.state.selection` reports `active` and `can_reselect` without copying mask data; `selection.read` explicitly requests encoded current/previous masks. Selection actions become workflow nodes and are available through MCP, JSONL and GUI attachment. Read-only inspection does not change revision or finish a stroke.

```json
{"action":"selection.shape","args":{"kind":"rectangle","x":20,"y":30,"width":160,"height":100}}
{"action":"layer.fill_color","args":{"color":"#8066dd"}}
{"action":"selection.invert"}
{"action":"native.invert"}
{"action":"selection.deselect"}
```

## Editing and compatibility

No current mask means unrestricted editing. An all-zero current mask blocks pixel changes. Brush and eraser freeze borrowed coverage at contact start and multiply dab opacity by that coverage, preserving pressure, stabilization and spacing. The host retains the immutable mask until the cached tool can no longer read it. No mask serialization, compression, copying or registry lookup is added to MOVE.

Native `edit_region` actions, including fill, gradient and direct filters, produce staged pixels. Before commit, the painting DLL mixes them with the original pixels according to selection coverage in premultiplied-alpha space. Coverage 0 preserves exact original storage; coverage 255 keeps the action result. Fractional coverage retains original hidden RGB when both weighted alphas are zero. Clear uses the same mixing kernel. Transparency lock still applies. Non-destructive image graph filters continue to affect their connected full layer; mask-aware composition ports are a later increment. Whole-layer structural operations such as merge, duplication and deletion remain whole-layer operations.

Connected fill traverses only positive selection coverage using four-neighbor connectivity. A zero-coverage seed is a no-op, including global color matching. Disconnected selected islands cannot share a connected fill through unselected source pixels. Expansion and feathering stay within the seed's selection component; they cannot jump a small excluded gap into another island. Global color matching can affect matching pixels in all selected islands when the seed is selected. Fractional selection coverage is applied once at output blending, not during traversal. An optional immutable action-entry snapshot lends coverage to the native fill kernel; no JSON mask decoding or byte copy is required.

Pixel kernels and geometry stay in the independent painting DLL. Optional size-checked ABI extensions preserve older tool prefixes; an old DLL lacking selection capabilities rejects selected painting or clipping instead of silently ignoring the mask. Restart after rebuilding/staging DLLs. See [Native plugins](native-plugins.md).

Selection changes create composition undo entries and participate in atomic document pipelines. Failed geometry, stale revisions, unsupported DLL capabilities and failed clipping do not publish a partial selection/pixel result. Project metadata and outcome journals retain both masks. Replay restores coverage and pixels without invoking geometry or drawing kernels. Existing no-selection metadata remains byte-compatible with legacy journal hashes. Canvas resize/crop reframes both current and previous masks.

Storage bounds remain 4096 pixels per side, 128 KiB compressed per mask and 1 MiB total project metadata. Construction caches the compressed encoding; snapshots share coverage allocations. Invalid dimensions, incomplete streams, trailing data and decoded lengths are rejected. Highly irregular masks can exceed the compressed limit and fail before selection replacement. See [Architecture](architecture.md).
