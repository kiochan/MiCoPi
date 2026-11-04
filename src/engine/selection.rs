//! Immutable selection storage. Construction/decoding is cold; clones share masks.
//! Pixel algorithms belong to native plugins, not this persistence container.
use flate2::{write::DeflateEncoder, Compression, Decompress, FlushDecompress, Status};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{io::Write, sync::Arc};

const MAX_PACKED: usize = 128 * 1024;

#[derive(Debug, PartialEq, Eq)]
struct Data {
    width: usize,
    height: usize,
    coverage: Vec<u8>,
    packed: String,
}

/// Full-canvas 8-bit coverage; zero means excluded, 255 means fully selected.
/// `None` in SelectionState means unrestricted editing; an empty mask is distinct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mask(Arc<Data>);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    width: usize,
    height: usize,
    deflate_hex: String,
}

impl Mask {
    pub fn new(width: usize, height: usize, coverage: Vec<u8>) -> Result<Self, String> {
        if !dimensions(width, height) || coverage.len() != width * height {
            return Err("Invalid selection dimensions or coverage length".into());
        }
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&coverage).map_err(|e| e.to_string())?;
        let bytes = encoder.finish().map_err(|e| e.to_string())?;
        if bytes.len() > MAX_PACKED {
            return Err("Selection exceeds the compressed metadata limit".into());
        }
        let packed = bytes.iter().map(|b| format!("{b:02x}")).collect();
        Ok(Self(Arc::new(Data {
            width,
            height,
            coverage,
            packed,
        })))
    }
    pub fn dimensions(&self) -> (usize, usize) {
        (self.0.width, self.0.height)
    }
    pub fn coverage(&self) -> &[u8] {
        &self.0.coverage
    }
    pub fn memory_bytes(&self) -> usize {
        self.0.coverage.len() + self.0.packed.len()
    }
    pub(crate) fn reframe(
        &self,
        width: usize,
        height: usize,
        dx: i32,
        dy: i32,
    ) -> Result<Self, String> {
        if !dimensions(width, height) {
            return Err("Invalid selection frame".into());
        }
        let mut coverage = vec![0; width * height];
        let sx = (-i64::from(dx)).max(0).min(self.0.width as i64) as usize;
        let sy = (-i64::from(dy)).max(0).min(self.0.height as i64) as usize;
        let tx = i64::from(dx).max(0).min(width as i64) as usize;
        let ty = i64::from(dy).max(0).min(height as i64) as usize;
        let cols = (self.0.width - sx).min(width - tx);
        let rows = (self.0.height - sy).min(height - ty);
        for row in 0..rows {
            coverage[(ty + row) * width + tx..(ty + row) * width + tx + cols].copy_from_slice(
                &self.0.coverage
                    [(sy + row) * self.0.width + sx..(sy + row) * self.0.width + sx + cols],
            );
        }
        Self::new(width, height, coverage)
    }
    fn from_wire(w: Wire) -> Result<Self, String> {
        if !dimensions(w.width, w.height)
            || w.deflate_hex.len() > MAX_PACKED * 2
            || w.deflate_hex.len() % 2 != 0
        {
            return Err("Invalid or oversized selection metadata".into());
        }
        let nibble = |b| match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            _ => Err("Invalid selection encoding"),
        };
        let bytes: Result<Vec<_>, &str> = w
            .deflate_hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| Ok(nibble(pair[0])? * 16 + nibble(pair[1])?))
            .collect();
        let bytes = bytes?;
        let mut decoder = Decompress::new(false);
        let size = w.width * w.height;
        let mut coverage = vec![0; size + 1];
        let status = decoder
            .decompress(&bytes, &mut coverage, FlushDecompress::Finish)
            .map_err(|e| e.to_string())?;
        if status != Status::StreamEnd
            || decoder.total_out() as usize != size
            || decoder.total_in() as usize != bytes.len()
        {
            return Err("Invalid selection payload length".into());
        }
        coverage.truncate(size);
        Ok(Self(Arc::new(Data {
            width: w.width,
            height: w.height,
            coverage,
            packed: w.deflate_hex,
        })))
    }
}
fn dimensions(w: usize, h: usize) -> bool {
    w > 0 && h > 0 && w <= super::MAX_SIDE && h <= super::MAX_SIDE
}
impl Serialize for Mask {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Borrowed<'a> {
            width: usize,
            height: usize,
            deflate_hex: &'a str,
        }
        Borrowed {
            width: self.0.width,
            height: self.0.height,
            deflate_hex: &self.0.packed,
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Mask {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_wire(Wire::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<Mask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Mask>,
}
impl SelectionState {
    pub fn is_default(&self) -> bool {
        self.current.is_none() && self.previous.is_none()
    }
    pub fn validate(&self, width: usize, height: usize) -> Result<(), String> {
        if [&self.current, &self.previous]
            .into_iter()
            .flatten()
            .any(|m| m.dimensions() != (width, height))
        {
            return Err("Selection dimensions differ from the document".into());
        }
        Ok(())
    }
    pub fn memory_bytes(&self) -> usize {
        self.current.as_ref().map_or(0, Mask::memory_bytes)
            + self.previous.as_ref().map_or(0, Mask::memory_bytes)
    }
    pub(crate) fn reframe(
        &self,
        width: usize,
        height: usize,
        dx: i32,
        dy: i32,
    ) -> Result<Self, String> {
        Ok(Self {
            current: self
                .current
                .as_ref()
                .map(|m| m.reframe(width, height, dx, dy))
                .transpose()?,
            previous: self
                .previous
                .as_ref()
                .map(|m| m.reframe(width, height, dx, dy))
                .transpose()?,
        })
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
