//! Presentation-only screenshot identities, FIFO acknowledgement and root readback.
use super::{capture_writer, Studio};
#[cfg(test)]
use eframe::egui;
use eframe::egui::{ColorImage, Context, Event, UserData, ViewportCommand, ViewportId};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub(super) const CAPTURE_QUEUE_LIMIT: usize = 128;
pub(super) const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub(super) struct PendingCapture {
    pub path: PathBuf,
    pub request_id: Option<u64>,
    pub panel: Option<String>,
    pub token: UserData,
    pub created: Instant,
    pub claimed: Arc<AtomicBool>,
    issued: bool,
    image: Option<Arc<ColorImage>>,
}
impl PendingCapture {
    pub(super) fn new(path: PathBuf, request_id: Option<u64>) -> Self {
        Self {
            path,
            request_id,
            panel: None,
            token: UserData::new(()),
            created: Instant::now(),
            claimed: Arc::new(AtomicBool::new(false)),
            issued: false,
            image: None,
        }
    }
    pub(super) fn matches(&self, token: &UserData) -> bool {
        // egui preserves the Arc identity when it clones screenshot UserData.
        self.token == *token
    }
    fn request_root(&mut self, ctx: &Context) {
        if !self.issued && !self.claimed.load(Ordering::Acquire) {
            ctx.send_viewport_cmd_to(
                ViewportId::ROOT,
                ViewportCommand::Screenshot(self.token.clone()),
            );
            self.issued = true;
        }
    }
    fn receive_root(&mut self, events: &[Event]) {
        if self.image.is_some() || self.claimed.load(Ordering::Acquire) {
            return;
        }
        self.image = events.iter().find_map(|event| match event {
            Event::Screenshot {
                viewport_id,
                user_data,
                image,
            } if *viewport_id == ViewportId::ROOT && self.matches(user_data) => Some(image.clone()),
            _ => None,
        });
    }
    fn submit_root(&mut self, writer: &capture_writer::CaptureSubmitter) -> Result<(), String> {
        if self.image.is_none() || self.claimed.load(Ordering::Acquire) {
            return Ok(());
        }
        let Some(permit) = writer.try_reserve()? else {
            return Ok(());
        };
        if self
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(());
        }
        permit.submit(capture_writer::Job {
            request_id: self.request_id,
            viewport: ViewportId::ROOT,
            path: self.path.clone(),
            token: self.token.clone(),
            image: self.image.take().unwrap(),
        })
    }
}
fn acknowledge_root(
    pending: &mut VecDeque<PendingCapture>,
    completion: &capture_writer::Completion,
) -> Option<PendingCapture> {
    if completion.viewport != ViewportId::ROOT
        || !pending.front().is_some_and(|front| {
            front.matches(&completion.token)
                && front.request_id == completion.request_id
                && front.path == completion.path
        })
    {
        return None;
    }
    pending.pop_front()
}
impl Studio {
    pub(super) fn queue_root_capture(&mut self, path: PathBuf, request_id: Option<u64>) {
        if self.pending_captures.len() >= CAPTURE_QUEUE_LIMIT {
            self.finish_root_capture(
                PendingCapture::new(path, request_id),
                Err("Workspace capture queue is full".into()),
            );
        } else {
            self.pending_captures
                .push_back(PendingCapture::new(path, request_id));
        }
    }
    fn finish_root_capture(&mut self, request: PendingCapture, result: Result<[u32; 2], String>) {
        if let Some(id) = request.request_id {
            self.editor.session.finish_capture(id, result.clone());
        }
        self.editor.status = match result {
            Ok(_) => format!("Screenshot Saved: {}", request.path.display()),
            Err(error) => error,
        };
    }
    pub(super) fn poll_captures(&mut self, ctx: &Context) {
        self.capture_writer.set_repaint(ctx.clone());
        for completion in self.capture_writer.drain() {
            if let Some(request) = acknowledge_root(&mut self.pending_captures, &completion) {
                self.finish_root_capture(request, completion.result);
            } else if completion.viewport == ViewportId::ROOT
                && self.preview.as_ref().is_some_and(|p| {
                    p.matches(&completion.token)
                        && p.request_id == completion.request_id
                        && p.path == completion.path
                })
            {
                let request = self.preview.take().unwrap();
                self.finish_root_capture(request, completion.result);
                self.allow_close = true;
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            } else {
                self.finish_native_capture(&completion);
            }
        }
        let submitter = self.capture_writer.submitter();
        loop {
            let Some(front) = self.pending_captures.front_mut() else {
                break;
            };
            let result = if !front.claimed.load(Ordering::Acquire)
                && front.created.elapsed() >= CAPTURE_TIMEOUT
            {
                Err("Workspace capture timed out before readback".into())
            } else {
                ctx.input(|i| front.receive_root(&i.events));
                front.request_root(ctx);
                front.submit_root(&submitter)
            };
            match result {
                Ok(()) => break,
                Err(error) => {
                    let request = self.pending_captures.pop_front().unwrap();
                    self.finish_root_capture(request, Err(error));
                }
            }
        }
        if let Some(preview) = &mut self.preview {
            let result = if !preview.claimed.load(Ordering::Acquire)
                && preview.created.elapsed() >= CAPTURE_TIMEOUT
            {
                Err("Preview capture timed out before readback".into())
            } else {
                ctx.input(|i| preview.receive_root(&i.events));
                preview.submit_root(&submitter)
            };
            if let Err(error) = result {
                let request = self.preview.take().unwrap();
                self.finish_root_capture(request, Err(error));
                self.allow_close = true;
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            }
        }
        if !self.pending_captures.is_empty() || self.preview.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
    pub(super) fn request_preview_capture(&mut self, ctx: &Context) {
        if let Some(preview) = &mut self.preview {
            preview.request_root(ctx);
        }
    }
}
#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
