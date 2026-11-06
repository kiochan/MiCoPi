//! Bounded presentation diagnostics, independent of document state and replay.
use super::{ActionError, Session};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};

const LIMIT: usize = 128;

#[derive(Clone, Serialize)]
struct Record {
    request_id: u64,
    target: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    panel: Option<String>,
    path: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Default)]
pub(crate) struct Captures {
    next_id: u64,
    pending: BTreeMap<u64, Record>,
    recent: VecDeque<Record>,
}

impl Session {
    pub(crate) fn queue_capture(
        &mut self,
        panel: Option<&str>,
        path: &str,
    ) -> Result<u64, ActionError> {
        if path.is_empty() || path.len() > 32768 {
            return Err(ActionError::new(
                "invalid_arguments",
                "Capture paths must contain 1 to 32768 UTF-8 bytes",
            ));
        }
        if self.presentation_captures.pending.len() >= LIMIT {
            return Err(ActionError::new(
                "capture_queue_full",
                "Wait for pending captures to finish before requesting another capture",
            ));
        }
        let id = self.presentation_captures.next_id + 1;
        if id > u32::MAX as u64 {
            return Err(ActionError::new(
                "capture_id_limit",
                "Capture request identities are exhausted for this session",
            ));
        }
        self.presentation_captures.next_id = id;
        self.presentation_captures.pending.insert(
            id,
            Record {
                request_id: id,
                target: if panel.is_some() {
                    "panel"
                } else {
                    "workspace"
                },
                panel: panel.map(str::to_owned),
                path: path.to_owned(),
                status: "queued",
                width: None,
                height: None,
                error: None,
            },
        );
        Ok(id)
    }

    /// Completion changes only bounded diagnostics, never document/UI metadata.
    /// A late or duplicate callback cannot complete a different request.
    pub(crate) fn finish_capture(
        &mut self,
        request_id: u64,
        result: Result<[u32; 2], String>,
    ) -> bool {
        let Some(mut record) = self.presentation_captures.pending.remove(&request_id) else {
            return false;
        };
        match result {
            Ok([width, height]) if width > 0 && height > 0 => {
                record.status = "saved";
                record.width = Some(width);
                record.height = Some(height);
            }
            outcome => {
                record.status = "failed";
                let message = outcome
                    .err()
                    .unwrap_or_else(|| "Capture produced an empty image".into());
                record.error = Some(message.chars().take(4096).collect());
            }
        }
        if self.presentation_captures.recent.len() == LIMIT {
            self.presentation_captures.recent.pop_front();
        }
        self.presentation_captures.recent.push_back(record);
        true
    }

    pub(crate) fn capture_diagnostics(&self) -> Value {
        let captures = &self.presentation_captures;
        // Pending and completed records have distinct ownership. Completion order
        // can differ across viewports, so clients correlate by request_id.
        json!({
            "pending": captures.pending.values().collect::<Vec<_>>(),
            "recent": captures.recent,
            "last": captures.recent.back(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{application::Action, plugins::Registry};

    fn desktop() -> Session {
        let mut session = Session::new(Registry::bundled()).unwrap();
        session.runtime = "desktop";
        session
    }

    #[test]
    fn interleaved_capture_completions_are_observable_without_changing_recorded_state() {
        let mut session = desktop();
        let before = session.state();
        let composition = session.doc.composition();
        let first = session
            .apply(Action::new("ui.capture", json!({"path":"first.png"})))
            .unwrap()["result"]["request_id"]
            .as_u64()
            .unwrap();
        let second = session
            .apply(Action::new(
                "window.capture",
                json!({"panel":"canvas","path":"second.png"}),
            ))
            .unwrap()["result"]["request_id"]
            .as_u64()
            .unwrap();
        assert!(first < second);
        assert_eq!(session.state(), before);
        let recorded = session.replay_journal(false).unwrap().entries.len();
        let state = session.state();
        assert!(session.finish_capture(second, Err("Viewport was docked".into())));
        assert!(session.finish_capture(first, Ok([1700, 1150])));
        assert!(!session.finish_capture(second, Ok([1, 1])));
        assert!(!session.finish_capture(second + 100, Ok([1, 1])));
        assert_eq!(session.state(), state);
        assert_eq!(
            session.replay_journal(false).unwrap().entries.len(),
            recorded
        );
        // Captures retain their authored external-effect audit boundary without
        // staling a document-bound draft or adding UI metadata on completion.
        assert_eq!(session.doc.composition(), composition);
        let diagnostics = session.capture_diagnostics();
        assert_eq!(diagnostics["pending"], json!([]));
        assert_eq!(diagnostics["recent"][0]["request_id"], second);
        assert_eq!(diagnostics["recent"][0]["status"], "failed");
        assert_eq!(diagnostics["recent"][1]["width"], 1700);
        let query = session
            .apply(Action::new("window.list", json!({})))
            .unwrap();
        assert_eq!(query["result"]["capture_requests"], diagnostics);
        assert_eq!(session.state(), state);
    }

    #[test]
    fn admission_and_terminal_history_are_bounded_without_losing_pending_requests() {
        let mut session = desktop();
        let mut ids = Vec::new();
        for _ in 0..LIMIT {
            ids.push(session.queue_capture(None, "same.png").unwrap());
        }
        let before = session.capture_diagnostics();
        let state = session.state();
        let effects = session.ui_effects.clone();
        assert_eq!(
            session
                .queue_capture(None, "overflow.png")
                .unwrap_err()
                .code,
            "capture_queue_full"
        );
        assert_eq!(session.capture_diagnostics(), before);
        for action in ["ui.capture", "window.capture"] {
            let args = if action == "ui.capture" {
                json!({"path":"overflow.png"})
            } else {
                json!({"panel":"canvas","path":"overflow.png"})
            };
            assert_eq!(
                session.apply(Action::new(action, args)).unwrap_err().code,
                "capture_queue_full"
            );
            assert_eq!(session.capture_diagnostics(), before);
            assert_eq!(session.ui_effects, effects);
            assert_eq!(session.state(), state);
        }
        for id in ids {
            assert!(session.finish_capture(id, Ok([1, 1])));
        }
        for _ in 0..LIMIT + 1 {
            let id = session.queue_capture(Some("canvas"), "next.png").unwrap();
            session.finish_capture(id, Ok([2, 3]));
        }
        let diagnostics = session.capture_diagnostics();
        assert_eq!(diagnostics["recent"].as_array().unwrap().len(), LIMIT);
        assert!(diagnostics["recent"][0]["request_id"].as_u64().unwrap() > LIMIT as u64);
    }

    #[test]
    fn invalid_paths_id_exhaustion_and_empty_frames_have_explicit_stable_outcomes() {
        let mut session = desktop();
        let before = session.capture_diagnostics();
        for path in [String::new(), "墨".repeat(10923)] {
            assert_eq!(
                session.queue_capture(None, &path).unwrap_err().code,
                "invalid_arguments"
            );
            assert_eq!(session.capture_diagnostics(), before);
        }
        let id = session.queue_capture(None, "frame.png").unwrap();
        assert_eq!(id, 1);
        assert!(session.finish_capture(id, Ok([0, 3])));
        assert_eq!(session.capture_diagnostics()["last"]["status"], "failed");
        session.presentation_captures.next_id = u32::MAX as u64;
        let diagnostics = session.capture_diagnostics();
        assert_eq!(
            session
                .queue_capture(None, "overflow.png")
                .unwrap_err()
                .code,
            "capture_id_limit"
        );
        assert_eq!(session.capture_diagnostics(), diagnostics);
    }
}
