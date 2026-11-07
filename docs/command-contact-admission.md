# Command contact admission

Desktop command entry uses a private cached policy to decide whether it may
perform the existing frontend contact cleanup before dispatch. Inspecting a
command or opening an argument draft must not complete a native Brush/Hand
contact before Session can apply its own admission rules. This policy also
protects externally driven contacts and unrelated local gesture previews.

The policy is a presentation detail. It does not change public `read_only`,
document-atomic declarations, action schemas, plugin ABI or revision rules.
Desktop controls, macros, MCP and headless clients continue to execute through
the same [Session and pipeline](ai-pipeline.md). Frontend preservation at entry
is not a promise that every later failure leaves a contact active.

## Cached policies

| Policy | Command entry behavior |
| --- | --- |
| `Observe` | Retain contact and local gesture state while observing; includes read-only built-ins, declared read-only native actions and valid all-observation declared expansions |
| `SharedAdmission` | Retain contact at entry and let Session decide from actual arguments, document capabilities, locks and execution order |
| `LegacyCleanup` | Retain the existing frontend finish/cleanup behavior for commands such as intentional tool/parameter changes |
| `InvalidDefinition` | Avoid premature cleanup for an unresolved/cyclic/out-of-bounds declared expansion; ordinary validation remains responsible for reporting the error |

Classification comes from explicit built-in operation contracts and native
read-only declarations. It is not inferred from an action name, JSON field or
output shape. Conditional handlers belong to `SharedAdmission`: for example,
layer organization flags can reject a live contact, while another admitted
layer-property change may complete it. A masked geometry or clipboard request
must reach its existing preflight rather than losing input in the frontend.

The desktop registry builds this bounded metadata cache after contributions
load. Command registration refreshes it when needed, including test-only direct
registry edits. UI routing uses cached lookups instead of expanding definitions
on each frame or pointer MOVE. A production CLI built without the `desktop`
feature has no frontend cache field, allocation or startup analysis. The analysis
helper stays window-free and its pure tests also compile in headless test builds. Plugin
contribution changes still follow the existing restart contract; this adds no
dynamic DLL reload mechanism.

## Declared expansions and runtime containers

The declared-reference walk follows Session's dispatch precedence: a built-in
operation, then a native handler, then declared steps. Built-in and native
handlers are opaque leaves; ignored `steps` fields on synthetic mixed definitions
do not become another execution path.

A declared expansion inherits its first non-observation leaf's entry policy.
All-observation expansions retain `Observe`. The complete remaining reference
structure is still checked: a missing action, cycle, excessive relative depth or
leaf/entry budget in a suffix makes the whole expansion `InvalidDefinition`,
even if an earlier leaf already chose a policy.

| Declared sequence | Entry policy |
| --- | --- |
| Read state, read native statistics | `Observe` |
| Read state, duplicate a group | `SharedAdmission` |
| Read state, select a tool, duplicate a group | `LegacyCleanup` |
| Duplicate a group, reference an unknown action | `InvalidDefinition` |

The metadata walk accounts for Session's eight-level declared expansion depth
and 256-leaf planning budget, including its accumulated-leaf check at every
recursive entry. Memoization keeps repeated declared-reference
analysis bounded. Cache tests include manually inserted empty tails to check
entry accounting; the real loader requires exactly one valid implementation
and rejects an otherwise empty action definition. Those synthetic fixtures do
not add empty commands to the package contract or promise efficient execution
of arbitrary manually injected exponential runtime graphs.

This is structural analysis of declared references. Schemas, literal arguments,
stable IDs, owner access and native capabilities remain Session's responsibility.
The cache does not execute a macro or predict whether its requested edit succeeds.

`pipeline.run` and `workflow.run` are `SharedAdmission` leaves. Their supplied
steps/graphs are not recursively guessed by the frontend. A non-atomic runtime
container can observe an active contact, reject at its first guarded edit, or
intentionally finish input at a tool/parameter prefix before continuing. Existing
atomic admission rejects active contacts, including atomic dry runs. Existing
runtime depth/action budgets, rollback and partial-report contracts remain in
[Atomic Operations](atomic-operations.md) and [Workflow Nodes](workflow-nodes.md).

