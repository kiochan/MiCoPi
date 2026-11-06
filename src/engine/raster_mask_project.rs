//! MICOPI04 byte-plane suffix. Legacy project writers never enter this path.
use super::*;
use crate::engine::raster_masks::{MaskPlane, SamplePlane, MAX_MASKS};

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid or oversized raster mask project payload",
    )
}
impl Document {
    pub(super) fn write_mask_project_suffix(
        &self,
        writer: &mut impl Write,
        replay: Option<&[u8]>,
    ) -> io::Result<()> {
        writer.write_all(&(self.mask_planes.len() as u32).to_le_bytes())?;
        for plane in &self.mask_planes {
            writer.write_all(&plane.id.to_le_bytes())?;
            let uniform = match &plane.samples {
                SamplePlane::Uniform(value) => Some(*value),
                SamplePlane::Dense(samples) => samples
                    .first()
                    .copied()
                    .filter(|value| samples.iter().all(|sample| sample == value)),
            };
            if let Some(value) = uniform {
                writer.write_all(&[0])?;
                writer.write_all(&1u32.to_le_bytes())?;
                writer.write_all(&[value])?;
            } else {
                let SamplePlane::Dense(samples) = &plane.samples else {
                    unreachable!()
                };
                writer.write_all(&[1])?;
                writer.write_all(&(samples.len() as u32).to_le_bytes())?;
                writer.write_all(samples)?;
            }
        }
        let replay = replay.unwrap_or_default();
        writer.write_all(&(replay.len() as u64).to_le_bytes())?;
        writer.write_all(replay)
    }
    pub(super) fn read_mask_project_suffix(
        &mut self,
        reader: &mut impl Read,
        size: u64,
    ) -> io::Result<()> {
        let mut count = [0; 4];
        reader.read_exact(&mut count).map_err(|_| invalid())?;
        let count = u32::from_le_bytes(count) as usize;
        if count > MAX_MASKS || count != self.raster_masks.items.len() {
            return Err(invalid());
        }
        let mut consumed = 4u64;
        let area = self.width * self.height;
        let mut planes = Vec::with_capacity(count);
        for item in &self.raster_masks.items {
            let mut id = [0; 8];
            let mut kind = [0; 1];
            let mut length = [0; 4];
            reader.read_exact(&mut id).map_err(|_| invalid())?;
            reader.read_exact(&mut kind).map_err(|_| invalid())?;
            reader.read_exact(&mut length).map_err(|_| invalid())?;
            let id = u64::from_le_bytes(id);
            let length = u32::from_le_bytes(length) as usize;
            if id != item.id
                || !matches!((kind[0], length), (0, 1)) && !(kind[0] == 1 && length == area)
            {
                return Err(invalid());
            }
            consumed = consumed
                .checked_add(13 + length as u64)
                .ok_or_else(invalid)?;
            if consumed.checked_add(8).is_none_or(|bytes| bytes > size) {
                return Err(invalid());
            }
            let samples = if kind[0] == 0 {
                let mut value = [0; 1];
                reader.read_exact(&mut value).map_err(|_| invalid())?;
                SamplePlane::Uniform(value[0])
            } else {
                let mut samples = vec![0; area];
                reader.read_exact(&mut samples).map_err(|_| invalid())?;
                SamplePlane::Dense(samples)
            };
            planes.push(MaskPlane {
                id,
                samples,
                content_revision: 0,
            });
        }
        let mut length = [0; 8];
        reader.read_exact(&mut length).map_err(|_| invalid())?;
        let length = u64::from_le_bytes(length);
        if length > crate::engine::replay::MAX_ARCHIVE as u64
            || consumed
                .checked_add(8)
                .and_then(|bytes| bytes.checked_add(length))
                != Some(size)
        {
            return Err(invalid());
        }
        self.replay_data = if length == 0 {
            None
        } else {
            let mut bytes = vec![0; length as usize];
            reader.read_exact(&mut bytes).map_err(|_| invalid())?;
            Some(bytes)
        };
        self.mask_planes = planes;
        self.bump_mask_storage_epoch();
        Ok(())
    }
}
