//! Bounded screenshot encoding. UI callbacks only borrow the current framebuffer.
#[cfg(test)]
use eframe::egui;
use eframe::egui::{ColorImage, Context, UserData, ViewportId};
use image::ImageEncoder;
use std::{
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender, TrySendError},
        Arc, Mutex, OnceLock,
    },
};

pub(super) const MAX_CAPTURE_PIXELS: usize = 16 * 1024 * 1024;
const QUEUE_CAPACITY: usize = 2;

pub(super) struct Job {
    pub request_id: Option<u64>,
    pub viewport: ViewportId,
    pub path: PathBuf,
    pub token: UserData,
    pub image: Arc<ColorImage>,
}

pub(super) struct Completion {
    pub request_id: Option<u64>,
    pub viewport: ViewportId,
    pub path: PathBuf,
    pub token: UserData,
    pub result: Result<[u32; 2], String>,
}

struct Shared {
    queued: AtomicUsize,
    available: AtomicBool,
    startup: OnceLock<Result<(), String>>,
    receiver: Mutex<Option<Receiver<Job>>>,
    failure: Mutex<Option<String>>,
    repaint: Mutex<Option<Context>>,
}

impl Shared {
    fn wake(&self) {
        if let Some(ctx) = self.repaint.lock().unwrap().as_ref() {
            ctx.request_repaint_of(ViewportId::ROOT);
        }
    }
}

#[derive(Clone)]
pub(super) struct CaptureSubmitter {
    jobs: SyncSender<Job>,
    completions: Sender<Completion>,
    shared: Arc<Shared>,
}

pub(super) struct Permit {
    submitter: CaptureSubmitter,
    reserved: bool,
}

impl CaptureSubmitter {
    fn start_worker(&self) -> Result<(), String> {
        self.shared
            .startup
            .get_or_init(|| {
                let receiver = self.shared.receiver.lock().unwrap().take().unwrap();
                let worker = self.shared.clone();
                let replies = self.completions.clone();
                std::thread::Builder::new()
                    .name("micopi-capture-writer".into())
                    .spawn(move || {
                        while let Ok(job) = receiver.recv() {
                            worker.queued.fetch_sub(1, Ordering::AcqRel);
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    write_image(&job.path, &job.image)
                                }))
                                .unwrap_or_else(|_| Err("Screenshot encoder panicked".into()));
                            let _ = replies.send(Completion {
                                request_id: job.request_id,
                                viewport: job.viewport,
                                path: job.path,
                                token: job.token,
                                result,
                            });
                            worker.wake();
                        }
                        worker.available.store(false, Ordering::Release);
                    })
                    .map(|_| ())
                    .map_err(|error| {
                        let error = format!("Screenshot encoder could not start: {error}");
                        *self.shared.failure.lock().unwrap() = Some(error.clone());
                        self.shared.available.store(false, Ordering::Release);
                        error
                    })
            })
            .clone()
    }

    /// Reserve a queue slot before a native callback claims or reads a frame.
    /// A full queue leaves the request untouched for a later paint pass.
    pub fn try_reserve(&self) -> Result<Option<Permit>, String> {
        self.start_worker()?;
        if !self.shared.available.load(Ordering::Acquire) {
            return Err(self
                .shared
                .failure
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "Screenshot encoder is unavailable".into()));
        }
        if self
            .shared
            .queued
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |queued| {
                (queued < QUEUE_CAPACITY).then_some(queued + 1)
            })
            .is_err()
        {
            return Ok(None);
        }
        Ok(Some(Permit {
            submitter: self.clone(),
            reserved: true,
        }))
    }

    pub fn complete(&self, completion: Completion) {
        let _ = self.completions.send(completion);
        self.shared.wake();
    }
}

