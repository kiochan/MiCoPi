# Screenshots

These are actual application captures, not interface mockups. Current images come from isolated GUI smoke checks and design previews of MiCoPi builds. Documentation deliberately reuses stable filenames so image updates do not require changing every guide.

| Image | Purpose |
| --- | --- |
| [Atomic workflow](atomic-workflow.png) | Default atomic execution control in the plugin node editor |
| [Verified replay](replay.png) | Event timeline and read-only recorded outcome preview |
| [Selection to layer](selection-to-layer.png) | Actual selected-pixel layer extraction, with the source preserved and the new layer active |
| [Canvas trim](trim-canvas.png) | Cached trim preview, selected sides and resulting dimensions beside visible artwork |
| [Image size](image-size.png) | Actual aspect-locked image sizing dialog with native sampling choices |
| [Canvas quarter turn](canvas-quarter-turn.png) | Actual clockwise document rotation, transformed selection and navigator preview |
| [Session clipboard](clipboard.png) | Actual shared paste parameter form; coordinates are optional and execution is explicit |
| [Selection outline](selection.png) | Command-generated ellipse overlay and active-selection status, independent of exported pixels |
| [Workbench](workbench.png) | Default independent control bar, canvas and property windows |
| [Light workbench](workbench-light.png) | Shared layout and controls in the light theme |
| [Warm workbench](workbench-warm.png) | Shared layout and controls in the warm theme |
| [Floating windows](floating-windows.png) | Independent native canvas window |
| [Panel group](native-panel-group.png) | One native window preserving tab groups and nested splits |
| [Native Tools](native-tools.png) | Independent native control bar |
| [Brush presets](brush-presets.png) | Rounded brush cards, tapered previews and preset selection |
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

`canvas-quarter-turn.png` was reviewed from the English `scripts/smoke-localization.py` capture on 2026-11-05. It shows the real application after `canvas.rotate_90_cw`; the mask, two source layers and navigator retain the shared document state.

`image-size.png` was reviewed from the English `scripts/smoke-localization.py` capture on 2026-11-05, alongside its Japanese capture. It shows the real Image Size dialog with nearest/bilinear/area guidance; the default remains bilinear. Opening/editing the form does not execute a resample. The smoke check also applies area reduction and restores through GUI Undo.

`trim-canvas.png` was reviewed from the English `scripts/smoke-localization.py` capture on 2026-11-05, alongside its Japanese capture. The draggable controls leave the central artwork visible and retain configured outer spacing. The source frame remains 320×256 while the read-only preview proposes 256×192; Cancel preserves the document.
