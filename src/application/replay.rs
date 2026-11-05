//! Session journal boundary and read-only outcome replay. No recorded command is reexecuted.
use super::{ActionError, ActionResult, Operation, Session};
use crate::engine::replay::{Journal, Metadata, Source, State, MAX_ARCHIVE};
use serde_json::{json, Value};
use std::path::PathBuf;
impl Session {
    pub(crate) fn replay_metadata(&self) -> Metadata {
        Metadata {
            width: self.doc.width,
            height: self.doc.height,
            active: self.doc.active,
            composition: self.doc.composition(),
            selected_tool: self.selected_tool.clone(),
            tool: self.tool,
            parameters: self.parameters.clone(),
            view: self.view,
            revision: self.revision,
            dirty: self.doc.dirty,
            file: self.file.clone(),
            source_file: self.source_file.clone(),
            ui: self.ui.clone(),
            drawing: self.is_drawing(),
            render_error: self.doc.render_error.clone(),
        }
    }
    pub(crate) fn replay_ensure(&mut self) -> Result<(), ActionError> {
        if let Some(error) = &self.replay_error {
            return Err(ActionError::new(
                "replay_storage",
                format!("Recording stopped after a journal error: {error}"),
            ));
        }
        if self.journal.is_none() {
            let meta = self.replay_metadata();
            let environment = self.registry.describe();
            self.journal = Some(Journal::new(&mut self.doc, meta, environment)?);
            self.replay_identity = self.doc.identity;
        }
        Ok(())
    }
    pub(crate) fn replay_flush(
        &mut self,
        complete: bool,
        error: Option<String>,
    ) -> Result<(), ActionError> {
        if self.replay_samples.is_empty() {
            return Ok(());
        }
        let samples = std::mem::take(&mut self.replay_samples);
        let result = self.replay_commit(
            Source::Pointer { samples, complete },
            error.is_none(),
            error,
        );
        if result.is_err() {
            if let Some((saved, mark)) = self.pointer_checkpoint.take() {
                let rollback = self.journal.as_mut().unwrap().rollback(mark);
                saved.restore(self);
                if let Err(error) = rollback {
                    self.replay_error = Some(format!("Contact rollback failed: {error}"));
                }
            }
        }
        self.replay_samples = Vec::with_capacity(256);
        result
    }
    pub(crate) fn replay_boundary(&mut self) -> Result<(), ActionError> {
        if self.atomic_depth != 0 {
            return Ok(());
        }
        self.replay_ensure()?;
        self.replay_flush(false, None)?;
        let meta = self.replay_metadata();
        let journal = self.journal.as_ref().unwrap();
        let prepared = journal.prepare(
            &mut self.doc,
            meta,
            Source::Observed {
                reason: "State synchronized before an operation".into(),
            },
            true,
            None,
        );
        let p = match prepared {
            Ok(p) => p,
            Err(error) => {
                self.replay_error = Some(error.to_string());
                return Err(error);
            }
        };
        if Journal::changed(&p) {
            if let Err(error) = self.journal.as_mut().unwrap().commit(p) {
                self.replay_error = Some(error.to_string());
                return Err(error);
            }
        }
        Ok(())
    }
    pub(crate) fn replay_commit(
        &mut self,
        source: Source,
        success: bool,
        error: Option<String>,
    ) -> Result<(), ActionError> {
        if self.atomic_depth != 0 {
            return Ok(());
        }
        self.replay_ensure()?;
        let metadata = self.replay_metadata();
        let result = (|| {
            let prepared = self.journal.as_ref().unwrap().prepare(
                &mut self.doc,
                metadata,
                source,
                success,
                error,
            )?;
            self.journal.as_mut().unwrap().commit(prepared)
        })();
        if let Err(error) = &result {
            self.replay_error = Some(error.to_string());
        }
        result
    }
    pub(crate) fn replay_action(
        &mut self,
        id: &str,
        args: &Value,
        op: Option<Operation>,
        success: bool,
        error: Option<String>,
    ) -> Result<(), ActionError> {
        if self.atomic_depth != 0 {
            return Ok(());
        }
        if let Some(journal) = self.restored_replay.take() {
            self.journal = Some(journal);
            self.replay_identity = self.doc.identity;
            self.replay_error = None;
        } else if self.replay_identity != self.doc.identity && op != Some(Operation::New) {
            self.journal = None;
            self.replay_error = None;
            self.replay_identity = self.doc.identity;
        }
        let context = if success
            && matches!(
                op,
                Some(
                    Operation::TransformPixels
                        | Operation::TransformRepeat
                        | Operation::TransformRepeatCopy
                )
            ) {
            self.transform_event.take()
        } else if success && matches!(op, Some(Operation::MovePixels | Operation::Nudge)) {
            Some(
                serde_json::json!({"movement":crate::plugins::movement::context(self,op.unwrap(),args)}),
            )
        } else if success && op.is_some_and(Operation::system_clipboard_mutation) {
            self.system_clipboard_event.take()
        } else if success
            && matches!(
                op,
                Some(
                    Operation::ClipboardCopy
                        | Operation::ClipboardCopyMerged
                        | Operation::ClipboardCut
                        | Operation::ClipboardPaste
                        | Operation::ClipboardPasteInPlace
                )
            )
        {
            Some(serde_json::json!({"clipboard": crate::plugins::clipboard::state(self)}))
        } else {
            None
        };
        let source = Source::Action {
            action: id.into(),
            args: args.clone(),
            depth: self.execution_depth,
            context,
        };
        if success && op == Some(Operation::New) {
            // Publish a new document recording only after its genesis and action
            // are durable; retain the old recording for the caller's rollback.
            let meta = self.replay_metadata();
            let mut replacement =
                Journal::new(&mut self.doc, meta.clone(), self.registry.describe())?;
            #[cfg(test)]
            if let Some(old) = self.journal.as_mut() {
                old.transfer_pending_write_fault(&mut replacement);
            }
            let prepared = replacement.prepare(&mut self.doc, meta, source, true, None)?;
            replacement.commit(prepared)?;
            self.journal = Some(replacement);
            self.replay_identity = self.doc.identity;
            self.replay_error = None;
            return Ok(());
        }
        if let Some((path, old_file, old_dirty, old_revision)) = self.pending_save.take() {
            // A saved project is a full verification boundary, including callers of
            // the public engine API which may have written pixel storage directly.
            self.doc.invalidate();
            let count = self.doc.width.div_ceil(crate::engine::TILE)
                * self.doc.height.div_ceil(crate::engine::TILE);
            self.doc
                .replay_tiles
                .extend((0..self.doc.layers.len()).flat_map(|l| (0..count).map(move |id| (l, id))));
            let preparation = (|| {
                self.replay_ensure()?;
                let meta = self.replay_metadata();
                let p = self.journal.as_ref().unwrap().prepare(
                    &mut self.doc,
                    meta,
                    source,
                    true,
                    None,
                )?;
                let bytes = self.journal.as_mut().unwrap().bytes(Some(&p))?;
                Ok::<_, ActionError>((p, bytes))
            })();
            let (p, bytes) = match preparation {
                Ok(value) => value,
                Err(error) => {
                    self.file = old_file;
                    self.doc.dirty = old_dirty;
                    self.revision = old_revision;
                    self.replay_error = Some(error.to_string());
                    return Err(error);
                }
            };
            let mark = self.journal.as_ref().unwrap().checkpoint();
            if let Err(error) = self.journal.as_mut().unwrap().commit(p) {
                self.file = old_file;
                self.doc.dirty = old_dirty;
                self.revision = old_revision;
                self.replay_error = Some(error.to_string());
                return Err(error);
            }
            match self.doc.save_with_replay(&path, Some(&bytes)) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    self.file = old_file;
                    self.doc.dirty = old_dirty;
                    self.revision = old_revision;
                    if let Err(rollback) = self.journal.as_mut().unwrap().rollback(mark) {
                        self.replay_error = Some(rollback.to_string());
                        return Err(rollback);
                    }
                    self.replay_commit(
                        Source::Action {
                            action: id.into(),
                            args: args.clone(),
                            depth: self.execution_depth,
                            context: None,
                        },
                        false,
                        Some(error.to_string()),
                    )?;
                    return Err(error.into());
                }
            }
        }
        let _ = op;
        self.replay_commit(source, success, error)
    }
    pub fn replay_status(&self) -> Value {
        let mut status = self
            .journal
            .as_ref()
            .map(Journal::status)
            .unwrap_or(json!({"recording":true,"events":0,"frames":0,"bytes":0}));
        status["pending_samples"] = json!(self.replay_samples.len());
        status["transaction_pending"] =
            json!(self.atomic_depth != 0 || self.pointer_checkpoint.is_some());
        status["committed_head_hash"] = self
            .pointer_checkpoint
            .as_ref()
            .map(|(_, m)| json!(m.head_hash()))
            .unwrap_or_else(|| status["head_hash"].clone());
        status["committed_events"] = self
            .pointer_checkpoint
            .as_ref()
            .map(|(_, m)| json!(m.event_count()))
            .unwrap_or_else(|| status["events"].clone());
        status["fault"] = json!(self.replay_error);
        status["recording"] = json!(self.replay_error.is_none());
        status["loaded"] = self
            .loaded_replay
            .as_ref()
            .map(Journal::status)
            .unwrap_or(Value::Null);
        status
    }
    pub fn replay_journal(&mut self, loaded: bool) -> Result<&mut Journal, ActionError> {
        if loaded {
            self.loaded_replay.as_mut().ok_or_else(|| {
                ActionError::new("replay_missing", "No replay archive has been loaded")
            })
        } else {
            self.journal.as_mut().ok_or_else(|| {
                ActionError::new("replay_missing", "No operations have been recorded yet")
            })
        }
    }
    pub fn replay_preview(&mut self, index: usize, loaded: bool) -> Result<State, ActionError> {
        self.replay_journal(loaded)?.preview(index)
    }
}
pub fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    let loaded = args["source"].as_str() == Some("loaded");
    let path = || {
        args["path"]
            .as_str()
            .map(PathBuf::from)
            .ok_or_else(|| ActionError::new("invalid_arguments", "Missing path"))
    };
    match op {
        ReplayOpen => {
            s.ui_effects.push_back(json!({"id":"replay.open"}));
            Ok(json!({"opened":true,"runtime":s.runtime}))
        }
        ReplayStatus => Ok(s.replay_status()),
        ReplayEvents => s.replay_journal(loaded)?.events(
            args["start"].as_u64().unwrap_or(0) as usize,
            args["limit"].as_u64().unwrap_or(64) as usize,
        ),
        ReplayVerify => {
            if !loaded && s.replay_error.is_some() {
                return Err(ActionError::new("replay_incomplete","The live document has an uncommitted recording failure; only the stored prefix can be trusted"));
            }
            let mut result = s
                .replay_journal(loaded)?
                .verify(args["expected_head"].as_str())?;
            let pending = !loaded && s.pointer_checkpoint.is_some();
            result["scope"] = json!(if pending {
                "provisional contact prefix"
            } else {
                "committed events"
            });
            result["transaction_pending"] = json!(pending || s.atomic_depth != 0);
            result["pending_samples"] = json!(if loaded { 0 } else { s.replay_samples.len() });
            result["stroke_in_progress"] = json!(!loaded && s.is_drawing());
            Ok(result)
        }
        ReplayExport => {
            if !loaded && s.pointer_checkpoint.is_some() {
                return Err(ActionError::new(
                    "stroke_active",
                    "Finish the current contact before sealing a replay archive",
                ));
            }
            let path = path()?;
            s.replay_journal(loaded)?.export(&path)?;
            Ok(json!({"path":path,"head_hash":s.replay_journal(loaded)?.status()["head_hash"]}))
        }
        ReplayLoad => {
            let file = path()?;
            let journal = Journal::from_path(&file)?;
            let status = journal.status();
            s.loaded_replay = Some(journal);
            Ok(status)
        }
        ReplaySeek | ReplaySnapshot => {
            let index = args["event"]
                .as_u64()
                .map(|n| n as usize)
                .unwrap_or(s.replay_journal(loaded)?.entries.len() - 1);
            let state = s.replay_preview(index, loaded)?;
            if op == ReplaySnapshot {
                let bytes = png(&state)?;
                Ok(
                    json!({"mimeType":"image/png","data":super::codec::base64(&bytes),"width":state.metadata.width,"height":state.metadata.height,"event":index}),
                )
            } else {
                Ok(
                    json!({"event":index,"metadata":state.metadata,"state_hash":crate::engine::replay::hex(&state.digest()),"surface_hash":crate::engine::replay::hex(&state.surface_hash())}),
                )
            }
        }
        ReplayFrames => {
            if !loaded && s.pointer_checkpoint.is_some() {
                return Err(ActionError::new(
                    "stroke_active",
                    "Finish the current contact before exporting replay frames",
                ));
            }
            let directory = path()?;
            if directory.exists() && std::fs::read_dir(&directory)?.next().is_some() {
                return Err(ActionError::new(
                    "replay_output_exists",
                    "Frame export requires a new or empty directory",
                ));
            }
            std::fs::create_dir_all(&directory)?;
            let limit = args["max_frames"].as_u64().unwrap_or(300) as usize;
            let candidates: Vec<_> = s
                .replay_journal(loaded)?
                .entries
                .iter()
                .filter(|e| e.visual)
                .map(|e| e.sequence)
                .collect();
            let step = candidates.len().div_ceil(limit.max(1)).max(1);
            let mut selected: Vec<_> = candidates.into_iter().step_by(step).collect();
            let last = s.replay_journal(loaded)?.entries.len() - 1;
            if selected.last() != Some(&last) {
                if selected.len() >= limit.max(1) {
                    selected.pop();
                }
                selected.push(last);
            }
            let mut frames = vec![];
            for (number, index) in selected.into_iter().enumerate() {
                let state = s.replay_preview(index, loaded)?;
                let name = format!("frame-{number:06}.png");
                std::fs::write(directory.join(&name), png(&state)?)?;
                frames.push(json!({"file":name,"event":index,"state_hash":crate::engine::replay::hex(&state.digest())}));
            }
            let manifest = json!({"format":"MiCoPi PNG time-lapse frames","fps":args["fps"].as_u64().unwrap_or(12),"head_hash":s.replay_journal(loaded)?.status()["head_hash"],"frames":frames});
            std::fs::write(
                directory.join("manifest.json"),
                serde_json::to_vec_pretty(&manifest)?,
            )?;
            Ok(json!({"path":directory,"frames":manifest["frames"].as_array().unwrap().len()}))
        }
        ReplayRecover => {
            let path = path()?;
            super::codec::export(&mut s.doc, &path)?;
            Ok(
                json!({"path":path,"verified_recording":false,"purpose":"Recover current canvas independently of a failed journal"}),
            )
        }
        _ => Err(ActionError::new("internal", "Invalid replay operation")),
    }
}
pub fn png(state: &State) -> Result<Vec<u8>, ActionError> {
    super::png_io::encode(
        state.metadata.width,
        state.metadata.height,
        state.pixels(),
        state.metadata.composition.document_info.export_density(),
    )
}

pub fn read_embedded(doc: &mut crate::engine::Document) -> Result<Option<Journal>, ActionError> {
    if let Some(bytes) = doc.replay_data.take() {
        if bytes.len() > MAX_ARCHIVE {
            return Err(ActionError::new("replay_limit", "Oversized project replay"));
        }
        let journal = Journal::import(&bytes)?;
        if !journal.state.matches_document(doc) {
            return Err(ActionError::new(
                "replay_integrity",
                "Saved document does not match its replay head",
            ));
        }
        Ok(Some(journal))
    } else {
        Ok(None)
    }
}
