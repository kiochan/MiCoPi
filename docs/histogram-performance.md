# Histogram sampling cost checks

The explicit histogram inspector must remain separate from pointer MOVE and idle
painting frames. [Histogram queries](histograms.md) bound observations to 65,536
centered samples; [prepared-plan sampling](histogram-plan.md) evaluates at most
64 instructions per sampled point plus one private validation point. These are
work bounds, not a latency guarantee.

## Release callback probe

CI runs the ignored `bounded_histogram_cost` test with:

```powershell
cargo test --release --lib --locked -p micopi-compositing bounded_histogram_cost -- --ignored --nocapture --test-threads=1
```

`plugins/compositing/src/histogram_cost.rs` invokes the exported, negotiated C
callback table. It covers 1024-square and 4096-square RGBA sources at budgets of
4,096 and 65,536, with four cases:

- Raw RGBA sampling.
- A two-instruction image/output plan.
- A full 64-instruction plan with 62 invert operations.
- A six-instruction dense-A8 mask, master table and R/G/B table plan.

An independent integer reference computes every expected sampled bin, including
transparent pixels and partially transparent pixels rounded to zero by coverage.
It does not call the production grid, filter, mask, LUT or instruction evaluator.
Each invocation checks all bins and count metadata. Final full source comparisons
verify that RGBA and A8 loans remained unchanged without making reference copies.

A test-only global allocator uses a thread-local measurement gate to count Rust
allocation, zeroed-allocation and reallocation attempts during each callback.
The probe requires zero attempts on both first and warm calls. Preparation,
references, formatting, assertions and printing happen outside the timed callback.
The allocator is absent from production builds; it does not intercept foreign
allocators or account for stack memory.

Each case prints first-call milliseconds and median/p95 over nine warm calls.
The first call includes validation and sampling, with source and table storage
already prepared. There is no machine-specific timing threshold. The maximum
prepared input storage is 64 MiB RGBA plus 16 MiB A8, released before moving to
another source size; this is not a process peak-memory measurement. Histograms
use caller-owned fixed output records, with no image output buffer.

## Evidence and limits

[CI for `4ffa90d`](https://github.com/kiochan/MiCoPi/actions/runs/42420476492)
passed 1,181 desktop, 874 headless, 64 compositor, 99 painting and 11 SDK tests,
native contact probes, C/C++ layouts and every configured transport smoke,
including fenced node histograms and channel Levels adjustment persistence/replay.
That run predates the specialized node picker in `694e5c7` and this cost probe;
it does not validate either increment. Their runtime results remain pending CI.

Callback timing excludes Session dispatch, source/actual graph discovery, color
table preparation, mask-cache preparation, JSON serialization, native DLL loading,
document/history/replay, GPU uploads and UI drawing. It is not end-to-end Refresh
latency, physical-tablet latency, process peak memory or a reviewed visual capture.
Keep those delivery gates separate in the [feature checklist](photoshop-essentials.md).
