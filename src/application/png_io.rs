//! Shared PNG encoding and bounded streaming metadata validation.
use super::ActionError;
use crate::engine::{
    document_info::{Density, DensityUnit},
    MAX_SIDE,
};
use image::ImageEncoder;
use std::io::Read;

pub(crate) const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_ANCILLARY_BYTES: u64 = 4 * 1024 * 1024;
const MAX_CHUNKS: usize = 65_536;
const SIGNATURE: [u8; 8] = *b"\x89PNG\r\n\x1a\n";
#[derive(Debug)]
pub(crate) struct Header {
    pub width: u32,
    pub height: u32,
    pub density: Option<Density>,
}
fn invalid(message: &str) -> ActionError {
    ActionError::new("png", message)
}

/// Validate all chunks before decoding. The raster decoder otherwise need not
/// visit trailing ancillary chunks after its first image frame.
pub(crate) fn read_header(reader: &mut impl Read) -> Result<Header, ActionError> {
    let mut signature = [0; 8];
    reader.read_exact(&mut signature)?;
    if signature != SIGNATURE {
        return Err(invalid("Invalid PNG signature"));
    }
    let mut total = 8u64;
    let mut ancillary = 0u64;
    let mut size = None;
    let mut density = None;
    let mut seen_idat = false;
    let mut ended_idat = false;
    let mut buffer = [0; 8192];
    for _ in 0..MAX_CHUNKS {
        let mut header = [0; 8];
        reader.read_exact(&mut header)?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as u64;
        let kind: [u8; 4] = header[4..].try_into().unwrap();
        total = total
            .checked_add(length + 12)
            .ok_or_else(|| invalid("PNG exceeds the input byte limit"))?;
        if length > i32::MAX as u64 || total > MAX_FILE_BYTES {
            return Err(invalid("PNG exceeds the input byte limit"));
        }
        if !kind.iter().all(u8::is_ascii_alphabetic) || kind[2].is_ascii_lowercase() {
            return Err(invalid("Invalid PNG chunk type"));
        }
        if kind[0].is_ascii_lowercase() {
            ancillary += length + 12;
            if ancillary > MAX_ANCILLARY_BYTES {
                return Err(invalid("PNG exceeds the ancillary metadata limit"));
            }
        }
        if size.is_none() && kind != *b"IHDR" {
            return Err(invalid("PNG must begin with IHDR"));
        }
        if kind == *b"IHDR" && (size.is_some() || length != 13) {
            return Err(invalid("Invalid or duplicate PNG IHDR"));
        }
        if kind == *b"pHYs" && (density.is_some() || seen_idat || length != 9) {
            return Err(invalid("Invalid, duplicate or late PNG pixel density"));
        }
        if kind == *b"IEND" && (!seen_idat || length != 0) {
            return Err(invalid("Invalid PNG end chunk"));
        }
        if kind == *b"IDAT" {
            if ended_idat {
                return Err(invalid("PNG image data chunks must be consecutive"));
            }
            seen_idat = true;
        } else if seen_idat {
            ended_idat = true;
        }
        let mut crc = flate2::Crc::new();
        crc.update(&kind);
        let mut remaining = length;
        let mut captured = [0; 13];
        while remaining != 0 {
            let take = remaining.min(buffer.len() as u64) as usize;
            reader.read_exact(&mut buffer[..take])?;
            crc.update(&buffer[..take]);
            if kind == *b"IHDR" || kind == *b"pHYs" {
                captured[..take].copy_from_slice(&buffer[..take]);
            }
            remaining -= take as u64;
        }
        let mut expected = [0; 4];
        reader.read_exact(&mut expected)?;
        if crc.sum() != u32::from_be_bytes(expected) {
            return Err(invalid("PNG chunk checksum mismatch"));
        }
        if kind == *b"IHDR" {
            let width = u32::from_be_bytes(captured[..4].try_into().unwrap());
            let height = u32::from_be_bytes(captured[4..8].try_into().unwrap());
            if width == 0 || height == 0 || width as usize > MAX_SIDE || height as usize > MAX_SIDE
            {
                return Err(ActionError::new(
                    "image_size",
                    "Image dimensions must be between 1 and 4096 pixels",
                ));
            }
            size = Some((width, height));
        } else if kind == *b"pHYs" {
            let value = Density {
                xppu: u32::from_be_bytes(captured[..4].try_into().unwrap()),
                yppu: u32::from_be_bytes(captured[4..8].try_into().unwrap()),
                unit: match captured[8] {
                    0 => DensityUnit::Unspecified,
                    1 => DensityUnit::Meter,
                    _ => return Err(invalid("Invalid PNG pixel-density unit")),
                },
            };
            value.validate().map_err(|e| invalid(&e))?;
            density = Some(value);
        } else if kind == *b"IEND" {
            if reader.read(&mut buffer[..1])? != 0 {
                return Err(invalid("Unexpected bytes after PNG IEND"));
            }
            let (width, height) = size.unwrap();
            return Ok(Header {
                width,
                height,
                density,
            });
        }
    }
    Err(invalid("PNG exceeds the chunk count limit"))
}

