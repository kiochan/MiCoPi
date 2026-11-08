# Canvas view mirroring

Search **Flip Canvas View Horizontally** or **Flip Canvas View Vertically** to
toggle either axis of the current desktop Canvas, independently or together.
The registered `view.mirror_horizontal` and `view.mirror_vertical` commands are
searchable, bindable and available as action nodes or through attached MCP/JSONL.
No default shortcut replaces the existing Photoshop-style tool keys. The zoom strip labels the
mirrored view. This source increment and its runtime/UI tests await CI; native
window captures and physical-device checks remain pending.

This is a display transform. It does not flip layer pixels, masks or source
composition, create a document Undo entry, or change exported PNG/project raster
content. Navigator and automation snapshots retain the committed document's
orientation; Navigator clicks and its viewport box account for the mirrored
Canvas. Arbitrary view rotation remains planned; two enabled mirrors produce
a half-turn display without changing the document.

## Public state and commands

`canvas.state` exposes the Session view. `view.mirror_horizontal` toggles
`view.mirror_x`; `view.mirror_vertical` toggles `view.mirror_y`. Each returns a
complete View result with its toggled boolean explicitly present, even when false.
The other flag is included when true. False flags remain omitted in
legacy-compatible state/record metadata. `view.set` also accepts explicit
`mirror_x` and `mirror_y` booleans alongside its existing zoom/pan/fit fields.
Mirror changes require `runtime=desktop` and report `ui_unavailable` in a headless Session. The original headless zoom/pan/fit
contract is unchanged when the new field is absent. A live native contact rejects
before any view field changes, including a patch that also contains zoom.

These are presentation-state changes, not document-atomic edits. They retain the
existing view-operation Session revision and recording behavior, so revision-bound
drafts may need an explicit refresh. The document Undo stack stays unchanged.
Source document coordinates and explicit edit-command offsets keep their normal
meaning; only desktop spatial input is mapped through the mirrored view.

Both mirror flags default to false and are omitted when false in serialized View
metadata. Existing view records retain their exact fields and remain readable;
new recordings may retain the enabled view flags in their metadata. Outcome replay
still restores original document surfaces, not a permanently flipped raster.
Mirror is Session-local view state, not a painting default or a pixel filter.

## Shared coordinate mapping

Canvas input maps screen points back through reflection on the enabled axes before
painting, picking, selection and edit gestures. The inverse adapter maps document
points/rectangles for gradient guides, selection paths, crop, Move and Free
Transform outlines. Rectangles normalize the reflected corner order. Zoom keeps
the document point under its screen anchor. Native floating Canvas viewports use
their existing DPI coordinate path; no new physical-pixel coordinate convention
is introduced.

Committed, direct-color-preview and transform textures use reflected UVs. Selection
and transform-mask textures share the same orientation. Nominal rotated elliptical
and rectangular brush contours negate their angle for one mirrored axis and
retain it for two, preserving pressure size and the native brush's document-space angle. The new rendering path creates no
mirrored raster buffer; existing history/recording costs still apply. Typed native
pressure/stabilization, dirty uploads, plugin instances and pixel algorithms are
unchanged; no JSON or registry lookup is added to pointer MOVE.

## Verification and remaining gates

Session cases cover desktop/headless behavior, reversible toggles, unchanged raw
pixels/composition/PNG/history, contact rejection without partial view edits,
continued pressure input, verified replay and legacy View serialization. Desktop
cases cover forward/inverse points, reflected rectangles, zoom anchors, actual egui
image UVs, rotated cursor contours and equal document pixels from mirrored versus
ordinary synthetic pen strokes with pressure/stabilization. Four-orientation
cases cover both independent toggles, combined output schemas, legacy omission,
normalized rectangles, off-canvas points, zoom anchoring and Navigator clicks.

The optional localization capture script includes horizontal, vertical and combined
Canvas mirrors in four languages and checks that committed PNG output stays unchanged. It has not run for
this increment and is not part of standard CI. Runtime regressions, native capture
review, actual multi-monitor/DPI behavior, driver compatibility and end-to-end costs
remain pending. Synthetic pen packets do not prove physical tablet compatibility.

CI for `4d0e7c6` passed the horizontal Session guards/serialization and Canvas
coordinate/UV/zoom/cursor/synthetic-pressure cases within 1,250 successful desktop
tests (nine ignored). Its Navigator test failed while loading the Material icon
font, before its click assertions. The fixture now installs the production font
set before rendering; all coordinate assertions remain. The vertical/combined
extension and this fixture repair require fresh exact-head CI; the earlier run
stopped before native/headless/transport stages and is not a full passing baseline.
