//! Stable-ID A8 outcome tiles. This namespace never shares RGBA tile references.
use super::{bad, hash, Hash, Metadata, State, TILE};
use crate::{application::ActionError, engine::raster_masks::PlaneView};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

pub(super) type Grids = BTreeMap<u64, Vec<Cell>>;
const MAX_CHANGES: usize = crate::engine::raster_masks::MAX_MASKS
    * crate::engine::MAX_SIDE.div_ceil(TILE)
    * crate::engine::MAX_SIDE.div_ceil(TILE);

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Block {
    #[serde(with = "byte_wire")]
    pub bytes: Vec<u8>,
}
impl Block {
    pub fn normalized(mut bytes: Vec<u8>) -> Self {
        if !bytes.is_empty() && bytes.iter().all(|v| *v == bytes[0]) {
            bytes.truncate(1);
        }
        Self { bytes }
    }
    pub fn read(plane: PlaneView<'_>, metadata: &Metadata, tile: usize) -> Self {
        match plane {
            PlaneView::Uniform(value) => Self { bytes: vec![value] },
            PlaneView::Dense(bytes) => {
                let cols = metadata.width.div_ceil(TILE);
                let x = tile % cols * TILE;
                let y = tile / cols * TILE;
                let mut result = Vec::with_capacity(metadata.tile_len(tile));
                for row in y..(y + TILE).min(metadata.height) {
                    result.extend_from_slice(
                        &bytes[row * metadata.width + x
                            ..row * metadata.width + (x + TILE).min(metadata.width)],
                    );
                }
                Self::normalized(result)
            }
        }
    }
    pub fn cell(self) -> Cell {
        Cell {
            digest: hash(&self.bytes),
            block: Arc::new(self),
        }
    }
}
#[derive(Clone)]
pub(super) struct Cell {
    pub block: Arc<Block>,
    pub digest: Hash,
}
impl Cell {
    fn empty() -> Self {
        Self {
            block: Arc::new(Block { bytes: vec![] }),
            digest: [0; 32],
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Change {
    pub mask_id: u64,
    pub tile: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<Block>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reuse: Option<usize>,
    #[serde(default)]
    pub xor: bool,
}
pub(super) fn deserialize_changes<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Change>, D::Error> {
    struct Bounded;
    impl<'de> serde::de::Visitor<'de> for Bounded {
        type Value = Vec<Change>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a bounded list of A8 tile changes")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            if sequence.size_hint().is_some_and(|size| size > MAX_CHANGES) {
                return Err(serde::de::Error::custom("Too many A8 tile changes"));
            }
            let mut changes =
                Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAX_CHANGES));
            while let Some(change) = sequence.next_element()? {
                if changes.len() == MAX_CHANGES {
                    return Err(serde::de::Error::custom("Too many A8 tile changes"));
                }
                changes.push(change);
            }
            Ok(changes)
        }
    }
    deserializer.deserialize_seq(Bounded)
}
impl Change {
    pub fn resolve(
        &self,
        previous: Option<&Grids>,
        resolved: &[Cell],
        metadata: &Metadata,
    ) -> Result<Cell, ActionError> {
        let len = metadata.tile_len(self.tile);
        let cell = match (&self.block, self.reuse) {
            (Some(block), None) if [1, len].contains(&block.bytes.len()) => {
                if self.xor {
                    let old = previous
                        .and_then(|grids| grids.get(&self.mask_id)?.get(self.tile))
                        .filter(|cell| [1, len].contains(&cell.block.bytes.len()))
                        .ok_or_else(|| bad("Missing A8 XOR base tile"))?;
                    Block::normalized(
                        (0..len)
                            .map(|i| {
                                block.bytes[if block.bytes.len() == 1 { 0 } else { i }]
                                    ^ old.block.bytes
                                        [if old.block.bytes.len() == 1 { 0 } else { i }]
                            })
                            .collect(),
                    )
                    .cell()
                } else {
                    Block::normalized(block.bytes.clone()).cell()
                }
            }
            (None, Some(index)) if !self.xor => resolved
                .get(index)
                .cloned()
                .ok_or_else(|| bad("Invalid or forward A8 tile reference"))?,
            _ => return Err(bad("Invalid A8 replay tile encoding")),
        };
        if ![1, len].contains(&cell.block.bytes.len()) {
            return Err(bad("A8 tile reference size mismatch"));
        }
        Ok(cell)
    }
}

pub(super) fn same_dimensions<'a>(
    previous: Option<&'a State>,
    metadata: &Metadata,
) -> Option<&'a State> {
    previous.filter(|old| {
        (old.metadata.width, old.metadata.height) == (metadata.width, metadata.height)
    })
}

