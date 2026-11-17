# Programmatic workbench diagnostics

Inspect and control the running desktop through its existing authenticated
Session attachment, CLI or MCP. No mouse, keyboard injection or Computer Use is
needed. Commands share the same registry, validation and presentation queues as
ordinary desktop controls; there is no separate debug mutation path.

## Query and capture

```powershell
dist/micopi-cli.exe call workbench.diagnostics '{}' --attach PROFILE/session.json
python scripts/inspect-workbench.py --cli dist/micopi-cli.exe --attach PROFILE/session.json --output target/test-output/ui-debug --capture
```

Replace `PROFILE/session.json` with the running instance's endpoint. The script
writes a local JSON report with workbench, document and plugin observations. The
optional PNG uses `ui.capture` and polls its exact request ID in
`window.list.capture_requests`. Endpoint credentials are consumed by the existing
CLI and are never copied into the report. The script does not change settings,
move the pointer, select tools or modify artwork.

`workbench.diagnostics` is a read-only registered action, backed by the existing
window-state handler. `workbench.debug` exposes its result as a discoverable state
for nodes and MCP. `window.list` includes the same diagnostic snapshot. Headless
or not-yet-observed desktops report `diagnostics_available: false`; a missing
snapshot is `null`, never fabricated GPU/window information. Existing
`window.command`, `settings.update` and `window.capture` remain the command paths
for layout, validated preferences and floating-window captures. See
[Plugin windows](plugin-windows.md) and [Capture requests](capture-requests.md).

## Drawing timing observations

The response also includes `drawing_performance`, available in both desktop and
headless Sessions. `tool_input` measures typed tool dispatch, including its tile
loans/history work; `replay_flush` measures provisional pointer outcome preparation,
compression and journal append. Each reports cumulative `count`, `total_us`,
`max_us` and `over_8ms`. The 8 ms counter is a diagnostic threshold, not a frame-rate
guarantee. A failed flush is counted before contact rollback; rollback time is
excluded. Action-based `canvas.stroke` does not use the direct-pointer flush path.

Counters use fixed-size scalar storage and monotonic clocks. JSON is created only
when querying the existing diagnostics/window inventory action, not on pointer
MOVE. They survive document Undo/rollback and reset with the Session; they are not
recorded or persisted. They do not measure input queue delay, GPU/DWM presentation,
physical tablet latency or the complete desktop frame. Compare tool and flush
spikes while reproducing a continuous stroke before attributing stalls to blur.

## Material observations

The cached snapshot includes renderer, active appearance, settings location and
root/detached-window material observations:

`root_frame` and each detached window's `frame` also expose the actual native
caption/edge bits and resize/system-menu/minimize/maximize style flags. The
desktop filters caption/sunken-client-edge styles before application and keeps native
metadata updates visibility-neutral. Windows may synthesize the sizing edge for a resizable window; this is reported
separately from a sunken client border. These are style observations, not a claim
that every desktop activation frame was captured.
`framework_focused` mirrors the root egui input-focus observation used by stroke
ownership, so native activation fixes can be checked without treating an OS
foreground handle as proof that the painting frontend received focus.

| Field | Meaning |
| --- | --- |
| `requested` | App blur preference |
| `native_negotiated` | DWM accepted the requested material with composition available |
| `system_transparency_enabled` | Read-only Windows preference; `null` if unavailable |
| `translucent_surfaces` | Whether the app uses translucent rather than opaque UI fills |
| `reason` | Disabled-in-app, system-disabled, unavailable, unknown, or negotiated status |
| `desktop_backdrop_verified` | Always false: native negotiation is not composed-pixel proof |

Windows **Settings > Personalization > Colors > Transparency effects** must be
enabled for system Acrylic. When the observed setting is off, MiCoPi reports
`system_transparency_disabled`, uses the opaque theme fallback and displays a
message beside Window Blur in Settings. It never changes this global preference.
The native cache re-observes the preference after Windows settings/theme/compositor
notifications; no registry reads or native negotiation occur on pointer MOVE or
ordinary repaint. Desktop JSON snapshots refresh on appearance/material/layout
changes, outside the typed drawing path.

Windows may still substitute a solid material because of accessibility, power or
hardware conditions. See Microsoft's [Acrylic fallback behavior](https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic).

## Capture limits and persistence

Renderer PNGs capture the OpenGL framebuffer, including its alpha. They exclude
the Windows compositor, lower windows and system shadows, so they can verify
labels and alpha but cannot prove visible background Acrylic. A separate reviewed
desktop-composed capture remains necessary for that claim. Do not treat
`native_negotiated: true` or a saved renderer PNG as successful glass acceptance.

Diagnostic snapshots are transient Session observations, separate from recorded
UI state, document metadata, Undo and project/recording persistence. Querying them
does not add a document revision, action outcome or history entry. No ABI,
recording format, screenshot backend or rendering dependency is added.

Local checks and bounded programmatic preview evidence do not replace full CI,
physical-tablet checks or representative GPU/pen-latency measurements.
