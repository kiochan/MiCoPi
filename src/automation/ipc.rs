//! Same-user Windows named pipe. The worker queues requests; only the UI thread
//! executes them, so MCP and a physical pen never mutate a document concurrently.
use super::batch::{line, Backend};
use crate::application::{Action, ActionError, Session};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    io::{BufReader, Write},
    os::windows::io::FromRawHandle,
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, Ordering},
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
    fn FlushFileBuffers(pipe: Handle) -> i32;
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
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let endpoint = Endpoint {
            version: 1,
            pid: std::process::id(),
            pipe: format!(r"\\.\pipe\micopi-{}-{nonce}", std::process::id()),
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
                            8,
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
                    let mut file = unsafe { File::from_raw_handle(handle) };
                    if first {
                        let _ = ready_tx.send(Ok(()));
                        first = false;
                    }
                    let connected = unsafe {
                        ConnectNamedPipe(handle, null_mut()) != 0 || GetLastError() == 535
                    };
                    if connected && !worker_stop.load(Ordering::Acquire) {
                        let reply = (|| -> Result<Value, ActionError> {
                            let text = line(&mut BufReader::new(file.try_clone()?))?
                                .ok_or_else(|| ActionError::new("ipc", "Empty request"))?;
                            let request: Request = serde_json::from_str(&text)?;
                            let (tx, rx) = mpsc::channel();
                            sender
                                .send(Pending {
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
                        let _ = serde_json::to_writer(&mut file, &value);
                        let _ = file.write_all(b"\n");
                        let _ = file.flush();
                        unsafe {
                            FlushFileBuffers(handle);
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
        let mut pipe = loop {
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
        serde_json::to_writer(&mut pipe, &request)?;
        pipe.write_all(b"\n")?;
        pipe.flush()?;
        let text = line(&mut BufReader::new(pipe))?
            .ok_or_else(|| ActionError::new("ipc", "GUI did not return a result"))?;
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
