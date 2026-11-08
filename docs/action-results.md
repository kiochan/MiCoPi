# Action results and revisions

Desktop commands, JSONL, MCP, workflow nodes and native host callbacks share
`Session::apply`. Its successful response has a Session-owned `revision` and an
operation-specific `result`. Transport wrappers may contain that response inside
their own message envelope.

The response revision is the Session revision at that return boundary. Use it as
the next action's `expected_revision` to reject stale edits. A successful response
can also contain a structured failed pipeline or workflow report; inspect `ok`,
`rolled_back` and `committed` before treating its steps as successful.

## State-bearing built-in results

Built-in actions that return the complete Session state publish its top-level
`result.revision` after the existing revision and outcome-recording steps. It
equals the surrounding response's `revision` and a following `canvas.state`
query when no operation intervenes. This includes document creation/opening, geometry,
Undo/Redo, layer/group state mutations, selection edits and layer batches. The
direct PNG export result also publishes its Session revision after recording.
Read-only queries and plans exposing a Session revision field already report
their observation revision; provider-owned values follow their own contracts.

The correction updates one existing field. It does not build another state
snapshot, increment a revision, add fields to compact results or recursively
rewrite JSON. For example, `layer.add` returns identities, mask mutations return
mask-specific state and `canvas.pointer` returns drawing status: use their outer
response revision. Their result shapes remain unchanged.

The shared boundary explicitly classifies the built-in operations whose field is
Session-owned in `Operation::publishes_session_result_revision`. A new handler
returning full Session state must retain that contract and its regression checks.
Do not infer field ownership from the spelling `revision` or the shape of a value.

## Observations, reports and plugin ownership

- Earlier pipeline steps and workflow nodes retain the revisions at which they
  returned. Later steps do not retroactively update their snapshots. A macro's
  final result follows the contract of its final expanded operation.
- Failed atomic execution restores the previous Session revision. Its report
  identifies rollback and has no committed steps; nested attempted results remain
  historical evidence, including versions that were rolled back. Sequential
  execution retains successful earlier steps. See [Atomic Operations](atomic-operations.md).
- Native DLL results, public state-provider values, settings JSON and user UI
  values remain owned by their producers. Their own `revision` fields may differ
  from the Session response. `workflow.state` likewise returns the provider's
  value without rewriting it. Host actions called by a DLL still follow their
  ordinary Session response contracts.
- A standalone document-atomic native action folds provisional child/loan
  increments into one outer publication. Forwarded provider reports keep their
  observed versions, which can exceed the final outer version. They are
  historical observations, not optimistic preconditions for a later edit; use
  the outer response revision instead. This does not rewrite older recordings.
- Captured review preconditions, nested export source plans and replay metadata
  describe the state they captured. Never replace those revisions with the latest
  Session version. See [Selected-only Merge](selected-merge.md) and [Replay](replay.md).

## Publication is not a universal commit counter

This contract preserves existing revision rules, including mutation no-ops,
read-only queries, contact samples and one-publication `layer.batch` behavior.
Active pointer contacts can expose provisional drawing state while recording raw
chunks. A current revision alone does not certify a completed stroke or durable
save; inspect contact/recording state. A later failure can roll back an enclosing
atomic execution. The broader recovery and committed/provisional presentation
work remains in the [roadmap](../ROADMAP.md).

Recording failures do not return a successful state-bearing result. Supported
document rollback restores the previous state and revision; file export can have
written a PNG before recording fails and reports that external-effect boundary
explicitly. This change does not make arbitrary file, network or plugin-private
effects transactional, change recorded outcomes or rerun plugins during replay.

## Example and verification

Run [the JSONL example](../examples/automation/result-revisions.jsonl) to bind a
freshly returned state revision to a guarded nested pipeline. Its earlier rename
snapshot keeps its own revision after a later rename.

`scripts/smoke-result-revisions.py` checks the contract through JSONL, MCP and
optional live GUI attachment with small isolated documents. Session regressions
also cover rollback, native-owned reports and exact pointer/stroke revision
counts. No physical-tablet compatibility or latency claim follows from these
synthetic checks. There is no new per-MOVE serialization, registry lookup, file
I/O or full-image copy, and no dependency, ABI, project or recording-format change.
