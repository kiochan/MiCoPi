# Clipboard

The clipboard subsystem provides bounded Session transfers for command search, configurable bindings, plugin workflow nodes, MCP and headless JSONL. Explicit Windows desktop actions add image clipboard access. The internal commands below never read or replace the Windows clipboard; desktop system commands are described separately.

| Action | Behavior |
| --- | --- |
| `clipboard.copy` | Copy raw active-layer pixels in the selection bounds |
| `clipboard.copy_merged` | Copy the rendered composite in the selection bounds |
| `clipboard.cut` | Copy raw active-layer pixels, then clear selected coverage |
| `clipboard.paste` | Insert a new normal layer centered in the current selection bounds, or canvas |
| `clipboard.paste_in_place` | Insert a new normal layer at the original document coordinates |
| `clipboard.state` | Read availability, dimensions, origin, source canvas dimensions, bytes and SHA-256 |

Paste actions accept an optional integer `x`/`y` pair from -4096 to 4096. An explicit pair overrides centering/original placement. Clipped placement is supported; an entirely off-canvas result is rejected. Coordinates remain document pixels when source and destination canvas sizes differ. Editing a node graph does not execute these actions. The read-only `core.clipboard` public resource uses `clipboard.state` and generates the `state.core.clipboard` node.

![Session clipboard paste command with optional position controls](screenshots/clipboard.png)

## Pixel and layer contract

Positive selection coverage defines the cropped bounding rectangle, preserving transparent padding inside that rectangle. No active selection copies the entire canvas. An active empty selection rejects copying, cutting and automatic centered placement. All-transparent source pixels are valid clipboard content.

Raw copy ignores layer visibility, layer opacity, blend mode and filters. Copy Merged samples the rendered composite and rejects rendering errors before replacing the clipboard. Source pixels remain unchanged. Straight alpha is multiplied by selection coverage once, rounded to the nearest integer. Fully selected pixels preserve exact storage, including hidden RGB; excluded pixels become transparent black. The independent painting DLL performs extraction and placement through a typed optional ABI tail.

Cut uses the existing native selection blend when clearing the source. Fractional coverage is applied once. Alpha-locked source layers reject cutting; copying is allowed. Cut creates one document undo entry when pixels change. Undo restores pixels without replacing the clipboard.

Paste inserts a visible, unlocked, full-opacity normal layer directly above the active layer. It copies raw clipboard pixels without blending or clipping them through the current selection. It deselects the current mask and makes it available for Reselect. Layer insertion, graph synchronization and deselection share one structural undo entry. Linked image graphs preserve existing layer/filter identities; enabled free graphs reject insertion until the layer stack is linked explicitly.

## Limits and resource ownership

One immutable payload belongs to a Session, with a maximum of 4096 × 4096 straight-alpha pixels (64 MiB). It survives document replacement within that Session, but is not stored in projects, settings or document undo. `clipboard.state` never exposes the pixel buffer or changes revision. Its digest binds the version marker, width, height, origin, source canvas dimensions (little-endian u32), and row-major little-endian ARGB words. Hashing happens once when publishing a payload, using a reused 16 KiB staging buffer.

Transfers are cold operations. Extraction allocates one cropped result; paste allocates one destination layer. Cut additionally stages a cropped clear buffer. Atomic checkpoints share payloads through `Arc`; rollback restores the previous payload and document/history together. Live and checkpointed payloads may coexist until a transaction/contact ends. Existing document/history limits still apply. No transfer, hashing or clipboard lookup is added to pointer MOVE. A local release measurement of unrestricted extraction took approximately 0.7 ms at 1024² and 13 ms at 4096², excluding allocation, host hashing/history and rendering. These are machine-specific kernel measurements, not end-to-end latency guarantees. Run the ignored `selection_transfer_cost` test explicitly to measure another machine.

## Events and verification

Successful direct transfer events carry a small optional `source.context.clipboard` descriptor with the payload digest and dimensions. Pixel payloads are not duplicated into event metadata. Paste/cut document outcomes use existing compressed tile deltas/references, with one event per direct command. Older events omit the optional context and retain their original hashes.

