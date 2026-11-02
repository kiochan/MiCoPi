//! Same-user Windows named pipe. The worker queues requests; only the UI thread
//! executes them, so MCP and a physical pen never mutate a document concurrently.
use super::batch::{line, Backend};
use crate::application::{Action, ActionError, Session};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    io::{self, BufReader, Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
type Handle = *mut c_void;
#[link(name = "kernel32")]
extern "system" {
    fn CreateNamedPipeW(
        name: *const u16,
        access: u32,
        mode: u32,
        instances: u32,
        out_size: u32,
        in_size: u32,
        timeout: u32,
        security: Handle,
    ) -> Handle;
    fn ConnectNamedPipe(pipe: Handle, overlapped: Handle) -> i32;
    fn DisconnectNamedPipe(pipe: Handle) -> i32;
    fn GetLastError() -> u32;
    fn ReadFile(
        file: Handle,
        buffer: *mut u8,
        size: u32,
        read: *mut u32,
        overlapped: Handle,
    ) -> i32;
    fn WriteFile(
        file: Handle,
        buffer: *const u8,
        size: u32,
        written: *mut u32,
        overlapped: Handle,
    ) -> i32;
    fn SetNamedPipeHandleState(
        pipe: Handle,
        mode: *const u32,
        count: *const u32,
        timeout: *const u32,
    ) -> i32;
}
// Byte-mode, nonblocking I/O lets both ends enforce deadlines without leaving
// uncancellable synchronous reads, writes or FlushFileBuffers on the worker.
const PIPE_NOWAIT: u32 = 1;
const IO_TIMEOUT: Duration = Duration::from_secs(30);
struct PipeIo {
    file: File,
    stop: Arc<AtomicBool>,
    deadline: Instant,
}
impl PipeIo {
    fn check(&self) -> io::Result<()> {
        if self.stop.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Pipe service stopped",
            ));
        }
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Pipe I/O timed out",
            ));
        }
        Ok(())
    }
    fn pause(&self) -> io::Result<()> {
        self.check()?;
        std::thread::sleep(Duration::from_millis(5));
        Ok(())
    }
}
impl Read for PipeIo {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        loop {
            self.check()?;
            // std::fs::File translates ERROR_NO_DATA into EOF on Windows.
            // Preserve it here so an empty nonblocking pipe remains retryable.
            let mut count = 0;
            if unsafe {
                ReadFile(
                    self.file.as_raw_handle(),
                    bytes.as_mut_ptr(),
                    bytes.len().min(u32::MAX as usize) as u32,
                    &mut count,
                    null_mut(),
                )
            } != 0
            {
                return Ok(count as usize);
            }
            let error = io::Error::last_os_error();
            match error.raw_os_error() {
                Some(232) => self.pause()?,
                Some(109) => return Ok(0),
                _ => return Err(error),
            }
        }
    }
}
impl Write for PipeIo {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        loop {
            self.check()?;
            let mut count = 0;
            if unsafe {
                WriteFile(
                    self.file.as_raw_handle(),
                    bytes.as_ptr(),
                    // A nonblocking write larger than the pipe's quota can
                    // report zero progress even when its buffer is empty.
                    bytes.len().min(8192) as u32,
                    &mut count,
                    null_mut(),
                )
            } != 0
            {
                if count != 0 {
                    return Ok(count as usize);
                }
                self.pause()?;
            } else {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(232) {
                    return Err(error);
                }
                self.pause()?;
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Endpoint {
    version: u32,
    pid: u32,
    pipe: String,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Actions,
    Dispatch { action: Action },
}
pub struct Pending {
    request: Request,
    reply: mpsc::Sender<Result<Value, ActionError>>,
    deadline: Instant,
}
pub struct Server {
    receiver: mpsc::Receiver<Pending>,
    stop: Arc<AtomicBool>,
    endpoint: Endpoint,
    path: PathBuf,
}
impl Server {
    pub fn start(
        path: PathBuf,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, ActionError> {
        Self::start_with_timeout(path, wake, IO_TIMEOUT)
    }
    fn start_with_timeout(
        path: PathBuf,
        wake: impl Fn() + Send + Sync + 'static,
        timeout: Duration,
    ) -> Result<Self, ActionError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let endpoint = Endpoint {
            version: 1,
            pid: std::process::id(),
            pipe: format!(
                r"\\.\pipe\micopi-{}-{nonce}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ),
        };
        let (sender, receiver) = mpsc::sync_channel::<Pending>(64);
        let (ready_tx, ready_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_endpoint = endpoint.clone();
        let wake = Arc::new(wake);
        std::thread::Builder::new()
            .name("micopi-action-pipe".into())
            .spawn(move || {
                let mut first = true;
                while !worker_stop.load(Ordering::Acquire) {
                    let name: Vec<u16> =
                        worker_endpoint.pipe.encode_utf16().chain(Some(0)).collect();
                    let handle = unsafe {
                        CreateNamedPipeW(
                            name.as_ptr(),
                            3 | 0x00080000,
                            8 | PIPE_NOWAIT,
                            1,
                            65536,
                            65536,
                            0,
                            null_mut(),
                        )
                    };
                    if handle as isize == -1 {
                        if first {
                            let _ = ready_tx.send(Err(ActionError::new(
                                "ipc",
                                std::io::Error::last_os_error().to_string(),
                            )));
                        }
                        break;
                    }
                    let mut pipe = PipeIo {
                        file: unsafe { File::from_raw_handle(handle) },
                        stop: worker_stop.clone(),
                        deadline: Instant::now() + timeout,
                    };
                    if first {
                        let _ = ready_tx.send(Ok(()));
                        first = false;
                    }
                    let connected = loop {
                        let result = unsafe { ConnectNamedPipe(handle, null_mut()) };
                        let error = unsafe { GetLastError() };
                        if result == 0 && error == 535 {
                            break true;
                        }
                        if (result == 0 && error != 536) || worker_stop.load(Ordering::Acquire) {
                            break false;
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    };
                    if connected && !worker_stop.load(Ordering::Acquire) {
                        pipe.deadline = Instant::now() + timeout;
                        let reply = (|| -> Result<Value, ActionError> {
                            let text = line(&mut BufReader::new(&mut pipe))?
                                .ok_or_else(|| ActionError::new("ipc", "Empty request"))?;
                            let request: Request = serde_json::from_str(&text)?;
                            let (tx, rx) = mpsc::channel();
                            sender
                                .try_send(Pending {
                                    request,
                                    reply: tx,
                                    deadline: Instant::now() + Duration::from_secs(30),
                                })
                                .map_err(|_| ActionError::new("closed", "GUI is closed"))?;
                            wake();
                            rx.recv_timeout(Duration::from_secs(30)).map_err(|_| {
                                ActionError::new(
                                    "timeout_unknown",
                                    "GUI request timed out; query state before retrying",
                                )
                            })?
                        })();
                        let value = match reply {
                            Ok(value) => json!({"ok":true,"data":value}),
                            Err(error) => json!({"ok":false,"error":error}),
                        };
                        pipe.deadline = Instant::now() + timeout;
                        if serde_json::to_writer(&mut pipe, &value).is_ok()
                            && pipe.write_all(b"\n").is_ok()
                        {
                            // Disconnect only after the client has consumed the full
                            // response. An unread response or missing ack is bounded.
                            let _ = line(&mut BufReader::new(&mut pipe));
                        }
                    }
                    unsafe {
                        DisconnectNamedPipe(handle);
                    }
                }
            })?;
        ready_rx
            .recv_timeout(Duration::from_secs(3))
            .map_err(|_| ActionError::new("ipc", "Local action service startup timed out"))??;
        let server = Self {
            receiver,
            stop,
            endpoint,
            path,
        };
        if let Some(parent) = server.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = server.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&server.endpoint)?)?;
        crate::platform::replace_file(&tmp, &server.path)?;
        Ok(server)
    }
    pub fn poll(&self, session: &mut Session) -> usize {
        let mut count = 0;
        while count < 64 {
            let Ok(pending) = self.receiver.try_recv() else {
                break;
            };
            count += 1;
            let result = if Instant::now() > pending.deadline {
                Err(ActionError::new(
                    "expired",
                    "Request expired before execution",
                ))
            } else {
                match pending.request {
                    Request::Actions => session.actions(),
                    Request::Dispatch { action } => session.apply(action),
                }
            };
            let _ = pending.reply.send(result);
        }
        count
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.endpoint.pipe);
        if std::fs::read(&self.path)
            .ok()
            .and_then(|s| serde_json::from_slice::<Endpoint>(&s).ok())
            .is_some_and(|e| e.pipe == self.endpoint.pipe)
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
pub struct Remote {
    endpoint: Endpoint,
}
impl Remote {
    pub fn from_file(path: &Path) -> Result<Self, ActionError> {
        let bytes = std::fs::read(path)?;
        if bytes.len() > 16384 {
            return Err(ActionError::new("ipc", "Session file is too large"));
        }
        let endpoint: Endpoint = serde_json::from_slice(&bytes)?;
        if endpoint.version != 1 || !endpoint.pipe.starts_with(r"\\.\pipe\micopi-") {
            return Err(ActionError::new("ipc", "Invalid MiCoPi session file"));
        }
        Ok(Self { endpoint })
    }
    fn request(&self, request: Request) -> Result<Value, ActionError> {
        let start = Instant::now();
        let pipe = loop {
            match OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.endpoint.pipe)
            {
                Ok(pipe) => break pipe,
                Err(e)
                    if matches!(e.raw_os_error(), Some(2 | 231))
                        && start.elapsed() < Duration::from_secs(3) =>
                {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(e) => return Err(e.into()),
            }
        };
        let mode = PIPE_NOWAIT;
        if unsafe {
            SetNamedPipeHandleState(
                pipe.as_raw_handle(),
                &mode,
                std::ptr::null(),
                std::ptr::null(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut pipe = PipeIo {
            file: pipe,
            stop: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + IO_TIMEOUT * 2 + Duration::from_secs(5),
        };
        serde_json::to_writer(&mut pipe, &request)?;
        pipe.write_all(b"\n")?;
        pipe.flush()?;
        let text = line(&mut BufReader::new(&mut pipe))?
            .ok_or_else(|| ActionError::new("ipc", "GUI did not return a result"))?;
        // The response is already complete; an older server may disconnect
        // immediately instead of waiting for this acknowledgement.
        let _ = pipe.write_all(b"ack\n");
        let reply: Value = serde_json::from_str(&text)?;
        if reply["ok"] == true {
            Ok(reply["data"].clone())
        } else {
            Err(serde_json::from_value(reply["error"].clone())?)
        }
    }
}
impl Backend for Remote {
    fn actions(&mut self) -> Result<Value, ActionError> {
        self.request(Request::Actions)
    }
    fn dispatch(&mut self, action: Action) -> Result<Value, ActionError> {
        self.request(Request::Dispatch { action })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn server(name: &str) -> Server {
        server_with_timeout(name, Duration::from_millis(250))
    }
    fn server_with_timeout(name: &str, timeout: Duration) -> Server {
        let path = std::env::current_dir()
            .unwrap()
            .join("target/test-output/ipc")
            .join(format!("{}-{name}.json", std::process::id()));
        Server::start_with_timeout(path, || {}, timeout).unwrap()
    }
    fn connect(server: &Server) -> File {
        let start = Instant::now();
        loop {
            match OpenOptions::new()
                .read(true)
                .write(true)
                .open(&server.endpoint.pipe)
            {
                Ok(file) => return file,
                Err(error)
                    if start.elapsed() < Duration::from_secs(3)
                        && matches!(error.raw_os_error(), Some(2 | 231)) =>
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("Could not connect: {error}"),
            }
        }
    }
    fn successful_request(server: &Server) {
        let endpoint = server.endpoint.clone();
        let client = std::thread::spawn(move || Remote { endpoint }.actions());
        let request = server
            .receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        request.reply.send(Ok(json!(["healthy"]))).unwrap();
        assert_eq!(client.join().unwrap().unwrap(), json!(["healthy"]));
    }
    #[test]
    fn idle_clients_release_the_service_after_the_read_deadline() {
        let server = server("idle");
        let _idle = connect(&server);
        // The second request must complete even while the idle client stays open.
        successful_request(&server);
    }
    #[test]
    fn unread_large_responses_release_the_service_after_the_write_deadline() {
        let server = server("unread");
        let mut unread = connect(&server);
        unread.write_all(b"{\"method\":\"actions\"}\n").unwrap();
        let request = server
            .receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        request
            .reply
            .send(Ok(json!("x".repeat(256 * 1024))))
            .unwrap();
        successful_request(&server);
    }
    #[test]
    fn large_responses_are_received_completely_before_disconnect() {
        let server = server_with_timeout("large", Duration::from_secs(3));
        let endpoint = server.endpoint.clone();
        let client = std::thread::spawn(move || Remote { endpoint }.actions());
        let request = server
            .receiver
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        let value = json!("x".repeat(256 * 1024));
        request.reply.send(Ok(value.clone())).unwrap();
        assert_eq!(client.join().unwrap().unwrap(), value);
    }
}
