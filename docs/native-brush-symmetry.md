# Native brush symmetry prerequisite

This increment implements an optional native painting capability. It is not yet
an available desktop/Session painting mode: host parameter schemas, capability
discovery, settings/presets, command controls, axis guides and end-to-end verified
recordings remain to be integrated. Existing hosts continue to configure old
prefixes and paint with symmetry disabled. Runtime/native verification is pending
CI; no physical tablet, native screenshot or latency result is claimed.

## Optional ABI

`TOOL_OPTION_SYMMETRY` / `MICOPI_TOOL_OPTION_SYMMETRY` is bit `0x20` in the existing
`micopi_tool_options_v1` feature/flag contract. Supporting clients configure the
complete `ToolOptionsSymmetry` / `MicopiToolOptionsSymmetry` before DOWN:

| Field | x64 offset | Contract |
| --- | ---: | --- |
| `base` | 0 | Unchanged complete 80-byte analytic tip prefix |
| `axes` | 80 | Bit 1 reflects X; bit 2 reflects Y; 0 disables |
| `anchor_x` | 84 | Finite canvas-width fraction, 0 through 1 |
| `anchor_y` | 88 | Finite canvas-height fraction, 0 through 1 |
| `reserved` | 92 | Zero |

The complete descriptor is 96 bytes. X reflects across the vertical line
`x = anchor_x * width`; Y reflects across `y = anchor_y * height`. Coordinates
are document pixel edges, not view/screen coordinates. Center defaults are
`(.5,.5)`. Reflection is `2 * anchor - position`, with clipping delegated to
the existing native raster bounds. A noncentral axis can put an arm outside the
canvas; it does not wrap or change the document frame.

The DLL rejects unknown axis bits, nonfinite/out-of-range anchors, reserved data,
requested incomplete tails and changes during a live contact before replacing
configuration. Without the flag, a complete symmetry tail must have default
axes/anchors. Real older allocations are read only through their declared size;
old option prefixes reset symmetry to off. Existing tool parameters, option
prefixes, host callbacks and symbols keep their layouts and IDs.

## One filtered contact, multiple native stamps

Pressure, stabilization, release completion, interpolation and spacing run once
on the original contact. Each spaced dab emits at most four stamps, in original,
X, Y, XY order. X/Y reflect the prepared tip orientation; XY retains it because
round and square analytic tips are centrally symmetric. No trigonometry,
registration, JSON, file I/O, instance creation or image flipping is introduced
per pointer MOVE. Stamp generation uses a fixed stack array, and disabled
symmetry calls the original raster routine through a direct branch.

Identical positions and equivalent cached orientations are deposited once.
Circles, cardinal tips and diagonal full-square tips on a symmetry axis avoid
duplicate deposition. An oblique nonsquare tip centered on an axis can have two
different reflected orientations; these are two intentional stamps. Ordinary
Build Up compounds overlapping stamps. Capped stroke opacity shares one sparse
influence cache and the same immutable-original loans across all arms, so overlap
cannot evade the existing stroke limit. Selection coverage is sampled at each
destination; the selection plane is not reflected. Alpha locking and raw A8
Brush/Eraser mixing retain their existing rules.

Each stamp requests its own bounded region. There is no full-span temporary
raster joining distant arms. The existing sparse capped cache may still grow
with the number of touched tiles; this is not an allocation-free capped stroke
claim. A failed loan stops further arms and ends/releases the native contact;
the host remains responsible for transaction rollback of any earlier writes.

## Verification and remaining integration

Rust SDK and C11/C++17 fixtures assert all offsets, size, feature independence
and unchanged prefixes. Native cases cover malformed/truncated configurations,
old-allocation reset, live-contact rejection, custom anchors, clipping, tip parity,
axis deduplication, exact four-point RGBA/A8 bytes, asymmetric fractional selection,
pressure/stabilization/Flow, Brush/Eraser, overlap caps, alpha locks and failed
borrow cleanup. Pressure cases compare reflected ordinary-contact surfaces in
every stored channel with a one-byte tolerance for reflected f32 coordinate
rounding; the simple point reference is byte exact. Bounded loan counters cover
four distant dabs without a full-span loan. These cases await CI.

Before exposing painting symmetry, integrate capability-gated scalar intent in
`brush.set`, searchable validated defaults and presets, public state, action/
workflow/MCP/headless execution and contact-frozen native options. Preserve old
settings, recordings and default hashes. Add one-stroke Undo/rollback/project/
DLL-free outcome replay and all four view-mirror combinations, plus localized
controls and axis/tip feedback. Measure disabled-path cost and enabled dirty
regions, GPU uploads, journal/history bytes, sparse cache peaks and contact
latency. Separate native loans do not prove the host avoids a large combined
dirty upload. Actual devices, native screenshots and multi-monitor/DPI behavior
require their own evidence.
