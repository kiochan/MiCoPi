# Native raw-A8 affine

The independent `micopi_a8_affine_v1` export samples a borrowed raw A8 plane
through an affine matrix. It is an independent SDK/loader primitive used by
[owner and linked Free Transform](owner-affine.md). Ownership, selection policy,
locks, staging and document history remain host responsibilities; directly
calling this table does not bypass shared action admission.

## ABI and sampling

The current unreleased SDK **1.3.0** keeps ABI **1** and all existing tables
unchanged. `A8AffineApi` is a separate 16-byte x64 table: version/size header,
then one `sample` callback. `A8Affine` is 56 bytes: `u32 method` at offset 0,
`u32 fill` at 4 and `[f64; 6] matrix` at 8. The callback receives request,
immutable `CompositeMask`, source width/height and caller-owned mutable
`A8Pixels` output. It is synchronous and retains no pointer.

An optional `A8AffineApiExt` tail appends `bounds` at offset 16 for a 24-byte
x64 table. Read the header/size before accessing it. A complete 16-byte table
remains sample-capable; the optional bounds callback may be absent. Its signature
receives immutable `CompositeMask`, width, height, neutral byte and mutable
`TrimBounds`. It returns the exclusive support rectangle for raw samples unequal
to the supplied neutral value, or all zeros for empty support. Uniform neutral
has no support; a different uniform byte covers the full finite frame. The scan
allocates no full-frame storage and validates metadata/output aliases before
writing. Owner handles use neutral 255, independently of effective mask effects.

The matrix is the forward document-edge map `[a,b,tx,c,d,ty]`:

```text
destination_x = a * source_x + b * source_y + tx
destination_y = c * source_x + d * source_y + ty
```

Every coefficient must be finite; linear coefficients are bounded by ±256 and
translations by ±4194304. Absolute determinant must be at least `1e-8`.
The kernel validates/inverts once, then maps destination pixel centers back into
the source. `method=0` uses nearest sampling (`floor` of source edge coordinates).
`method=1` uses bilinear scalar sampling around source pixel centers, Q16 weights
on each axis and half-up aggregate rounding over `2^32`. No RGB, alpha
premultiplication, density or feather operation is performed.

`fill` is an explicit byte, 0–255, used for **each** out-of-frame tap. Bilinear
sampling can mix interior values with exterior fill. Uniform input still has a
finite frame and obeys the same border rule. Raster-mask consumers use neutral
reveal **255**, rather than the zero exterior used by pixel selections.

## Storage and validation

Source `CompositeMask` uses the existing dense/uniform convention. Uniform means
null data, zero length/stride and a byte constant; dense means nonnull data,
constant zero and complete checked row storage. Source and output sides are
1–4096; dense/output stride is at least width and at most 4096, with lengths
bounded to 4096² bytes. Output origin is zero.

Validate request/descriptor alignment, matrix/method/fill, dimensions, row
extents and complete spans before writing. Writable output must not overlap
source storage or request/descriptor metadata. Source bytes and stride padding
remain untouched. The kernel allocates no full-frame scratch. The host caches
the optional callback; `native[].samplers.raw_a8_affine` in registry inventory
reports provider capability, not document-operation availability. An absent export does
not disable unrelated plugin capabilities.
`native[].samplers.raw_a8_bounds` independently reports the optional bounds tail;
sample-only providers remain usable for consumers that do not require bounds.

The host wrapper borrows input and accepts a caller-owned output descriptor; it
does not allocate an output secretly. The owner-affine action preflights locks,
contacts, source storage, logical budgets and callbacks before staging checked
RGBA/A8 results. Each raw A8 output is at most 16 MiB at 4096² and counts toward
the shared 192 MiB aggregate. The host provides atomic grouped history,
selection/link policies, exact repeat scopes and verified outcomes. Primitive
capability alone does not imply that every document geometry path is supported.

## Validation status

Kernel/host coverage is authored for inverse-coordinate references, nearest and
Q16 bilinear borders, dense/uniform sources, strides/padding, malformed spans,
aliases, invalid matrices and missing/malformed optional tables. Validation is
pending CI; this guide claims no completed run, measured speedup, new screenshot
or tablet evidence. See [Native Plugins](native-plugins.md) and
[Development](development.md) for the existing ABI and CI-first workflow.
