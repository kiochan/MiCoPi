//! Verified outcome journal. Playback applies recorded tile results, never commands or DLL code.
use crate::{
    application::{ActionError, Parameters, Pointer, ToolKind, View},
    engine::{document::Composition, Document, TILE},
};
use flate2::{read::DeflateDecoder, write::DeflateEncoder, Compression};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
#[cfg(test)]
mod mask_tests;
mod masks;

pub const MAGIC: &[u8; 8] = b"MCRPLY02";
pub const MAX_ARCHIVE: usize = 1024 * 1024 * 1024;
const MAX_DECODED: usize = 512 * 1024 * 1024;
const MAX_EVENTS: usize = 1_000_000;
struct Limited<W> {
    inner: W,
    written: usize,
}
impl<W: Write> Write for Limited<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_DECODED.saturating_sub(self.written) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Replay entry exceeds the encoding limit",
            ));
        }
        let n = self.inner.write(bytes)?;
        self.written += n;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
pub type Hash = [u8; 32];
fn bad(message: impl Into<String>) -> ActionError {
    ActionError::new("replay_integrity", message)
}
pub fn hex(hash: &Hash) -> String {
    hash.iter().map(|b| format!("{b:02x}")).collect()
}
fn hash(bytes: &[u8]) -> Hash {
    Sha256::digest(bytes).into()
}
fn chain(previous: &Hash, bytes: &[u8]) -> Hash {
    let mut h = Sha256::new();
    h.update(previous);
    h.update((bytes.len() as u32).to_le_bytes());
    h.update(bytes);
    h.finalize().into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub width: usize,
    pub height: usize,
    pub active: usize,
    pub composition: Composition,
    pub selected_tool: String,
    pub tool: ToolKind,
    pub parameters: Parameters,
    pub view: View,
    pub revision: u64,
    pub dirty: bool,
    pub file: Option<PathBuf>,
    pub source_file: Option<PathBuf>,
    pub ui: Value,
    pub drawing: bool,
    pub render_error: Option<String>,
}
impl Metadata {
    pub fn validate(&self) -> Result<(), ActionError> {
        let n = self.composition.layers.len();
        if self.width == 0
            || self.height == 0
            || self.width > 4096
            || self.height > 4096
            || n == 0
            || n > 16
            || self.active >= n
            || self.width * self.height * n > 64 * 1024 * 1024
            || self.selected_tool.len() > 200
            || !self.view.zoom.is_finite()
            || self.view.zoom <= 0.
            || !self.view.pan_x.is_finite()
            || !self.view.pan_y.is_finite()
        {
            return Err(bad("Invalid replay document metadata"));
        }
        self.composition.graph.compile(n).map_err(bad)?;
        self.composition.validate_layer_metadata().map_err(bad)?;
        self.composition
            .validate_layer_selection(self.active)
            .map_err(bad)?;
        self.composition
            .selection
            .validate(self.width, self.height)
            .map_err(bad)?;
        if !self.composition.raster_masks.is_default() {
            let layers: Vec<_> = self
                .composition
                .layer_metadata
                .iter()
                .map(|layer| layer.id)
                .collect();
            let groups: Vec<_> = self
                .composition
                .groups
                .as_ref()
                .map(|tree| tree.groups.iter().map(|group| group.id).collect())
                .unwrap_or_default();
            self.composition
                .raster_masks
                .validate_with_adjustments(
                    self.width,
                    self.height,
                    &layers,
                    &groups,
                    &self
                        .composition
                        .groups
                        .as_ref()
                        .map_or_else(Vec::new, |tree| {
                            tree.adjustments.iter().map(|a| a.id).collect()
                        }),
                )
                .map_err(bad)?;
        }
        self.composition.workflow.validate_structure()?;
        Parameters::default().patched(&serde_json::to_value(&self.parameters)?)?;
        Ok(())
    }
    fn count(&self) -> usize {
        self.width.div_ceil(TILE) * self.height.div_ceil(TILE)
    }
    fn tile_len(&self, id: usize) -> usize {
        let cols = self.width.div_ceil(TILE);
        (self.width - (id % cols) * TILE).min(TILE) * (self.height - (id / cols) * TILE).min(TILE)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Genesis {
        version: String,
        environment: Value,
    },
    Action {
        action: String,
        args: Value,
        depth: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<Value>,
    },
    Pointer {
        samples: Vec<Pointer>,
        complete: bool,
    },
    Observed {
        reason: String,
    },
    Rejected {
        input: String,
    },
}
impl Source {
    pub fn label(&self) -> String {
        match self {
            Self::Genesis { .. } => "Initial snapshot".into(),
            Self::Action { action, .. } => action.clone(),
            Self::Pointer { complete, .. } => if *complete {
                "Stroke"
            } else {
                "Stroke segment"
            }
            .into(),
            Self::Observed { .. } => "State checkpoint".into(),
            Self::Rejected { .. } => "Rejected input".into(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Block {
    #[serde(with = "pixel_wire")]
    pixels: Vec<u32>,
}
impl Block {
    fn normalized(mut pixels: Vec<u32>) -> Self {
        if !pixels.is_empty() && pixels.iter().all(|p| *p == pixels[0]) {
            pixels.truncate(1);
        }
        Self { pixels }
    }
    fn read(pixels: &[u32], meta: &Metadata, id: usize) -> Self {
        let cols = meta.width.div_ceil(TILE);
        let x = (id % cols) * TILE;
        let y = (id / cols) * TILE;
        let mut data = Vec::with_capacity(meta.tile_len(id));
        for row in y..(y + TILE).min(meta.height) {
            data.extend_from_slice(
                &pixels[row * meta.width + x..row * meta.width + (x + TILE).min(meta.width)],
            );
        }
        if data.iter().all(|p| *p == data[0]) {
            data.truncate(1);
        }
        Self { pixels: data }
    }
    fn digest(&self) -> Hash {
        let mut h = Sha256::new();
        for p in &self.pixels {
            h.update(p.to_le_bytes());
        }
        h.finalize().into()
    }
}
#[derive(Clone)]
struct Cell {
    block: Arc<Block>,
    digest: Hash,
}
#[derive(Clone)]
pub struct State {
    pub metadata: Metadata,
    layers: Vec<Vec<Cell>>,
    surface: Vec<Cell>,
    masks: masks::Grids,
}
impl State {
    pub(crate) fn restore_document(&self, doc: &mut Document) {
        let m = &self.metadata;
        if (doc.width, doc.height) != (m.width, m.height) {
            *doc = Document::new(m.width, m.height);
        }
        doc.layers
            .resize_with(self.layers.len(), || crate::engine::document::Layer {
                pixels: vec![0; m.width * m.height],
                visible: true,
                opacity: 255,
                mode: Default::default(),
            });
        let cols = m.width.div_ceil(TILE);
        for (layer, grid) in doc.layers.iter_mut().zip(&self.layers) {
            for (id, cell) in grid.iter().enumerate() {
                let x = id % cols * TILE;
                let y = id / cols * TILE;
                let mut i = 0;
                for row in y..(y + TILE).min(m.height) {
                    for col in x..(x + TILE).min(m.width) {
                        layer.pixels[row * m.width + col] =
                            cell.block.pixels[if cell.block.pixels.len() == 1 { 0 } else { i }];
                        i += 1;
                    }
                }
            }
        }
        doc.active = m.active;
        doc.restore_composition(m.composition.clone());
        doc.restore_mask_storage(
            &m.composition.raster_masks,
            |id| masks::uniform(&self.masks[&id]),
            |id, output| masks::fill(&self.masks[&id], m, output),
        );
        doc.dirty = m.dirty;
        doc.render_error = m.render_error.clone();
        doc.composite.resize(m.width * m.height, 0);
        for (id, cell) in self.surface.iter().enumerate() {
            let x = id % cols * TILE;
            let y = id / cols * TILE;
            let mut i = 0;
            for row in y..(y + TILE).min(m.height) {
                for col in x..(x + TILE).min(m.width) {
                    doc.composite[row * m.width + col] =
                        cell.block.pixels[if cell.block.pixels.len() == 1 { 0 } else { i }];
                    i += 1;
                }
            }
        }
        doc.finish_verified_surface_restore();
        doc.replay_tiles = (0..self.layers.len())
            .flat_map(|l| (0..m.count()).map(move |t| (l, t)))
            .collect();
        doc.replay_mask_tiles = self
            .masks
            .keys()
            .flat_map(|id| (0..m.count()).map(move |tile| (*id, tile)))
            .collect();
    }
    pub fn digest(&self) -> Hash {
        let mut h = Sha256::new();
        h.update(serde_json::to_vec(&self.metadata).expect("finite validated replay metadata"));
        for grid in self.layers.iter().chain(std::iter::once(&self.surface)) {
            for c in grid {
                h.update(c.digest);
            }
        }
        if !self.masks.is_empty() {
            h.update(b"MiCoPi/A8-state/v1\0");
            h.update((self.masks.len() as u32).to_le_bytes());
            for (id, grid) in &self.masks {
                h.update(id.to_le_bytes());
                h.update((grid.len() as u32).to_le_bytes());
                for cell in grid {
                    h.update(cell.digest);
                }
            }
        }
        h.finalize().into()
    }
    pub fn surface_hash(&self) -> Hash {
        let mut h = Sha256::new();
        h.update((self.metadata.width as u32).to_le_bytes());
        h.update((self.metadata.height as u32).to_le_bytes());
        for c in &self.surface {
            h.update(c.digest);
        }
        h.finalize().into()
    }
    pub fn pixels(&self) -> Vec<u32> {
        let m = &self.metadata;
        let mut out = vec![0; m.width * m.height];
        let cols = m.width.div_ceil(TILE);
        for (id, c) in self.surface.iter().enumerate() {
            let x = id % cols * TILE;
            let y = id / cols * TILE;
            let mut p = 0;
            for row in y..(y + TILE).min(m.height) {
                for col in x..(x + TILE).min(m.width) {
                    out[row * m.width + col] =
                        c.block.pixels[if c.block.pixels.len() == 1 { 0 } else { p }];
                    p += 1;
                }
            }
        }
        out
    }
    pub fn preview(&self, max_side: usize) -> (usize, usize, Vec<[u8; 4]>) {
        let m = &self.metadata;
        let scale = (max_side as f32 / m.width.max(m.height) as f32).min(1.);
        let w = ((m.width as f32 * scale) as usize).max(1);
        let h = ((m.height as f32 * scale) as usize).max(1);
        let cols = m.width.div_ceil(TILE);
        let mut out = Vec::with_capacity(w * h);
        for y in 0..h {
            let sy = y * m.height / h;
            for x in 0..w {
                let sx = x * m.width / w;
                let id = sy / TILE * cols + sx / TILE;
                let cell = &self.surface[id];
                let tw = (m.width - sx / TILE * TILE).min(TILE);
                let p = cell.block.pixels[if cell.block.pixels.len() == 1 {
                    0
                } else {
                    sy % TILE * tw + sx % TILE
                }];
                out.push([(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]);
            }
        }
        (w, h, out)
    }
    pub fn matches_document(&self, doc: &Document) -> bool {
        let m = &self.metadata;
        let mut composition = doc.composition();
        // Legacy journals hashed the old metadata shape. Compare their pixels
        // and original fields before introducing IDs at the next event boundary.
        if m.composition.layer_metadata.is_empty() {
            composition.layer_metadata.clear();
            composition.next_layer_id = 0;
        }
        if (doc.width, doc.height, doc.active) != (m.width, m.height, m.active)
            || composition != m.composition
        {
            return false;
        }
        if doc.validate_mask_storage().is_err()
            || self.masks.len() != m.composition.raster_masks.items.len()
        {
            return false;
        }
        if !self.masks.iter().all(|(id, grid)| {
            grid.iter().enumerate().all(|(tile, cell)| {
                doc.mask_plane(*id).is_ok_and(|plane| {
                    masks::Block::read(plane, m, tile).cell().digest == cell.digest
                })
            })
        }) {
            return false;
        }
        self.layers.iter().zip(&doc.layers).all(|(grid, layer)| {
            grid.iter()
                .enumerate()
                .all(|(id, c)| Block::read(&layer.pixels, m, id).digest() == c.digest)
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    layer: usize,
    tile: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    block: Option<Block>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reuse: Option<usize>,
    #[serde(default)]
    xor: bool,
}
impl Change {
    fn resolve(
        &self,
        previous: Option<&State>,
        resolved: &[Cell],
        m: &Metadata,
    ) -> Result<Cell, ActionError> {
        let len = m.tile_len(self.tile);
        let block = match (&self.block, self.reuse) {
            (Some(block), None) if [1, len].contains(&block.pixels.len()) => {
                if self.xor {
                    let old = previous
                        .and_then(|s| {
                            if self.layer == s.layers.len() {
                                s.surface.get(self.tile)
                            } else {
                                s.layers.get(self.layer)?.get(self.tile)
                            }
                        })
                        .ok_or_else(|| bad("Missing XOR base tile"))?;
                    Arc::new(Block::normalized(
                        (0..len)
                            .map(|i| {
                                block.pixels[if block.pixels.len() == 1 { 0 } else { i }]
                                    ^ old.block.pixels
                                        [if old.block.pixels.len() == 1 { 0 } else { i }]
                            })
                            .collect(),
                    ))
                } else {
                    Arc::new(block.clone())
                }
            }
            (None, Some(index)) if !self.xor => resolved
                .get(index)
                .map(|c| c.block.clone())
                .ok_or_else(|| bad("Invalid or forward tile reference"))?,
            _ => return Err(bad("Invalid replay tile encoding")),
        };
        if ![1, len].contains(&block.pixels.len()) {
            return Err(bad("Replay tile reference size mismatch"));
        }
        Ok(Cell {
            digest: block.digest(),
            block,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub sequence: usize,
    pub elapsed_ms: u64,
    pub source: Source,
    pub success: bool,
    pub error: Option<String>,
    pub before: Hash,
    pub after: Hash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metadata: Option<Metadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metadata_delta: Option<Value>,
    replace: bool,
    changes: Vec<Change>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mask_version: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "masks::deserialize_changes"
    )]
    mask_changes: Vec<masks::Change>,
}
impl Event {
    fn metadata(&self, previous: Option<&State>) -> Result<Metadata, ActionError> {
        if let Some(m) = &self.metadata {
            if self.metadata_delta.is_some() {
                return Err(bad("Ambiguous replay metadata"));
            }
            return Ok(m.clone());
        }
        let old = previous.ok_or_else(|| bad("Missing metadata base"))?;
        let mut value = serde_json::to_value(&old.metadata)?;
        let delta = self
            .metadata_delta
            .as_ref()
            .and_then(Value::as_object)
            .ok_or_else(|| bad("Missing metadata delta"))?;
        for (k, v) in delta {
            value.as_object_mut().unwrap().insert(k.clone(), v.clone());
        }
        Ok(serde_json::from_value(value).map_err(|e| bad(e.to_string()))?)
    }
    fn compact(&mut self, previous: &State, current: &mut State) -> Result<(), ActionError> {
        if !self.replace {
            let old = serde_json::to_value(&previous.metadata)?;
            let value = serde_json::to_value(self.metadata.as_ref().unwrap())?;
            self.metadata_delta = Some(Value::Object(
                value
                    .as_object()
                    .unwrap()
                    .iter()
                    .filter(|(k, v)| old.get(k.as_str()) != Some(*v))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ));
            self.metadata = None;
        }
        let n = current.layers.len();
        let mut seen = std::collections::BTreeMap::<Hash, (usize, Cell)>::new();
        for (index, c) in self.changes.iter_mut().enumerate() {
            let digest = if c.layer == n {
                current.surface[c.tile].digest
            } else {
                current.layers[c.layer][c.tile].digest
            };
            if let Some((prior, cell)) = seen.get(&digest) {
                c.block = None;
                c.reuse = Some(*prior);
                if c.layer == n {
                    current.surface[c.tile] = cell.clone();
                } else {
                    current.layers[c.layer][c.tile] = cell.clone();
                }
                continue;
            }
            let cell = if c.layer == n {
                current.surface[c.tile].clone()
            } else {
                current.layers[c.layer][c.tile].clone()
            };
            seen.insert(digest, (index, cell));
            let block = c.block.as_mut().unwrap();
            if !self.replace && block.pixels.len() > 1 {
                let old = if c.layer == n {
                    &previous.surface[c.tile]
                } else {
                    &previous.layers[c.layer][c.tile]
                };
                let delta: Vec<_> = block
                    .pixels
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        p ^ old.block.pixels[if old.block.pixels.len() == 1 { 0 } else { i }]
                    })
                    .collect();
                let zeros = delta.iter().filter(|p| **p == 0).count();
                if zeros > block.pixels.iter().filter(|p| **p == 0).count() + block.pixels.len() / 8
                {
                    *block = Block::normalized(delta);
                    c.xor = true;
                }
            }
        }
        masks::compact(&mut self.mask_changes, Some(previous), current);
        Ok(())
    }
    fn apply(&self, previous: Option<&State>) -> Result<State, ActionError> {
        let metadata = self.metadata(previous)?;
        metadata.validate()?;
        let masks_used = !metadata.composition.raster_masks.is_default()
            || previous.is_some_and(|state| !state.metadata.composition.raster_masks.is_default());
        if self.mask_version != masks_used.then_some(1)
            || !masks_used && !self.mask_changes.is_empty()
        {
            return Err(bad("Missing, unsupported or redundant A8 replay extension"));
        }
        if previous.is_some_and(|state| {
            state.metadata.composition.raster_masks.next_id
                > metadata.composition.raster_masks.next_id
        }) {
            return Err(bad("Mask allocator moved backwards"));
        }
        if self.before != previous.map(State::digest).unwrap_or([0; 32]) {
            return Err(bad("Replay state before-hash mismatch"));
        }
        let m = &metadata;
        let count = m.count();
        let n = m.composition.layers.len();
        if self.mask_changes.len() > m.composition.raster_masks.items.len() * count {
            return Err(bad("Too many A8 tile changes for this frame"));
        }
        let empty = Cell {
            block: Arc::new(Block { pixels: vec![] }),
            digest: [0; 32],
        };
        let mut state = if self.replace {
            State {
                metadata: m.clone(),
                layers: vec![vec![empty.clone(); count]; n],
                surface: vec![empty; count],
                masks: BTreeMap::new(),
            }
        } else {
            let old = previous.ok_or_else(|| bad("Missing replay genesis"))?;
            if (old.metadata.width, old.metadata.height, old.layers.len()) != (m.width, m.height, n)
            {
                return Err(bad(
                    "Replay dimensions changed without a replacement snapshot",
                ));
            }
            let mut next = old.clone();
            next.metadata = m.clone();
            next
        };
        state.masks = masks::seed(previous, m);
        let mut seen = BTreeSet::new();
        let mut resolved = Vec::with_capacity(self.changes.len());
        for change in &self.changes {
            if change.layer > n || change.tile >= count || !seen.insert((change.layer, change.tile))
            {
                return Err(bad("Invalid, missing or duplicate replay tile"));
            }
            let cell = change.resolve(if self.replace { None } else { previous }, &resolved, m)?;
            resolved.push(cell.clone());
            if change.layer == n {
                state.surface[change.tile] = cell;
            } else {
                state.layers[change.layer][change.tile] = cell;
            }
        }
        let mut mask_seen = BTreeSet::new();
        let mut mask_resolved = Vec::with_capacity(self.mask_changes.len());
        let mask_previous = masks::same_dimensions(previous, m).map(|old| &old.masks);
        for change in &self.mask_changes {
            if change.tile >= count
                || !state.masks.contains_key(&change.mask_id)
                || !mask_seen.insert((change.mask_id, change.tile))
            {
                return Err(bad("Invalid or duplicate A8 replay tile"));
            }
            let cell = change.resolve(mask_previous, &mask_resolved, m)?;
            mask_resolved.push(cell.clone());
            state.masks.get_mut(&change.mask_id).unwrap()[change.tile] = cell;
        }
        if state
            .layers
            .iter()
            .chain(std::iter::once(&state.surface))
            .any(|grid| grid.iter().any(|c| c.block.pixels.is_empty()))
            || !masks::complete(&state.masks, m)
            || state.digest() != self.after
        {
            return Err(bad("Replay state after-hash mismatch"));
        }
        Ok(state)
    }
}
pub struct Prepared {
    event: Event,
    bytes: Vec<u8>,
    hash: Hash,
    state: State,
    visual: bool,
}
pub struct Entry {
    pub sequence: usize,
    pub elapsed_ms: u64,
    pub label: String,
    pub success: bool,
    pub visual: bool,
    pub hash: Hash,
    offset: u64,
    length: usize,
}
pub struct Journal {
    #[cfg(test)]
    fail_after: Option<usize>,
    file: File,
    path: PathBuf,
    pub entries: Vec<Entry>,
    pub state: State,
    head: Hash,
    length: u64,
    clock: Instant,
    base_time: u64,
    cached: Option<(usize, State)>,
}
pub(crate) struct JournalCheckpoint {
    state: State,
    length: u64,
    head: Hash,
    entries: usize,
}
impl JournalCheckpoint {
    pub fn head_hash(&self) -> String {
        hex(&self.head)
    }
    pub fn event_count(&self) -> usize {
        self.entries.saturating_sub(1)
    }
}
impl Drop for Journal {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
impl Journal {
    pub(crate) fn checkpoint(&self) -> JournalCheckpoint {
        JournalCheckpoint {
            state: self.state.clone(),
            length: self.length,
            head: self.head,
            entries: self.entries.len(),
        }
    }
    pub(crate) fn rollback(&mut self, mark: JournalCheckpoint) -> Result<(), ActionError> {
        self.length = mark.length;
        self.head = mark.head;
        self.state = mark.state;
        self.entries.truncate(mark.entries);
        self.cached = None;
        self.file.set_len(self.length)?;
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn fail_next_write_after(&mut self, bytes: usize) {
        self.fail_after = Some(bytes);
    }
    #[cfg(test)]
    pub(crate) fn transfer_pending_write_fault(&mut self, replacement: &mut Self) {
        replacement.fail_after = self.fail_after.take();
    }
    fn storage() -> Result<(File, PathBuf), ActionError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join("MiCoPi-replay");
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!(
            "{}-{}-{}.journal",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok((file, path))
    }
    pub fn new(
        doc: &mut Document,
        metadata: Metadata,
        environment: Value,
    ) -> Result<Self, ActionError> {
        metadata.validate()?;
        let (file, path) = Self::storage()?;
        let mut journal = Self {
            #[cfg(test)]
            fail_after: None,
            file,
            path,
            entries: vec![],
            state: State {
                metadata: metadata.clone(),
                layers: vec![],
                surface: vec![],
                masks: BTreeMap::new(),
            },
            head: [0; 32],
            length: 8,
            clock: Instant::now(),
            base_time: 0,
            cached: None,
        };
        journal.file.write_all(MAGIC)?;
        let prepared = journal.prepare(
            doc,
            metadata,
            Source::Genesis {
                version: env!("CARGO_PKG_VERSION").into(),
                environment,
            },
            true,
            None,
        )?;
        journal.commit(prepared)?;
        Ok(journal)
    }
    pub fn prepare(
        &self,
        doc: &mut Document,
        mut metadata: Metadata,
        source: Source,
        success: bool,
        error: Option<String>,
    ) -> Result<Prepared, ActionError> {
        metadata.validate()?;
        doc.validate_mask_storage().map_err(bad)?;
        if (metadata.width, metadata.height) != (doc.width, doc.height)
            || metadata.composition.raster_masks != *doc.mask_state()
        {
            return Err(bad(
                "Recording metadata differs from the raw A8 document frame",
            ));
        }
        if !self.entries.is_empty()
            && self.state.metadata.composition.raster_masks.next_id
                > metadata.composition.raster_masks.next_id
        {
            return Err(bad("Mask allocator moved backwards"));
        }
        doc.compose();
        if let Some(error) = &doc.render_error {
            return Err(ActionError::new(
                "render_error",
                format!("Document composition failed: {error}"),
            ));
        }
        metadata.render_error = doc.render_error.clone();
        let replace = self.entries.is_empty()
            || (doc.width, doc.height, doc.layers.len())
                != (
                    self.state.metadata.width,
                    self.state.metadata.height,
                    self.state.layers.len(),
                );
        let before = if self.entries.is_empty() {
            [0; 32]
        } else {
            self.state.digest()
        };
        let count = metadata.count();
        let n = doc.layers.len();
        let mut changes = vec![];
        let mut touched = std::mem::take(&mut doc.replay_tiles);
        if replace {
            touched = (0..n)
                .flat_map(|l| (0..count).map(move |i| (l, i)))
                .collect();
        }
        for (layer, id) in touched {
            if layer >= n || id >= count {
                continue;
            }
            let block = Block::read(&doc.layers[layer].pixels, &metadata, id);
            if replace || block.digest() != self.state.layers[layer][id].digest {
                changes.push(Change {
                    layer,
                    tile: id,
                    block: Some(block),
                    reuse: None,
                    xor: false,
                });
            }
        }
        let surface = doc.replay_surface.take();
        let mut ids = BTreeSet::new();
        if replace {
            ids.extend(0..count);
        } else if let Some([x0, y0, x1, y1]) = surface {
            let cols = metadata.width.div_ceil(TILE);
            if x1 > x0 && y1 > y0 {
                for y in y0 / TILE..=(y1 - 1) / TILE {
                    for x in x0 / TILE..=(x1 - 1) / TILE {
                        ids.insert(y * cols + x);
                    }
                }
            }
        }
        let mut visual = replace;
        for id in ids {
            let block = Block::read(&doc.composite, &metadata, id);
            if replace || block.digest() != self.state.surface[id].digest {
                visual = true;
                changes.push(Change {
                    layer: n,
                    tile: id,
                    block: Some(block),
                    reuse: None,
                    xor: false,
                });
            }
        }
        let mut mask_changes = vec![];
        let mask_previous = masks::same_dimensions(Some(&self.state), &metadata);
        let mut mask_touched = std::mem::take(&mut doc.replay_mask_tiles);
        for mask in &metadata.composition.raster_masks.items {
            if self.entries.is_empty()
                || mask_previous.is_none_or(|state| !state.masks.contains_key(&mask.id))
            {
                mask_touched.extend((0..count).map(|tile| (mask.id, tile)));
            }
        }
        for (mask_id, tile) in mask_touched {
            if tile >= count
                || !metadata
                    .composition
                    .raster_masks
                    .items
                    .iter()
                    .any(|mask| mask.id == mask_id)
            {
                continue;
            }
            let block = masks::Block::read(doc.mask_plane(mask_id).map_err(bad)?, &metadata, tile);
            if mask_previous
                .and_then(|state| state.masks.get(&mask_id)?.get(tile))
                .is_none_or(|old| hash(&block.bytes) != old.digest)
            {
                mask_changes.push(masks::Change {
                    mask_id,
                    tile,
                    block: Some(block),
                    reuse: None,
                    xor: false,
                });
            }
        }
        let mask_version = (!metadata.composition.raster_masks.is_default()
            || !self.state.metadata.composition.raster_masks.is_default())
        .then_some(1);
        let mut event = Event {
            sequence: self.entries.len(),
            elapsed_ms: self
                .base_time
                .saturating_add(self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64),
            source,
            success,
            error,
            before,
            after: [0; 32],
            metadata: Some(metadata.clone()),
            metadata_delta: None,
            replace,
            changes,
            mask_version,
            mask_changes,
        };
        let empty = Cell {
            block: Arc::new(Block { pixels: vec![0] }),
            digest: hash(&0u32.to_le_bytes()),
        };
        let mut state = if replace {
            State {
                metadata,
                layers: vec![vec![empty.clone(); count]; n],
                surface: vec![empty; count],
                masks: BTreeMap::new(),
            }
        } else {
            let mut s = self.state.clone();
            s.metadata = metadata;
            s
        };
        state.masks = masks::seed(
            if self.entries.is_empty() {
                None
            } else {
                Some(&self.state)
            },
            &state.metadata,
        );
        for change in &event.changes {
            let block = change.block.as_ref().unwrap();
            let cell = Cell {
                digest: block.digest(),
                block: Arc::new(block.clone()),
            };
            if change.layer == n {
                state.surface[change.tile] = cell;
            } else {
                state.layers[change.layer][change.tile] = cell;
            }
        }
        for change in &event.mask_changes {
            let cell = change.block.as_ref().unwrap().clone().cell();
            state.masks.get_mut(&change.mask_id).unwrap()[change.tile] = cell;
        }
        if !masks::complete(&state.masks, &state.metadata) {
            return Err(bad("Incomplete A8 recording snapshot"));
        }
        event.after = state.digest();
        event.compact(&self.state, &mut state)?;
        let mut encoder = Limited {
            inner: DeflateEncoder::new(Vec::new(), Compression::fast()),
            written: 0,
        };
        serde_json::to_writer(&mut encoder, &event)?;
        let bytes = encoder.inner.finish()?;
        if bytes.len() > MAX_ARCHIVE
            || self.length as usize + bytes.len() + 80 > MAX_ARCHIVE
            || self.entries.len() >= MAX_EVENTS
        {
            return Err(ActionError::new("replay_limit","Replay journal limit reached; recover current pixels with Export PNG or replay.recover_png, then start a new session"));
        }
        let hash = chain(&self.head, &bytes);
        Ok(Prepared {
            event,
            bytes,
            hash,
            state,
            visual,
        })
    }
    pub fn changed(prepared: &Prepared) -> bool {
        prepared.event.before != prepared.event.after
    }
    pub fn commit(&mut self, p: Prepared) -> Result<(), ActionError> {
        let written = (|| -> std::io::Result<()> {
            self.file.seek(SeekFrom::Start(self.length))?;
            self.file.write_all(&(p.bytes.len() as u32).to_le_bytes())?;
            self.file.write_all(&p.hash)?;
            #[cfg(test)]
            if let Some(n) = self.fail_after.take() {
                self.file.write_all(&p.bytes[..n.min(p.bytes.len())])?;
                return Err(std::io::Error::other("Injected partial journal write"));
            }
            self.file.write_all(&p.bytes)?;
            Ok(())
        })();
        if let Err(error) = written {
            if let Err(truncate) = self.file.set_len(self.length) {
                return Err(ActionError::new(
                    "replay_storage",
                    format!("{error}; truncating the uncommitted tail failed: {truncate}"),
                ));
            }
            return Err(error.into());
        }
        self.entries.push(Entry {
            sequence: p.event.sequence,
            elapsed_ms: p.event.elapsed_ms,
            label: p.event.source.label(),
            success: p.event.success,
            visual: p.visual,
            hash: p.hash,
            offset: self.length,
            length: p.bytes.len(),
        });
        self.length += 36 + p.bytes.len() as u64;
        self.head = p.hash;
        self.state = p.state;
        Ok(())
    }
    fn event(&mut self, index: usize) -> Result<Event, ActionError> {
        let e = self
            .entries
            .get(index)
            .ok_or_else(|| bad("Replay event index is out of range"))?;
        self.file.seek(SeekFrom::Start(e.offset + 36))?;
        let mut bytes = vec![0; e.length];
        self.file.read_exact(&mut bytes)?;
        let previous = if index == 0 {
            [0; 32]
        } else {
            self.entries[index - 1].hash
        };
        if chain(&previous, &bytes) != e.hash {
            return Err(bad("Replay entry hash mismatch"));
        }
        Self::decode(&bytes)
    }
    fn decode(bytes: &[u8]) -> Result<Event, ActionError> {
        let decoder = DeflateDecoder::new(bytes);
        let mut json = vec![];
        decoder
            .take(MAX_DECODED as u64 + 1)
            .read_to_end(&mut json)?;
        if json.len() > MAX_DECODED {
            return Err(bad("Replay entry exceeds the decoding limit"));
        }
        Ok(serde_json::from_slice(&json).map_err(|e| bad(e.to_string()))?)
    }
    pub fn preview(&mut self, index: usize) -> Result<State, ActionError> {
        if index >= self.entries.len() {
            return Err(bad("Replay event index is out of range"));
        }
        if index + 1 == self.entries.len() {
            return Ok(self.state.clone());
        }
        let (mut cursor, mut state) = match self.cached.take() {
            Some((i, s)) if i <= index => (i, s),
            _ => {
                let e = self.event(0)?;
                (0, e.apply(None)?)
            }
        };
        while cursor < index {
            cursor += 1;
            let e = self.event(cursor)?;
            state = e.apply(Some(&state))?;
        }
        self.cached = Some((cursor, state.clone()));
        Ok(state)
    }
    pub fn events(&mut self, start: usize, limit: usize) -> Result<Value, ActionError> {
        let mut out = vec![];
        for i in start..self.entries.len().min(start.saturating_add(limit.min(256))) {
            let e = self.event(i)?;
            let summary = &self.entries[i];
            let mut row = json!({"sequence":i,"elapsed_ms":e.elapsed_ms,"source":e.source,"success":e.success,"error":e.error,"before":hex(&e.before),"after":hex(&e.after),"hash":hex(&summary.hash),"visual":summary.visual,"changed_tiles":e.changes.len()});
            if let Some(version) = e.mask_version {
                row["mask_version"] = json!(version);
                row["changed_mask_tiles"] = json!(e.mask_changes.len());
            }
            out.push(row);
        }
        Ok(json!({"events":out,"total":self.entries.len()}))
    }
    pub fn status(&self) -> Value {
        json!({"format":"MCRPLY02","events":self.entries.len().saturating_sub(1),"frames":self.entries.iter().filter(|e|e.visual).count(),"bytes":self.length,"head_hash":hex(&self.head),"state_hash":hex(&self.state.digest()),"surface_hash":hex(&self.state.surface_hash()),"width":self.state.metadata.width,"height":self.state.metadata.height,"elapsed_ms":self.entries.last().map_or(0,|e|e.elapsed_ms),"recording":true})
    }
    pub fn bytes(&mut self, pending: Option<&Prepared>) -> Result<Vec<u8>, ActionError> {
        self.file.flush()?;
        self.file.seek(SeekFrom::Start(0))?;
        let mut out = Vec::with_capacity(self.length as usize + 44);
        out.resize(self.length as usize, 0);
        self.file.read_exact(&mut out)?;
        if let Some(p) = pending {
            out.extend_from_slice(&(p.bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&p.hash);
            out.extend_from_slice(&p.bytes);
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(
            &((self.entries.len() + usize::from(pending.is_some())) as u64).to_le_bytes(),
        );
        out.extend_from_slice(&pending.map_or(self.head, |p| p.hash));
        Ok(out)
    }
    pub fn import(bytes: &[u8]) -> Result<Self, ActionError> {
        if bytes.len() > MAX_ARCHIVE
            || (!bytes.starts_with(MAGIC) && !bytes.starts_with(b"MCRPLY01"))
        {
            return Err(bad("Invalid or oversized replay archive"));
        }
        let mut reader = std::io::Cursor::new(bytes);
        let legacy = bytes.starts_with(b"MCRPLY01");
        reader.set_position(8);
        let mut head = [0; 32];
        let mut state: Option<State> = None;
        let mut entries = vec![];
        let mut time = 0;
        let end = loop {
            let offset = reader.position();
            let mut len = [0; 4];
            reader
                .read_exact(&mut len)
                .map_err(|_| bad("Truncated replay record or missing footer"))?;
            let length = u32::from_le_bytes(len) as usize;
            if length == 0 {
                let mut count = [0; 8];
                let mut seal = [0; 32];
                reader.read_exact(&mut count)?;
                reader.read_exact(&mut seal)?;
                if u64::from_le_bytes(count) != entries.len() as u64
                    || head != seal
                    || reader.position() != bytes.len() as u64
                    || entries.is_empty()
                {
                    return Err(bad("Replay footer, count or final hash mismatch"));
                }
                break offset;
            }
            if length > MAX_ARCHIVE
                || length as u64 + 32 > bytes.len() as u64 - reader.position()
                || entries.len() >= MAX_EVENTS
            {
                return Err(bad("Invalid replay record length"));
            }
            let mut stored = [0; 32];
            reader.read_exact(&mut stored)?;
            let mut payload = vec![0; length];
            reader.read_exact(&mut payload)?;
            if chain(&head, &payload) != stored {
                return Err(bad("Replay hash chain mismatch"));
            }
            let event = Self::decode(&payload)?;
            if legacy
                && (event.metadata.is_none()
                    || event.metadata_delta.is_some()
                    || event.mask_version.is_some()
                    || !event.mask_changes.is_empty()
                    || event
                        .changes
                        .iter()
                        .any(|c| c.xor || c.reuse.is_some() || c.block.is_none()))
            {
                return Err(bad("Version 1 archive contains version 2 encoding"));
            }
            if event.sequence != entries.len()
                || event.elapsed_ms < time
                || (entries.is_empty() && !matches!(event.source, Source::Genesis { .. }))
            {
                return Err(bad("Replay order, timestamp or genesis mismatch"));
            }
            let visual = state.as_ref().is_none_or(|s| {
                event.replace || event.changes.iter().any(|c| c.layer == s.layers.len())
            });
            state = Some(event.apply(state.as_ref())?);
            time = event.elapsed_ms;
            entries.push(Entry {
                sequence: event.sequence,
                elapsed_ms: time,
                label: event.source.label(),
                success: event.success,
                visual,
                hash: stored,
                offset,
                length,
            });
            head = stored;
        };
        let (file, path) = Self::storage()?;
        let mut journal = Self {
            #[cfg(test)]
            fail_after: None,
            file,
            path,
            entries,
            state: state.ok_or_else(|| bad("Missing replay genesis"))?,
            head,
            length: end,
            clock: Instant::now(),
            base_time: time,
            cached: None,
        };
        journal.file.write_all(MAGIC)?;
        journal.file.write_all(&bytes[8..end as usize])?;
        Ok(journal)
    }
    pub fn verify(&mut self, expected: Option<&str>) -> Result<Value, ActionError> {
        let bytes = self.bytes(None)?;
        let verified = Self::import(&bytes)?;
        if expected.is_some_and(|s| s != hex(&verified.head)) {
            return Err(bad("Replay does not match the expected external head hash"));
        }
        Ok(
            json!({"valid":true,"checked_events":verified.entries.len(),"head_hash":hex(&verified.head),"state_hash":hex(&verified.state.digest()),"surface_hash":hex(&verified.state.surface_hash()),"checks":["sequence","timestamps","SHA-256 chain","tile bounds","before state","after state","sealed footer"]}),
        )
    }
    pub fn export(&mut self, path: &Path) -> Result<(), ActionError> {
        let bytes = self.bytes(None)?;
        let tmp = path.with_extension("replay.tmp");
        let mut file = File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        crate::platform::replace_file(&tmp, path)?;
        Ok(())
    }
    pub fn from_path(path: &Path) -> Result<Self, ActionError> {
        if std::fs::metadata(path)?.len() > MAX_ARCHIVE as u64 {
            return Err(bad("Replay archive exceeds the size limit"));
        }
        Self::import(&std::fs::read(path)?)
    }
}
mod pixel_wire {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(pixels: &[u32], serializer: S) -> Result<S::Ok, S::Error> {
        let bytes: Vec<_> = pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
        serializer.serialize_str(&crate::application::codec::base64(&bytes))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u32>, D::Error> {
        let text = String::deserialize(d)?;
        if text.is_empty() || text.len() > 21848 || text.len() % 4 != 0 {
            return Err(serde::de::Error::custom("Invalid replay pixel block"));
        }
        let mut bytes = vec![];
        let mut padded = false;
        for chunk in text.as_bytes().chunks_exact(4) {
            let mut n = 0u32;
            let mut pad = 0;
            for c in chunk {
                let v = match c {
                    b'A'..=b'Z' => c - b'A',
                    b'a'..=b'z' => c - b'a' + 26,
                    b'0'..=b'9' => c - b'0' + 52,
                    b'+' => 62,
                    b'/' => 63,
                    b'=' => {
                        pad += 1;
                        0
                    }
                    _ => return Err(serde::de::Error::custom("Invalid base64 pixel data")),
                };
                if padded || pad > 0 && *c != b'=' {
                    return Err(serde::de::Error::custom("Invalid base64 padding"));
                }
                n = n << 6 | v as u32;
            }
            if pad > 2 {
                return Err(serde::de::Error::custom("Invalid base64 padding"));
            }
            bytes.push((n >> 16) as u8);
            if pad < 2 {
                bytes.push((n >> 8) as u8);
            }
            if pad < 1 {
                bytes.push(n as u8);
            }
            padded = pad > 0;
        }
        if bytes.len() % 4 != 0 {
            return Err(serde::de::Error::custom("Invalid pixel byte count"));
        }
        Ok(bytes
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect())
    }
}
