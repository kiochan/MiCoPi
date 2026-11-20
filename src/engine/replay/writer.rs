//! One writer with a bounded queue of completed contact snapshots.
use super::*;
use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
    time::Duration,
};

const MAX_TAILS: usize = 4;
const MAX_TAIL_BYTES: usize = 32 * 1024 * 1024;

struct Draft {
    captured: Captured,
    samples: usize,
    bytes: usize,
    complete: bool,
    #[cfg(test)]
    fail_after: Option<usize>,
    #[cfg(test)]
    gate: Option<Receiver<()>>,
}

struct Job {
    captured: Captured,
    previous: Arc<State>,
    head: Hash,
    length: u64,
    #[cfg(test)]
    fail_after: Option<usize>,
    #[cfg(test)]
    gate: Option<Receiver<()>>,
}

type Completion = (Result<Prepared, ActionError>, Duration);

pub(super) struct Writer {
    jobs: Option<SyncSender<Job>>,
    completions: Receiver<Completion>,
    thread: Option<JoinHandle<()>>,
    current: Option<(usize, usize, bool)>,
    waiting: VecDeque<Draft>,
}

fn disconnected() -> ActionError {
    ActionError::new("replay_storage", "Background replay writer stopped")
}

impl Writer {
    fn start(path: PathBuf) -> Result<Self, ActionError> {
        let (jobs, input) = mpsc::sync_channel::<Job>(1);
        let (output, completions) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("micopi-replay".into())
            .spawn(move || {
                // A separate open gives readers and the writer independent seek positions.
                let mut file = OpenOptions::new().write(true).open(path);
                while let Ok(job) = input.recv() {
                    #[cfg(test)]
                    if let Some(gate) = job.gate {
                        let _ = gate.recv_timeout(Duration::from_secs(10));
                    }
                    let start = Instant::now();
                    let result = (|| {
                        let file = file
                            .as_mut()
                            .map_err(|e| ActionError::new("replay_storage", e.to_string()))?;
                        let prepared =
                            Journal::encode(job.captured, &job.previous, job.head, job.length)?;
                        Journal::append(
                            file,
                            job.length,
                            &prepared,
                            #[cfg(test)]
                            job.fail_after,
                        )?;
                        Ok(prepared)
                    })();
                    if output.send((result, start.elapsed())).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            jobs: Some(jobs),
            completions,
            thread: Some(thread),
            current: None,
            waiting: VecDeque::new(),
        })
    }

    fn submit(
        &mut self,
        mut draft: Draft,
        previous: &Arc<State>,
        head: Hash,
        length: u64,
        sequence: usize,
    ) -> Result<(), ActionError> {
        debug_assert!(self.current.is_none());
        draft.captured.event.sequence = sequence;
        self.jobs
            .as_ref()
            .unwrap()
            .try_send(Job {
                captured: draft.captured,
                previous: previous.clone(),
                head,
                length,
                #[cfg(test)]
                fail_after: draft.fail_after,
                #[cfg(test)]
                gate: draft.gate,
            })
            .map_err(|_| disconnected())?;
        self.current = Some((draft.samples, draft.bytes, draft.complete));
        Ok(())
    }

    fn complete(&mut self, wait: bool) -> Option<Completion> {
        self.current?;
        let completion = if wait {
            self.completions
                .recv()
                .map_err(|_| TryRecvError::Disconnected)
        } else {
            self.completions.try_recv()
        };
        if matches!(completion, Err(TryRecvError::Empty)) {
            return None;
        }
        self.current = None;
        Some(completion.unwrap_or_else(|_| (Err(disconnected()), Duration::ZERO)))
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        drop(self.jobs.take());
        // Exactly one job can be outstanding and the reply channel holds one reply.
        // Join before Journal removes the spool file (also on Windows).
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Journal {
    pub(crate) fn in_flight_samples(&self) -> usize {
        self.writer.as_ref().map_or(0, |writer| {
            writer.current.map_or(0, |job| job.0)
                + writer.waiting.iter().map(|job| job.samples).sum::<usize>()
        })
    }

    pub(crate) fn can_queue_tail(&self, doc: &Document) -> bool {
        let Some(writer) = &self.writer else {
            return true;
        };
        let pending = usize::from(writer.current.is_some()) + writer.waiting.len();
        if pending == 0 {
            return true;
        }
        let (_, current_bytes, complete) = writer.current.unwrap_or((0, 0, true));
        complete
            && pending < MAX_TAILS
            && current_bytes
                + writer.waiting.iter().map(|job| job.bytes).sum::<usize>()
                + Self::capture_pixel_budget(doc)
                <= MAX_TAIL_BYTES
    }

    fn capture_pixel_budget(doc: &Document) -> usize {
        let raw = doc.replay_tiles.len() * TILE * TILE * 4;
        let mask = doc.replay_mask_tiles.len() * TILE * TILE;
        let surface = if doc.requires_renderer() {
            doc.replay_surface
                .regions(false)
                .first()
                .map_or(0, |[x0, y0, x1, y1]| {
                    (x1.div_ceil(TILE) - x0 / TILE)
                        * (y1.div_ceil(TILE) - y0 / TILE)
                        * TILE
                        * TILE
                        * 4
                })
        } else {
            0
        };
        raw + mask + surface
    }

    pub(crate) fn queue_pointer_segment(
        &mut self,
        doc: &mut Document,
        metadata: Metadata,
        samples: Vec<Pointer>,
        complete: bool,
    ) -> Result<(), ActionError> {
        debug_assert!(self.in_flight_samples() == 0 || (complete && self.can_queue_tail(doc)));
        if self.writer.is_none() {
            self.writer = Some(Writer::start(self.path.clone())?);
        }
        let sample_count = samples.len();
        let bytes = Self::capture_pixel_budget(doc);
        let captured = self.capture(
            doc,
            metadata,
            Source::Pointer { samples, complete },
            true,
            None,
            true,
        )?;
        let draft = Draft {
            captured,
            samples: sample_count,
            bytes,
            complete,
            #[cfg(test)]
            fail_after: self.fail_after.take(),
            #[cfg(test)]
            gate: self.writer_gate.take(),
        };
        let writer = self.writer.as_mut().unwrap();
        if writer.current.is_some() || !writer.waiting.is_empty() {
            writer.waiting.push_back(draft);
            Ok(())
        } else {
            writer.submit(
                draft,
                &self.state,
                self.head,
                self.length,
                self.entries.len(),
            )
        }
    }

    pub(crate) fn poll_pointer_segment(
        &mut self,
        wait: bool,
    ) -> Option<(Result<bool, ActionError>, Duration)> {
        let writer = self.writer.as_mut()?;
        if writer.current.is_none() {
            if let Some(draft) = writer.waiting.pop_front() {
                if let Err(error) = writer.submit(
                    draft,
                    &self.state,
                    self.head,
                    self.length,
                    self.entries.len(),
                ) {
                    writer.waiting.clear();
                    return Some((Err(error), Duration::ZERO));
                }
            }
        }
        let (result, elapsed) = writer.complete(wait)?;
        if result.is_err() {
            writer.waiting.clear();
        }
        let complete = result.map(|prepared| {
            let complete = matches!(
                prepared.event.source,
                Source::Pointer { complete: true, .. }
            );
            self.publish(prepared);
            complete
        });
        Some((complete, elapsed))
    }
}
