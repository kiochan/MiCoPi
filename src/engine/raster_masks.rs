//! First-class A8 mask metadata and raw storage, independent of pixel selection.
use serde::{Deserialize, Serialize};

pub const MAX_MASKS: usize = 32;
pub const MASK_SAMPLE_LIMIT: usize = 64 * 1024 * 1024;
pub const MAX_MASK_FEATHER: u16 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaskOwner {
    Layer { id: u64 },
    Group { id: u64 },
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PaintTarget {
    #[default]
    LayerPixels,
    Mask {
        id: u64,
    },
}
impl PaintTarget {
    pub fn is_default(&self) -> bool {
        *self == Self::LayerPixels
    }
}
fn yes() -> bool {
    true
}
fn full() -> u8 {
    255
}
fn is_yes(value: &bool) -> bool {
    *value
}
fn is_full(value: &u8) -> bool {
    *value == 255
}
fn is_zero(value: &u16) -> bool {
    *value == 0
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaskMetadata {
    pub id: u64,
    pub owner: MaskOwner,
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub enabled: bool,
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub linked: bool,
    #[serde(default = "full", skip_serializing_if = "is_full")]
    pub density: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub feather_radius_px: u16,
}
impl MaskMetadata {
    pub fn new(id: u64, owner: MaskOwner) -> Self {
        Self {
            id,
            owner,
            enabled: true,
            linked: true,
            density: 255,
            feather_radius_px: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaskState {
    pub version: u32,
    pub items: Vec<MaskMetadata>,
    pub next_id: u64,
    #[serde(default, skip_serializing_if = "PaintTarget::is_default")]
    pub target: PaintTarget,
}
impl Default for MaskState {
    fn default() -> Self {
        Self {
            version: 1,
            items: Vec::new(),
            next_id: 0,
            target: PaintTarget::LayerPixels,
        }
    }
}
impl MaskState {
    pub fn is_default(&self) -> bool {
        self.version == 1 && self.items.is_empty() && self.next_id == 0 && self.target.is_default()
    }
    pub fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.items.len() * std::mem::size_of::<MaskMetadata>()
    }
    pub fn validate_owners(&self, layer_ids: &[u64], group_ids: &[u64]) -> Result<(), String> {
        if self.version != 1 || self.items.len() > MAX_MASKS || self.next_id > u32::MAX as u64 + 1 {
            return Err("Invalid raster mask version, count or allocator".into());
        }
        if self.next_id == 0 && !self.is_default() {
            return Err("Raster mask identities require an allocator".into());
        }
        let mut previous = 0;
        for (index, item) in self.items.iter().enumerate() {
            if item.id <= previous
                || item.id >= self.next_id
                || item.id > u32::MAX as u64
                || item.feather_radius_px > MAX_MASK_FEATHER
                || self.items[..index]
                    .iter()
                    .any(|prior| prior.owner == item.owner)
            {
                return Err(
                    "Invalid or duplicate raster mask identity, owner or properties".into(),
                );
            }
            let exists = match item.owner {
                MaskOwner::Layer { id } => {
                    id != 0 && id <= u32::MAX as u64 && layer_ids.contains(&id)
                }
                MaskOwner::Group { id } => {
                    id != 0 && id <= u32::MAX as u64 && group_ids.contains(&id)
                }
            };
            if !exists {
                return Err("Raster mask owner does not exist".into());
            }
            previous = item.id;
        }
        if let PaintTarget::Mask { id } = self.target {
            if !self.items.iter().any(|item| item.id == id) {
                return Err("Raster mask painting target does not exist".into());
            }
        }
        Ok(())
    }
    pub fn validate_dimensions(&self, width: usize, height: usize) -> Result<(), String> {
        if width == 0
            || height == 0
            || width > super::MAX_SIDE
            || height > super::MAX_SIDE
            || width
                .checked_mul(height)
                .and_then(|area| area.checked_mul(self.items.len()))
                .is_none_or(|samples| samples > MASK_SAMPLE_LIMIT)
        {
            return Err("Raster masks exceed the logical byte-plane budget".into());
        }
        Ok(())
    }
    pub fn validate(
        &self,
        width: usize,
        height: usize,
        layer_ids: &[u64],
        group_ids: &[u64],
    ) -> Result<(), String> {
        self.validate_owners(layer_ids, group_ids)?;
        self.validate_dimensions(width, height)
    }
}

/// Raw samples are never serialized as composition JSON or shared for mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SamplePlane {
    Uniform(u8),
    Dense(Vec<u8>),
}
impl SamplePlane {
    pub(crate) fn validate(&self, area: usize) -> Result<(), String> {
        if matches!(self, Self::Dense(samples) if samples.len() != area) {
            return Err("Invalid raster mask sample count".into());
        }
        Ok(())
    }
    pub(crate) fn memory_bytes(&self) -> usize {
        match self {
            Self::Uniform(_) => 1,
            Self::Dense(samples) => samples.len(),
        }
    }
    pub fn view(&self) -> PlaneView<'_> {
        match self {
            Self::Uniform(value) => PlaneView::Uniform(*value),
            Self::Dense(samples) => PlaneView::Dense(samples),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum PlaneView<'a> {
    Uniform(u8),
    Dense(&'a [u8]),
}
impl PlaneView<'_> {
    pub fn sample(&self, index: usize) -> u8 {
        match self {
            Self::Uniform(value) => *value,
            Self::Dense(samples) => samples[index],
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct MaskPlane {
    pub id: u64,
    pub samples: SamplePlane,
    pub content_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaskLease {
    pub document_identity: u64,
    pub id: u64,
    pub slot: usize,
    pub storage_epoch: u64,
}
