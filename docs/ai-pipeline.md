# AI and the Unified Plugin Pipeline

AI clients and native plugins use the same public contract as the GUI. Discover schemas, read public state, plan actions, validate and execute. No private memory access or parallel document implementation is required.

## Discovery and public state

`app.describe` describes the runtime, registered plugins, actions, tools, schema capabilities and execution entry points. `context.read {resources:[...]}` returns per-resource success or failure, with validated output. Available resources include active document/layers, open files and source/save paths, selected tool and brush settings, latest input, registry diagnostics, UI mode, image graph and workflow graph.

The current version has one open document per Session. Input state is the latest received sample, not a device inventory or live pressure poll. Reading state does not end a stroke, modify pixels or increment revision. A read-only provider cannot dispatch mutating actions or edit pixel regions.

Plugins register namespaced providers:

```json
{"states":[{"id":"my_plugin.status","title":"Plugin Status","action":"my_plugin.query","args":{},"output_schema":{"type":"object","properties":{"ready":{"type":"boolean"}},"required":["ready"]}}]}
```

The referenced action must be read-only and its arguments valid. A provider failure is isolated in its resource result. MCP exposes providers through resources/list and resources/read, with URIs such as `micopi://state/my_plugin.status`.

## Pipeline execution

```json
{"action":"pipeline.run","args":{"steps":[{"id":"read","action":"canvas.state","args":{}},{"id":"view","action":"view.set","args":{"zoom":{"$ref":"read#/result/view/zoom"}}}],"dry_run":true}}
```

Steps use unique IDs, an action and JSON arguments. `{"$ref":"step-id#/result/field"}` inserts the earlier result at the original JSON type, using JSON Pointer escaping. A reference must occupy its entire object. Future or missing references, malformed paths, unknown actions and static invalid arguments fail before mutations. Dynamic arguments are checked after resolving upstream results.

`dry_run` validates the static plan without invoking actions or providers. It lists deferred validation rather than inventing future state. An integer `expected_revision` rejects stale plans. The execution limit is 128 steps per pipeline, depth 16 and a shared budget of 1024 leaf calls; nested operations share the limits.

Execution is sequential and not transactional. A failure stops subsequent steps and returns completed IDs, results and the failed step. Already completed painting or file operations remain. A producer may mutate successfully and then fail output-schema validation. Nested failures propagate to outer pipelines, JSONL returns a failing exit code, and MCP marks `isError`.

DLLs call the same entry points through HostApi.dispatch. Cross-DLL calls are allowed; same-instance reentry, call cycles, callback-time host calls and exhausted budgets fail explicitly. Instances belong to a Session and its execution thread.

## Workflow nodes

`node.catalog` adapts action input/output schemas and public state into typed nodes. Plugins may register aliases with titles, categories and defaults. AI can create and modify document graphs through `workflow.set/patch`, inspect with read/validate, and run through workflow.run. This compiles back to the pipeline above. See [Workflow Nodes](workflow-nodes.md) for schemas, limits and manual-run semantics.

The default [ai-assistant DLL](../plugins/ai-assistant/README.md) demonstrates reading another plugin's public state and submitting a cross-DLL pipeline. [examples/automation/ai-pipeline.jsonl](../examples/automation/ai-pipeline.jsonl) provides a runnable sequence. The [native SDK](native-plugins.md) documents ownership and ABI rules.

## Locale and AI contracts

English is the canonical metadata/documentation language. Display language affects presentation, not action IDs, argument names, state IDs, schema types, revision checks, node ports or file paths. Always use IDs and schemas for automation rather than matching visible translated labels. Unknown third-party messages fall back to their original text.
