# Image Node Editor

The egui-snarl editor shares the workbench's theme, configurable spacing and docking. Each document stores an image graph and a separate plugin workflow graph.

![Image graph](screenshots/image-nodes.png)

## Editing

Open View > Node Editor, the Layers panel button or the command palette. The panel initially docks below the canvas and can be moved, grouped or floated. This independent window edits image compositing. Plugin workflows have their own window.

New documents use **Linked Layer Stack**. Each source is followed by its own optional filters, then blended over the accumulated lower layers. The graph and Layers panel describe the same stacking order:

```text
Bottom layer -------------------+
                                Blend A ---------------+
Next layer -> optional filters -+                      Blend B -> Canvas Output
Top layer -> optional filters -------------------------+
```

1. Add, duplicate, move or delete layers normally. The applied linked chain follows these operations automatically. Filters follow stable layer identities; duplication copies a layer's filters with new node IDs. Deleting a layer removes its branch, with undo restoring it.
2. Right-click a source or filter and choose **Insert Filter**. The menu uses registered plugin filter definitions and inserts a connected node. Deleting a filter reconnects its neighbors. Linked wiring is protected against manual disconnection.
3. Source cards show the layer name, identity, visibility, opacity and blend mode. Source-card controls use the same `layer.set` action as the Layers panel. Blend-node mode edits update that layer's mode when applied. Layer opacity is applied once at the source; linked blends have opacity 1.
4. Edit filter/blend parameters and choose **Apply to Canvas**, or enable **Live Preview** for valid changes after a 300 ms pause. Source-card property changes atomically apply the current valid graph draft and the layer property edit. Original pixels remain intact.
5. **Graph Options > Arrange Stack** aligns sources, filter chains, merges and output. Structural layer edits arrange the linked chain automatically. Clean editor views refresh when another panel or client changes the applied graph; unapplied drafts remain protected by a conflict message. Fit Graph includes the taller source-property cards.

**Graph Options > Switch to Free Compositing** converts the draft to explicit wiring without changing its connections or output. Apply to commit the switch. In this advanced mode, Add Node and port dragging are available; only connected layers participate, new layers are not automatically included, and layer reordering preserves source identity without changing the graph's explicit order. Referenced layers cannot be deleted until their source nodes are removed or retargeted.

**Graph Options > Rebuild from Layers** replaces the draft, including its filters, with a fresh linked stack. It is an explicit reset, not a required synchronization step. Apply to commit it. Disable **Node Compositing** in Graph Options and apply to bypass filters/custom wiring and render the ordinary layer stack; linked structural synchronization still operates while effects are disabled. Load Current is also available in Graph Options.

Drag headers to move nodes, Ctrl+scroll to zoom, double-click empty space to center, and use Fit Graph to see the graph. Right-click a node to delete it. Closing the editor does not disable effects. Unapplied drafts remain only in the current UI; projects store applied graphs. When external edits or undo conflict with an unapplied draft, use Graph Options > Load Current before overwriting the applied graph.

Free-graph sources honor layer visibility and opacity; free blend nodes own their own blend parameters. Existing projects and graphs without `linked: true` retain these legacy semantics and are never silently rewired. New linked projects/recordings require a host that understands the `linked` field; old recording hashes remain unchanged when that field is absent.

## Nodes and limits

| Node | Behavior |
| --- | --- |
| Layer | Read a source layer without changing pixels |
| Filter | Invert, grayscale, brightness, contrast or saturation |
| Blend | Normal, multiply, screen, overlay, darken, lighten, linear dodge or difference |
| Output | The unique canvas and export result |

Invert/grayscale strength is 0..1; brightness/contrast/saturation are -1..1; blend opacity is 0..1. Filters preserve alpha; blending uses straight-alpha source-over. Fully transparent composited pixels are canonicalized to zero. Spatial filters such as blur are not implemented.

An image graph has at most 64 nodes, 128 edges and one output. Cycles, duplicate inputs, missing required inputs, invalid layer references and parameters are rejected before changing the document. Unconnected drafts are allowed, but all nodes must remain structurally valid. Only dependencies of the output are compiled for rendering.

Linked graphs additionally require every layer exactly once in stack order, linear per-layer filter chains, unit-opacity merges and no detached nodes. A structural edit that would exceed the node budget fails before changing the stack. Linked topology uses the existing image kernels; arbitrary plugin algorithms still require compositor support. Spatial, mask and alpha-changing filters need separate execution contracts.

## Actions and plugin registration

- graph.catalog: image schema, supported filters/modes and registered node definitions.
- graph.read: applied graph, `mode` (`linked` or `free`), stable layer identities/properties, default linked graph, renderer availability and errors.
- graph.set: validate and apply as an undoable metadata edit.
- graph.open: emit the editor-open UI effect; headless does not create a window.
- layer.set: edit ordinary layer properties, including blend mode.
- document.nodes and compositing.graph: public state resources.

```json
{"action":"graph.set","args":{"graph":{"enabled":true,"nodes":[{"id":1,"node_type":"compositing.layer","kind":"layer","index":0,"position":[0,20]},{"id":2,"node_type":"compositing.filter","kind":"filter","filter":"invert","amount":1,"position":[240,20]},{"id":3,"node_type":"compositing.output","kind":"output","position":[480,20]}],"edges":[{"from":1,"to":2,"input":0},{"from":2,"to":3,"input":0}]}}}
```

To create a linked graph through MCP/headless, start with `graph.read.default_graph`, retain its layer order and set `linked: true` (already present in the default). Insert registered filter nodes on individual source branches and submit `graph.set`. Layer sources retain the compatible `index` wire format; structural edits resolve them through document-scoped stable IDs before rebuilding. Use the returned `layers` mapping and revision guards rather than caching numeric indices across edits. Applying a linked graph updates layer modes and topology in one undoable document operation. The same operation supports atomic pipelines, outcome recording and verified playback.

The complete [linked-layer JSONL example](../examples/automation/linked-layers.jsonl) adds a filter, duplicates and reorders its layer, exports a PNG and verifies the recording.

Image definitions come from plugins/compositing/manifest.json and use binding.kind=image with a kernel operation and parameters_schema. Legacy graphs without node_type migrate to compositing.* IDs when applied. New aliases use existing kernel semantics; a new image algorithm requires extending the compositor implementation. JSON workflow nodes and image-buffer nodes have separate typed domains. See [Workflow Nodes](workflow-nodes.md).

The independent compositor DLL receives one dirty rectangle, immutable source layers and typed instructions. It processes the region without a full image allocation per node. A linked graph without filters uses the existing ordinary-stack fast path, including the CPU normal-blend path without a compositor DLL. Topology validation/rebuilding happens on graph or layer edits, never on individual pen MOVE calls. No dependency or ABI extension is introduced. Graph properties and positions support undo/redo; project files retain original layers and applied graphs and continue to read legacy files. See [Compositor Plugin](../plugins/compositing/README.md).

Regression coverage includes all blend modes with alpha/opacity edges, layer insertion/reordering, identity-preserving filters, filter duplication/deletion, protected UI connections, draft conflicts, budget rejection, atomic rollback, undo/redo, saved projects, legacy journals and identical PNG bytes through JSONL, MCP and desktop attachment. Run `python scripts/smoke-linked-graphs.py` after building the host and DLLs.
