//! One in-flight provisional segment. Only Session publishes acknowledged outcomes.
use super::*;
use std::{
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
    time::Duration,
};

struct Job {
    captured: (Event, usize),
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
    samples: usize,
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
            samples: 0,
        })
    }

    fn submit(&mut self, job: Job, samples: usize) -> Result<(), ActionError> {
        debug_assert_eq!(self.samples, 0);
        self.jobs
            .as_ref()
            .unwrap()
            .try_send(job)
            .map_err(|_| disconnected())?;
        self.samples = samples;
        Ok(())
    }

    fn complete(&mut self, wait: bool) -> Option<Completion> {
        if self.samples == 0 {
            return None;
        }
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
        self.samples = 0;
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
        self.writer.as_ref().map_or(0, |writer| writer.samples)
    }

    pub(crate) fn queue_pointer_segment(
        &mut self,
        doc: &mut Document,
        metadata: Metadata,
        samples: Vec<Pointer>,
    ) -> Result<(), ActionError> {
        debug_assert_eq!(self.in_flight_samples(), 0);
        if self.writer.is_none() {
            self.writer = Some(Writer::start(self.path.clone())?);
        }
        let sample_count = samples.len();
        let captured = self.capture(
            doc,
            metadata,
            Source::Pointer {
                samples,
                complete: false,
            },
            true,
            None,
        )?;
        let job = Job {
            captured,
            previous: self.state.clone(),
            head: self.head,
            length: self.length,
            #[cfg(test)]
            fail_after: self.fail_after.take(),
            #[cfg(test)]
            gate: self.writer_gate.take(),
        };
        self.writer.as_mut().unwrap().submit(job, sample_count)
    }

    pub(crate) fn poll_pointer_segment(
        &mut self,
        wait: bool,
    ) -> Option<(Result<(), ActionError>, Duration)> {
        let (result, elapsed) = self.writer.as_mut()?.complete(wait)?;
        Some((result.map(|prepared| self.publish(prepared)), elapsed))
    }
}
