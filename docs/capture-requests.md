# Asynchronous workspace and window captures

`ui.capture` and `window.capture` save rendered desktop UI to PNG through the shared action pipeline. Each accepted desktop request receives a `request_id`. A queued response means admission, not that readback or file saving has completed. Poll the matching ID in `window.list.capture_requests` for `saved` or `failed`.

## Targets and availability

| Action | Arguments | Captured area |
| --- | --- | --- |
| `ui.capture` | `path` | The main workbench's rendered client framebuffer, including its open UI overlays |
| `window.capture` | `panel`, `path` | The native client viewport containing that already-floating registered panel |

A floating group capture includes its rendered tabs/splits and group controls. It does not select an inactive tab, focus the window or capture the surrounding desktop/DWM shadow. A root capture does not combine its floating windows into one image. Rendering readback is independent of window occlusion.

Use an absolute destination path with an existing parent directory. Paths contain 1 through 32768 UTF-8 bytes. The saved data is PNG; action responses and CLI stdout contain JSON metadata, not image bytes. Use `canvas.snapshot` for document pixels or its encoded payload when a GUI capture is not required.

These actions need an attached desktop. Headless `ui.capture` returns `ui_unavailable`; headless `window.capture` reports `queued: false`, `applied: false` and its runtime, without allocating a request ID or writing a file. A docked, hidden or missing floating target produces an explicit failure after an accepted desktop request. Unknown panel IDs fail validation before admission.

## Identity and completion records

IDs increase within one Session from 1 through 4294967295. They are stored as integers, never reused in that Session, and are transient across application restarts. Exhaustion returns `capture_id_limit`. At most 128 requests may remain pending across workspace and panel targets; excess admission returns `capture_queue_full`. The existing presentation-effect queue has its own 128-entry admission check and reports `presentation_queue_full` when full.

Desktop results include:

```json
{"queued":true,"path":"C:/captures/workspace.png","request_id":71}
```

For a native window, the result also identifies the runtime and unapplied presentation state:

```json
{"queued":true,"applied":false,"runtime":"desktop","request_id":72}
```

Read `window.list` or its `workbench.windows` state to observe `capture_requests`:

```json
{
  "pending": [
    {"request_id":72,"target":"panel","panel":"tool-options","path":"C:/captures/options.png","status":"queued"}
  ],
  "recent": [
    {"request_id":71,"target":"workspace","path":"C:/captures/workspace.png","status":"saved","width":1700,"height":1150}
  ],
  "last": {"request_id":71,"target":"workspace","path":"C:/captures/workspace.png","status":"saved","width":1700,"height":1150}
}
```

`queued` remains the status while waiting for readback, encoder capacity or a file acknowledgment. A `saved` record has positive physical-pixel `width` and `height`; a `failed` record has an `error`, bounded to 4096 characters. Completed requests leave `pending` and enter `recent` in completion order. The newest 128 terminal records are retained; pending records are not evicted to make room for them. `last` is the most recent terminal record or null.

`last_capture` remains available for older clients and normally mirrors `capture_requests.last`, with a legacy fallback when no identified capture has completed. It may describe a different concurrent request. Correlate by the returned ID and path instead of relying on that single last record or the presence of an older file at the destination. Use distinct paths when both captures must remain available; later saves to a reused path replace its contents.

## Queue and rendering lifecycle

The workbench has a root FIFO and each floating viewport has its own FIFO. Only a queue's front request reads a frame; it remains pending until its exact completion is acknowledged. Different viewports may complete out of admission order. Private scoped tokens additionally match viewport, request ID and path, so duplicate/stale events, preview requests and reused destination names cannot complete another request.

Root requests explicitly target `ViewportId::ROOT` and accept only their matching egui Screenshot event. Native immediate viewports use a final paint callback: the pinned eframe Glow immediate-render path does not emit ordinary Screenshot events for those children. The callback survives discarded layout passes because UI construction does not consume its request. It validates the renderer's actual physical framebuffer dimensions, reserves encoder capacity and claims readback once. A panel docked/hidden before readback fails even if another group reuses its old viewport index.

