# Brush opacity and Flow

The Brush panel separates **Opacity** from **Flow**. Flow controls how quickly
paint builds up while drawing. Enable **Limit opacity per stroke** to keep each
continuous stroke within its opacity limit; lift the pen to add another coat.
The **Glazing Marker** preset selects this behavior with 40% Opacity and 25% Flow.

The default remains **Build Up** (`dab`) with Flow at 100%. It uses the existing
per-dab arithmetic exactly. At lower Flow, each dab deposits less paint. In
**Per Stroke** (`stroke`) mode, Flow controls accumulation independently of the
whole-contact opacity limit. Revisiting a point within one stroke approaches
that limit rather than repeatedly blending fresh paint over the current result.

The limit applies to **paint influence**, not final pixel alpha. A 40% stroke
over an opaque layer leaves it opaque; it mixes the brush color into that
original color. For a fully covered transparent pixel with 100% Flow, one 40%
contact produces approximately 40% alpha, while two lifted contacts produce
approximately 64%. Byte rounding still applies. This is MiCoPi's defined
arithmetic, not a claim of exact Photoshop brush behavior.

## Controls and public contract

| Field | Values | Default |
| --- | --- | --- |
| `opacity` | 0–1; per-dab strength or the stroke influence limit | 1 |
| `flow` | 0–1; strength deposited by each dab | 1 |
| `opacity_mode` | `dab` or `stroke` | `dab` |

These are existing `brush.set` parameters. They apply to typed Brush/Eraser
contacts on the active raster or selected raster painting mask. Fill, gradients
and `layer.fill_color` keep their separate opacity contracts; Flow and stroke
mode do not turn those actions into brush contacts. Painting remains single
target even when raster layers are linked for movement.

```json
{"action":"brush.set","args":{"opacity":0.4,"flow":0.25,"opacity_mode":"stroke"}}
{"action":"tool.select","args":{"tool":"glaze_marker"}}
```

Use `painting.flow` and `painting.opacity_mode` in searchable Settings to preview,
cancel, reset or save startup defaults. The enum is displayed as **Build Up** /
**Per Stroke**. Brush-panel changes configure the current Session. Existing
Pencil, Ink, Brush, Soft, Marker and Eraser presets explicitly restore Flow 1 and
`dab`; base tools and third-party partial presets retain unspecified parameters.
Preset ribbon thumbnails are schematic illustrations, not sampled brush output.

`canvas.state.parameters` and the `painting.tool` public resource explicitly
report all parameter defaults. Its resource schema describes the parameters and
`capabilities.flow` / `capabilities.stroke_opacity` for the selected native tool.
Registry inventory also reports these capabilities per native plugin. Discovery
is cold; it is not repeated for each pointer sample. `brush.set` retains its
compact parameter-result convention.

The valid 1% spacing boundary is serialized as decimal `0.01`, including public
state, settings and recording. Only that `f32` boundary is canonicalized; other
wire values and default omission stay unchanged. Raw action/settings values
below `0.01` still reject before conversion.

Registered actions and schema-derived nodes share these parameters through
desktop command forms, JSONL, MCP and headless Sessions. Editing a workflow never
executes a stroke. See the [runnable example](../examples/automation/stroke-opacity.jsonl).

## Pressure, selection and live drawing

Color, mode, Flow, selection and tool options freeze at DOWN. Pressure size,
pressure opacity, gamma, minimum size and the minimum-opacity floor retain their
existing meanings. Pressure zero deposits no paint. Mouse input ignores pen
pressure. Changing parameters or selecting a preset finishes the previous
contact before applying the new values.

Stroke mode tracks a monotone influence at each touched pixel. Higher pressure
can raise the local limit; lowering pressure does not remove influence already
deposited. Fractional pixel-selection coverage is applied once to the final
stroke influence, rather than once per overlapping deposit. Dab mode retains
its existing per-dab selection multiplication and accumulation.

RGBA painting uses source-over against the contact's original pixels. Erasing
reduces their original alpha. Alpha-locked painting uses source-atop and retains
the original alpha and zero-alpha storage; locked erasing remains rejected.
Raw A8 painting mixes toward the frozen brush grayscale, and A8 erasing reveals
toward 255. Density and feather remain separate non-destructive mask effects.

Each dirty region is recomposed live. Stroke mode does not hide the result until
UP. Cancellation retains the existing native finish/stabilized-tail behavior;
it is not a new rollback gesture. Errors still roll back the complete native
contact through Session's existing checkpoint.

For precise numeric references, let `S` be the stored `u16` influence and use
half-up rounding:

```text
cap    = round(65535 * pressure_adjusted_opacity)
amount = round(65535 * flow * radial_edge * softness)
S     += round(max(cap - S, 0) * amount / 65535)
alpha  = round(S * selection_byte / 65535)
```

