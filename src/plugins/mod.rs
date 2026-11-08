//! Startup-loaded contributions. Manifests describe tools, action recipes and panels;
//! Native DLL handlers use the versioned C ABI in micopi-plugin-api.
#[cfg(any(feature = "desktop", test))]
mod command_contact;
pub(crate) mod native;
pub mod nodes;
mod packages;
pub(crate) mod registry;
pub mod tools;
#[cfg(feature = "desktop")]
pub(crate) use command_contact::CommandContactPolicy;
pub use packages::{PluginInfo, PluginPlan, PluginStatus};
pub use registry::*;
/// Match the integer schema for bounded command selectors, including `2.0`.
pub(super) fn bounded_integer(value: &serde_json::Value) -> Option<u64> {
    value
        .as_f64()
        .filter(|number| {
            number.is_finite() && number.fract() == 0. && (0.0..=u32::MAX as f64).contains(number)
        })
        .map(|number| number as u64)
}
pub fn panel_key(id: &str) -> u32 {
    id.bytes().fold(2166136261, |hash, b| {
        (hash ^ b as u32).wrapping_mul(16777619)
    })
}

pub(crate) mod adjustments;
pub(crate) mod affine;
pub(crate) mod brush_presets;
pub(crate) mod brush_tip;
pub(crate) mod clipboard;
pub(crate) mod color_preview;
mod compositing;
mod document;
pub(crate) mod export;
mod geometry;
pub(crate) mod grayscale_preview;
pub(crate) mod groups;
pub(crate) mod histogram;
pub(crate) mod hsl_preview;
pub(crate) mod layer_arrangement;
pub(crate) mod layer_batch;
pub(crate) mod layer_links;
mod layer_transfer;
mod layers;
mod linked_movement;
pub(crate) mod liquify;
mod mask_geometry;
pub(crate) mod movement;
pub(crate) mod owner_affine;
mod owner_movement;
mod painting;
pub(crate) mod quantization_preview;
pub(crate) mod raster_masks;
pub(crate) mod resample;
pub(crate) mod selected_merge;
mod selection;
mod system;
mod system_clipboard;
pub(crate) mod tonal_preview;
mod trim;
pub(crate) mod windows;
mod workbench;
pub(crate) fn execute(
    session: &mut crate::application::Session,
    op: crate::application::Operation,
    args: serde_json::Value,
) -> crate::application::ActionResult {
    use crate::application::Operation::*;
    match op {
        Liquify => liquify::execute(session, args),
        LiquifyPlan => liquify::describe(session, args),
        LiquifyPreview => liquify::preview(session, args),
        LiquifyDialog => liquify::request_dialog(session),
        HslPreview => hsl_preview::execute(session, args),
        TonalPreview => tonal_preview::execute(session, args),
        TonalDialog => tonal_preview::request_dialog(session, args),
        GrayscalePreview => grayscale_preview::execute(session, args),
        GrayscaleDialog => grayscale_preview::request_dialog(session, args),
        ThresholdPreview => quantization_preview::execute(
            session,
            args,
            crate::engine::quantization::Effect::Threshold,
        ),
        PosterizePreview => quantization_preview::execute(
            session,
            args,
            crate::engine::quantization::Effect::Posterize,
        ),
        ThresholdDialog => quantization_preview::request_dialog(
            session,
            args,
            crate::engine::quantization::Effect::Threshold,
        ),
        PosterizeDialog => quantization_preview::request_dialog(
            session,
            args,
            crate::engine::quantization::Effect::Posterize,
        ),
        HslDialog => hsl_preview::request_dialog(session, args),
        BrushAngleAdjust => brush_tip::execute(session, args),
        BrushPresetsQuery | BrushPresetUpsert | BrushPresetDelete | BrushPresetApply
        | BrushPresetsImport | BrushPresetsExport | BrushPresetsReload => {
            brush_presets::execute(session, op, args)
        }
        ArrangeLayers | ArrangeLayersPlan => layer_arrangement::execute(session, op, args),
        AdjustmentCatalog | AdjustmentPreview | AdjustmentQuery | AdjustmentRead
        | AdjustmentCreate | AdjustmentSet | AdjustmentMove | AdjustmentDelete
        | AdjustmentInspect => adjustments::execute(session, op, args),
        MaskQuery | MaskCreate | MaskDelete | MaskProperties | MaskTarget | MaskFill
        | MaskInvert | MaskFromSelection | MaskToSelection | MaskSnapshot | MaskApply => {
            raster_masks::execute(session, op, args)
        }
        MergeLayers | MergeSelectedLayers | MergeSelectedLayersPlan | OpenSelectedMerge => {
            selected_merge::execute(session, op, args)
        }
        GroupQuery | GroupCreate | GroupCreateSelected | GroupProperties | GroupMove
        | GroupUngroup | GroupUngroupParent | GroupDelete | GroupDuplicate | GroupMerge
        | GroupSelectLayers => groups::execute(session, op, args),
        LayerQuery => crate::application::layer_query::execute(session, &args),
        LayerBatch => layer_batch::execute(session, args),
        LayerLinks => layer_links::execute(session, args),
        LayerLinksQuery => Ok(layer_links::query(session)),
        Export | Render | ExportPlan | ExportPreview | ExportDialog => {
            export::execute(session, op, args)
        }
        MovePixels | Nudge => movement::execute(session, op, args),
        TransformRepeat | TransformRepeatCopy => affine::repeat(
            session,
            op == TransformRepeatCopy || args["duplicate"] == true,
        ),
        TransformState => Ok(affine::repeat_state(session)),
        TransformPixels => affine::execute(session, args),
        TransformPreview => affine::request_preview(session, args),
        TransformPlan => affine::describe(session, args),
        CanvasTrim | TrimBounds => trim::execute(session, op, args),
        ImageSize => resample::execute(session, args),
        CanvasGeometryPlan => {
            let plan = crate::application::canvas_geometry::plan(&session.doc, &args)?;
            Ok(plan.state(
                session.revision,
                session.is_drawing(),
                mask_geometry::available(session),
                session.stroke.active() || session.pointer_checkpoint.is_some(),
            ))
        }
        ImageSizePlan => {
            let plan = crate::application::image_size::plan(&session.doc, &args)?;
            Ok(plan.state(
                session.revision,
                session.is_drawing(),
                mask_geometry::available(session),
                session.stroke.active() || session.pointer_checkpoint.is_some(),
            ))
        }
        RotateCanvasClockwise
        | RotateCanvasCounterclockwise
        | RotateCanvasHalf
        | FlipCanvasHorizontal
        | FlipCanvasVertical
        | RotateLayerClockwise
        | RotateLayerCounterclockwise => geometry::execute(session, op),
        SystemClipboardCopy
        | SystemClipboardCopyMerged
        | SystemClipboardCut
        | SystemClipboardPaste
        | SystemClipboardPasteInPlace
        | SystemClipboardState => system_clipboard::execute(session, op),
        LayerViaCopy | LayerViaCut => layer_transfer::execute(session, op),
        ClipboardCopy
        | ClipboardCopyMerged
        | ClipboardCut
        | ClipboardPaste
        | ClipboardPasteInPlace
        | ClipboardState => clipboard::execute(session, op, args),
        SelectionShape | SelectAll | Deselect | Reselect | InvertSelection | ReadSelection
        | SelectColor | GrowSelection | SimilarSelection | ExpandSelection | ContractSelection
        | FeatherSelection | SmoothSelection => selection::execute(session, op, args),
        ReplayStatus | ReplayOpen | ReplayEvents | ReplayVerify | ReplayExport | ReplayLoad
        | ReplaySeek | ReplaySnapshot | ReplayFrames | ReplayRecover => {
            crate::application::replay::execute(session, op, args)
        }
        NodeCatalog | WorkflowRead | WorkflowSet | WorkflowPatch | WorkflowValidate
        | WorkflowRun | WorkflowState => crate::application::workflow::execute(session, op, args),
        GraphRead | GraphSet | GraphCatalog => compositing::execute(session, op, args),
        HistogramQuery => histogram::execute(session, args),
        New | NewPlan | DocumentInfo | DocumentResolution | CanvasSize | Crop | Open | Save
        | Undo | Redo => document::execute(session, op, args),
        AddLayer | RenameLayer | DuplicateLayer | MoveLayer | DeleteLayer | SelectLayer
        | LayerSelection | MergeDown | MergeVisible | FlattenImage | StampVisible
        | LayerProperties | ClearLayer => layers::execute(session, op, args),
        SelectTool | Parameters | Pointer | Stroke | Pick | View | MirrorView
        | MirrorViewVertical => painting::execute(session, op, args),
        WindowCommand | WindowState | WindowLayout | WindowCapture | WindowDrop => {
            windows::execute(session, op, args)
        }
        CaptureWorkspace | SettingsRead | SettingsWrite | Ui => {
            workbench::execute(session, op, args)
        }
        State | Plugins | PluginPlan | Events => system::execute(session, op, args),
        Describe => Ok(crate::application::pipeline::describe(session)),
        ContextRead => crate::application::pipeline::context(session, args),
        PipelineRun => crate::application::pipeline::run(session, args),
    }
}

#[cfg(test)]
#[path = "../../tests/reference/tools.rs"]
pub(crate) mod tool_reference;
