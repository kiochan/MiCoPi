# Histogram sampling cost checks

The explicit histogram inspector must remain separate from pointer MOVE and idle
painting frames. [Histogram queries](histograms.md) bound observations to 65,536
centered samples; [prepared-plan sampling](histogram-plan.md) evaluates at most
64 instructions per sampled point plus one private validation point. These are
work bounds, not a latency guarantee.

## Release callback probe

CI runs the ignored tests matching `bounded_histogram_cost` with:

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

## HSL dirty-region and observation probe

The same CI filter also selects
`bounded_histogram_cost_hsl_regions_and_sampling`. It negotiates the independent
HSL API and uses opaque callback pointers, prepared 1024/4096-square immutable
RGBA sources and two centered output regions: 32-square and 256-square. Each
output has eight padding pixels per row. An integer 180-degree hue reference
reflects RGB about `(min + max)/2`, preserves nonzero alpha and canonicalizes
alpha-zero source-composition pixels. It shares no HSL conversion code with the
native implementation.

Every first/warm render checks all output pixels and padding, zero callback Rust
heap allocations and unchanged output allocation identity/capacity. Failures name
the first mismatching pixel without dumping an image-sized array. The same
independent reference checks every bin for budgets 4,096 and 65,536. Full source
integrity checks run after the calls. References, source/output preparation,
validation assertions and printing stay outside timings.

The HSL probe prints first-call, nine-warm-call median/p95 and allocation results.
It includes native parameter preparation, admission and scalar color work; it
excludes host/Session parameter caches, input processing, history/replay, UI/GPU,
DLL loading and physical-device latency. Its largest prepared source is 64 MiB;
ROI outputs/references are bounded small buffers. Tests run serially and release
their input storage between cases. This is not a full-frame repaint or process
peak-memory measurement. The first verified CI results are recorded below;
there are no local large-image or machine-specific timing assertions.

### Verified HSL callback baseline

[CI for `4c8f19d`](https://github.com/kiochan/MiCoPi/actions/runs/42431038394)
passed the ignored Release probes on the hosted Windows runner. These are
single-run observations over prepared input and nine warm calls, not targets:

| Source side | ROI side | First ms | Warm median ms | Warm p95 ms |
| ---: | ---: | ---: | ---: | ---: |
| 1024 | 32 | 0.0562 | 0.0377 | 0.0552 |
| 1024 | 256 | 2.4765 | 2.4818 | 2.5465 |
| 4096 | 32 | 0.0555 | 0.0405 | 0.0415 |
| 4096 | 256 | 2.7248 | 2.6403 | 2.6695 |

For 4096-square input, the 4,096-sample HSL histogram measured 0.2465 ms first,
0.1742 ms median and 0.2010 ms p95; 65,536 samples measured 3.2849, 2.9371 and
3.3198 ms respectively. Every callback reported zero counted Rust heap
allocations, valid independent RGBA/bin references, preserved ROI padding and
reused output allocation. Input, references and output were allocated outside
timers; these figures do not describe a full-frame document preview, process
peak memory, GUI/physical-device latency or the newer shared tonal pipeline.

## Evidence and limits

[CI for `4ffa90d`](https://github.com/kiochan/MiCoPi/actions/runs/42420476492)
passed 1,181 desktop, 874 headless, 64 compositor, 99 painting and 11 SDK tests,
native contact probes, C/C++ layouts and every configured transport smoke,
including fenced node histograms and channel Levels adjustment persistence/replay.
That run predates the specialized node picker and cost probe. The later green
`4c8f19d` run passed 1,212 desktop, 893 headless, 72 compositor, 18 native-paint,
99 painting and 12 SDK tests, both histogram probes, native contact/tip probes,
SDK layouts and all configured transport smokes. This includes the HSL UI unit
tests, direct/source/adjustment operations, fenced PNG previews and persistence.
It predates the newer tonal/shared-staging work. Standard CI did not run native
window capture scripts, so no actual screenshot production/review is certified.

Callback timing excludes Session dispatch, source/actual graph discovery, color
table preparation, mask-cache preparation, JSON serialization, native DLL loading,
document/history/replay, GPU uploads and UI drawing. It is not end-to-end Refresh
latency, physical-tablet latency, process peak memory or a reviewed visual capture.
Keep those delivery gates separate in the [feature checklist](photoshop-essentials.md).
