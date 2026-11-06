//! Built-in document plugin action handlers; callable without a GUI.
use crate::application::{ActionError, ActionResult, Operation, Session};
use crate::{application::codec, engine::Document};
use serde_json::{json, Value};
use std::path::PathBuf;
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    let path = || {
        args["path"]
            .as_str()
            .map(PathBuf::from)
            .ok_or_else(|| ActionError::new("invalid_arguments", "Missing path"))
    };
    match op {
        CanvasSize | Crop => {
            super::mask_geometry::require_idle(s)?;
            let plan = crate::application::canvas_geometry::for_operation(&s.doc, op, &args)
                .map_err(|e| {
                    if e.code == "invalid_crop" {
                        e
                    } else {
                        ActionError::new("canvas_size", e.message)
                    }
                })?;
            let (width, height, dx, dy) = (plan.width, plan.height, plan.dx, plan.dy);
            if (width, height, dx, dy) == (s.doc.width, s.doc.height, 0, 0) {
                return Ok(s.state());
            }
            let masks = super::mask_geometry::stage(
                s,
                width,
                height,
                &super::mask_geometry::Sampling::Reframe(micopi_plugin_api::A8Reframe {
                    x: dx,
                    y: dy,
                    fill: 255,
                    reserved: 0,
                }),
            )
            .map_err(|e| ActionError::new("canvas_size", e))?;
            s.doc
                .reframe_with_masks(width, height, dx, dy, masks)
                .map_err(|e| ActionError::new("canvas_size", e))?;
            Ok(s.state())
        }
        DocumentInfo => Ok(s.doc.info().state(s.doc.width, s.doc.height)),
        DocumentResolution => {
            require_idle(s)?;
            s.doc
                .set_resolution(args["resolution_ppi"].as_f64().unwrap() as u32)
                .map_err(|e| ActionError::new("document_info", e))?;
            Ok(s.doc.info().state(s.doc.width, s.doc.height))
        }
        NewPlan => {
            let plan = crate::application::creation::plan(&args)?;
            Ok(plan.state(s.revision, s.is_drawing()))
        }
        New => {
            require_idle(s)?;
            let plan = crate::application::creation::plan(&args)?;
            s.doc = Document::new(plan.width, plan.height);
            s.doc.initialize_info(plan.info);
            s.file = None;
            s.doc.renderer = s.registry.renderer();
            if let Some(graph) = s.workflow_stack.first() {
                s.doc.workflow = graph.clone();
                s.doc.dirty = !graph.nodes.is_empty();
            }
            s.source_file = None;
            s.view = Default::default();
            Ok(s.state())
        }
        Open => {
            let path = path()?;
            let project = codec::is_project(&path)?;
            let mut doc = codec::open(&path)?;
            let journal = crate::application::replay::read_embedded(&mut doc)?;
            doc.renderer = s.registry.renderer();
            doc.ensure_graph_renderer()
                .map_err(|e| ActionError::new("renderer_missing", e))?;
            if let Some(graph) = s.workflow_stack.first() {
                if doc.workflow != *graph {
                    doc.workflow = graph.clone();
                    doc.dirty = true;
                }
            }
            if doc.requires_renderer() && doc.renderer.is_none() {
                return Err(ActionError::new(
                    "renderer_missing",
                    "Project requires the compositing DLL",
                ));
            }
            s.finish_stroke();
            s.doc = doc;
            if let Some(journal) = journal {
                let meta = &journal.state.metadata;
                s.parameters = meta.parameters.clone();
                s.selected_tool = meta.selected_tool.clone();
                s.tool = meta.tool;
                s.revision = s.revision.max(meta.revision);
                s.restored_replay = Some(journal);
            }
            s.source_file = Some(path.clone());
            s.file = if project { Some(path) } else { None };
            s.view = Default::default();
            Ok(s.state())
        }
        Save => {
            let path = path()?;
            s.finish_stroke();
            s.pending_save = Some((path.clone(), s.file.clone(), s.doc.dirty, s.revision));
            s.file = Some(path.clone());
            s.doc.dirty = false;
            Ok(json!({"path":path}))
        }
        Undo => {
            s.finish_stroke();
            s.doc.undo();
            Ok(s.state())
        }
        Redo => {
            s.finish_stroke();
            s.doc.redo();
            Ok(s.state())
        }

        _ => Err(ActionError::new(
            "internal",
            "operation routed to wrong plugin",
        )),
    }
}

fn require_idle(s: &Session) -> Result<(), ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before changing document setup",
        ));
    }
    Ok(())
}