/// A raster-count replacement does not invalidate independent A8 tile grids.
pub(super) fn seed(previous: Option<&State>, metadata: &Metadata) -> Grids {
    let previous = same_dimensions(previous, metadata);
    metadata
        .composition
        .raster_masks
        .items
        .iter()
        .map(|mask| {
            let grid = previous
                .and_then(|state| state.masks.get(&mask.id))
                .cloned()
                .unwrap_or_else(|| vec![Cell::empty(); metadata.count()]);
            (mask.id, grid)
        })
        .collect()
}

pub(super) fn complete(grids: &Grids, metadata: &Metadata) -> bool {
    grids.len() == metadata.composition.raster_masks.items.len()
        && grids.values().all(|grid| {
            grid.len() == metadata.count()
                && grid.iter().enumerate().all(|(tile, cell)| {
                    [1, metadata.tile_len(tile)].contains(&cell.block.bytes.len())
                })
        })
}

pub(super) fn compact(changes: &mut [Change], previous: Option<&State>, current: &mut State) {
    let mut seen = BTreeMap::<Hash, (usize, Cell)>::new();
    let previous = same_dimensions(previous, &current.metadata);
    for (index, change) in changes.iter_mut().enumerate() {
        let cell = &current.masks[&change.mask_id][change.tile];
        if let Some((prior, shared)) = seen.get(&cell.digest) {
            change.block = None;
            change.reuse = Some(*prior);
            current.masks.get_mut(&change.mask_id).unwrap()[change.tile] = shared.clone();
            continue;
        }
        seen.insert(cell.digest, (index, cell.clone()));
        let block = change.block.as_mut().unwrap();
        if block.bytes.len() == 1 {
            continue;
        }
        if let Some(old) = previous
            .and_then(|state| state.masks.get(&change.mask_id)?.get(change.tile))
            .filter(|cell| [1, block.bytes.len()].contains(&cell.block.bytes.len()))
        {
            let delta: Vec<_> = block
                .bytes
                .iter()
                .enumerate()
                .map(|(i, byte)| {
                    byte ^ old.block.bytes[if old.block.bytes.len() == 1 { 0 } else { i }]
                })
                .collect();
            let zeros = delta.iter().filter(|byte| **byte == 0).count();
            if zeros > block.bytes.iter().filter(|byte| **byte == 0).count() + block.bytes.len() / 8
            {
                *block = Block::normalized(delta);
                change.xor = true;
            }
        }
    }
}

pub(super) fn uniform(grid: &[Cell]) -> Option<u8> {
    let value = *grid.first()?.block.bytes.first()?;
    grid.iter()
        .all(|cell| cell.block.bytes.as_slice() == [value])
        .then_some(value)
}

pub(super) fn fill(grid: &[Cell], metadata: &Metadata, destination: &mut [u8]) {
    let cols = metadata.width.div_ceil(TILE);
    for (tile, cell) in grid.iter().enumerate() {
        let x = tile % cols * TILE;
        let y = tile / cols * TILE;
        let width = (metadata.width - x).min(TILE);
        for row in y..(y + TILE).min(metadata.height) {
            let target =
                &mut destination[row * metadata.width + x..row * metadata.width + x + width];
            if cell.block.bytes.len() == 1 {
                target.fill(cell.block.bytes[0]);
            } else {
                let offset = (row - y) * width;
                target.copy_from_slice(&cell.block.bytes[offset..offset + width]);
            }
        }
    }
}

mod byte_wire {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&crate::application::codec::base64(bytes))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        if text.is_empty() || text.len() > 5464 || text.len() % 4 != 0 {
            return Err(serde::de::Error::custom("Invalid A8 replay byte block"));
        }
        let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
        let mut padded = false;
        for chunk in text.as_bytes().chunks_exact(4) {
            let mut word = 0u32;
            let mut pad = 0;
            for c in chunk {
                let value = match c {
                    b'A'..=b'Z' => c - b'A',
                    b'a'..=b'z' => c - b'a' + 26,
                    b'0'..=b'9' => c - b'0' + 52,
                    b'+' => 62,
                    b'/' => 63,
                    b'=' => {
                        pad += 1;
                        0
                    }
                    _ => return Err(serde::de::Error::custom("Invalid A8 base64 data")),
                };
                if padded || pad > 0 && *c != b'=' {
                    return Err(serde::de::Error::custom("Invalid A8 base64 padding"));
                }
                word = word << 6 | value as u32;
            }
            if pad > 2 || pad == 2 && word & 0xffff != 0 || pad == 1 && word & 0xff != 0 {
                return Err(serde::de::Error::custom("Noncanonical A8 base64 padding"));
            }
            bytes.push((word >> 16) as u8);
            if pad < 2 {
                bytes.push((word >> 8) as u8);
            }
            if pad < 1 {
                bytes.push(word as u8);
            }
            padded = pad != 0;
        }
        if bytes.len() > 4096 {
            return Err(serde::de::Error::custom("Oversized A8 replay byte block"));
        }
        Ok(bytes)
    }
}
