//! Synchronous Win32 clipboard ownership; no decoding or native drawing while open.
use super::*;
use serde_json::{json, Value};
use std::{ffi::c_void, ptr::null_mut, sync::OnceLock};
const DIB: u32 = 8;
const DIBV5: u32 = 17;
#[link(name = "user32")]
extern "system" {
    fn OpenClipboard(window: *mut c_void) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, memory: *mut c_void) -> *mut c_void;
    fn GetClipboardData(format: u32) -> *mut c_void;
    fn IsClipboardFormatAvailable(format: u32) -> i32;
    fn RegisterClipboardFormatW(name: *const u16) -> u32;
    fn IsWindow(window: *mut c_void) -> i32;
    fn GetWindowThreadProcessId(window: *mut c_void, process: *mut u32) -> u32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GlobalAlloc(flags: u32, size: usize) -> *mut c_void;
    fn GlobalFree(memory: *mut c_void) -> *mut c_void;
    fn GlobalLock(memory: *mut c_void) -> *mut c_void;
    fn GlobalUnlock(memory: *mut c_void) -> i32;
    fn GlobalSize(memory: *mut c_void) -> usize;
    fn GetCurrentProcessId() -> u32;
    fn GetCurrentThreadId() -> u32;
}
fn error(label: &str) -> String {
    format!("{label}: {}", std::io::Error::last_os_error())
}
fn formats() -> Result<[u32; 2], String> {
    static IDS: OnceLock<[u32; 2]> = OnceLock::new();
    let ids = *IDS.get_or_init(|| unsafe {
        let register = |name: &str| {
            let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            RegisterClipboardFormatW(name.as_ptr())
        };
        [register("PNG"), register("MiCoPi.Image.v1")]
    });
    if ids.contains(&0) {
        Err("Could not register clipboard image formats".into())
    } else {
        Ok(ids)
    }
}
fn owner(window: isize) -> Result<*mut c_void, String> {
    let window = window as *mut c_void;
    let mut process = 0;
    let thread = unsafe { GetWindowThreadProcessId(window, &mut process) };
    if window.is_null()
        || unsafe { IsWindow(window) } == 0
        || process != unsafe { GetCurrentProcessId() }
        || thread != unsafe { GetCurrentThreadId() }
    {
        Err("Clipboard access requires a live desktop window on its owning thread".into())
    } else {
        Ok(window)
    }
}
struct Open;
impl Open {
    fn new(window: *mut c_void) -> Result<Self, String> {
        if unsafe { OpenClipboard(window) } == 0 {
            Err(error("System clipboard is busy; retry"))
        } else {
            Ok(Self)
        }
    }
}
impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}
struct Memory(*mut c_void);
impl Memory {
    fn new(bytes: Vec<u8>) -> Result<Self, String> {
        let memory = Self(unsafe { GlobalAlloc(2, bytes.len()) });
        if memory.0.is_null() {
            return Err(error("Clipboard allocation failed"));
        }
        let pointer = unsafe { GlobalLock(memory.0) };
        if pointer.is_null() {
            return Err(error("Clipboard memory lock failed"));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer as *mut u8, bytes.len());
            GlobalUnlock(memory.0);
        }
        Ok(memory)
    }
    fn publish(&mut self, format: u32) -> Result<(), String> {
        if unsafe { SetClipboardData(format, self.0) }.is_null() {
            Err(error("Clipboard publication failed"))
        } else {
            self.0 = null_mut();
            Ok(())
        }
    }
}
impl Drop for Memory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                GlobalFree(self.0);
            }
        }
    }
}
fn bytes(format: u32, max: usize) -> Result<Vec<u8>, String> {
    let memory = unsafe { GetClipboardData(format) };
    if memory.is_null() {
        return Err(error("Clipboard data is unavailable"));
    }
    let size = unsafe { GlobalSize(memory) };
    if size == 0 || size > max {
        return Err("Clipboard data exceeds supported storage bounds".into());
    }
    let pointer = unsafe { GlobalLock(memory) };
    if pointer.is_null() {
        return Err(error("Clipboard data lock failed"));
    }
    let output = unsafe { std::slice::from_raw_parts(pointer as *const u8, size).to_vec() };
    unsafe {
        GlobalUnlock(memory);
    }
    Ok(output)
}
pub(crate) fn status(window: isize) -> Result<Value, String> {
    owner(window)?;
    let [png, _] = formats()?;
    let present = |format| unsafe { IsClipboardFormatAvailable(format) != 0 };
    Ok(
        json!({"available":present(png)||present(DIBV5)||present(DIB),"png":present(png),"dibv5":present(DIBV5),"dib":present(DIB),"scope":"windows"}),
    )
}
pub(crate) fn read(window: isize) -> Result<ReadImage, String> {
    let window = owner(window)?;
    let [png, metadata] = formats()?;
    let (format, data, metadata) = {
        let _open = Open::new(window)?;
        let format = [png, DIBV5, DIB]
            .into_iter()
            .find(|id| unsafe { IsClipboardFormatAvailable(*id) != 0 })
            .ok_or("System clipboard has no supported image")?;
        let data = bytes(format, MAX_BYTES)?;
        // Optional origin hints are never required for interoperable pixel data.
        let metadata = if unsafe { IsClipboardFormatAvailable(metadata) } != 0 {
            bytes(metadata, 1024).ok()
        } else {
            None
        };
        (format, data, metadata)
    };
    let image = if format == png {
        decode_png(&data)?
    } else {
        decode_dib(&data)?
    };
    Ok(ReadImage {
        image,
        metadata,
        format: if format == png {
            "png"
        } else if format == DIBV5 {
            "dibv5"
        } else {
            "dib"
        },
    })
}
pub(crate) fn write(window: isize, image: &View<'_>, metadata: &[u8]) -> Result<Value, String> {
    let window = owner(window)?;
    let [png, private] = formats()?;
    if metadata.len() > 1024 {
        return Err("Clipboard origin metadata exceeds 1 KiB".into());
    }
    // Prepare every movable allocation before touching the current system clipboard.
    let mut png_data = Memory::new(encode_png(image)?)?;
    let mut dib_data = Memory::new(encode_dib(image)?)?;
    let mut origin = Memory::new(metadata.to_vec())?;
    let _open = Open::new(window)?;
    if unsafe { EmptyClipboard() } == 0 {
        return Err(error("Could not claim the system clipboard"));
    }
    png_data.publish(png)?;
    // PNG publication is the success boundary. Optional fallback/hints may fail;
    // report partial availability rather than making a successful copy look absent.
    let mut warnings = Vec::new();
    let dibv5 = match dib_data.publish(DIBV5) {
        Ok(()) => true,
        Err(e) => {
            warnings.push(e);
            false
        }
    };
    let origin = match origin.publish(private) {
        Ok(()) => true,
        Err(e) => {
            warnings.push(e);
            false
        }
    };
    Ok(
        json!({"scope":"windows","png":true,"dibv5":dibv5,"origin_metadata":origin,"warnings":warnings}),
    )
}

