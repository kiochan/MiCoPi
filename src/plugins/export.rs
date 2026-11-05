//! Borrowed-frame PNG export; native sampling never edits the source document.
use crate::application::{codec, output_size, ActionError, ActionResult, Operation, Session};
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    if op == Operation::ExportDialog {
        if s.runtime != "desktop" {
            return Err(ActionError::new(
                "ui_unavailable",
                "Export options require the desktop",
            ));
        }
        require_idle(s)?;
        let parameters = args.get("parameters").cloned().unwrap_or_else(|| json!({}));
        let plan = output_size::plan([s.doc.width, s.doc.height], s.doc.info(), &parameters)?;
        if s.ui_effects.len() >= 128 {
            return Err(ActionError::new(
                "presentation_queue_full",
                "Wait for queued presentation commands before retrying",
            ));
        }
        s.ui_effects
            .push_back(json!({"id":"canvas.export.open","value":parameters}));
        return Ok(json!({"queued":true,"plan":plan.state(s.revision,s.is_drawing())}));
    }
    let plan = output_size::plan([s.doc.width, s.doc.height], s.doc.info(), &args)?;
    if op == Operation::ExportPlan {
        return Ok(plan.state(s.revision, s.is_drawing()));
    }
    let destination = if op == Operation::Export {
        require_idle(s)?;
        let path = args["path"]
            .as_str()
            .map(PathBuf::from)
            .ok_or_else(|| ActionError::new("invalid_arguments", "Missing path"))?;
        guard_destination(s, &path)?;
        Some(path)
    } else {
        None
    };
    let bytes = render(s, &plan)?;
    let mut result = plan.state(s.revision, s.is_drawing());
    if let Some(path) = destination {
        use sha2::{Digest, Sha256};
        let digest = crate::engine::replay::hex(&Sha256::digest(&bytes).into());
        // Recheck immediately before replacement after any native/encoding work.
        guard_destination(s, &path)?;
        codec::write_png(&path, &bytes)?;
        result["path"] = json!(path);
        result["sha256"] = json!(digest);
        // Bind the published buffer to its compact external-artifact context.
        // Outcome replay retains this claim without reopening or rewriting files.
        s.export_event = Some(json!({"export":{
            "path":path,"sha256":digest,"width":plan.width,"height":plan.height,
            "document_info":plan.info.state(plan.width,plan.height),"method":plan.method
        }}));
    } else {
        result["mimeType"] = json!("image/png");
        result["data"] = json!(codec::base64(&bytes));
    }
    Ok(result)
}

fn render(s: &mut Session, plan: &output_size::Plan) -> Result<Vec<u8>, ActionError> {
    s.doc.compose();
    if let Some(error) = &s.doc.render_error {
        return Err(ActionError::new("compositing", error));
    }
    if plan.identity() {
        // Old DLLs and plugin-free normal composition retain identity output.
        crate::application::png_io::encode(
            plan.width,
            plan.height,
            s.doc.composite.iter().copied(),
            plan.info.export_density(),
        )
    } else {
        let pixels = super::native::resample_pixels(
            s,
            &plan.method.native(),
            &s.doc.composite,
            plan.source[0],
            plan.source[1],
            plan.width,
            plan.height,
        )
        .map_err(|error| ActionError::new("export_resample", error))?;
        crate::application::png_io::encode(
            plan.width,
            plan.height,
            pixels.into_iter(),
            plan.info.export_density(),
        )
    }
}

fn normalized(path: &Path) -> Result<PathBuf, ActionError> {
    if let Ok(path) = path.canonicalize() {
        return Ok(path);
    }
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut lexical = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                lexical.pop();
            }
            _ => lexical.push(component.as_os_str()),
        }
    }
    if let (Some(parent), Some(name)) = (lexical.parent(), lexical.file_name()) {
        if let Ok(parent) = parent.canonicalize() {
            return Ok(parent.join(name));
        }
    }
    Ok(lexical)
}
fn same_target(a: &Path, b: &Path) -> Result<bool, ActionError> {
    let a = normalized(a)?;
    let b = normalized(b)?;
    #[cfg(windows)]
    {
        Ok(a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase())
    }
    #[cfg(not(windows))]
    {
        Ok(a == b)
    }
}
fn guard_destination(s: &Session, path: &Path) -> Result<(), ActionError> {
    let active_project = s
        .file
        .as_ref()
        .map(|file| same_target(file, path))
        .transpose()?
        .unwrap_or(false);
    if active_project {
        return Err(ActionError::new(
            "project_overwrite",
            "PNG export cannot replace a native project",
        ));
    }
    codec::guard_png_destination(path)
}

fn require_idle(s: &Session) -> Result<(), ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before exporting",
        ));
    }
    Ok(())
}
