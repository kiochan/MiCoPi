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
    Ok(
        matches!(header.as_slice(), b"MICOPI01" | b"MICOPI02" | b"MICOPI03")
            || path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("mcp")),
    )
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
    let bytes = png(doc)?;
    let tmp = path.with_extension("png.tmp");
    std::fs::write(&tmp, bytes)?;
    replace_file(&tmp, path)?;
    Ok(())
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