pub(super) struct Clipboard(pub isize);
impl SystemClipboard for Clipboard {
    fn status(&self) -> Result<Value, String> {
        status(self.0)
    }
    fn read(&self) -> Result<ReadImage, String> {
        read(self.0)
    }
    fn write(&self, image: &View<'_>, metadata: &[u8]) -> Result<Value, String> {
        write(self.0, image, metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[link(name = "user32")]
    extern "system" {
        fn GetProcessWindowStation() -> *mut c_void;
        fn GetUserObjectInformationW(
            handle: *mut c_void,
            index: i32,
            data: *mut c_void,
            len: u32,
            needed: *mut u32,
        ) -> i32;
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: *mut c_void,
            menu: *mut c_void,
            instance: *mut c_void,
            param: *mut c_void,
        ) -> *mut c_void;
        fn DestroyWindow(window: *mut c_void) -> i32;
    }
    #[test]
    fn invalid_owner_rejects_before_any_clipboard_access() {
        let image = Image {
            width: 1,
            height: 1,
            pixels: vec![0],
        };
        assert!(status(0).is_err());
        assert!(read(0).is_err());
        assert!(write(0, &image.view(), b"{}\0").is_err());
    }
    #[test]
    #[ignore = "Run scripts/smoke-system-clipboard.py; refuses the interactive clipboard"]
    fn isolated_windows_clipboard_roundtrip() {
        let mut name = [0u16; 512];
        let mut needed = 0;
        assert_ne!(
            unsafe {
                GetUserObjectInformationW(
                    GetProcessWindowStation(),
                    2,
                    name.as_mut_ptr() as *mut c_void,
                    1024,
                    &mut needed,
                )
            },
            0
        );
        let name = String::from_utf16_lossy(
            &name[..name.iter().position(|c| *c == 0).unwrap_or(name.len())],
        );
        assert!(
            name.to_ascii_lowercase()
                .starts_with("micopi-clipboard-test-"),
            "Refusing clipboard writes outside a dedicated test window station: {name}"
        );
        let class: Vec<u16> = "STATIC".encode_utf16().chain(Some(0)).collect();
        let window = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                1,
                1,
                -3isize as *mut c_void,
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        assert!(
            !window.is_null(),
            "{}",
            error("Fixture window creation failed")
        );
        struct OwnedWindow(*mut c_void);
        impl Drop for OwnedWindow {
            fn drop(&mut self) {
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }
        let _owned = OwnedWindow(window);
        let input = Image {
            width: 3,
            height: 2,
            pixels: vec![
                0x00112233, 0x80402010, 0xffffffff, 0, 0xff123456, 0xffabcdef,
            ],
        };
        let result = write(window as isize, &input.view(), b"{}\0").unwrap();
        assert_eq!(result["png"], true);
        assert_eq!(result["dibv5"], true);
        let read_image = read(window as isize).unwrap();
        assert_eq!(read_image.format, "png");
        assert_eq!(read_image.image.pixels, input.pixels);
        {
            let _open = Open::new(window).unwrap();
            let raw = bytes(DIBV5, MAX_BYTES).unwrap();
            assert_eq!(u32::from_le_bytes(raw[..4].try_into().unwrap()), 124);
            for (i, p) in input.pixels.iter().enumerate() {
                assert_eq!(&raw[124 + i * 4..128 + i * 4], &p.to_le_bytes());
            }
        }
        // Independent producer: legacy 24-bit bottom-up DIB with row padding.
        let mut legacy = vec![0; 56];
        legacy[..4].copy_from_slice(&40u32.to_le_bytes());
        legacy[4..8].copy_from_slice(&2i32.to_le_bytes());
        legacy[8..12].copy_from_slice(&2i32.to_le_bytes());
        legacy[12..14].copy_from_slice(&1u16.to_le_bytes());
        legacy[14..16].copy_from_slice(&24u16.to_le_bytes());
        legacy[40..].copy_from_slice(&[7, 8, 9, 10, 11, 12, 0, 0, 1, 2, 3, 4, 5, 6, 0, 0]);
        {
            let mut memory = Memory::new(legacy.clone()).unwrap();
            let _open = Open::new(window).unwrap();
            assert_ne!(unsafe { EmptyClipboard() }, 0);
            memory.publish(DIB).unwrap();
        }
        let legacy_read = read(window as isize).unwrap();
        assert_eq!(
            legacy_read.image.pixels,
            vec![0xff030201, 0xff060504, 0xff090807, 0xff0c0b0a]
        );
        {
            let mut bad = Memory::new(b"not png".to_vec()).unwrap();
            let mut fallback = Memory::new(legacy).unwrap();
            let _open = Open::new(window).unwrap();
            assert_ne!(unsafe { EmptyClipboard() }, 0);
            bad.publish(formats().unwrap()[0]).unwrap();
            fallback.publish(DIB).unwrap();
        }
        assert!(
            read(window as isize).is_err(),
            "Malformed preferred PNG must not silently use fallback"
        );
        use crate::application::{Action, Session};
        let registry = crate::plugins::Registry::load(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        let mut session = Session::new(registry).unwrap();
        session.runtime = "desktop";
        session.system_clipboard = Some(crate::platform::clipboard::for_window(window as isize));
        let call = |s: &mut Session, id: &str, args: Value| {
            s.apply(Action::new(id, args)).unwrap()["result"].clone()
        };
        call(&mut session, "canvas.new", json!({"width":12,"height":8}));
        call(&mut session, "native.checkerboard", json!({"cell":2}));
        let source = session.doc.layers[0].pixels.clone();
        call(
            &mut session,
            "selection.shape",
            json!({"kind":"rectangle","x":3,"y":2,"width":5,"height":3}),
        );
        call(&mut session, "clipboard.system_copy", json!({}));
        call(&mut session, "clipboard.system_paste_in_place", json!({}));
        assert_eq!(session.doc.layers[1].pixels[27], source[27]);
        assert_eq!(session.doc.layers.len(), 2);
        call(&mut session, "history.undo", json!({}));
        assert_eq!(session.doc.layers.len(), 1);
        call(&mut session, "history.redo", json!({}));
        assert_eq!(
            call(&mut session, "replay.verify", json!({}))["valid"],
            true
        );
        eprintln!("Private Win32 PNG/DIBV5 publication, legacy DIB producer, strict fallback and Session action roundtrip passed in {name}");
    }
}