impl Permit {
    pub fn submit(mut self, job: Job) -> Result<(), String> {
        // Every producer owns a slot until this send; the receiver releases one
        // immediately on receipt. The channel therefore cannot become full
        // behind a valid reservation, even while the worker is encoding.
        match self.submitter.jobs.try_send(job) {
            Ok(()) => {
                self.reserved = false;
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                Err("Screenshot encoder queue reservation was lost".into())
            }
            Err(TrySendError::Disconnected(_)) => Err("Screenshot encoder is unavailable".into()),
        }
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        if self.reserved {
            self.submitter.shared.queued.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

pub(super) struct CaptureWriter {
    submitter: CaptureSubmitter,
    completions: Receiver<Completion>,
}

impl Default for CaptureWriter {
    fn default() -> Self {
        let (jobs, receiver) = mpsc::sync_channel::<Job>(QUEUE_CAPACITY);
        let (sender, completions) = mpsc::channel();
        let shared = Arc::new(Shared {
            queued: AtomicUsize::new(0),
            available: AtomicBool::new(true),
            startup: OnceLock::new(),
            receiver: Mutex::new(Some(receiver)),
            failure: Mutex::new(None),
            repaint: Mutex::new(None),
        });
        Self {
            submitter: CaptureSubmitter {
                jobs,
                completions: sender,
                shared,
            },
            completions,
        }
    }
}

impl Drop for CaptureWriter {
    fn drop(&mut self) {
        // Context graphics can retain callbacks holding a submitter. Remove the
        // reverse edge so shutting the studio cannot keep that Context alive.
        // Accepted jobs may finish without blocking the native event loop.
        self.submitter.shared.repaint.lock().unwrap().take();
    }
}

impl CaptureWriter {
    pub fn submitter(&self) -> CaptureSubmitter {
        self.submitter.clone()
    }

    pub fn set_repaint(&self, context: Context) {
        *self.submitter.shared.repaint.lock().unwrap() = Some(context);
    }

    pub fn drain(&self) -> Vec<Completion> {
        self.completions.try_iter().collect()
    }
}

pub(super) fn checked_size(size: [usize; 2]) -> Result<[u32; 2], String> {
    if size[0] == 0
        || size[1] == 0
        || size[0] > u32::MAX as usize
        || size[1] > u32::MAX as usize
        || size[0]
            .checked_mul(size[1])
            .is_none_or(|pixels| pixels > MAX_CAPTURE_PIXELS)
    {
        return Err("Screenshot dimensions must contain between 1 and 16777216 pixels".into());
    }
    Ok([size[0] as u32, size[1] as u32])
}

struct Temporary {
    path: PathBuf,
}

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn write_image(path: &Path, image: &ColorImage) -> Result<[u32; 2], String> {
    let size = checked_size(image.size)?;
    if image.pixels.len() != image.size[0] * image.size[1] {
        return Err("Screenshot pixel count does not match its dimensions".into());
    }
    let rgba: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|color| color.to_array())
        .collect();
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut name = OsString::from(".");
    name.push(
        path.file_name()
            .ok_or("Screenshot destination has no file name")?,
    );
    name.push(format!(
        ".capture-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = path.with_file_name(name);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    let temporary = Temporary { path: temporary };
    let result = (|| {
        let mut writer = std::io::BufWriter::new(&mut file);
        image::codecs::png::PngEncoder::new_with_quality(
            &mut writer,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::Adaptive,
        )
        .write_image(&rgba, size[0], size[1], image::ExtendedColorType::Rgba8)
        .map_err(|error| error.to_string())?;
        writer.flush().map_err(|error| error.to_string())
    })()
    .and_then(|_| file.sync_all().map_err(|error| error.to_string()));
    drop(file);
    result?;
    crate::platform::replace_file(&temporary.path, path).map_err(|error| error.to_string())?;
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_bounds_reject_zero_overflow_and_oversized_images() {
        assert_eq!(checked_size([4096, 4096]).unwrap(), [4096, 4096]);
        for size in [[0, 1], [1, 0], [4096, 4097], [usize::MAX, 2]] {
            assert!(checked_size(size).is_err());
        }
    }

    #[test]
    fn reservations_bound_queued_work_and_unused_permits_release_capacity() {
        let writer = CaptureWriter::default();
        assert!(writer.submitter.shared.startup.get().is_none());
        let submitter = writer.submitter();
        let first = submitter.try_reserve().unwrap().unwrap();
        let second = submitter.try_reserve().unwrap().unwrap();
        assert!(submitter.try_reserve().unwrap().is_none());
        drop(first);
        assert!(submitter.try_reserve().unwrap().is_some());
        drop(second);
    }

    #[test]
    fn encoder_reports_exact_request_identity_and_file_errors() {
        let writer = CaptureWriter::default();
        let submitter = writer.submitter();
        let token = UserData::new(());
        let viewport = ViewportId::from_hash_of("capture-writer-test");
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-output")
            .join(format!("missing-capture-parent-{}", std::process::id()))
            .join("capture.png");
        submitter
            .try_reserve()
            .unwrap()
            .unwrap()
            .submit(Job {
                request_id: Some(17),
                viewport,
                path: path.clone(),
                token: token.clone(),
                image: Arc::new(ColorImage::new([1, 1], vec![egui::Color32::RED])),
            })
            .unwrap();
        let completion = writer
            .completions
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(completion.request_id, Some(17));
        assert_eq!(completion.viewport, viewport);
        assert_eq!(completion.path, path);
        assert_eq!(completion.token, token);
        assert!(completion.result.is_err());
    }

    #[test]
    fn encoder_preserves_pixels_and_existing_destination_on_invalid_input() {
        static NEXT_TEST: AtomicU64 = AtomicU64::new(0);
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-output")
            .join(format!(
                "capture-writer-{}-{}",
                std::process::id(),
                NEXT_TEST.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("frame.png");
        let writer = CaptureWriter::default();
        let submitter = writer.submitter();
        let image = Arc::new(ColorImage::new(
            [3, 2],
            vec![egui::Color32::from_rgb(23, 71, 199); 6],
        ));
        let job = |image| Job {
            request_id: Some(3),
            viewport: ViewportId::ROOT,
            path: path.clone(),
            token: UserData::new(()),
            image,
        };
        submitter
            .try_reserve()
            .unwrap()
            .unwrap()
            .submit(job(image))
            .unwrap();
        let completion = writer
            .completions
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(completion.result.unwrap(), [3, 2]);
        let encoded = std::fs::read(&path).unwrap();
        let decoded = image::load_from_memory(&encoded).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (3, 2));
        assert!(decoded.pixels().all(|pixel| pixel.0 == [23, 71, 199, 255]));

        let mut invalid = ColorImage::new([1, 1], vec![egui::Color32::RED]);
        invalid.size = [2, 2];
        submitter
            .try_reserve()
            .unwrap()
            .unwrap()
            .submit(job(Arc::new(invalid)))
            .unwrap();
        let completion = writer
            .completions
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(completion.result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), encoded);
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn dropping_writer_releases_the_repaint_context() {
        let writer = CaptureWriter::default();
        let submitter = writer.submitter();
        writer.set_repaint(Context::default());
        assert!(submitter.shared.repaint.lock().unwrap().is_some());
        drop(writer);
        assert!(submitter.shared.repaint.lock().unwrap().is_none());
        assert!(submitter.shared.startup.get().is_none());
    }
}
