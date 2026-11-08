//! Cold frontend admission policy over declared action references only.
use super::registry::ActionDefinition;
#[cfg(feature = "desktop")]
use super::registry::Registry;
use crate::application::Operation;
use std::collections::{BTreeMap, BTreeSet};

const MAX_DEPTH: u8 = 8;
const MAX_LEAVES: u16 = 256;

/// Presentation policy, separate from public read-only/atomic capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandContactPolicy {
    Observe,
    SharedAdmission,
    LegacyCleanup,
    InvalidDefinition,
}
impl CommandContactPolicy {
    pub(crate) fn preserves_contact(self) -> bool {
        self != Self::LegacyCleanup
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Cache {
    policies: BTreeMap<String, CommandContactPolicy>,
}
#[derive(Clone, Copy)]
struct Summary {
    policy: CommandContactPolicy,
    leaves: u16,
    // Session checks the accumulated leaf count at every recursive entry,
    // including entry into a macro that eventually contributes no leaves.
    entry: u16,
}
impl Summary {
    fn invalid() -> Self {
        Self {
            policy: CommandContactPolicy::InvalidDefinition,
            leaves: 0,
            entry: 0,
        }
    }
    fn observation() -> Self {
        Self {
            policy: CommandContactPolicy::Observe,
            leaves: 0,
            entry: 0,
        }
    }
    fn leaf(policy: CommandContactPolicy) -> Self {
        Self {
            policy,
            leaves: 1,
            entry: 0,
        }
    }
}

fn operation_policy(operation: Operation) -> CommandContactPolicy {
    if operation.read_only() {
        CommandContactPolicy::Observe
    } else if operation.is_contact_guarded_command()
        || operation.is_mask()
        || matches!(
            operation,
            // Runtime steps/graphs remain opaque here. Session owns atomic
            // admission, dry-run semantics and sequential leaf ordering.
            Operation::PipelineRun
                | Operation::WorkflowRun
                // Group and layer organization include idle-only and
                // conditional admission. The backend decides from actual args.
                | Operation::GroupProperties
                | Operation::GroupUngroup
                | Operation::GroupUngroupParent
                | Operation::GroupMerge
                | Operation::GroupSelectLayers
                | Operation::LayerSelection
                | Operation::LayerProperties
                | Operation::MergeLayers
                | Operation::MergeSelectedLayers
                | Operation::OpenSelectedMerge
                | Operation::MergeVisible
                // These can reject masks, owner locks or source/candidate
                // preflight before their permitted contact completion.
                | Operation::MovePixels
                | Operation::Nudge
                | Operation::TransformPixels
                | Operation::Liquify
                | Operation::LiquifyDialog
                | Operation::TransformPreview
                | Operation::TransformRepeat
                | Operation::TransformRepeatCopy
                | Operation::LayerViaCopy
                | Operation::LayerViaCut
                | Operation::MergeDown
                | Operation::FlattenImage
                | Operation::StampVisible
                | Operation::New
                | Operation::Open
                | Operation::AddLayer
                | Operation::SelectLayer
                | Operation::ClearLayer
                | Operation::SelectionShape
                | Operation::SelectAll
                | Operation::Deselect
                | Operation::Reselect
                | Operation::InvertSelection
                | Operation::ExpandSelection
                | Operation::ContractSelection
                | Operation::FeatherSelection
                | Operation::SmoothSelection
                | Operation::SelectColor
                | Operation::GrowSelection
                | Operation::SimilarSelection
                | Operation::WorkflowSet
                | Operation::WorkflowPatch
                | Operation::GraphSet
                | Operation::ClipboardCopy
                | Operation::ClipboardCopyMerged
                | Operation::ClipboardCut
                | Operation::ClipboardPaste
                | Operation::ClipboardPasteInPlace
                | Operation::SystemClipboardCopy
                | Operation::SystemClipboardCopyMerged
                | Operation::SystemClipboardCut
                | Operation::SystemClipboardPaste
                | Operation::SystemClipboardPasteInPlace
                | Operation::Export
                | Operation::ExportDialog
                | Operation::Stroke
                | Operation::ReplayExport
                | Operation::ReplayFrames
        )
    {
        // Preserve UI entry, not every failed result: an admitted backend may
        // intentionally finish input before a later error. Reconcile afterward.
        CommandContactPolicy::SharedAdmission
    } else {
        CommandContactPolicy::LegacyCleanup
    }
}

fn summarize<'a>(
    id: &'a str,
    remaining_depth: u8,
    actions: &'a BTreeMap<String, ActionDefinition>,
    memo: &mut BTreeMap<(&'a str, u8), Summary>,
    visiting: &mut BTreeSet<&'a str>,
) -> Summary {
    if let Some(summary) = memo.get(&(id, remaining_depth)) {
        return *summary;
    }
    let Some(definition) = actions.get(id) else {
        return Summary::invalid();
    };
    if !visiting.insert(id) {
        return Summary::invalid();
    }
    // Match Session::plan precedence even for definitions inserted directly by
    // tests: an operation wins over native, and native wins over declared steps.
    let summary = if let Some(operation) = definition.operation {
        Summary::leaf(operation_policy(operation))
    } else if definition.native.is_some() {
        Summary::leaf(if definition.is_read_only() {
            CommandContactPolicy::Observe
        } else if definition.requires_idle {
            CommandContactPolicy::SharedAdmission
        } else {
            CommandContactPolicy::LegacyCleanup
        })
    } else {
        let mut summary = Summary::observation();
        for step in &definition.steps {
            let Some(depth) = remaining_depth.checked_sub(1) else {
                summary = Summary::invalid();
                break;
            };
            let child = summarize(&step.action, depth, actions, memo, visiting);
            if child.policy == CommandContactPolicy::InvalidDefinition {
                summary = Summary::invalid();
                break;
            }
            summary.entry = summary.entry.max(summary.leaves + child.entry);
            summary.leaves += child.leaves;
            if summary.entry >= MAX_LEAVES || summary.leaves > MAX_LEAVES {
                summary = Summary::invalid();
                break;
            }
            if summary.policy == CommandContactPolicy::Observe {
                summary.policy = child.policy;
            }
        }
        summary
    };
    visiting.remove(id);
    memo.insert((id, remaining_depth), summary);
    summary
}

impl Cache {
    fn build(actions: &BTreeMap<String, ActionDefinition>) -> Self {
        let mut memo = BTreeMap::new();
        let mut visiting = BTreeSet::new();
        let policies = actions
            .keys()
            .map(|id| {
                let summary = summarize(id, MAX_DEPTH, actions, &mut memo, &mut visiting);
                (id.clone(), summary.policy)
            })
            .collect();
        Self { policies }
    }
    fn get(&self, action: &str) -> CommandContactPolicy {
        self.policies
            .get(action)
            .copied()
            .unwrap_or(CommandContactPolicy::InvalidDefinition)
    }
}
#[cfg(feature = "desktop")]
impl Registry {
    /// Startup contributions are immutable. Direct action-map edits in tests or
    /// editor command registration must refresh this cold, bounded metadata cache.
    pub(crate) fn rebuild_command_contact_policies(&mut self) {
        self.command_contacts = Cache::build(&self.actions);
    }
    pub(crate) fn command_contact_policy(&self, action: &str) -> CommandContactPolicy {
        self.command_contacts.get(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::Action;
    use serde_json::json;

    fn operation(id: &str, operation: Operation) -> ActionDefinition {
        serde_json::from_value(json!({"id":id,"title":id,"operation":operation})).unwrap()
    }
    fn native(id: &str, read_only: bool) -> ActionDefinition {
        serde_json::from_value(json!({"id":id,"title":id,"native":"fixture","read_only":read_only}))
            .unwrap()
    }
    fn declared(id: &str, steps: &[&str]) -> ActionDefinition {
        serde_json::from_value(json!({"id":id,"title":id,"steps":steps.iter().map(|id| Action::new(*id,json!({}))).collect::<Vec<_>>()})).unwrap()
    }
    fn actions() -> BTreeMap<String, ActionDefinition> {
        [
            operation("read", Operation::State),
            operation("plan", Operation::ImageSizePlan),
            operation("guard", Operation::GroupDuplicate),
            operation("legacy", Operation::SelectTool),
            operation("pipeline", Operation::PipelineRun),
            operation("workflow", Operation::WorkflowRun),
            native("native_read", true),
            native("native_mutable", false),
        ]
        .into_iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect()
    }

    #[test]
    fn native_idle_requirement_preserves_frontend_contact_and_macro_admission() {
        let mut actions = actions();
        let mut idle = native("idle_native", false);
        idle.requires_idle = true;
        put(&mut actions, idle);
        put(
            &mut actions,
            declared("idle_macro", &["read", "idle_native", "legacy"]),
        );
        let cache = Cache::build(&actions);
        assert_eq!(
            cache.get("idle_native"),
            CommandContactPolicy::SharedAdmission
        );
        assert_eq!(
            cache.get("idle_macro"),
            CommandContactPolicy::SharedAdmission
        );
        assert_eq!(
            cache.get("native_mutable"),
            CommandContactPolicy::LegacyCleanup
        );
    }
    fn put(actions: &mut BTreeMap<String, ActionDefinition>, definition: ActionDefinition) {
        actions.insert(definition.id.clone(), definition);
    }

    #[test]
    fn declared_contact_policy_keeps_public_capabilities_and_explicit_leaf_ownership() {
        let actions = actions();
        let cache = Cache::build(&actions);
        for id in ["read", "plan", "native_read"] {
            assert_eq!(cache.get(id), CommandContactPolicy::Observe);
        }
        for id in ["guard", "pipeline", "workflow"] {
            assert_eq!(cache.get(id), CommandContactPolicy::SharedAdmission);
        }
        for id in ["legacy", "native_mutable"] {
            assert_eq!(cache.get(id), CommandContactPolicy::LegacyCleanup);
        }
        assert_eq!(
            cache.get("unknown"),
            CommandContactPolicy::InvalidDefinition
        );
        assert!(CommandContactPolicy::Observe.preserves_contact());
        assert!(CommandContactPolicy::SharedAdmission.preserves_contact());
        assert!(CommandContactPolicy::InvalidDefinition.preserves_contact());
        assert!(!CommandContactPolicy::LegacyCleanup.preserves_contact());
        assert!(!actions["pipeline"].is_read_only());
        assert!(!actions["workflow"].is_read_only());
        assert!(actions["native_read"].is_read_only());
    }

    #[test]
    fn declared_contact_policy_preserves_builtin_admission_before_conditional_cleanup() {
        use Operation::*;
        // Each family has backend idle-only, mask/owner access or fallible
        // preflight before completion. Runtime arguments remain authoritative.
        let shared = [
            GroupCreate,
            GroupCreateSelected,
            GroupProperties,
            GroupMove,
            GroupUngroup,
            GroupUngroupParent,
            GroupDelete,
            GroupDuplicate,
            GroupMerge,
            GroupSelectLayers,
            MaskCreate,
            MaskDelete,
            MaskProperties,
            MaskTarget,
            MaskFill,
            MaskInvert,
            MaskFromSelection,
            MaskToSelection,
            MaskApply,
            LayerSelection,
            LayerProperties,
            GraphSet,
            New,
            Open,
            AddLayer,
            SelectLayer,
            ClearLayer,
            CanvasSize,
            ImageSize,
            DocumentResolution,
            TransformPixels,
            Liquify,
            LiquifyDialog,
            TransformPreview,
            TransformRepeat,
            TransformRepeatCopy,
            MovePixels,
            Nudge,
            RotateLayerClockwise,
            LayerViaCopy,
            LayerViaCut,
            MergeDown,
            MergeLayers,
            MergeSelectedLayers,
            OpenSelectedMerge,
            MergeVisible,
            FlattenImage,
            StampVisible,
            LayerBatch,
            SelectionShape,
            Reselect,
            SelectColor,
            GrowSelection,
            ExpandSelection,
            WorkflowSet,
            WorkflowPatch,
            ClipboardCopy,
            ClipboardCopyMerged,
            ClipboardCut,
            ClipboardPaste,
            ClipboardPasteInPlace,
            SystemClipboardCopy,
            SystemClipboardCopyMerged,
            SystemClipboardCut,
            SystemClipboardPaste,
            SystemClipboardPasteInPlace,
            Export,
            ExportDialog,
            ReplayExport,
            ReplayFrames,
            Stroke,
            PipelineRun,
            WorkflowRun,
        ];
        let mut actions = actions();
        for op in shared {
            let id = format!("shared_{op:?}");
            put(&mut actions, operation(&id, op));
            put(
                &mut actions,
                declared(&format!("wrapper_{op:?}"), &["read", &id]),
            );
        }
        let cache = Cache::build(&actions);
        for op in shared {
            let id = format!("shared_{op:?}");
            assert_eq!(
                cache.get(&id),
                CommandContactPolicy::SharedAdmission,
                "{op:?}"
            );
            assert_eq!(
                cache.get(&format!("wrapper_{op:?}")),
                CommandContactPolicy::SharedAdmission,
                "{op:?}"
            );
            assert!(!actions[&id].is_read_only(), "{op:?}");
        }
    }

    #[test]
    fn declared_contact_policy_retains_observation_and_intentional_legacy_contracts() {
        use Operation::*;
        let observe = [
            GroupQuery,
            MaskQuery,
            MaskSnapshot,
            MergeSelectedLayersPlan,
            TransformPlan,
            LiquifyPlan,
            LiquifyPreview,
            ImageSizePlan,
            ExportPlan,
            WorkflowState,
            ReplayVerify,
        ];
        let legacy = [
            Save,
            Undo,
            Redo,
            SelectTool,
            Parameters,
            Ui,
            SettingsWrite,
            WindowCommand,
            ReplayRecover,
        ];
        let mut actions = actions();
        for op in observe.into_iter().chain(legacy) {
            put(&mut actions, operation(&format!("builtin_{op:?}"), op));
        }
        put(
            &mut actions,
            declared(
                "readonly_boundaries",
                &["builtin_MaskSnapshot", "builtin_TransformPlan"],
            ),
        );
        put(
            &mut actions,
            declared(
                "tool_before_guard",
                &["builtin_MaskSnapshot", "builtin_SelectTool", "guard"],
            ),
        );
        let cache = Cache::build(&actions);
        for op in observe {
            let id = format!("builtin_{op:?}");
            assert_eq!(cache.get(&id), CommandContactPolicy::Observe, "{op:?}");
            assert!(actions[&id].is_read_only(), "{op:?}");
        }
        for op in legacy {
            let id = format!("builtin_{op:?}");
            assert_eq!(
                cache.get(&id),
                CommandContactPolicy::LegacyCleanup,
                "{op:?}"
            );
            assert!(!actions[&id].is_read_only(), "{op:?}");
        }
        assert_eq!(
            cache.get("readonly_boundaries"),
            CommandContactPolicy::Observe
        );
        assert!(!actions["readonly_boundaries"].is_read_only());
        assert_eq!(
            cache.get("tool_before_guard"),
            CommandContactPolicy::LegacyCleanup
        );
        assert_eq!(
            cache.get("native_mutable"),
            CommandContactPolicy::LegacyCleanup
        );
    }

    #[test]
    fn declared_contact_policy_uses_first_mutable_leaf_but_checks_the_complete_suffix() {
        let mut actions = actions();
        put(
            &mut actions,
            declared("observe", &["read", "native_read", "plan"]),
        );
        put(
            &mut actions,
            declared("guarded", &["observe", "guard", "legacy"]),
        );
        put(
            &mut actions,
            declared("tool_first", &["read", "legacy", "guard"]),
        );
        put(
            &mut actions,
            declared("native_first", &["native_read", "native_mutable", "guard"]),
        );
        put(&mut actions, declared("bad_suffix", &["guard", "missing"]));
        put(
            &mut actions,
            declared("bad_legacy_suffix", &["legacy", "missing"]),
        );
        put(&mut actions, declared("nested", &["read", "guarded"]));
        let cache = Cache::build(&actions);
        assert_eq!(cache.get("observe"), CommandContactPolicy::Observe);
        assert_eq!(cache.get("guarded"), CommandContactPolicy::SharedAdmission);
        assert_eq!(cache.get("nested"), CommandContactPolicy::SharedAdmission);
        assert_eq!(cache.get("tool_first"), CommandContactPolicy::LegacyCleanup);
        assert_eq!(
            cache.get("native_first"),
            CommandContactPolicy::LegacyCleanup
        );
        assert_eq!(
            cache.get("bad_suffix"),
            CommandContactPolicy::InvalidDefinition
        );
        assert_eq!(
            cache.get("bad_legacy_suffix"),
            CommandContactPolicy::InvalidDefinition
        );
        assert!(
            !actions["observe"].is_read_only(),
            "Frontend analysis must not redefine public macro capabilities"
        );
    }

    #[test]
    fn declared_contact_policy_matches_operation_native_steps_precedence() {
        let mut actions = actions();
        let mut direct = operation("direct", Operation::GroupDuplicate);
        direct.native = Some("fixture".into());
        direct.steps = vec![Action::new("missing", json!({}))];
        put(&mut actions, direct);
        let mut opaque = native("opaque", true);
        opaque.steps = vec![Action::new("missing", json!({}))];
        put(&mut actions, opaque);
        let mut runtime = operation("runtime", Operation::PipelineRun);
        runtime.steps = vec![Action::new("missing", json!({}))];
        put(&mut actions, runtime);
        let cache = Cache::build(&actions);
        assert_eq!(cache.get("direct"), CommandContactPolicy::SharedAdmission);
        assert_eq!(cache.get("opaque"), CommandContactPolicy::Observe);
        assert_eq!(cache.get("runtime"), CommandContactPolicy::SharedAdmission);
    }

    #[test]
    fn declared_contact_policy_rejects_cycles_and_matches_relative_depth_without_cache_poisoning() {
        let mut actions = actions();
        put(&mut actions, declared("cycle_a", &["read", "cycle_b"]));
        put(&mut actions, declared("cycle_b", &["guard", "cycle_a"]));
        let mut previous = "guard".to_string();
        for depth in 1..=9 {
            let id = format!("depth_{depth}");
            put(&mut actions, declared(&id, &[&previous]));
            previous = id;
        }
        let cache = Cache::build(&actions);
        assert_eq!(
            cache.get("cycle_a"),
            CommandContactPolicy::InvalidDefinition
        );
        assert_eq!(
            cache.get("cycle_b"),
            CommandContactPolicy::InvalidDefinition
        );
        assert_eq!(cache.get("depth_8"), CommandContactPolicy::SharedAdmission);
        assert_eq!(
            cache.get("depth_9"),
            CommandContactPolicy::InvalidDefinition
        );
        assert_eq!(cache.get("guard"), CommandContactPolicy::SharedAdmission);
    }

    #[test]
    fn declared_contact_policy_matches_exact_leaf_entry_limits_including_empty_tail() {
        let mut actions = actions();
        put(&mut actions, declared("empty", &[]));
        put(&mut actions, declared("limit", &vec!["read"; 256]));
        put(&mut actions, declared("over", &vec!["read"; 257]));
        put(&mut actions, declared("tail", &["limit", "empty"]));
        put(&mut actions, declared("head", &["empty", "limit"]));
        put(&mut actions, declared("pair", &["read", "read"]));
        put(&mut actions, declared("nested_limit", &vec!["pair"; 128]));
        put(&mut actions, declared("nested_over", &vec!["pair"; 129]));
        let cache = Cache::build(&actions);
        for id in ["empty", "limit", "head", "nested_limit"] {
            assert_eq!(cache.get(id), CommandContactPolicy::Observe, "{id}");
        }
        for id in ["over", "tail", "nested_over"] {
            assert_eq!(
                cache.get(id),
                CommandContactPolicy::InvalidDefinition,
                "{id}"
            );
        }
    }

    #[test]
    fn declared_contact_policy_bounds_repeated_zero_leaf_graphs_without_expanding_paths() {
        let mut actions = actions();
        put(&mut actions, declared("empty", &[]));
        let mut previous = "empty".to_string();
        for depth in 1..=9 {
            let id = format!("empty_{depth}");
            put(&mut actions, declared(&id, &vec![previous.as_str(); 256]));
            previous = id;
        }
        let cache = Cache::build(&actions);
        assert_eq!(cache.get("empty_8"), CommandContactPolicy::Observe);
        assert_eq!(
            cache.get("empty_9"),
            CommandContactPolicy::InvalidDefinition
        );
    }
}
