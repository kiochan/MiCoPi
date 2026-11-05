//! PNG and packed DIB codecs. Decode untrusted metadata before allocating pixel storage.
use image::{ImageEncoder, ImageFormat, ImageReader};
use std::io::Cursor;
pub(crate) const MAX_BYTES: usize = 72 * 1024 * 1024;
pub(crate) struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}
fn size(w: usize, h: usize) -> Result<usize, String> {
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        Err("Clipboard dimensions must be between 1 and 4096 pixels".into())
    } else {
        Ok(w * h)
    }
}
pub(crate) struct View<'a> {
    pub width: usize,
    pub height: usize,
    pub pixels: &'a [u32],
}
#[cfg(test)]
impl Image {
    pub fn view(&self) -> View<'_> {
        View {
            width: self.width,
            height: self.height,
            pixels: &self.pixels,
        }
    }
}
fn validate(image: &View<'_>) -> Result<(), String> {
    if size(image.width, image.height)? != image.pixels.len() {
        return Err("Invalid clipboard pixel storage".into());
    }
    Ok(())
}
pub(crate) fn encode_png(image: &View<'_>) -> Result<Vec<u8>, String> {
    validate(image)?;
    let mut rgba = Vec::with_capacity(image.pixels.len() * 4);
    for p in image.pixels {
        rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]);
    }
    let mut output = Vec::new();
    image::codecs::png::PngEncoder::new(&mut output)
        .write_image(
            &rgba,
            image.width as u32,
            image.height as u32,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    if output.len() > MAX_BYTES {
        return Err("Clipboard PNG exceeds the encoded size limit".into());
    }
    Ok(output)
}
pub(crate) fn decode_png(data: &[u8]) -> Result<Image, String> {
    if data.len() > MAX_BYTES {
        return Err("Clipboard PNG exceeds the encoded size limit".into());
    }
    // Validate the mandatory first IHDR before the decoder processes ancillary data.
    let header = data.get(..33).ok_or("Truncated clipboard PNG header")?;
    if &header[..8] != b"\x89PNG\r\n\x1a\n" || &header[8..16] != b"\0\0\0\rIHDR" {
        return Err("Invalid clipboard PNG header".into());
    }
    let w = u32::from_be_bytes(header[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(header[20..24].try_into().unwrap());
    size(w as usize, h as usize)?;
    let mut reader = ImageReader::with_format(Cursor::new(data), ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let rgba = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
    let pixels = rgba
        .chunks_exact(4)
        .map(|p| u32::from_be_bytes([p[3], p[0], p[1], p[2]]))
        .collect();
    Ok(Image {
        width: w as usize,
        height: h as usize,
        pixels,
    })
}
fn word(data: &[u8], at: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        data.get(at..at + 2)
            .ok_or("Truncated clipboard bitmap")?
            .try_into()
            .unwrap(),
    ))
}
fn dword(data: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        data.get(at..at + 4)
            .ok_or("Truncated clipboard bitmap")?
            .try_into()
            .unwrap(),
    ))
}
fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
pub(crate) fn encode_dib(image: &View<'_>) -> Result<Vec<u8>, String> {
    validate(image)?;
    let mut data = vec![0; 124 + image.pixels.len() * 4];
    put(&mut data, 0, 124);
    put(&mut data, 4, image.width as u32);
    put(&mut data, 8, (-(image.height as i32)) as u32);
    data[12..14].copy_from_slice(&1u16.to_le_bytes());
    data[14..16].copy_from_slice(&32u16.to_le_bytes());
    put(&mut data, 16, 3);
    put(&mut data, 20, (image.pixels.len() * 4) as u32);
    for (i, value) in [0xff0000, 0xff00, 0xff, 0xff000000].into_iter().enumerate() {
        put(&mut data, 40 + 4 * i, value);
    }
    put(&mut data, 56, 0x73524742); // LCS_sRGB; straight-alpha BGRA bitfields.
    put(&mut data, 108, 4); // LCS_GM_IMAGES.
    for (p, bytes) in image.pixels.iter().zip(data[124..].chunks_exact_mut(4)) {
        bytes.copy_from_slice(&p.to_le_bytes());
    }
    Ok(data)
}
fn channel(pixel: u32, mask: u32) -> u32 {
    if mask == 0 {
        return 255;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    ((((pixel & mask) >> shift) as u64 * 255 + max as u64 / 2) / max as u64) as u32
}
fn masks_valid(masks: [u32; 4], bits: u16) -> bool {
    masks[..3].iter().all(|m| *m != 0)
        && masks.iter().enumerate().all(|(i, m)| {
            (*m == 0 || {
                let n = *m >> m.trailing_zeros();
                n.wrapping_add(1) & n == 0
            }) && (bits == 32 || (*m >> bits) == 0)
                && masks[..i].iter().all(|before| *before & *m == 0)
        })
}
pub(crate) fn decode_dib(data: &[u8]) -> Result<Image, String> {
    if data.len() > MAX_BYTES {
        return Err("Clipboard bitmap exceeds the encoded size limit".into());
    }
    let header = dword(data, 0)? as usize;
    if ![40, 52, 56, 108, 124].contains(&header) || data.len() < header {
        return Err("Unsupported or truncated clipboard bitmap header".into());
    }
    let (w, h) = (dword(data, 4)? as i32, dword(data, 8)? as i32);
    if w <= 0 {
        return Err("Invalid clipboard bitmap width".into());
    }
    let (width, height) = (w as usize, h.unsigned_abs() as usize);
    let count = size(width, height)?;
    if word(data, 12)? != 1 {
        return Err("Invalid clipboard bitmap plane count".into());
    }
    let bits = word(data, 14)?;
    let compression = dword(data, 16)?;
    if ![1, 4, 8, 16, 24, 32].contains(&bits)
        || !matches!(compression, 0 | 3)
        || compression == 3 && !matches!(bits, 16 | 32)
    {
        return Err("Unsupported clipboard bitmap depth or compression".into());
    }
    // ICC/calibrated transforms belong to the later color-management milestone.
    if header == 124 && (dword(data, 112)? != 0 || dword(data, 116)? != 0) {
        return Err("Profiled clipboard bitmaps are not supported; use PNG".into());
    }
    if header >= 108 && !matches!(dword(data, 56)?, 0 | 0x73524742 | 0x57696e20) {
        return Err("Unsupported clipboard bitmap color space".into());
    }
    let mut offset = header;
    let masks = if compression == 3 {
        let masks = if header == 40 {
            offset += 12;
            [dword(data, 40)?, dword(data, 44)?, dword(data, 48)?, 0]
        } else {
            [
                dword(data, 40)?,
                dword(data, 44)?,
                dword(data, 48)?,
                if header >= 56 { dword(data, 52)? } else { 0 },
            ]
        };
        if !masks_valid(masks, bits) {
            return Err("Invalid clipboard bitmap channel masks".into());
        }
        masks
    } else if bits == 16 {
        [0x7c00, 0x3e0, 0x1f, 0]
    } else {
        [0xff0000, 0xff00, 0xff, 0]
    };
    let colors = dword(data, 32)? as usize;
    let colors = if bits <= 8 {
        if colors == 0 {
            1usize << bits
        } else if colors > 1usize << bits {
            return Err("Invalid clipboard palette size".into());
        } else {
            colors
        }
    } else {
        if colors > 256 {
            return Err("Clipboard color table exceeds supported bounds".into());
        }
        colors
    };
    let table = offset;
    offset = offset
        .checked_add(colors * 4)
        .ok_or("Invalid clipboard bitmap offset")?;
    let row = (width * bits as usize).div_ceil(32) * 4;
    let required = row * height;
    let end = offset
        .checked_add(required)
        .ok_or("Invalid clipboard bitmap storage")?;
    if end > data.len() {
        return Err("Truncated clipboard bitmap pixel storage".into());
    }
    let declared = dword(data, 20)? as usize;
    if declared != 0 && (declared < required || declared > data.len() - offset) {
        return Err("Invalid clipboard bitmap image size".into());
    }
    let mut pixels = Vec::with_capacity(count);
    for y in 0..height {
        let sy = if h > 0 { height - 1 - y } else { y };
        let row = &data[offset + sy * row..offset + (sy + 1) * row];
        for x in 0..width {
            let p = match bits {
                1 | 4 | 8 => {
                    let index = match bits {
                        1 => (row[x / 8] >> (7 - x % 8)) & 1,
                        4 => (row[x / 2] >> (if x % 2 == 0 { 4 } else { 0 })) & 15,
                        _ => row[x],
                    } as usize;
                    if index >= colors {
                        return Err("Clipboard palette index is out of range".into());
                    }
                    dword(data, table + index * 4)? | 0xff000000
                }
                24 => {
                    0xff000000 | u32::from_le_bytes([row[x * 3], row[x * 3 + 1], row[x * 3 + 2], 0])
                }
                16 | 32 => {
                    let n = bits as usize / 8;
                    let p = if bits == 16 {
                        u16::from_le_bytes(row[x * n..x * n + 2].try_into().unwrap()) as u32
                    } else {
                        u32::from_le_bytes(row[x * n..x * n + 4].try_into().unwrap())
                    };
                    (channel(p, masks[3]) << 24)
                        | (channel(p, masks[0]) << 16)
                        | (channel(p, masks[1]) << 8)
                        | channel(p, masks[2])
                }
                _ => unreachable!(),
            };
            pixels.push(p);
        }
    }
    Ok(Image {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn header(width: i32, height: i32, bits: u16, compression: u32, extra: usize) -> Vec<u8> {
        let row = (width.unsigned_abs() as usize * bits as usize).div_ceil(32) * 4;
        let mut b = vec![0; 40 + extra + row * height.unsigned_abs() as usize];
        put(&mut b, 0, 40);
        put(&mut b, 4, width as u32);
        put(&mut b, 8, height as u32);
        b[12..14].copy_from_slice(&1u16.to_le_bytes());
        b[14..16].copy_from_slice(&bits.to_le_bytes());
        put(&mut b, 16, compression);
        b
    }
    #[test]
    fn png_and_dibv5_preserve_exact_argb_including_hidden_rgb() {
        let image = Image {
            width: 3,
            height: 2,
            pixels: vec![
                0x00112233, 0x80402010, 0xffffffff, 0, 0xffabcdef, 0x017fa123,
            ],
        };
        for result in [
            decode_png(&encode_png(&image.view()).unwrap()),
            decode_dib(&encode_dib(&image.view()).unwrap()),
        ] {
            let got = result.unwrap();
            assert_eq!((got.width, got.height), (3, 2));
            assert_eq!(got.pixels, image.pixels);
        }
    }
    #[test]
    fn rgb24_padding_and_both_vertical_directions_are_exact() {
        for h in [2, -2] {
            let mut b = header(1, h, 24, 0, 0);
            let rows = if h > 0 {
                [[7, 8, 9, 99], [1, 2, 3, 99]]
            } else {
                [[1, 2, 3, 99], [7, 8, 9, 99]]
            };
            b[40..].copy_from_slice(&rows.concat());
            assert_eq!(decode_dib(&b).unwrap().pixels, vec![0xff030201, 0xff090807]);
        }
    }
    #[test]
    fn indexed_one_four_and_eight_bit_rows_use_bounded_palettes() {
        for bits in [1, 4, 8] {
            let mut b = header(2, -1, bits, 0, 8);
            put(&mut b, 32, 2);
            put(&mut b, 40, 0x123456);
            put(&mut b, 44, 0xabcdef);
            b[48] = match bits {
                1 => 0x40,
                4 => 0x01,
                _ => 0,
            };
            if bits == 8 {
                b[49] = 1;
            }
            assert_eq!(decode_dib(&b).unwrap().pixels, vec![0xff123456, 0xffabcdef]);
            if bits != 1 {
                b[48] = if bits == 4 { 0xf0 } else { 255 };
                assert!(decode_dib(&b).is_err());
            }
        }
    }
    #[test]
    fn rgb555_rgb565_and_explicit_alpha_masks_have_defined_scaling() {
        let mut b = header(1, -1, 16, 0, 0);
        b[40..42].copy_from_slice(&0x7c00u16.to_le_bytes());
        assert_eq!(decode_dib(&b).unwrap().pixels, vec![0xffff0000]);
        let mut b = header(1, -1, 16, 3, 12);
        for (i, n) in [0xf800, 0x7e0, 0x1f].into_iter().enumerate() {
            put(&mut b, 40 + i * 4, n);
        }
        b[52..54].copy_from_slice(&0x7e0u16.to_le_bytes());
        assert_eq!(decode_dib(&b).unwrap().pixels, vec![0xff00ff00]);
        let mut b = header(1, -1, 32, 0, 0);
        put(&mut b, 40, 0x00123456);
        assert_eq!(decode_dib(&b).unwrap().pixels, vec![0xff123456]);
        for header_size in [52, 56, 108, 124] {
            let mut b = vec![0; header_size + 4];
            put(&mut b, 0, header_size as u32);
            put(&mut b, 4, 1);
            put(&mut b, 8, (-1i32) as u32);
            b[12..14].copy_from_slice(&1u16.to_le_bytes());
            b[14..16].copy_from_slice(&32u16.to_le_bytes());
            put(&mut b, 16, 3);
            for (i, n) in [0xff, 0xff00, 0xff0000].into_iter().enumerate() {
                put(&mut b, 40 + i * 4, n);
            }
            if header_size >= 56 {
                put(&mut b, 52, 0xff000000);
            }
            put(&mut b, header_size, 0x80112233);
            assert_eq!(
                decode_dib(&b).unwrap().pixels,
                vec![if header_size >= 56 {
                    0x80332211
                } else {
                    0xff332211
                }]
            );
        }
    }
    #[test]
    fn malformed_headers_profiles_masks_and_truncation_reject_before_publication() {
        let valid = encode_dib(
            &Image {
                width: 2,
                height: 2,
                pixels: vec![0; 4],
            }
            .view(),
        )
        .unwrap();
        for n in 0..valid.len() {
            assert!(decode_dib(&valid[..n]).is_err(), "{n}");
        }
        for (offset, value) in [
            (0, 12),
            (4, 0),
            (4, 4097),
            (8, i32::MIN as u32),
            (16, 1),
            (20, 99),
            (32, 257),
            (40, 0),
            (44, 0xff0000),
            (52, 0xff),
            (112, 124),
            (116, 16),
            (56, 0x1234),
        ] {
            let mut b = valid.clone();
            put(&mut b, offset, value);
            assert!(decode_dib(&b).is_err(), "{offset} {value}");
        }
        assert!(decode_png(b"not a png").is_err());
        let mut bad = Image {
            width: 4097,
            height: 1,
            pixels: vec![],
        };
        assert!(encode_dib(&bad.view()).is_err());
        bad.width = 1;
        assert!(encode_png(&bad.view()).is_err());
    }
    #[test]
    fn png_oversized_ihdr_rejects_before_decoder_allocation() {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&vec![0; 4097 * 4], 4097, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        assert!(decode_png(&png)
            .err()
            .unwrap()
            .contains("dimensions must be between"));
    }
    #[test]
    fn bounded_mutation_fuzz_does_not_panic_or_allocate_from_unchecked_sizes() {
        let original = encode_dib(
            &Image {
                width: 3,
                height: 2,
                pixels: vec![0xffffffff; 6],
            }
            .view(),
        )
        .unwrap();
        let mut seed = 7u32;
        for _ in 0..4000 {
            let mut b = original.clone();
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let at = seed as usize % 124;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            b[at] = seed as u8;
            if let Ok(image) = decode_dib(&b) {
                assert!(image.width <= 4096 && image.height <= 4096);
                assert_eq!(image.pixels.len(), image.width * image.height);
            }
        }
    }
}
