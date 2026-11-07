# Analytic Brush and Eraser Tips

Brush and Eraser contacts support round/elliptical and square/rectangular tips
with a fixed document-space angle. The Brush panel, searchable Painting settings,
presets, action nodes, MCP and headless clients share the same scalar parameters.
This source increment is awaiting CI validation. Sampled tips, textures, tilt,
barrel rotation and directional/random tip dynamics remain planned.

## Shape, size and angle

| Parameter | Values | Default |
| --- | --- | --- |
| `tip_shape` | `round` or `square` | `round` |
| `tip_roundness` | 0.25–1; minor/major diameter ratio | 1 |
| `tip_angle` | -180–180 degrees, clockwise in document coordinates | 0 |

The existing radius defines the major semiaxis. A round tip with a ratio below
1 is an ellipse; a square tip with a ratio below 1 is a rectangle. Positive
angles rotate the major axis clockwise, with document X pointing right and Y
pointing down. This angle stays fixed throughout a contact. Pressure size scales
both axes, and pressure opacity, hardness and stabilization keep their existing
contracts. [Flow and stroke opacity](stroke-opacity.md) apply to both shapes on
RGBA and raw A8 painting targets. Fill and gradient actions keep their own shape
and opacity semantics.

The canvas outline and Brush-panel preview show the nominal rotated tip contour.
The outline follows the existing size/pressure feedback; it is not a prediction
of antialiased coverage, accumulated Flow, selection clipping or feather effects.
The circular default retains the existing circle outline. Noncircular contours
use cached bounded geometry, with no change to the native tool ABI's UI boundary.

## Shared commands and defaults

```json
{"action":"brush.set","args":{"tip_shape":"square","tip_roundness":0.5,"tip_angle":30}}
{"action":"brush.angle.adjust","args":{"delta_degrees":15}}
```

`brush.angle.adjust` accepts a finite delta from -180 to 180 and wraps the result
to `[-180,180)`. Its result contains resolved `parameters` and the published
Session `revision`. It changes painting intent rather than document pixels and
adds no canvas Undo record. Idle document-atomic failure restores the parameters;
the existing revision and outcome-recording contracts still apply.

These searchable, bindable macros are deliberately unbound by default:

| Action | Clockwise delta |
| --- | --- |
| `brush.angle.clockwise` | +1 degree |
| `brush.angle.counterclockwise` | -1 degree |
| `brush.angle.clockwise_coarse` | +15 degrees |
| `brush.angle.counterclockwise_coarse` | -15 degrees |

The angle action validates the selected Brush/Eraser provider and new parameters
before intentionally finishing an accepted contact through the checked pointer
UP boundary. A missing tip capability rejects without ending that contact.
`brush.set` retains its existing tool-parameter admission semantics. Setting an
angle is not an instruction to alter an already frozen native contact in place.

Searchable startup defaults use `painting.tip_shape`, `painting.tip_roundness`
and `painting.tip_angle` in `settings.json`. Public `canvas.state.parameters` and
`painting.tool.parameters` always report all three resolved values. Persistence
omits Round/1/0 defaults, retaining older parameter bytes and recording hashes.
Explicit nondefaults require a reader that understands these fields.

[User brush presets](brush-presets.md) retain twelve required scalar/dynamics
fields and add these three optional geometry fields with Round/1/0 defaults.
Applying a legacy twelve-field record resets current geometry to those defaults;
nondefault geometry survives save, update, export, import and Apply. Foreground
and background swatches remain excluded. Built-in round presets reset geometry;
base Brush/Eraser tools retain current geometry. Strict older preset readers may
reject records containing the new fields.

## Native coverage and compatibility

The optional `micopi_tool_options_v1` feature bit
`TOOL_OPTION_TIP_GEOMETRY` (`0x10`) negotiates an 80-byte `ToolOptionsTip` tail
without changing existing v1 tables or `ToolParameters`. Its unchanged 64-byte
`ToolOptionsStroke` prefix is followed by shape at offset 64, roundness at 68,
angle at 72 and a zero reserved word at 76. Shape IDs are Round 0 and Square 1.
Native code validates the complete requested tail, finite ranges, flags and
reserved values before freezing the contact. Prefix-only options reset geometry
to Round/1/0. See [Native Plugins](native-plugins.md#analytic-tip-geometry).

Any nondefault geometry field requires the capability, including a nonzero
angle on a circular tip. Unsupported contacts reject before DOWN exposes pixels
or materializes a uniform A8 plane. Older DLLs continue to support default round
contacts; older hosts continue through the old prefixes. Discovery reports
`painting.tool.capabilities.tip_geometry` for the selected painting tool and
`plugins.registry.native[].tool_options.tip_geometry` for each provider.

Round/1 retains the previous radial raster arithmetic exactly, regardless of
angle. Nondefault tips use physical-distance antialiasing and normalized hardness
falloff: the rectangle uses an exact signed distance, while the ellipse uses a
first-order normal-distance estimate. This is a defined native approximation,
not an exact closest-point solve or a Photoshop pixel-parity claim. Selection,
alpha lock, grayscale A8 targets and per-stroke origin recomposition retain
their existing blending rules. Nondefault spacing uses the minor diameter and
the existing half-pixel minimum interval.

## Bounds and verification status

The DLL caches angle trigonometry before DOWN and resolves geometry per dab.
Conservative rotated bounds enclose antialiased coverage; row-span culling skips
pixels outside a wider safe enclosure. Painting remains scoped to host region
loans, dirty rectangles and tiled Undo. No sampled asset, new dependency,
per-MOVE JSON, file access, registry lookup or whole-frame geometry buffer is
introduced. Stroke influence keeps its existing sparse coverage bounds.

Authored native, Session and actual-control/cursor regressions cover independent
hard-rectangle stencils, exact quarter turns, pressure/stabilization, RGBA and
A8 blending, capability rejection, presets, rollback, chunk continuity and
outcome replay. The transport smoke checks five 7x7 cases across JSONL/MCP,
projects, Undo/Redo and playback without DLLs:

```powershell
python scripts/smoke-brush-tips.py --cli <built-cli>
```

CI also selects the ignored Release `bounded_tip_cost` probe. It uses synthetic
64x64 RGBA/A8 frames, 41 samples per contact and nine measured contacts after a
warm-up for four shapes in Dab/Stroke modes. It reports timing, candidate/positive
pixel visits, region-loan work and retained coverage separately. Its scope
excludes frame setup, Undo capture, Session validation, journaling, GUI and tablet
input. Results remain pending; no latency, throughput, screenshot or physical
device compatibility claim follows from authored tests. See
[Development](development.md) for actual verification records and resource limits.

## Ownership

`src/application/brush_tip.rs` owns the scalar enum/defaults, while
`src/plugins/brush_tip.rs` owns the angle action's shared admission. The SDK and
host negotiate the optional tail; `plugins/painting/src/tip.rs` owns geometry and
native raster coverage. `plugins/brush/tip_geometry.rs` owns cached presentation
contours, and Brush/Canvas feature UI adapters present those contours. Session,
native and UI regression files plus `scripts/smoke-brush-tips.py` exercise these
boundaries without introducing a separate editing path.
