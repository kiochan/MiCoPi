# Channel Levels native foundation

This increment adds a negotiated **native prerequisite**, not a new document
operation. Existing Levels source nodes and adjustment layers still use their
five scalar parameters. Per-channel document intent, schemas, controls,
Undo/project integration and verified outcomes remain planned.

## Optional ABI

The bundled `micopi_levels_v1` advertises a complete 40-byte `LevelsApiChannels`:
the unchanged 24-byte scalar preparation/rendering prefix, `prepare_channels`
at offset 24 and `render_channels` at offset 32. Startup negotiation caches both
callbacks with the DLL. Inventory exposes `native[].filters.levels_channels`.
Old 16-byte preparation-only and 24-byte rendering providers remain valid.
Partial channel tails or missing advertised callbacks reject loading; larger
tables retain known-prefix semantics.

`LevelsChannelsParameters` is 136 bytes: version/size at 0/4, then master, red,
green and blue `LevelsParameters` at 8/40/72/104. Each fixed inline range has ABI
version 1 and size exactly 32; extending it requires a new containing layout.
The outer record permits a larger known-prefix size. The host normalizes headers
and validates all byte endpoints, ordered input ranges and finite gamma 0.1–10
before calling native code.

Preparation writes exactly three 256-byte `CompositeLut` records in R/G/B order.
Output length counts records, not bytes. Short capacity, overflowing spans, null
pointers, parameter/output overlap, invalid headers or any invalid range rejects
without writes. Extra output records remain unchanged. Compilation uses fixed
stack tables with no image access, allocation, retained pointer or file I/O.

## Mapping and rendering

Each scalar table uses existing Levels normalization, clipping, gamma and byte
rounding. Master precedes the selected channel, with byte rounding at both steps:

```text
red_final[x]   = red_table[master_table[x]]
green_final[x] = green_table[master_table[x]]
blue_final[x]  = blue_table[master_table[x]]
```

For example, master gamma 2 and red output white 128 map input 128 to 90;
reversing those operations would give 128. This specifies MiCoPi behavior without
claiming Photoshop or color-management parity. Reversed output ranges work.

Channel rendering reuses the native three-table evaluator: kind 10 reads prior
image `a`, starts R/G/B LUTs at `mode`, and requires `b=0, amount=1`. The 64-table
budget, mask/blend validation, alpha behavior, unchanged padding and preflight
rules remain intact. Kind 9 still works; the older Levels callback continues
rejecting kind 10. This tail does not advertise Curves preparation or authorize
unsupported Curves nodes.

## Delivery gates

Authored Rust/C/C++ layout assertions, independent all-byte/order/gamma references,
late-channel failure/alias/capacity/header checks, alpha/legacy render tests and
host decoder/adapter/DLL discovery tests run through existing CI targets. No
local runtime test or measurement is claimed. Bounds are not latency measurements.

Next, preserve old five-field serialization by omitting absent channel intent,
preflight disabled/disconnected declarations, cache three tables without per-MOVE
work, and retain atomic history, projects, replay and source/adjustment parity.
Existing [Levels](levels.md), [Curves](curves.md) and [histogram](histograms.md)
contracts remain separate.