Verified replay restores the recorded document outcomes without invoking the transfer DLL or consulting/replacing the live clipboard. A copy event audits a resource transfer; it does not make clipboard bytes recoverable from a recording. An exported project retains pasted pixels, not the ephemeral clipboard. [Selection-to-layer Copy/Cut](layer-editing.md#layer-via-copy-and-cut) is available without replacing the clipboard. The Windows adapter is implemented below; its native interoperability acceptance gate remains open in the [editing plan](photoshop-essentials.md). Explicit payload interchange remains planned.

Try [the JSONL example](../examples/automation/clipboard.jsonl). Rebuild/stage the painting DLL and restart before using the new transfer capability; older tables remain usable for previous selection features and explicitly reject transfers.

## Windows desktop adapter

**Validation status:** bounded PNG/DIB codecs, adapter failure paths, local rollback, schemas and replay are covered by automated tests. Real native clipboard write/read roundtrip is not yet verified on this host: Windows denies `CreateWindowStationW` with Access Denied, including a retry outside the filesystem sandbox with reduced access rights. The isolated fixture remains opt-in. No default test opens, reads or replaces the interactive clipboard's image data. Cross-application compatibility remains an explicit acceptance gate, not a completed claim.

| Action | Default binding | Behavior |
| --- | --- | --- |
| `clipboard.system_copy` | Ctrl+C | Copy raw selected active-layer pixels to Windows and Session clipboards |
| `clipboard.system_copy_merged` | Ctrl+Shift+C | Copy the selected rendered composite |
| `clipboard.system_cut` | Ctrl+X | Publish selected pixels to Windows, then clear source coverage |
| `clipboard.system_paste` | Ctrl+V | Import a Windows image and paste centered in selection/canvas |
| `clipboard.system_paste_in_place` | Ctrl+Shift+V | Import and use a verified MiCoPi origin, otherwise (0, 0) |
| `clipboard.system_state` | — | Read supported-format availability flags without reading image data |

These actions are registered and available in Edit, command search and action nodes. `core.system_clipboard` is a read-only public resource. Only desktop Sessions with a native window adapter can execute them; headless callers receive `ui_unavailable` and retain the explicit internal clipboard commands. CLI/MCP clients attached to a running GUI use that GUI's Session. Keyboard shortcuts yield to existing explicit preferences. Focused text fields keep their normal text-editing shortcuts.

### Formats and limits

Publication prepares all bytes and movable global-memory allocations before opening or emptying the clipboard. The primary registered `PNG` format retains straight alpha and hidden RGB. An optional top-down, 32-bit `CF_DIBV5` fallback uses explicit RGBA masks and sRGB tagging. A small optional `MiCoPi.Image.v1` JSON hint binds original coordinates/source dimensions to the payload digest. Partial fallback/hint publication is reported in typed `warnings` and shown in the desktop status line; successful primary PNG publication is the copy success boundary.

Import prefers PNG, then DIBV5, then DIB. A malformed preferred image fails instead of falling back to stale Session data or a lower-priority format. Packed Windows DIB headers of 40/52/56/108/124 bytes support uncompressed 1/4/8-bit indexed images, RGB 16/24/32-bit images and contiguous nonoverlapping 16/32-bit channel masks. Bottom-up/top-down rows and DWORD padding are supported. Legacy BI_RGB 32-bit images are opaque; alpha requires PNG or an explicit alpha mask. Color tables are limited to 256 entries. Indexed palette entries ignore their reserved alpha byte. RLE, OS/2 headers, embedded compressed DIB payloads, ICC-profile DIB data, files, text and HTML are outside this contract. Clipboard colors remain unmanaged, as in the current image-import pipeline; broader color management is planned separately.

Dimensions are 1..4096 per side, encoded data at most 72 MiB per format, and private origin metadata at most 1 KiB. PNG checks its mandatory IHDR before decoder allocation and uses the existing image library with a 128 MiB decoder allocation limit. DIB validates depth, masks, palette counts, stride and complete storage before creating pixels. These are individual buffer limits, not a bound on total process memory: imported pixels, temporary RGBA bytes, encoded PNG, DIB and movable OS buffers can coexist during a cold transfer. No new dependency, clipboard watcher or per-MOVE serialization/lookup is added.

Origin hints are optional and untrusted. Bounds and version must be valid, and the digest must match the actual imported pixels with the proposed source coordinates. Invalid or mismatched hints reset the origin to (0, 0) and source size to the imported dimensions. This is consistency checking, not authentication. No file path or user document text is placed in the hint.

### External effects and recovery

The native adapter checks that the owner is a live window on the current process/UI thread. It tries opening the clipboard once, returns a busy error for retry, closes it through RAII and transfers memory ownership only after successful `SetClipboardData`. Read bytes are copied under the short OS lock; decoding and native painting occur after closing it. No borrowed clipboard pointer reaches a plugin.

System mutation actions are excluded from atomic document groups, including workflows with Atomic Document Changes enabled. They still checkpoint local document/history and Session clipboard state. Cut stages its native clear result before publication; publication failure never clears source pixels. After publication succeeds, cut commits through existing tiled history. Paste uses one new-layer structural undo. A storage/journal failure restores local state, but cannot undo a system copy that already succeeded. A failure after `EmptyClipboard` may leave the external clipboard empty or partly populated; arbitrary prior clipboard formats are not snapshotted or restored. Finish contacts through the typed pressure/stabilization path before accessing the resource.

Direct successful events retain a compact digest, transfer direction, selected format/origin check or publication report. Cut/paste outcomes retain normal compressed tile deltas/references. Replay never reads or writes Windows clipboard data and does not invoke its adapter. Projects retain resulting document pixels, not clipboard resources.

### Safe native fixture

After building desktop library tests with the repository's locked Windows toolchain, run:

```powershell
$clipboardTest = Get-ChildItem -LiteralPath target/build/x86_64-pc-windows-gnu/debug/deps -Filter 'micopi-*.exe' |
    Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
python scripts/smoke-system-clipboard.py --test-binary $clipboardTest.FullName
```

The script creates a unique noninteractive window station/desktop and launches one ignored test there. The Rust fixture checks the actual station name before accessing any clipboard. It exercises actual PNG/DIBV5 publication, an independently built legacy DIB producer, strict malformed-PNG rejection and shared Session actions. It refuses WinSta0 and never backs up or overwrites the user's clipboard. On restricted hosts, creation can fail before the fixture runs; that is an unverified gate, not a passed roundtrip test. See [Microsoft's station/clipboard ownership model](https://learn.microsoft.com/en-us/windows/win32/winstation/window-stations), [memory transfer contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setclipboarddata) and [bitmap header specification](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-bitmapv5header).
