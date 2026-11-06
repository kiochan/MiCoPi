//! Window-free codecs shared by GUI, MCP and batch rendering.
use super::ActionError;
use crate::{
    engine::{Document, MAX_SIDE},
    platform::replace_file,
};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
pub(crate) fn is_project(path: &Path) -> Result<bool, ActionError> {
    let mut header = Vec::with_capacity(8);
    std::fs::File::open(path)?
        .take(8)
        .read_to_end(&mut header)?;
    Ok(matches!(
        header.as_slice(),
        b"MICOPI01" | b"MICOPI02" | b"MICOPI03" | b"MICOPI04"
    ) || path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mcp")))
}
pub fn open(path: &Path) -> Result<Document, ActionError> {
    if is_project(path)? {
        return Ok(Document::load(path)?);
    }
    let mut input = BufReader::new(std::fs::File::open(path)?);
    let mut signature = [0; 8];
    let complete = match input.read_exact(&mut signature) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => false,
        Err(e) => return Err(e.into()),
    };
    input.seek(SeekFrom::Start(0))?;
    let header = if complete && signature == *b"\x89PNG\r\n\x1a\n" {
        let header = super::png_io::read_header(&mut input)?;
        input.seek(SeekFrom::Start(0))?;
        Some(header)
    } else {
        None
    };
    let mut reader = image::ImageReader::new(input)
        .with_guessed_format()
        .map_err(ActionError::from)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE as u32);
    limits.max_image_height = Some(MAX_SIDE as u32);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let pixels = reader
        .decode()
        .map_err(|e| ActionError::new("image", e.to_string()))?
        .to_rgba8();
    let (w, h) = pixels.dimensions();
    if w == 0 || h == 0 || w as usize > MAX_SIDE || h as usize > MAX_SIDE {
        return Err(ActionError::new(
            "image_size",
            "Image dimensions must be between 1 and 4096 pixels",
        ));
    }
    if header
        .as_ref()
        .is_some_and(|header| header.width != w || header.height != h)
    {
        return Err(ActionError::new(
            "png",
            "PNG dimensions changed while reading",
        ));
    }
    let mut doc = Document::new(w as usize, h as usize);
    for (out, p) in doc.layers[0].pixels.iter_mut().zip(pixels.chunks_exact(4)) {
        *out = u32::from_be_bytes([p[3], p[0], p[1], p[2]]);
    }
    if let Some(density) = header.and_then(|h| h.density) {
        doc.set_import_info(crate::engine::document_info::Info::with_png_density(
            density,
        ))
        .map_err(|e| ActionError::new("png", e))?;
    }
    doc.dirty = true;
    Ok(doc)
}
pub fn png(doc: &mut Document) -> Result<Vec<u8>, ActionError> {
    doc.compose();
    if let Some(error) = &doc.render_error {
        return Err(ActionError::new("compositing", error));
    }
    super::png_io::encode(
        doc.width,
        doc.height,
        doc.composite.iter().copied(),
        doc.info().export_density(),
    )
}