pub(crate) fn encode(
    width: usize,
    height: usize,
    pixels: impl IntoIterator<Item = u32>,
    density: Option<Density>,
) -> Result<Vec<u8>, ActionError> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(invalid("PNG dimensions must be between 1 and 4096 pixels"));
    }
    if let Some(value) = density {
        value.validate().map_err(|e| invalid(&e))?;
    }
    let mut rgba = Vec::with_capacity(width * height * 4);
    for p in pixels {
        if rgba.len() == width * height * 4 {
            return Err(invalid("PNG pixel count differs from its dimensions"));
        }
        rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]);
    }
    if rgba.len() != width * height * 4 {
        return Err(invalid("PNG pixel count differs from its dimensions"));
    }
    let mut bytes = vec![];
    if let Some(value) = density {
        let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        encoder.set_filter(png::Filter::Adaptive);
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: value.xppu,
            yppu: value.yppu,
            unit: match value.unit {
                DensityUnit::Meter => png::Unit::Meter,
                DensityUnit::Unspecified => png::Unit::Unspecified,
            },
        }));
        let mut writer = encoder
            .write_header()
            .map_err(|e| invalid(&e.to_string()))?;
        writer
            .write_image_data(&rgba)
            .map_err(|e| invalid(&e.to_string()))?;
        writer.finish().map_err(|e| invalid(&e.to_string()))?;
    } else {
        // Keep the exact legacy wrapper/settings for default recordings and PNGs.
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                &rgba,
                width as u32,
                height as u32,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| invalid(&e.to_string()))?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc = flate2::Crc::new();
        crc.update(kind);
        crc.update(data);
        out.extend_from_slice(&crc.sum().to_be_bytes());
        out
    }
    fn pieces(bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut out = vec![];
        let mut offset = 8;
        while offset < bytes.len() {
            let length =
                u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize + 12;
            out.push(bytes[offset..offset + length].to_vec());
            offset += length;
        }
        out
    }
    fn join(chunks: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
        let mut out = SIGNATURE.to_vec();
        for c in chunks {
            out.extend(c);
        }
        out
    }
    fn phys(x: u32, y: u32, unit: u8) -> Vec<u8> {
        let mut data = x.to_be_bytes().to_vec();
        data.extend(y.to_be_bytes());
        data.push(unit);
        chunk(b"pHYs", &data)
    }
    #[test]
    fn png_io_default_encoder_preserves_exact_legacy_bytes() {
        let pixels = [0x00010203, 0x11335577, 0x80fedcba, 0xff123456];
        let mut rgba = vec![];
        for p in pixels {
            rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]);
        }
        let mut expected = vec![];
        image::codecs::png::PngEncoder::new(&mut expected)
            .write_image(&rgba, 2, 2, image::ExtendedColorType::Rgba8)
            .unwrap();
        let encoded = encode(2, 2, pixels, None).unwrap();
        assert_eq!(encoded, expected);
        assert!(read_header(&mut Cursor::new(&encoded))
            .unwrap()
            .density
            .is_none());
    }
    #[test]
    fn png_io_density_only_adds_phys_without_changing_legacy_compression() {
        let mut seed = 0x76a5b4c3u32;
        for (width, height) in [(1, 1), (2, 3), (31, 17), (128, 64)] {
            for pattern in 0..3 {
                let pixels: Vec<u32> = (0..width * height)
                    .map(|i| match pattern {
                        0 => [0x00112233, 0xffccbbaa, 0x80123456, 0x01020304][i % 4],
                        1 => {
                            let x = i % width;
                            let y = i / width;
                            0xff000000
                                | (((x * 7) as u32 & 255) << 16)
                                | (((y * 11) as u32 & 255) << 8)
                                | ((x + y) as u32 & 255)
                        }
                        _ => {
                            seed ^= seed << 13;
                            seed ^= seed >> 17;
                            seed ^= seed << 5;
                            seed
                        }
                    })
                    .collect();
                let legacy = encode(width, height, pixels.iter().copied(), None).unwrap();
                for density in [
                    Density {
                        xppu: 11811,
                        yppu: 23622,
                        unit: DensityUnit::Meter,
                    },
                    Density {
                        xppu: 4,
                        yppu: 3,
                        unit: DensityUnit::Unspecified,
                    },
                    Density {
                        xppu: 0,
                        yppu: i32::MAX as u32,
                        unit: DensityUnit::Meter,
                    },
                ] {
                    let encoded =
                        encode(width, height, pixels.iter().copied(), Some(density)).unwrap();
                    let chunks = pieces(&encoded);
                    assert_eq!(chunks.iter().filter(|c| &c[4..8] == b"pHYs").count(), 1);
                    let without_density = join(chunks.into_iter().filter(|c| &c[4..8] != b"pHYs"));
                    assert_eq!(
                        without_density, legacy,
                        "PNG compression changed for {width}x{height}, pattern {pattern}, {density:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn png_io_density_roundtrip_retains_raw_axes_unit_and_pixels() {
        let pixels = [0x01020304, 0xfffedcba, 0x80123456, 0];
        for value in [
            Density {
                xppu: 11811,
                yppu: 23622,
                unit: DensityUnit::Meter,
            },
            Density {
                xppu: 4,
                yppu: 3,
                unit: DensityUnit::Unspecified,
            },
            Density {
                xppu: 0,
                yppu: 0,
                unit: DensityUnit::Meter,
            },
            Density {
                xppu: i32::MAX as u32,
                yppu: 0,
                unit: DensityUnit::Unspecified,
            },
        ] {
            let encoded = encode(2, 2, pixels, Some(value)).unwrap();
            let header = read_header(&mut Cursor::new(&encoded)).unwrap();
            assert_eq!((header.width, header.height), (2, 2));
            assert_eq!(header.density, Some(value));
            let image = image::load_from_memory(&encoded).unwrap().to_rgba8();
            let actual: Vec<u32> = image
                .chunks_exact(4)
                .map(|p| u32::from_be_bytes([p[3], p[0], p[1], p[2]]))
                .collect();
            assert_eq!(actual, pixels);
        }
    }
    #[test]
    fn png_io_rejects_malformed_and_late_density_even_after_valid_image_data() {
        let original = encode(1, 1, [0xff112233], None).unwrap();
        let base = pieces(&original);
        let idat = base.iter().position(|c| &c[4..8] == b"IDAT").unwrap();
        let end = base.len() - 1;
        let good = phys(11811, 11811, 1);
        let mut bad_crc = good.clone();
        *bad_crc.last_mut().unwrap() ^= 1;
        for bad in [
            chunk(b"pHYs", &[0; 8]),
            chunk(b"pHYs", &[0; 10]),
            phys(11811, 11811, 2),
            phys(0x80000000, 1, 1),
            phys(1, 0xffffffff, 0),
            bad_crc,
        ] {
            let mut chunks = base.clone();
            chunks.insert(idat, bad);
            assert!(read_header(&mut Cursor::new(join(chunks))).is_err());
        }
        let mut duplicate = base.clone();
        duplicate.insert(idat, good.clone());
        duplicate.insert(idat, good.clone());
        assert!(read_header(&mut Cursor::new(join(duplicate))).is_err());
        let mut late = base.clone();
        late.insert(end, good);
        assert!(read_header(&mut Cursor::new(join(late))).is_err());
        let mut bad_idat = base;
        bad_idat[idat][8] ^= 1;
        assert!(read_header(&mut Cursor::new(join(bad_idat))).is_err());
        let mut trailing = original;
        trailing.push(0);
        assert!(read_header(&mut Cursor::new(trailing)).is_err());
    }
    #[test]
    fn png_io_stream_limits_reject_before_reading_large_chunk_bodies() {
        let mut ihdr = vec![];
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend([8, 6, 0, 0, 0]);
        let header = chunk(b"IHDR", &ihdr);
        for (kind, length, fragment) in [
            (b"IDAT", MAX_FILE_BYTES as u32, "input byte limit"),
            (
                b"tEXt",
                MAX_ANCILLARY_BYTES as u32,
                "ancillary metadata limit",
            ),
        ] {
            let mut bytes = join([header.clone()]);
            bytes.extend(length.to_be_bytes());
            bytes.extend(kind);
            let error = read_header(&mut Cursor::new(bytes)).unwrap_err();
            assert!(error.message.contains(fragment), "{error}");
        }
        let mut bytes = join([header]);
        for _ in 1..MAX_CHUNKS {
            bytes.extend(chunk(b"IDAT", &[]));
        }
        bytes.extend(chunk(b"IEND", &[]));
        assert!(read_header(&mut Cursor::new(bytes))
            .unwrap_err()
            .message
            .contains("chunk count limit"));
    }
    #[test]
    fn png_io_rejects_invalid_structure_and_descriptors() {
        let valid = encode(1, 1, [0], None).unwrap();
        let base = pieces(&valid);
        let mut first = base.clone();
        first.insert(0, phys(1, 1, 0));
        assert!(read_header(&mut Cursor::new(join(first))).is_err());
        let mut repeated = base.clone();
        repeated.insert(1, base[0].clone());
        assert!(read_header(&mut Cursor::new(join(repeated))).is_err());
        for n in [0, 4097] {
            let mut data = base[0][8..21].to_vec();
            data[..4].copy_from_slice(&(n as u32).to_be_bytes());
            let mut chunks = base.clone();
            chunks[0] = chunk(b"IHDR", &data);
            assert_eq!(
                read_header(&mut Cursor::new(join(chunks)))
                    .unwrap_err()
                    .code,
                "image_size"
            );
        }
        assert!(encode(0, 1, [], None).is_err());
        assert!(encode(1, 1, [], None).is_err());
        assert!(encode(1, 1, [0, 0], None).is_err());
        assert!(encode(
            1,
            1,
            [0],
            Some(Density {
                xppu: 0xffffffff,
                yppu: 1,
                unit: DensityUnit::Meter
            })
        )
        .is_err());
    }
    #[test]
    #[ignore = "Explicit release cost measurement; excludes filesystem I/O and document history"]
    fn png_io_cost() {
        use std::time::Instant;
        let side = 2048;
        let mut seed = 0x1a2b3c4du32;
        let pixels: Vec<u32> = (0..side * side)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                0xff000000 | (seed & 0xffffff)
            })
            .collect();
        let density = Density {
            xppu: 11811,
            yppu: 23622,
            unit: DensityUnit::Meter,
        };
        let started = Instant::now();
        let legacy = encode(side, side, pixels.iter().copied(), None).unwrap();
        let legacy_encode = started.elapsed();
        let started = Instant::now();
        let encoded = encode(side, side, pixels.iter().copied(), Some(density)).unwrap();
        let density_encode = started.elapsed();
        let started = Instant::now();
        let header = read_header(&mut Cursor::new(&encoded)).unwrap();
        let scan = started.elapsed();
        assert_eq!(header.density, Some(density));
        let started = Instant::now();
        let image = image::load_from_memory_with_format(&encoded, image::ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        let decode = started.elapsed();
        assert_eq!(image.dimensions(), (side as u32, side as u32));
        assert!(image
            .chunks_exact(4)
            .zip(&pixels)
            .all(|(p, expected)| { u32::from_be_bytes([p[3], p[0], p[1], p[2]]) == *expected }));
        eprintln!(
            "PNG {side}x{side} deterministic opaque noise: legacy encode={legacy_encode:?}, \
             density encode={density_encode:?}, strict in-memory scan={scan:?}, \
             in-memory decode={decode:?}; legacy bytes={}, density bytes={}; \
             scanner scratch=8192 bytes, input limit={MAX_FILE_BYTES}, \
             ancillary limit={MAX_ANCILLARY_BYTES}, chunk limit={MAX_CHUNKS}. \
             Source and decoded RGBA buffers each {} bytes; encoder/decoder internal \
             allocations, file I/O, document composition/history and journal are separate.",
            legacy.len(),
            encoded.len(),
            pixels.len() * 4,
        );
    }
}
