# Screenshots

These are actual application captures, not interface mockups. Current images come from isolated GUI smoke checks and design previews of MiCoPi builds. Documentation deliberately reuses stable filenames so image updates do not require changing every guide.

| Image | Purpose |
| --- | --- |
| [Atomic workflow](atomic-workflow.png) | Default atomic execution control in the plugin node editor |
| [Verified replay](replay.png) | Event timeline and read-only recorded outcome preview |
| [Workbench](workbench.png) | Default independent control bar, canvas and property windows |
| [Light workbench](workbench-light.png) | Shared layout and controls in the light theme |
| [Warm workbench](workbench-warm.png) | Shared layout and controls in the warm theme |
| [Floating windows](floating-windows.png) | Independent native canvas window |
| [Panel group](native-panel-group.png) | One native window preserving tab groups and nested splits |
| [Native Tools](native-tools.png) | Independent native control bar |
| [Image nodes](image-nodes.png) | Linked layer sources, per-layer filters, synchronized merges and output |
| [Workflow nodes](workflow-nodes.png) | DLL-contributed public state connected to an action |
| [English settings](settings-en.png) | Display language setting |
| [Chinese settings](settings-zh-CN.png) | Simplified Chinese UI verification |
| [Japanese settings](settings-ja.png) | Japanese UI and font verification |
| [Korean settings](settings-ko.png) | Korean UI and font verification |

The checkerboard, pen strokes and simple landscape are generated through the DLL/action pipeline. They are demonstration content rather than a benchmark or a claim about an artist's work.

Run `scripts/smoke-runtime.py`, `scripts/smoke-linked-graphs.py`, `scripts/smoke-workflows.py` and `scripts/smoke-localization.py` to generate new captures under ignored `target/test-output/`. Inspect the results before promoting selected files here. Do not commit entire test-output folders.

Older screenshots live in [archive](archive/README.md) for historical reference. They do not represent the current interface and are omitted from portable release screenshots.

`floating-windows.png` and `native-tools.png` show the independent native Canvas and Tools windows captured by `scripts/smoke-windows.py`. The workbench variants show their default docked arrangement with a layered landscape. All captures use the application renderer.