The shared encoder starts lazily on the first reservation. One background worker accepts at most two queued images plus one active job; full capacity causes a retry without dropping the pending request. Native callbacks reserve before reading pixels. Encoding, conversion and filesystem writes run off the UI thread with Fast PNG compression. Files use a unique same-directory temporary, checked buffered flush and sync, then the existing atomic replacement path. Errors leave an existing destination intact and become a failed completion.

Each image must contain 1 through 16777216 pixels. Native capture validates this before GL readback; the backend's root Screenshot image is validated by the encoder before writing. This is a capture-image limit, not a bound on the application's total memory or on all backend readback allocations.

Desktop requests waiting for readback/encoder submission have a 30-second deadline. Once encoding has been submitted, it waits for its file acknowledgment rather than being reported failed while a write is still running. Closing or docking that viewport afterward does not cancel an already-submitted job mid-write. These queues and diagnostics are not crash recovery; a stopped process does not provide a later completion.

## Two accepted requests and ID-based polling

With the desktop running and `tool-options` already floating, this example submits both requests before polling. Set the CLI and attached profile endpoint paths for that desktop process:

```python
import json
from pathlib import Path
import subprocess
import time

cli = Path("dist/micopi-cli.exe").resolve()
endpoint = Path("path/to/desktop-profile/session.json").resolve()
output = Path("target/test-output/captures").resolve()
output.mkdir(parents=True, exist_ok=True)

def call(action, args):
    response = subprocess.run(
        [str(cli), "call", action, json.dumps(args), "--attach", str(endpoint)],
        check=True, capture_output=True, text=True, encoding="utf-8", timeout=35,
    )
    return json.loads(response.stdout)["result"]

requested = {}
for action, args in [
    ("ui.capture", {"path": str(output / "workspace.png")}),
    ("window.capture", {"panel": "tool-options", "path": str(output / "options.png")}),
]:
    result = call(action, args)
    assert result["queued"]
    requested[result["request_id"]] = args["path"]

deadline = time.monotonic() + 45
while requested:
    records = call("window.list", {})["capture_requests"]
    for record in records["recent"]:
        expected_path = requested.get(record["request_id"])
        if expected_path is None:
            continue
        assert record["path"] == expected_path
        if record["status"] == "failed":
            raise RuntimeError(record["error"])
        assert record["status"] == "saved"
        assert record["width"] > 0 and record["height"] > 0
        del requested[record["request_id"]]
    if requested:
        if time.monotonic() >= deadline:
            raise TimeoutError(f"Captures still pending: {requested}; status: {records}")
        time.sleep(0.05)
```

A client timeout is not a cancellation. Query current IDs before retrying an operation whose response was interrupted. Avoid requesting more captures than a client can observe within the bounded recent-record window.

## Document, replay and verification boundaries

Identified capture admission/completion does not change document pixels, composition, revision or `Session.state()`. Its bounded diagnostic status is separate from UI metadata restored by document replay and is not persisted in projects or recordings. It can therefore observe an open document-bound form without making that form stale. Legacy generic UI effects remain supported separately.

Capture does not freeze the Session. Independent editing or delayed `workspace.layout.apply` commands can still advance its revision while a PNG is pending. Tests of capture isolation wait for requested geometry and its quiet-period layout publication before taking their baseline. A UI PNG represents the rendered frame read back for that request, rather than an atomic snapshot of the document at admission time.

Capture commands are explicit external file effects and cannot participate in atomic document groups. Their action audit boundary does not turn the asynchronously encoded UI PNG into a verified document outcome. Replay does not rerun capture commands or reproduce their file writes. Capture records are transient diagnostics, not screenshot content hashes or authorship evidence; a `saved` acknowledgment reports file publication, not a binding of those UI pixels to an outcome hash. See [Verified Replay](replay.md) and [Atomic Operations](atomic-operations.md) for their distinct contracts.

Unit regressions cover FIFO/token matching, discarded layouts, backpressure/readback count, dimensions, target removal, timeout boundaries, file errors and state invariance. `scripts/smoke-captures.py` and the capture helper used by `scripts/smoke-localization.py` exercise attached-process behavior and compare decoded PNG dimensions against the owned native client rectangle. Runtime results belong to the specific validation run; source coverage alone is not a claim that a GUI smoke passed on the current desktop.
