# Native HSL composition prerequisite

This is the independent native prerequisite for non-destructive Hue/Saturation.
It is implemented in source with SDK/native/host fixtures, pending CI. Editable
HSL image nodes, adjustment-layer intent, project schemas, public document actions
and dedicated reversible previews are **not implemented by this increment**.
The existing [direct HSL command](hue-saturation.md) remains the document-editing path.

## Independent capability

Optional `micopi_hsl_v1` returns a complete 24-byte `HslApi`: ABI version and size
at offsets 0/4, `render` at 8 and `histogram` at 16. Both callbacks are mandatory.
A missing symbol remains compatible. Advertised null, short, incompatible or
missing-callback tables reject startup; larger compatible tables preserve their
known prefix. Startup caches both function pointers. Plugin inventory reports
`filters.hsl_plan` and `analysis.histogram_hsl`; this describes native capability,
not editable HSL node availability.

The callbacks borrow at most 64 fixed-stride `HslParameters` records. Each record
is 24 bytes: three C `double` values for hue degrees at 0, saturation percentage
at 8 and lightness percentage at 16. Values are finite and bounded respectively
to [-180,180], [-100,100] and [-100,100]. Record layout is fixed by API version;
it has no per-record extension header or pointers.

Prepared instruction kind **11** means HSL: `a` is an earlier image slot, `b=0`,
`mode` indexes the parameter records and `amount=1`. Coverage is not an image.
Every instruction and every parameter record, including unused declarations,
validates before any output write. Existing compositor, Levels, Curves and
histogram entry points still reject kind 11; callers must negotiate the new API.
No existing table prefix, blend ID, filter ID or arithmetic is changed.

## Shared math and bounded execution

`plugins/native-paint/src/hsl_math.rs` contains pure scalar HSL math compiled
independently into the painting and compositor DLLs. The JSON adapter stays in
the painting package. No host types, JSON parser or new dependency enters the
compositor. Direct and composited HSL use the same binary64 parameters, rounding,
achromatic handling and alpha-preserving operation. Existing source composition
still canonicalizes alpha-zero pixels before effects; direct editing preserves
the raw layer's hidden RGB bytes.

Rendering accepts immutable RGBA/A8 loans, master/channel tables and caller-owned
ROI output. It reuses the validated image/coverage evaluator with one HSL operation
per referenced pixel and retains row padding, masks, Mix, Levels and Curves order.
Parameter preparation uses a fixed 64-entry scalar stack array. Output may not
overlap parameters, LUTs, instruction metadata, descriptors or image/mask loans.
Failed validation leaves output unchanged. No image or row buffer is allocated.

The histogram callback uses the same prepared evaluator and bounded centered grid
as [plan histograms](histogram-plan.md): at most budget + 1 pixel evaluations,
including one private validation point. Parameter/result overlap rejects before
publication. Counts preserve alpha-zero exclusion and the explicit RGB aggregate.
No pointer is retained and no document operation or typed contact invokes discovery.
These bounds are not measured latency or process peak memory.

## Verification and next integration

Rust/C11/C++17 SDK fixtures assert record/table offsets. Native references cover
primary rotations, independent grayscale/half-turn math, alpha, HSL mixed with
A8/master/RGB tables, render/histogram agreement, unused invalid parameters,
instruction fields, aliases, capacity/null cases, cropped padded ROI output and
legacy rejection. Host fixtures cover complete/malformed/future tables, cached
live-DLL callbacks and missing capability on another DLL. Runtime checks remain
pending CI; no local heavy suite or physical-device test was run.

Next, add typed source/adjustment intent and schemas; capability admission for
all declared branches; stable parameter slots in prepared plans; atomic
history/project/outcome integration; shared histogram projection and UI controls.
Keep the [feature checklist](photoshop-essentials.md) unchecked until those pieces,
failure/replay/transport references, reviewed captures and measured costs pass.
