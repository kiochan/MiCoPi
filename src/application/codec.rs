//! Window-free codecs shared by GUI, MCP and batch rendering.
use super::ActionError;
use crate::{
    engine::{Document, MAX_SIDE},
    platform::replace_file,
};
use image::ImageEncoder;
use std::io::Read;
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
    let reader = || {
        image::ImageReader::open(path)
            .map_err(|e| ActionError::from(e))?
            .with_guessed_format()
            .map_err(ActionError::from)
    };
    let (w, h) = reader()?
        .into_dimensions()
        .map_err(|e| ActionError::new("image", e.to_string()))?;
    if w == 0 || h == 0 || w as usize > MAX_SIDE || h as usize > MAX_SIDE {
        return Err(ActionError::new(
            "image_size",
            "Image dimensions must be between 1 and 4096 pixels",
        ));
    }
    let pixels = reader()?
        .decode()
        .map_err(|e| ActionError::new("image", e.to_string()))?
        .to_rgba8();
    let mut doc = Document::new(w as usize, h as usize);
    for (out, p) in doc.layers[0].pixels.iter_mut().zip(pixels.chunks_exact(4)) {
        *out = u32::from_be_bytes([p[3], p[0], p[1], p[2]]);
    }
    doc.dirty = true;
    Ok(doc)
}
pub fn png(doc: &mut Document) -> Result<Vec<u8>, ActionError> {
    doc.compose();
    if let Some(error) = &doc.render_error {
        return Err(ActionError::new("compositing", error));
    }
    let mut rgba = Vec::with_capacity(doc.composite.len() * 4);
    for p in &doc.composite {
        rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]);
    }
    let mut data = vec![];
    image::codecs::png::PngEncoder::new(&mut data)
        .write_image(
            &rgba,
            doc.width as u32,
            doc.height as u32,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| ActionError::new("png", e.to_string()))?;
    Ok(data)
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