pub fn export(doc: &mut Document, path: &Path) -> Result<(), ActionError> {
    guard_png_destination(path)?;
    let bytes = png(doc)?;
    write_png(path, &bytes)
}
/// A PNG destination must never replace a native project, even after renaming.
pub(crate) fn guard_png_destination(path: &Path) -> Result<(), ActionError> {
    #[cfg(windows)]
    if let Some(name) = path.file_name() {
        let name = name.to_string_lossy();
        if name.ends_with(['.', ' ']) || name.contains(':') {
            return Err(ActionError::new("invalid_export_path", "PNG output needs a regular filename without trailing dots, spaces or stream syntax"));
        }
    }

    let native_extension = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mcp"));
    let native_header = match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => is_project(path)?,
        Ok(_) => {
            return Err(ActionError::new(
                "io",
                "PNG output path is not a regular file",
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if native_extension || native_header {
        return Err(ActionError::new(
            "project_overwrite",
            "PNG export cannot replace a native project",
        ));
    }
    Ok(())
}
/// Write completely to a unique sibling before atomically replacing a destination.
pub(crate) fn write_png(path: &Path, bytes: &[u8]) -> Result<(), ActionError> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    guard_png_destination(path)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if path.file_name().is_none() {
        return Err(ActionError::new(
            "invalid_arguments",
            "PNG output needs a file name",
        ));
    }
    let mut candidate = None;
    for _ in 0..16 {
        let number = NEXT.fetch_add(1, Ordering::Relaxed);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = parent.join(format!(
            ".micopi-export-{}-{stamp}-{number}.tmp",
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => {
                candidate = Some((temporary, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let (temporary, mut file) = candidate
        .ok_or_else(|| ActionError::new("io", "Could not reserve a unique PNG temporary file"))?;
    struct Cleanup(Option<std::path::PathBuf>);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if let Some(path) = &self.0 {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let mut cleanup = Cleanup(Some(temporary.clone()));
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        guard_png_destination(path)?;
        replace_file(&temporary, path)?;
        cleanup.0 = None;
        Ok(())
    })();
    drop(cleanup);
    result
}
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        for shift in [18, 12] {
            out.push(TABLE[((n >> shift) & 63) as usize] as char);
        }
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod export_file_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    fn directory() -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-output/export-files-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn entries(path: &Path) -> usize {
        std::fs::read_dir(path).unwrap().count()
    }
    #[test]
    fn png_file_replacement_leaves_unrelated_old_temp_names_untouched() {
        let directory = directory();
        let destination = directory.join("art.png");
        let unrelated = destination.with_extension("png.tmp");
        std::fs::write(&unrelated, b"unrelated data").unwrap();
        std::fs::write(&destination, b"previous output").unwrap();
        write_png(&destination, b"first encoded PNG").unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"first encoded PNG");
        write_png(&destination, b"replacement encoded PNG").unwrap();
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"replacement encoded PNG"
        );
        assert_eq!(std::fs::read(&unrelated).unwrap(), b"unrelated data");
        assert_eq!(entries(&directory), 2);
    }
    #[test]
    fn png_file_guard_preserves_renamed_project_headers_and_native_extensions() {
        let directory = directory();
        let renamed = directory.join("project.data");
        std::fs::write(&renamed, b"MICOPI03 native project data").unwrap();
        assert_eq!(
            write_png(&renamed, b"encoded PNG").unwrap_err().code,
            "project_overwrite"
        );
        assert_eq!(
            std::fs::read(&renamed).unwrap(),
            b"MICOPI03 native project data"
        );
        let native = directory.join("new.MCP");
        assert_eq!(
            write_png(&native, b"encoded PNG").unwrap_err().code,
            "project_overwrite"
        );
        assert!(!native.exists());
        assert_eq!(entries(&directory), 1);
    }
    #[test]
    #[cfg(windows)]
    fn png_file_guard_rejects_normalized_native_aliases_and_stream_names() {
        let directory = directory();
        for name in ["new.mcp.", "new.mcp ", "missing.data.", "out.png:stream"] {
            assert_eq!(
                write_png(&directory.join(name), b"PNG").unwrap_err().code,
                "invalid_export_path"
            );
        }
        assert_eq!(entries(&directory), 0);
    }
    #[test]
    #[cfg(windows)]
    fn png_file_failed_replace_cleans_owned_temp_and_retains_readonly_destination() {
        let directory = directory();
        let destination = directory.join("readonly.png");
        std::fs::write(&destination, b"original output").unwrap();
        let mut permissions = std::fs::metadata(&destination).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&destination, permissions).unwrap();
        let result = write_png(&destination, b"new encoded PNG");
        let mut permissions = std::fs::metadata(&destination).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&destination, permissions).unwrap();
        assert!(result.is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"original output");
        assert_eq!(entries(&directory), 1);
    }
}
