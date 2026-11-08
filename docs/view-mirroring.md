# Horizontal canvas view mirroring

Search **Flip Canvas View Horizontally** to toggle the current desktop Canvas.
The registered `view.mirror_horizontal` command is searchable, bindable and
available as an action node or through attached MCP/JSONL. No default shortcut
replaces the existing Photoshop-style tool keys. The zoom strip labels the
mirrored view. This source increment and its runtime/UI tests await CI; native
window captures and physical-device checks remain pending.

This is a display transform. It does not flip layer pixels, masks or source
composition, create a document Undo entry, or change exported PNG/project raster
content. Navigator and automation snapshots retain the committed document's
orientation; Navigator clicks and its viewport box account for the mirrored
Canvas. View rotation and vertical mirroring remain planned.

## Public state and commands

`canvas.state` exposes the Session view. `view.mirror_horizontal` toggles
`view.mirror_x` and returns a complete View result including the explicit boolean;
false remains omitted in legacy-compatible state/record metadata. `view.set` also accepts an explicit `mirror_x` boolean alongside
its existing zoom/pan/fit fields. Mirror changes require `runtime=desktop` and
report `ui_unavailable` in a headless Session. The original headless zoom/pan/fit
contract is unchanged when the new field is absent. A live native contact rejects
before any view field changes, including a patch that also contains zoom.

These are presentation-state changes, not document-atomic edits. They retain the
existing view-operation Session revision and recording behavior, so revision-bound
drafts may need an explicit refresh. The document Undo stack stays unchanged.
Source document coordinates and explicit edit-command offsets keep their normal
meaning; only desktop spatial input is mapped through the mirrored view.

The mirror flag defaults to false and is omitted when false in serialized View
metadata. Existing view records retain their exact fields and remain readable;
new recordings may retain the enabled view flag in their metadata. Outcome replay
still restores original document surfaces, not a permanently flipped raster.
Mirror is Session-local view state, not a painting default or a pixel filter.

## Shared coordinate mapping

Canvas input maps screen points back through horizontal reflection before
painting, picking, selection and edit gestures. The inverse adapter maps document
points/rectangles for gradient guides, selection paths, crop, Move and Free
Transform outlines. Rectangles normalize the reflected corner order. Zoom keeps
the document point under its screen anchor. Native floating Canvas viewports use
their existing DPI coordinate path; no new physical-pixel coordinate convention
is introduced.

Committed, direct-color-preview and transform textures use reflected UVs. Selection
and transform-mask textures share the same orientation. Nominal rotated elliptical
and rectangular brush contours use the reflected angle, preserving pressure size
and the native brush's document-space angle. The new rendering path creates no
mirrored raster buffer; existing history/recording costs still apply. Typed native
pressure/stabilization, dirty uploads, plugin instances and pixel algorithms are
unchanged; no JSON or registry lookup is added to pointer MOVE.

## Verification and remaining gates

Session cases cover desktop/headless behavior, reversible toggles, unchanged raw
pixels/composition/PNG/history, contact rejection without partial view edits,
continued pressure input, verified replay and legacy View serialization. Desktop
cases cover forward/inverse points, reflected rectangles, zoom anchors, actual egui
image UVs, rotated cursor contours and equal document pixels from mirrored versus
ordinary synthetic pen strokes with pressure/stabilization.

The optional localization capture script includes the mirrored Canvas in four
languages and checks that committed PNG output stays unchanged. It has not run for
this increment and is not part of standard CI. Runtime regressions, native capture
review, actual multi-monitor/DPI behavior, driver compatibility and end-to-end costs
remain pending. Synthetic pen packets do not prove physical tablet compatibility.
