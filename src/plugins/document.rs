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
        New => {
            s.finish_stroke();
            s.doc = Document::new(
                args["width"].as_f64().unwrap() as usize,
                args["height"].as_f64().unwrap() as usize,
            );
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
        Export => {
            let path = path()?;
            s.finish_stroke();
            codec::export(&mut s.doc, &path)?;
            Ok(json!({"path":path}))
        }
        Render => {
            let bytes = codec::png(&mut s.doc)?;
            Ok(
                json!({"mimeType":"image/png","data":codec::base64(&bytes),"width":s.doc.width,"height":s.doc.height}),
            )
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