Unrestricted selection uses byte 255. The existing round brush geometry,
interpolation, spacing and stabilized tail supply the dabs. The final byte feeds
the established RGBA/A8 blending equations against immutable original samples.
The cursor remains a round size/pressure outline; this feature adds no textured
or sampled brush tip.

## Storage, ABI and compatibility

SDK **1.3.0** adds optional tails while keeping ABI **1**, the v1 tool tables and
`ToolParameters` unchanged. `TOOL_OPTION_STROKE_OPACITY` (`0x8`) negotiates the
Flow tail; its configured flag selects Stroke mode. Nondefault Flow or Stroke
with an unsupported DLL rejects at DOWN before exposing mutable pixels or
materializing a uniform A8 mask. Default settings continue through older tables.
See [the native extension](native-plugins.md#stroke-opacity-and-flow-sdk-13).

The host lends immutable originals from the existing **64×64 Undo tiles** beside
the current writable region. It prepares the region and splits callbacks at
tile intersections. Original and current buffers are disjoint; these scoped
views allocate and copy nothing. Plugins cannot retain either pointer, reenter
callbacks, cross targets or bypass pixel/alpha locks. No duplicate original
image is retained for the feature. Existing first-touch Undo capture still has
its normal allocation and copying cost.

The painting DLL owns sparse 64×64 `u16` coverage tiles. At most 4096 active tiles
contain **32 MiB** of coverage samples for a 4096² canvas. After UP/Cancel it
retains at most 64 tiles (**512 KiB**) per cached native tool state, plus bounded
index/vector metadata. Reused tiles are cleared when touched for a new contact;
there is no full-canvas clear on each MOVE. These are storage bounds, not a
measurement of process memory or input latency.

Default Flow 1 / Dab mode enters the previous raster functions without allocating
coverage tiles. Typed MOVE retains cached instances, options and target leases;
it adds no JSON, registry discovery, file I/O, extra configuration call or
whole-image snapshot. Stroke mode can allocate coverage tiles on first touch.

Default Flow and mode are omitted from persisted `Parameters`, preserving old
parameter/recording bytes and hashes. Explicit nondefaults are saved in replay
metadata and require a reader that understands those fields. Undo and outcome
replay retain the existing document contract. Sample-chunk recording boundaries
do not reset the contact's original tiles or accumulated influence; verified
replay restores recorded surfaces without executing the painting DLL.

## Verification status and measurement scope

Regression coverage is authored for independent RGBA/A8 influence references,
pressure ramps/floors, selection-once rounding, alpha lock, old prefixes,
original/current spans, rollback, chunk continuity, presets, searchable Settings
and English/CJK layouts. The transport smoke uses tiny 7×5 fixtures through
JSONL/MCP and optional owned-GUI attachment, projects and DLL-free replay.
Desktop/headless/native regressions passed CI for `927702b`; corrected transport
validation remains pending as explained below. No new screenshots or
physical-tablet compatibility are claimed.

```powershell
python scripts/smoke-stroke-opacity.py --cli <built-cli> --gui <built-desktop>
```

Stage matching hosts and DLLs. `--skip-gui` runs the transport subset. The authored
native benchmark uses 64×64 synthetic RGBA/A8 fixtures and 41 samples per contact,
reporting timing and coverage reuse independently from full Session work. Its
native timing excludes fixture setup, Undo capture, Session validation, journaling,
GUI and tablet input. The recorded probe below has that limited scope. Follow
[Development](development.md) and the repository's local resource limits.

### Recorded synthetic contact probe

[CI for `927702b`](https://github.com/kiochan/MiCoPi/actions/runs/42381092545)
ran the Release native probe on its hosted Windows x64 worker:

| Plane | Mode / Flow | Median per contact | p95 per contact | Retained coverage samples |
| --- | --- | --- | --- | --- |
| RGBA | Dab / 1 | 121.3 microseconds | 132.0 microseconds | 0 bytes |
| RGBA | Stroke / 0.25 | 158.1 microseconds | 169.1 microseconds | 8192 bytes |
| A8 | Dab / 1 | 65.7 microseconds | 76.1 microseconds | 0 bytes |
| A8 | Stroke / 0.25 | 95.5 microseconds | 107.0 microseconds | 8192 bytes |

Each row uses Opacity 0.4, radius 6, stabilizer 0.35 and 41 synthetic samples on
64x64 pixels: one discarded warm-up plus 21 measured contacts. Each Stroke case
allocated one coverage tile and reused it; indexing/vector metadata is separate.
Different Flow and compositing semantics make these separate workloads, not a
controlled overhead comparison. The exclusions above still apply; these are
native contact timings, not tablet latency or a universal performance promise.
The same run passed desktop/headless/native regressions, but its stroke transport
smoke stopped on an incorrectly encoded unmasked project fixture. Full corrected
transport validation remains pending separately from this measured probe.