## Argument drafts and the parameter/view fence

Command entry, the Commands palette and contact-preserving argument forms fence
automatic publication of the editor's local painting parameters and view. A form
frame must not send an incidental `brush.set` or `view.set` before its explicit
Run action. This matters when a pen stream remains active or an external pointer
MOVE advances Session revision while a read-only form is open.

Observation forms do not acquire an automatic mutation `expected_revision` guard
or a stale-mutation warning merely because the pointer advanced. Mutating/runtime
forms retain their applicable revision and shared admission rules. Clicking Run
dispatches the actual registered action; editing fields alone does not execute it.

Closing or cancelling a preserving command draft discards its pending local
painting/view synchronization and resynchronizes those visible fields from
Session before normal automatic pushes resume. It does not commit stale values
on the next ordinary frame. Artist layer/group name drafts remain in their panel
context and are retained separately. A separately queued preserving command also
defers a lost-focus name commit until shared admission can occur.

The command form and canvas modal routing share this fence so a release over a
form does not become another painting path. Other settings, binding, transform,
replay and window controls retain their established modal policies.

## Reconcile actual backend transitions

The frontend records whether a native contact existed before dispatch. It clears
local painting/pen/panning ownership when that original contact ends or is
replaced. The identity uses the document and original recording checkpoint,
which stay fixed while sample chunks flush; it does not use the changing live
journal length or pointer timestamp. This reconciliation runs after successful
results, ordinary errors and soft-failure reports such as a partial pipeline
result.

A non-atomic pipeline or one IPC poll can finish a local contact and start an
external contact before returning. Clearing the old local ownership must not
send an extra UP to that replacement. The new backend stream and its pressure
remain intact, and drawing feedback follows the current Session. A query with
no initial backend contact does not cancel an unrelated local preview.

An admitted sequential tool/parameter prefix can finish input and then encounter
a later group error. Its local flags must reflect the ended backend contact; a
failure does not restore input unless the shared rollback contract actually does
so. Conversely, a shared rejection that retains the backend contact must retain
the local flags and pressure stream. Recorded chunks and pending samples are
counted together when checking continuity.

Backend inactivity alone is insufficient to clear local state. A Move outline
or selection gesture can exist without a native tool contact. Observing state
must not cancel that preview merely because Session was idle before and after
the query. Existing bespoke static controls retain their own admission policies;
the cached dynamic-command policy does not make every desktop action observational.

## Source ownership and verification scope

| Source | Responsibility |
| --- | --- |
| `src/plugins/command_contact.rs` | Window-free private policy/analysis helper and pure structural/classification tests; production cache integration is desktop-only |
| `src/plugins/registry.rs` | Contribution loading and desktop-only cache lifecycle |
| `src/application/session.rs`, `src/application/actions.rs` | Authoritative validation, operation contracts, revision, contacts, execution and rollback |
| `src/desktop/commands.rs` | Dynamic command entry, palette/cancellation routing and existing static controls |
| `plugins/workbench/command_panel.rs` | Argument drafts, explicit Run and observation/mutation revision behavior |
| `src/desktop/mod.rs` | Parameter/view synchronization fence and actual-contact transition reconciliation |
| `plugins/layers/panel.rs` | Lost-focus name deferral and shared pending-command dispatch |
| `plugins/layers/contact_admission_ui_tests.rs` | Actual Studio command/form routes, macro/runtime cases, raw/history/draft snapshots and local/external pressure continuity |

After building and staging matching native plugins as described in
[Development](development.md), run the focused regressions:

```powershell
cargo test --locked -j 1 --lib contact_admission_ui_tests -- --test-threads=1
cargo test --locked -j 1 --no-default-features --lib declared_contact_policy -- --test-threads=1
```

The actual UI regressions cover command entry, Run/Cancel, palette transitions,
direct macro result synchronization, replacement contacts and continuation,
including exact raw masks and recorded-plus-pending pressure samples. They do
not establish physical tablet compatibility or pen latency. The broader
rollback, document lifecycle and UI-routing audit remains open in the roadmap.
Use the local CPU/priority limits in the development guide when running these
checks on the maintainer's desktop.
