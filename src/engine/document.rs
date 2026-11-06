//! Pixels are straight-alpha 0xAARRGGBB. History stores only touched 64px tiles.
use super::graph::{BlendMode, Graph, Node, NodeKind, Renderer};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    path::Path,
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Composition {
    #[serde(
        default,
        skip_serializing_if = "super::document_info::Info::is_default"
    )]
    pub document_info: super::document_info::Info,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_transform: Option<super::transform::Recipe>,
    pub graph: Graph,
    pub layers: Vec<(bool, u8, BlendMode)>,
    #[serde(default)]
    pub workflow: super::workflow::Graph,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layer_metadata: Vec<LayerMetadata>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next_layer_id: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_layer_ids: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groups: Option<super::groups::GroupTree>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next_group_id: u64,
    #[serde(
        default,
        skip_serializing_if = "super::selection::SelectionState::is_default"
    )]
    pub selection: super::selection::SelectionState,
    #[serde(
        default,
        skip_serializing_if = "super::raster_masks::MaskState::is_default"
    )]
    pub raster_masks: super::raster_masks::MaskState,
}
fn is_false(value: &bool) -> bool {
    !*value
}
fn is_zero(value: &u64) -> bool {
    *value == 0
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerColor {
    #[default]
    None,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Violet,
    Gray,
}
impl LayerColor {
    pub const ALL: [Self; 8] = [
        Self::None,
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Violet,
        Self::Gray,
    ];
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::None => "No Color Label",
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Violet => "Violet",
            Self::Gray => "Gray",
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerMetadata {
    pub id: u64,
    pub name: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub alpha_locked: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub pixel_locked: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub position_locked: bool,
    #[serde(default, skip_serializing_if = "LayerColor::is_none")]
    pub color_label: LayerColor,
}
impl Composition {
    pub fn validate_layer_metadata(&self) -> Result<(), String> {
        self.document_info.validate()?;
        if let Some(recipe) = self.last_transform {
            recipe.validate()?;
        }
        if self.graph.linked {
            let branches = self.graph.stack_branches(self.layers.len())?;
            for (branch, (_, _, mode)) in branches.iter().zip(&self.layers) {
                if let Some(Node {
                    data:
                        NodeKind::Blend {
                            mode: graph_mode, ..
                        },
                    ..
                }) = &branch.blend
                {
                    if graph_mode != mode {
                        return Err("Linked blend mode differs from its layer".into());
                    }
                }
            }
        }
        if self.next_group_id > u32::MAX as u64 + 1 {
            return Err("Invalid group allocator".into());
        }
        if self.layer_metadata.is_empty() {
            if self.groups.is_some() || !self.raster_masks.is_default() {
                return Err("Group trees require stable raster identities".into());
            }
            return if self.next_layer_id == 0 {
                Ok(())
            } else {
                Err("Missing layer identities".into())
            };
        }
        let mut ids = std::collections::BTreeSet::new();
        if self.layer_metadata.len() != self.layers.len()
            || self.next_layer_id == 0
            || self.next_layer_id > u32::MAX as u64 + 1
            || self.layer_metadata.iter().any(|m| {
                m.id == 0
                    || m.id >= self.next_layer_id
                    || !ids.insert(m.id)
                    || m.name.len() > 512
                    || m.name.chars().any(char::is_control)
            })
        {
            return Err("Invalid layer identities or names".into());
        }
        if let Some(tree) = &self.groups {
            if !self.graph.linked {
                return Err("Groups require linked layer-filter composition".into());
            }
            let ids: Vec<_> = self.layer_metadata.iter().map(|m| m.id).collect();
            tree.validate(&ids, self.next_group_id)?;
            tree.compile_graph(
                &self.graph,
                &ids,
                &self.layers.iter().map(|v| v.2).collect::<Vec<_>>(),
            )?;
        }
        self.raster_masks.validate_owners(
            &self
                .layer_metadata
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            &self
                .groups
                .as_ref()
                .map(|tree| tree.groups.iter().map(|group| group.id).collect::<Vec<_>>())
                .unwrap_or_default(),
        )?;
        Ok(())
    }
}
pub const TILE: usize = 64;
const HISTORY_LIMIT: usize = 64 * 1024 * 1024;
pub const MAX_SIDE: usize = 4096;
pub const MAX_LAYERS: usize = 16;
#[derive(Clone)]
pub struct Layer {
    pub pixels: Vec<u32>,
    pub visible: bool,
    pub opacity: u8,
    pub mode: BlendMode,
}
#[derive(Clone)]
struct Edit {
    layer: usize,
    tiles: BTreeMap<usize, Arc<Vec<u32>>>,
    mask_tiles: BTreeMap<(u64, usize), Arc<Vec<u8>>>,
    mask_change: Option<MaskChange>,
    composition: Option<Composition>,
    metadata_bytes: usize,
    layer_change: Option<LayerChange>,
    group: Option<HistoryGroup>,
}
#[derive(Clone)]
enum MaskChange {
    Toggle {
        slot: usize,
        removed: Option<Arc<super::raster_masks::MaskPlane>>,
    },
    Replace {
        slot: usize,
        saved: Arc<super::raster_masks::MaskPlane>,
    },
}
#[derive(Clone)]
struct HistoryGroup {
    edits: Vec<Edit>,
    active: usize,
    selected: Vec<u64>,
}
#[derive(Clone)]
enum LayerChange {
    Permutation {
        order: Vec<usize>,
        active: usize,
    },
    Canvas(Arc<CanvasState>),
    Replace {
        current: Vec<usize>,
        saved: Vec<(usize, Arc<Layer>)>,
        active: usize,
    },
    Toggle {
        index: usize,
        active: usize,
        removed: Option<Arc<Layer>>,
    },
    Move {
        from: usize,
        to: usize,
        active: usize,
    },
}
#[derive(Clone)]
struct CanvasState {
    width: usize,
    height: usize,
    layers: Vec<Layer>,
    mask_planes: Vec<super::raster_masks::MaskPlane>,
}
#[derive(Clone)]
pub(crate) struct HistoryCheckpoint {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    identity: u64,
    renderer: Option<Arc<dyn Renderer>>,
    stroke: Option<Edit>,
    last: Option<(f32, f32)>,
    last_brush: Option<Brush>,
    distance: f32,
    next_layer_id: u64,
    history_group: Option<HistoryGroup>,
    next_group_id: u64,
    dirty: bool,
    mask_storage_epoch: u64,
    mask_revisions: Vec<(u64, u64)>,
    mask_contact: Option<super::raster_masks::MaskLease>,
    next_mask_id: u64,
}
impl Edit {
    fn bytes(&self) -> usize {
        self.tiles.values().map(|v| v.len() * 4).sum::<usize>()
            + self.mask_tiles.values().map(|v| v.len()).sum::<usize>()
            + self.mask_change.as_ref().map_or(0, |change| match change {
                MaskChange::Toggle { removed, .. } => removed
                    .as_ref()
                    .map_or(0, |plane| plane.samples.memory_bytes()),
                MaskChange::Replace { saved, .. } => saved.samples.memory_bytes(),
            })
            + self.metadata_bytes
            + self.layer_change.as_ref().map_or(0, |c| match c {
                LayerChange::Toggle { removed, .. } => {
                    removed.as_ref().map_or(0, |l| l.pixels.len() * 4)
                }
                LayerChange::Canvas(state) => {
                    state
                        .layers
                        .iter()
                        .map(|l| l.pixels.len() * 4)
                        .sum::<usize>()
                        + state
                            .mask_planes
                            .iter()
                            .map(|plane| plane.samples.memory_bytes())
                            .sum::<usize>()
                }
                LayerChange::Replace { saved, .. } => {
                    saved.iter().map(|(_, l)| l.pixels.len() * 4).sum()
                }
                LayerChange::Permutation { order, .. } => {
                    order.len() * std::mem::size_of::<usize>()
                }
                _ => 0,
            })
            + self.group.as_ref().map_or(0, |g| {
                g.edits.iter().map(Edit::bytes).sum::<usize>() + g.selected.len() * 8 + 64
            })
    }
}
#[derive(Clone, Copy)]
pub struct Brush {
    pub radius: f32,
    pub opacity: f32,
    pub hardness: f32,
    pub color: u32,
    pub erase: bool,
}
pub struct Document {
    document_info: super::document_info::Info,
    last_transform: Option<super::transform::Recipe>,
    selection: super::selection::SelectionState,
    layer_metadata: Vec<LayerMetadata>,
    next_layer_id: u64,
    selected_layer_ids: Vec<u64>,
    groups: Option<super::groups::GroupTree>,
    next_group_id: u64,
    raster_masks: super::raster_masks::MaskState,
    mask_planes: Vec<super::raster_masks::MaskPlane>,
    mask_storage_epoch: u64,
    mask_contact: Option<super::raster_masks::MaskLease>,
    mask_effects: Vec<mask_effects::CoverageCache>,
    pub(crate) replay_mask_tiles: std::collections::BTreeSet<(u64, usize)>,
    effective_graph: Option<Graph>,
    group_visibility: [bool; MAX_LAYERS],
    masked_layers: [bool; MAX_LAYERS],
    group_nodes: BTreeMap<u64, u32>,
    groups_revision: u64,
    pub(crate) replay_tiles: std::collections::BTreeSet<(usize, usize)>,
    pub(crate) replay_surface: Option<[usize; 4]>,
    pub(crate) replay_data: Option<Vec<u8>>,
    pub workflow: super::workflow::Graph,
    pub identity: u64,
    pub graph: Graph,
    pub(crate) renderer: Option<Arc<dyn Renderer>>,
    plan: Vec<micopi_plugin_api::CompositeNode>,
    pub render_error: Option<String>,
    pub width: usize,
    pub height: usize,
    pub layers: Vec<Layer>,
    pub active: usize,
    pub composite: Vec<u32>,
    pub dirty: bool,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    history_group: Option<HistoryGroup>,
    stroke: Option<Edit>,
    last: Option<(f32, f32)>,
    last_brush: Option<Brush>,
    distance: f32,
    changed: Option<[usize; 4]>,
    display_changed: Option<[usize; 4]>,
}
pub fn over(dst: u32, src: u32) -> u32 {
    let a = src >> 24;
    if a == 0 {
        return dst;
    }
    if a == 255 {
        return src;
    }
    let da = dst >> 24;
    let out_a = a + (da * (255 - a) + 127) / 255;
    let mut out = out_a << 24;
    for shift in [0, 8, 16] {
        let s = (src >> shift) & 255;
        let d = (dst >> shift) & 255;
        let c = (s * a * 255 + d * da * (255 - a) + out_a * 127) / (out_a * 255);
        out |= c.min(255) << shift;
    }
    out
}
impl Document {
    pub(crate) fn checkpoint_history(&self) -> HistoryCheckpoint {
        HistoryCheckpoint {
            undo: self.undo.clone(),
            redo: self.redo.clone(),
            identity: self.identity,
            renderer: self.renderer.clone(),
            stroke: self.stroke.clone(),
            last: self.last,
            last_brush: self.last_brush,
            distance: self.distance,
            next_layer_id: self.next_layer_id,
            history_group: self.history_group.clone(),
            next_group_id: self.next_group_id,
            dirty: self.dirty,
            mask_storage_epoch: self.mask_storage_epoch,
            mask_revisions: self
                .mask_planes
                .iter()
                .map(|plane| (plane.id, plane.content_revision))
                .collect(),
            mask_contact: self.mask_contact,
            next_mask_id: self.raster_masks.next_id,
        }
    }
    pub(crate) fn restore_history(&mut self, saved: HistoryCheckpoint) {
        self.undo = saved.undo;
        self.redo = saved.redo;
        self.identity = saved.identity;
        self.renderer = saved.renderer;
        self.stroke = saved.stroke;
        self.last = saved.last;
        self.last_brush = saved.last_brush;
        self.distance = saved.distance;
        self.next_layer_id = saved.next_layer_id;
        self.history_group = saved.history_group;
        self.next_group_id = saved.next_group_id;
        self.dirty = saved.dirty;
        self.mask_storage_epoch = saved.mask_storage_epoch;
        self.mask_contact = saved.mask_contact;
        self.raster_masks.next_id = saved.next_mask_id;
        for (id, revision) in saved.mask_revisions {
            if let Some(plane) = self.mask_planes.iter_mut().find(|plane| plane.id == id) {
                plane.content_revision = revision;
            }
        }
    }
    pub fn new(width: usize, height: usize) -> Self {
        assert!(width > 0 && height > 0 && width <= MAX_SIDE && height <= MAX_SIDE);
        let n = width * height;
        let layers = vec![Layer {
            pixels: vec![0; n],
            visible: true,
            opacity: 255,
            mode: BlendMode::Normal,
        }];
        let graph = Graph::linked_from_layers(&layers);
        let plan = graph.compile(1).unwrap();
        Self {
            document_info: Default::default(),
            last_transform: None,
            layer_metadata: vec![LayerMetadata {
                id: 1,
                name: String::new(),
                alpha_locked: false,
                ..Default::default()
            }],
            next_layer_id: 2,
            selected_layer_ids: Vec::new(),
            groups: None,
            next_group_id: 1,
            raster_masks: Default::default(),
            mask_planes: Vec::new(),
            mask_storage_epoch: 0,
            mask_contact: None,
            mask_effects: Vec::new(),
            replay_mask_tiles: Default::default(),
            effective_graph: None,
            group_visibility: [true; MAX_LAYERS],
            masked_layers: [false; MAX_LAYERS],
            group_nodes: BTreeMap::new(),
            groups_revision: 0,
            selection: Default::default(),
            workflow: Default::default(),
            identity: {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
            graph,
            replay_tiles: Default::default(),
            replay_surface: Some([0, 0, width, height]),
            replay_data: None,
            renderer: None,
            plan,
            render_error: None,
            width,
            height,
            layers,
            active: 0,
            composite: vec![0; n],
            dirty: false,
            undo: vec![],
            redo: vec![],
            history_group: None,
            stroke: None,
            last: None,
            last_brush: None,
            distance: 0.,
            changed: Some([0, 0, width, height]),
            display_changed: Some([0, 0, width, height]),
        }
    }
    pub fn invalidate(&mut self) {
        self.replay_surface = Some([0, 0, self.width, self.height]);
        self.changed = Some([0, 0, self.width, self.height]);
        self.display_changed = self.changed;
    }
    /// The journal restored verified composite pixels directly into this buffer.
    /// Refresh consumers without invoking a renderer again during rollback.
    pub(crate) fn finish_verified_surface_restore(&mut self) {
        self.changed = None;
        self.display_changed = Some([0, 0, self.width, self.height]);
        self.replay_surface = Some([0, 0, self.width, self.height]);
    }
    pub fn requires_renderer(&self) -> bool {
        self.has_raster_masks()
            || self.has_groups()
            || (self.graph.enabled && (!self.graph.linked || self.graph.has_filters()))
            || self.layers.iter().any(|l| l.mode != BlendMode::Normal)
    }
    pub fn layer_is_rendered(&self, index: usize) -> bool {
        self.graph.linked
            || !self.graph.enabled
            || self
                .plan
                .iter()
                .any(|n| n.kind == 0 && n.a as usize == index)
    }
    pub fn composition(&self) -> Composition {
        Composition {
            document_info: self.document_info,
            last_transform: self.last_transform,
            selection: self.selection.clone(),
            layer_metadata: self.layer_metadata.clone(),
            next_layer_id: self.next_layer_id,
            workflow: self.workflow.clone(),
            selected_layer_ids: self.selected_layer_ids.clone(),
            groups: self.groups.clone(),
            next_group_id: if self.next_group_id == 1 {
                0
            } else {
                self.next_group_id
            },
            graph: self.graph.clone(),
            raster_masks: self.raster_masks.clone(),
            layers: self
                .layers
                .iter()
                .map(|l| (l.visible, l.opacity, l.mode))
                .collect(),
        }
    }
    pub fn info(&self) -> super::document_info::Info {
        self.document_info
    }
    /// Initialize only a newly allocated document, without an intermediate undo.
    pub(crate) fn initialize_info(&mut self, info: super::document_info::Info) {
        self.document_info = info;
        self.layers[0].pixels.fill(info.initial_background);
    }
    /// Install validated image-import metadata without changing decoded pixels/history.
    pub(crate) fn set_import_info(
        &mut self,
        info: super::document_info::Info,
    ) -> Result<(), String> {
        info.validate()?;
        self.document_info = info;
        Ok(())
    }
    pub(crate) fn set_resolution(&mut self, resolution_ppi: u32) -> Result<(), String> {
        let mut info = self.document_info;
        info.resolution_ppi = resolution_ppi;
        info.resolution_explicit = true;
        info.png_density = None;
        info.validate()?;
        if info == self.document_info {
            return Ok(());
        }
        self.finish();
        let before = self.composition();
        self.document_info = info;
        self.commit_composition(before, None);
        Ok(())
    }
    pub fn last_transform(&self) -> Option<super::transform::Recipe> {
        self.last_transform
    }
    pub fn selection(&self) -> &super::selection::SelectionState {
        &self.selection
    }
    /// Internal document primitive; public clients must use Session actions.
    pub fn set_selection(
        &mut self,
        selection: super::selection::SelectionState,
    ) -> Result<(), String> {
        selection.validate(self.width, self.height)?;
        if self.selection == selection {
            return Ok(());
        }
        self.finish();
        let before = self.composition();
        self.selection = selection;
        self.commit_composition(before, None);
        Ok(())
    }
    pub(crate) fn restore_composition(&mut self, state: Composition) {
        let mask_metadata_changed = self.raster_masks.items != state.raster_masks.items;
        self.document_info = state.document_info;
        self.last_transform = state.last_transform;
        self.selection = state.selection;
        let next_mask_id = self.raster_masks.next_id.max(state.raster_masks.next_id);
        self.raster_masks = state.raster_masks;
        self.raster_masks.next_id = next_mask_id;
        if mask_metadata_changed {
            self.retain_mask_effect_caches();
            self.invalidate_mask_effects();
        }
        self.selected_layer_ids = state.selected_layer_ids;
        self.groups = state.groups;
        self.next_group_id = self.next_group_id.max(state.next_group_id).max(1);
        self.layer_metadata = if state.layer_metadata.is_empty() {
            (0..self.layers.len())
                .map(|i| LayerMetadata {
                    id: i as u64 + 1,
                    name: String::new(),
                    alpha_locked: false,
                    ..Default::default()
                })
                .collect()
        } else {
            state.layer_metadata
        };
        self.next_layer_id = self
            .next_layer_id
            .max(state.next_layer_id)
            .max(self.layer_metadata.iter().map(|m| m.id).max().unwrap_or(0) + 1);
        self.workflow = state.workflow;
        self.graph = state.graph;
        self.plan = self.graph.compile(self.layers.len()).unwrap_or_default();
        for (l, (visible, opacity, mode)) in self.layers.iter_mut().zip(state.layers) {
            l.visible = visible;
            l.opacity = opacity;
            l.mode = mode;
        }
        self.normalize_layer_selection();
        let _ = self.refresh_group_cache();
        self.render_error = None;
        self.invalidate();
    }
    fn composition_edit(
        before: Composition,
        layer_change: Option<LayerChange>,
        layer: usize,
    ) -> Edit {
        Edit {
            group: None,
            layer_change,
            layer,
            tiles: BTreeMap::new(),
            mask_tiles: BTreeMap::new(),
            mask_change: None,
            metadata_bytes: before.graph.nodes.len() * 160
                + before.groups.as_ref().map_or(0, |g| g.memory_bytes())
                + before.selected_layer_ids.len() * 8
                + before.selection.memory_bytes()
                + before.raster_masks.memory_bytes()
                + std::mem::size_of_val(&before.last_transform)
                + std::mem::size_of_val(&before.document_info)
                + before.graph.edges.len() * 16
                + before.workflow.bytes()
                + before.layers.len() * 8
                + before
                    .layer_metadata
                    .iter()
                    .map(|m| m.name.len() + std::mem::size_of::<LayerMetadata>())
                    .sum::<usize>()
                + 64,
            composition: Some(before),
        }
    }
    fn commit_composition(&mut self, before: Composition, layer_change: Option<LayerChange>) {
        self.stroke = Some(Self::composition_edit(before, layer_change, self.active));
        self.finish();
        self.render_error = None;
        self.invalidate();
    }
    pub fn set_graph(&mut self, graph: Graph) -> Result<(), String> {
        if graph
            .nodes
            .iter()
            .any(|n| matches!(n.data, NodeKind::Transparent))
        {
            self.ensure_group_renderer()?;
        }
        let mut modes: Vec<_> = self.layers.iter().map(|l| l.mode).collect();
        if graph.linked {
            for (mode, branch) in modes
                .iter_mut()
                .zip(graph.stack_branches(self.layers.len())?)
            {
                if let Some(Node {
                    data: NodeKind::Blend { mode: next, .. },
                    ..
                }) = branch.blend
                {
                    *mode = next;
                }
            }
        }
        let prepared = self.prepare_group_plan(
            &graph,
            self.groups.as_ref(),
            &self.layer_metadata,
            &modes,
            self.next_group_id,
            true,
        )?;
        if ((graph.enabled && (!graph.linked || graph.has_filters()))
            || (graph.linked
                && graph.nodes.iter().any(
                    |n| matches!(n.data, NodeKind::Blend { mode, .. } if mode != BlendMode::Normal),
                )))
            && self.renderer.is_none()
        {
            return Err("Node compositor DLL is not loaded".into());
        }
        if self.graph == graph {
            return Ok(());
        }
        self.finish();
        let before = self.composition();
        if graph.linked {
            let branches = graph.stack_branches(self.layers.len())?;
            for (layer, branch) in self.layers.iter_mut().zip(branches) {
                if let Some(node) = branch.blend {
                    if let NodeKind::Blend { mode, .. } = node.data {
                        layer.mode = mode;
                    }
                }
            }
        }
        self.graph = graph;
        self.install_group_plan(prepared);
        self.commit_composition(before, None);
        Ok(())
    }
    pub fn set_workflow(
        &mut self,
        graph: super::workflow::Graph,
    ) -> Result<(), crate::application::ActionError> {
        graph.validate_structure()?;
        if self.workflow == graph {
            return Ok(());
        }
        self.finish();
        let before = self.composition();
        self.workflow = graph;
        self.commit_composition(before, None);
        Ok(())
    }
    pub fn set_layer_properties(
        &mut self,
        index: usize,
        visible: bool,
        opacity: u8,
        mode: BlendMode,
    ) -> Result<(), String> {
        let locked = self
            .layer_metadata
            .get(index)
            .is_some_and(|m| m.alpha_locked);
        self.set_layer_properties_with_lock(index, visible, opacity, mode, locked)
    }
    pub fn set_layer_properties_with_lock(
        &mut self,
        index: usize,
        visible: bool,
        opacity: u8,
        mode: BlendMode,
        alpha_locked: bool,
    ) -> Result<(), String> {
        if index >= self.layers.len() {
            return Err("Layer does not exist".into());
        }
        self.set_layer_properties_organized(
            index,
            visible,
            opacity,
            mode,
            alpha_locked,
            self.pixel_locked(index),
            self.position_locked(index),
            self.color_label(index),
        )
    }
    pub fn set_layer_properties_organized(
        &mut self,
        index: usize,
        visible: bool,
        opacity: u8,
        mode: BlendMode,
        alpha_locked: bool,
        pixel_locked: bool,
        position_locked: bool,
        color_label: LayerColor,
    ) -> Result<(), String> {
        if index >= self.layers.len() {
            return Err("Layer does not exist".into());
        }
        if mode != BlendMode::Normal && self.renderer.is_none() {
            return Err("Layer compositor DLL is not loaded".into());
        }
        let l = &self.layers[index];
        if (
            l.visible,
            l.opacity,
            l.mode,
            self.alpha_locked(index),
            self.pixel_locked(index),
            self.position_locked(index),
            self.color_label(index),
        ) == (
            visible,
            opacity,
            mode,
            alpha_locked,
            pixel_locked,
            position_locked,
            color_label,
        ) {
            return Ok(());
        }
        let mut modes: Vec<_> = self.layers.iter().map(|l| l.mode).collect();
        modes[index] = mode;
        let graph = if self.graph.linked {
            let ids: Vec<_> = self.layer_metadata.iter().map(|m| m.id).collect();
            self.graph.sync_stack(&modes, &ids, &ids, None)?
        } else {
            self.graph.clone()
        };
        let mut opacities: Vec<_> = self.layers.iter().map(|layer| layer.opacity).collect();
        opacities[index] = opacity;
        let prepared = self.prepare_group_plan_with_masks(
            &graph,
            self.groups.as_ref(),
            &self.layer_metadata,
            &modes,
            &opacities,
            self.next_group_id,
            &self.raster_masks,
            true,
        )?;
        self.finish();
        let before = self.composition();
        let l = &mut self.layers[index];
        l.visible = visible;
        l.opacity = opacity;
        l.mode = mode;
        self.layer_metadata[index].alpha_locked = alpha_locked;
        self.layer_metadata[index].pixel_locked = pixel_locked;
        self.layer_metadata[index].position_locked = position_locked;
        self.layer_metadata[index].color_label = color_label;
        self.graph = graph;
        self.install_group_plan(prepared);
        self.commit_composition(before, None);
        Ok(())
    }
    fn damage(&mut self, r: [usize; 4]) {
        self.replay_surface = Some(match self.replay_surface {
            Some(p) => [
                p[0].min(r[0]),
                p[1].min(r[1]),
                p[2].max(r[2]),
                p[3].max(r[3]),
            ],
            None => r,
        });
        self.changed = Some(match self.changed {
            Some(p) => [
                p[0].min(r[0]),
                p[1].min(r[1]),
                p[2].max(r[2]),
                p[3].max(r[3]),
            ],
            None => r,
        });
    }
    pub fn compose(&mut self) {
        if let Some([x0, y0, x1, y1]) = self.changed.take() {
            self.display_changed = Some(match self.display_changed {
                Some(p) => [x0.min(p[0]), y0.min(p[1]), x1.max(p[2]), y1.max(p[3])],
                None => [x0, y0, x1, y1],
            });
            if self.requires_renderer() {
                if self.renderer.is_some() {
                    let mut layers = [micopi_plugin_api::CompositeLayer {
                        data: std::ptr::null(),
                        len: 0,
                        opacity: 0,
                        visible: 0,
                        mode: 0,
                    }; MAX_LAYERS];
                    for (slot, l) in layers.iter_mut().zip(&self.layers) {
                        *slot = micopi_plugin_api::CompositeLayer {
                            data: l.pixels.as_ptr(),
                            len: l.pixels.len(),
                            opacity: l.opacity as u32,
                            visible: l.visible as u32,
                            mode: l.mode as u32,
                        };
                    }
                    let offset = y0 * self.width + x0;
                    let mut pixels = micopi_plugin_api::Pixels {
                        data: unsafe { self.composite.as_mut_ptr().add(offset) },
                        len: (y1 - y0 - 1) * self.width + x1 - x0,
                        width: (x1 - x0) as u32,
                        height: (y1 - y0) as u32,
                        stride: self.width as u32,
                        origin_x: x0 as u32,
                        origin_y: y0 as u32,
                    };
                    self.render_error = if self.has_raster_masks() {
                        self.render_masked_composition(
                            &mut layers[..self.layers.len()],
                            &mut pixels,
                        )
                    } else {
                        self.renderer.as_ref().unwrap().render(
                            if self.has_groups()
                                || self.graph.enabled
                                    && (!self.graph.linked || self.graph.has_filters())
                            {
                                &self.plan
                            } else {
                                &[]
                            },
                            &layers[..self.layers.len()],
                            self.width as u32,
                            self.height as u32,
                            &mut pixels,
                        )
                    }
                    .err();
                    return;
                }
                self.render_error = Some("Compositor DLL is not loaded".into());
                return;
            }
            self.render_error = None;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = y * self.width + x;
                    let mut c = 0;
                    for l in &self.layers {
                        if l.visible {
                            let p = l.pixels[i];
                            c = over(
                                c,
                                (p & 0xffffff) | (((p >> 24) * l.opacity as u32 / 255) << 24),
                            );
                        }
                    }
                    self.composite[i] = c;
                }
            }
        }
    }
    pub fn display_pixels(&mut self, out: &mut Vec<u32>) {
        self.update_display(out);
    }
    /// Return only the changed rectangle, so GPU frontends upload touched pixels.
    pub fn update_display(&mut self, out: &mut Vec<u32>) -> Option<[usize; 4]> {
        self.compose();
        let region = if out.len() != self.composite.len() {
            Some([0, 0, self.width, self.height])
        } else {
            self.display_changed
        };
        self.display_changed = None;
        out.resize(self.composite.len(), 0);
        if let Some([x0, y0, x1, y1]) = region {
            for y in y0..y1 {
                for x in x0..x1 {
                    let bg = if ((x / 12) + (y / 12)) % 2 == 0 {
                        0xfffdfdfd
                    } else {
                        0xfff3f5f7
                    };
                    let i = y * self.width + x;
                    out[i] = over(bg, self.composite[i]);
                }
            }
        }
        region
    }
    pub(crate) fn begin_plugin_stroke(&mut self) {
        self.finish();
        self.stroke = Some(Edit {
            group: None,
            layer_change: None,
            layer: self.active,
            tiles: BTreeMap::new(),
            mask_tiles: BTreeMap::new(),
            mask_change: None,
            composition: None,
            metadata_bytes: 0,
        });
    }
    pub(crate) fn prepare_plugin_region(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> bool {
        if self.pixel_locked(self.active)
            || self.stroke.is_none()
            || width == 0
            || height == 0
            || x.checked_add(width).is_none_or(|n| n > self.width)
            || y.checked_add(height).is_none_or(|n| n > self.height)
            || !self.layer_visible(self.active)
        {
            return false;
        }
        for ty in y / TILE..=(y + height - 1) / TILE {
            for tx in x / TILE..=(x + width - 1) / TILE {
                self.save_tile(tx, ty);
            }
        }
        self.damage([x, y, x + width, y + height]);
        true
    }
    pub(crate) fn rollback_plugin_stroke(&mut self) {
        let dirty = self.dirty;
        if let Some(edit) = self.stroke.take() {
            self.swap_edit(edit);
        }
        self.dirty = dirty;
        self.last = None;
        self.last_brush = None;
        self.mask_contact = None;
    }
    pub(crate) fn locked_region_empty(&self, x: usize, y: usize, w: usize, h: usize) -> bool {
        self.alpha_locked(self.active)
            && self.stroke.is_some()
            && self.layer_visible(self.active)
            && w > 0
            && h > 0
            && x.checked_add(w).is_some_and(|v| v <= self.width)
            && y.checked_add(h).is_some_and(|v| v <= self.height)
            && (y..y + h).all(|py| {
                self.layers[self.active].pixels[py * self.width + x..py * self.width + x + w]
                    .iter()
                    .all(|p| p >> 24 == 0)
            })
    }
    /// Reuse the existing undo tiles to validate locked tool edits, without a
    /// region copy or allocation on the typed path.
    pub(crate) fn locked_tool_region_valid(&self, x: usize, y: usize, w: usize, h: usize) -> bool {
        if !self.alpha_locked(self.active) {
            return true;
        }
        let Some(edit) = &self.stroke else {
            return false;
        };
        let cols = self.width.div_ceil(TILE);
        for ty in y / TILE..=(y + h - 1) / TILE {
            for tx in x / TILE..=(x + w - 1) / TILE {
                let tile = ty * cols + tx;
                let Some(saved) = edit.tiles.get(&tile) else {
                    return false;
                };
                let tile_x = tx * TILE;
                let tile_y = ty * TILE;
                let tile_width = (self.width - tile_x).min(TILE);
                let start = x.max(tile_x);
                let end = (x + w).min(tile_x + tile_width);
                for py in y.max(tile_y)..(y + h).min(tile_y + TILE) {
                    let offset = (py - tile_y) * tile_width + start - tile_x;
                    let before = &saved[offset..offset + end - start];
                    let after = &self.layers[self.active].pixels
                        [py * self.width + start..py * self.width + end];
                    for (&before, &after) in before.iter().zip(after) {
                        if before >> 24 != after >> 24 || (before >> 24 == 0 && before != after) {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }
    pub fn begin(&mut self, x: f32, y: f32, b: Brush) {
        self.finish();
        if !self.layer_visible(self.active) || self.pixel_locked(self.active) {
            return;
        }
        self.stroke = Some(Edit {
            group: None,
            layer_change: None,
            layer: self.active,
            tiles: BTreeMap::new(),
            mask_tiles: BTreeMap::new(),
            mask_change: None,
            composition: None,
            metadata_bytes: 0,
        });
        self.last = Some((x, y));
        self.last_brush = Some(b);
        self.distance = 0.;
        self.dab(x, y, b);
    }
    pub fn line(&mut self, x: f32, y: f32, b: Brush) {
        if self.stroke.is_none() {
            return;
        }
        if let Some((px, py)) = self.last {
            let dx = x - px;
            let dy = y - py;
            let len = dx.hypot(dy);
            let spacing = (b.radius * 0.22).max(0.5);
            let previous = self.last_brush.unwrap_or(b);
            if len < 0.0001 && previous.opacity <= 0. && b.opacity > 0. {
                self.dab(x, y, b);
            }
            // A rapid pressure decrease can shrink spacing below the old residual.
            // Never extrapolate a dab backwards along the incoming segment.
            let mut d = (spacing - self.distance).max(0.);
            if len < 0.0001 {
                self.last = Some((x, y));
                self.last_brush = Some(b);
                return;
            }
            while d <= len {
                let t = d / len;
                self.dab(
                    px + dx * t,
                    py + dy * t,
                    Brush {
                        radius: previous.radius + (b.radius - previous.radius) * t,
                        opacity: previous.opacity + (b.opacity - previous.opacity) * t,
                        ..b
                    },
                );
                d += spacing;
            }
            self.distance = (self.distance + len) % spacing;
        }
        self.last = Some((x, y));
        self.last_brush = Some(b);
    }
    fn save_tile(&mut self, tx: usize, ty: usize) {
        self.save_layer_tile(self.active, tx, ty);
    }
    fn save_layer_tile(&mut self, index: usize, tx: usize, ty: usize) {
        let cols = (self.width + TILE - 1) / TILE;
        let id = ty * cols + tx;
        self.replay_tiles.insert((index, id));
        let edit = self.stroke.as_mut().unwrap();
        debug_assert_eq!(edit.layer, index);
        if edit.tiles.contains_key(&id) {
            return;
        }
        let mut data = Vec::with_capacity(TILE * TILE);
        for y in ty * TILE..((ty + 1) * TILE).min(self.height) {
            data.extend_from_slice(
                &self.layers[index].pixels[y * self.width + tx * TILE
                    ..y * self.width + ((tx + 1) * TILE).min(self.width)],
            );
        }
        edit.tiles.insert(id, Arc::new(data));
    }
    fn dab(&mut self, x: f32, y: f32, b: Brush) {
        if self.alpha_locked(self.active) || self.pixel_locked(self.active) {
            return;
        }
        if b.opacity <= 0. || b.radius <= 0. {
            return;
        }
        let r = b.radius.max(0.5);
        let x0 = (x - r - 1.).max(0.).min(self.width as f32) as usize;
        let y0 = (y - r - 1.).max(0.).min(self.height as f32) as usize;
        let x1 = (x + r + 1.).ceil().max(0.).min(self.width as f32) as usize;
        let y1 = (y + r + 1.).ceil().max(0.).min(self.height as f32) as usize;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for ty in y0 / TILE..=(y1 - 1) / TILE {
            for tx in x0 / TILE..=(x1 - 1) / TILE {
                self.save_tile(tx, ty);
            }
        }
        let layer = &mut self.layers[self.active];
        for py in y0..y1 {
            for px in x0..x1 {
                let d = (px as f32 + 0.5 - x).hypot(py as f32 + 0.5 - y);
                let edge = (r - d + 0.5).max(0.).min(1.);
                let soft = if b.hardness >= 0.999 {
                    1.
                } else {
                    ((r - d) / (r * (1. - b.hardness))).max(0.).min(1.)
                };
                let a = (255. * b.opacity * edge * soft).round() as u32;
                if a == 0 {
                    continue;
                }
                let i = py * self.width + px;
                let old = layer.pixels[i];
                layer.pixels[i] = if b.erase {
                    (old & 0xffffff) | ((((old >> 24) * (255 - a) + 127) / 255) << 24)
                } else {
                    over(old, (b.color & 0xffffff) | (a << 24))
                };
            }
        }
        self.damage([x0, y0, x1, y1]);
    }
    pub fn finish(&mut self) {
        if let Some(edit) = self.stroke.take() {
            if !edit.tiles.is_empty()
                || !edit.mask_tiles.is_empty()
                || edit.mask_change.is_some()
                || edit.composition.is_some()
            {
                self.dirty = true;
                if let Some(group) = &mut self.history_group {
                    group.edits.push(edit);
                } else {
                    self.undo.push(edit);
                    self.redo.clear();
                    self.prune_history();
                }
            }
        }
        self.last = None;
        self.last_brush = None;
        self.mask_contact = None;
    }
    pub fn history_bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(self.redo.iter())
            .map(Edit::bytes)
            .sum()
    }
    fn prune_history(&mut self) {
        while self.undo.len() > 1 && self.history_bytes() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
    }
    fn swap_edit(&mut self, mut e: Edit) -> Edit {
        if let Some(group) = &mut e.group {
            let active = self.active;
            let selected = self.selected_layer_ids.clone();
            group.edits = std::mem::take(&mut group.edits)
                .into_iter()
                .rev()
                .map(|child| self.swap_edit(child))
                .collect();
            self.active = std::mem::replace(&mut group.active, active);
            self.selected_layer_ids = std::mem::replace(&mut group.selected, selected);
            self.normalize_layer_selection();
            self.invalidate();
            self.dirty = true;
            return e;
        }
        if let Some(state) = &mut e.composition {
            let before = self.composition();
            if let Some(change) = &mut e.layer_change {
                match change {
                    LayerChange::Permutation { order, active } => {
                        let inverse = Self::inverse_order(order);
                        self.permute_buffers(order);
                        *order = inverse;
                        std::mem::swap(&mut self.active, active);
                    }
                    LayerChange::Replace {
                        current,
                        saved,
                        active,
                    } => {
                        let mut removed = Vec::with_capacity(current.len());
                        for &index in current.iter().rev() {
                            removed.push((index, Arc::new(self.layers.remove(index))));
                        }
                        removed.reverse();
                        *current = saved.iter().map(|(index, _)| *index).collect();
                        for (index, layer) in std::mem::replace(saved, removed) {
                            self.layers.insert(index, Arc::unwrap_or_clone(layer));
                        }
                        std::mem::swap(&mut self.active, active);
                    }
                    LayerChange::Canvas(saved) => {
                        let old = std::mem::replace(
                            saved,
                            Arc::new(CanvasState {
                                width: self.width,
                                height: self.height,
                                layers: std::mem::take(&mut self.layers),
                                mask_planes: std::mem::take(&mut self.mask_planes),
                            }),
                        );
                        let old = Arc::unwrap_or_clone(old);
                        self.width = old.width;
                        self.height = old.height;
                        self.layers = old.layers;
                        self.mask_planes = old.mask_planes;
                        self.bump_mask_storage_epoch();
                        self.composite = vec![0; self.width * self.height];
                        self.replay_tiles.clear();
                    }
                    LayerChange::Toggle {
                        index,
                        active,
                        removed,
                    } => {
                        if let Some(layer) = removed.take() {
                            self.layers.insert(*index, Arc::unwrap_or_clone(layer));
                        } else {
                            *removed = Some(Arc::new(self.layers.remove(*index)));
                        }
                        std::mem::swap(&mut self.active, active);
                    }
                    LayerChange::Move { from, to, active } => {
                        let layer = self.layers.remove(*from);
                        self.layers.insert(*to, layer);
                        std::mem::swap(from, to);
                        std::mem::swap(&mut self.active, active);
                    }
                }
                self.track_layer_order();
            }
            if let Some(change) = &mut e.mask_change {
                self.swap_mask_change(change);
            }
            self.restore_composition(state.clone());
            *state = before;
            e.metadata_bytes = state.graph.nodes.len() * 160
                + state.groups.as_ref().map_or(0, |g| g.memory_bytes())
                + state.selected_layer_ids.len() * 8
                + state.selection.memory_bytes()
                + state.raster_masks.memory_bytes()
                + std::mem::size_of_val(&state.last_transform)
                + std::mem::size_of_val(&state.document_info)
                + state.graph.edges.len() * 16
                + state.workflow.bytes()
                + state.layers.len() * 8
                + state
                    .layer_metadata
                    .iter()
                    .map(|m| m.name.len() + std::mem::size_of::<LayerMetadata>())
                    .sum::<usize>()
                + 64;
        }
        let cols = (self.width + TILE - 1) / TILE;
        for (id, data) in e.tiles.iter_mut() {
            let data = Arc::make_mut(data);
            self.replay_tiles.insert((e.layer, *id));
            let tx = *id % cols;
            let ty = *id / cols;
            let mut j = 0;
            for y in ty * TILE..((ty + 1) * TILE).min(self.height) {
                for x in tx * TILE..((tx + 1) * TILE).min(self.width) {
                    std::mem::swap(
                        &mut self.layers[e.layer].pixels[y * self.width + x],
                        &mut data[j],
                    );
                    j += 1;
                }
            }
        }
        self.swap_mask_tiles(&mut e.mask_tiles);
        self.invalidate();
        self.dirty = true;
        e
    }
    pub fn undo(&mut self) {
        self.finish();
        if let Some(e) = self.undo.pop() {
            let e = self.swap_edit(e);
            self.redo.push(e);
        }
    }
    pub fn redo(&mut self) {
        self.finish();
        if let Some(e) = self.redo.pop() {
            let e = self.swap_edit(e);
            self.undo.push(e);
        }
    }
    pub fn add_layer(&mut self) {
        let _ = self.try_add_layer();
    }
    pub fn try_add_layer(&mut self) -> Result<(), String> {
        self.try_add_layer_in(None)
    }
    pub fn try_add_layer_in(&mut self, parent: Option<u64>) -> Result<(), String> {
        if self.layers.len() >= MAX_LAYERS
            || self.width * self.height * (self.layers.len() + 1) > 64 * 1024 * 1024
        {
            return Err("Layer count or document pixel limit reached".into());
        }
        let mut metadata = self.layer_metadata.clone();
        let mut groups = self.groups.clone();
        if let Some(tree) = &mut groups {
            if let Some(parent) = parent {
                tree.children_mut(parent)?
                    .push(super::groups::GroupItem::layer(self.next_layer_id));
            } else {
                tree.insert_layer_after(self.layer_id(self.active), self.next_layer_id)?;
            }
        } else if parent.is_some_and(|id| id != 0) {
            return Err("Group does not exist".into());
        }
        let index = groups
            .as_ref()
            .map(|t| t.layer_ids())
            .transpose()?
            .map(|ids| ids.iter().position(|id| *id == self.next_layer_id).unwrap())
            .unwrap_or(self.layers.len());
        metadata.insert(
            index,
            LayerMetadata {
                id: self.next_layer_id,
                name: String::new(),
                alpha_locked: false,
                ..Default::default()
            },
        );
        let graph = self.reindexed_graph(&metadata, None)?;
        if self.next_layer_id > u32::MAX as u64 {
            return Err("Layer identity limit reached".into());
        }
        let modes: Vec<_> = metadata
            .iter()
            .map(|m| {
                self.layer_index(m.id)
                    .map_or(BlendMode::Normal, |i| self.layers[i].mode)
            })
            .collect();
        let prepared = self.prepare_group_plan(
            &graph,
            groups.as_ref(),
            &metadata,
            &modes,
            self.next_group_id,
            true,
        )?;
        self.finish();
        let before = self.composition();
        let active = self.active;
        self.layer_metadata = metadata;
        self.next_layer_id += 1;
        self.layers.insert(
            index,
            Layer {
                pixels: vec![0; self.width * self.height],
                visible: true,
                opacity: 255,
                mode: BlendMode::Normal,
            },
        );
        self.select_layer(index);
        self.graph = graph;
        self.groups = groups;
        self.install_group_plan(prepared);
        self.commit_composition(
            before,
            Some(LayerChange::Toggle {
                index: self.active,
                active,
                removed: None,
            }),
        );
        Ok(())
    }
    /// Commit a contiguous plugin-owned region, recording only changed tiles.
    pub fn replace_region(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        pixels: &[u32],
    ) -> bool {
        self.replace_layer_region(self.active, x, y, width, height, pixels)
    }
    /// Replace an explicit raster without changing the active painting layer or
    /// multi-selection. Permissions use the target, not the current active row.
    pub fn replace_layer_region(
        &mut self,
        index: usize,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        pixels: &[u32],
    ) -> bool {
        if index >= self.layers.len()
            || self.pixel_locked(index)
            || width == 0
            || height == 0
            || x.checked_add(width).is_none_or(|v| v > self.width)
            || y.checked_add(height).is_none_or(|v| v > self.height)
            || width.checked_mul(height) != Some(pixels.len())
        {
            return false;
        }
        if self.alpha_locked(index)
            && pixels.iter().enumerate().any(|(i, after)| {
                let before =
                    self.layers[index].pixels[(y + i / width) * self.width + x + i % width];
                before >> 24 != *after >> 24 || (before >> 24 == 0 && before != *after)
            })
        {
            return false;
        }
        self.finish();
        self.stroke = Some(Edit {
            group: None,
            layer_change: None,
            layer: index,
            tiles: BTreeMap::new(),
            mask_tiles: BTreeMap::new(),
            mask_change: None,
            composition: None,
            metadata_bytes: 0,
        });
        let changed = self.write_layer_region(index, x, y, width, height, pixels);
        self.finish();
        changed
    }
    /// Write into a preflighted region under the caller's open history entry.
    fn write_region(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        pixels: &[u32],
    ) -> bool {
        self.write_layer_region(self.active, x, y, width, height, pixels)
    }
    fn write_layer_region(
        &mut self,
        index: usize,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        pixels: &[u32],
    ) -> bool {
        let mut changed = false;
        for ty in y / TILE..=(y + height - 1) / TILE {
            for tx in x / TILE..=(x + width - 1) / TILE {
                let x0 = x.max(tx * TILE);
                let x1 = (x + width).min((tx + 1) * TILE);
                let y0 = y.max(ty * TILE);
                let y1 = (y + height).min((ty + 1) * TILE);
                let differs = (y0..y1).any(|row| {
                    self.layers[index].pixels[row * self.width + x0..row * self.width + x1]
                        != pixels[(row - y) * width + x0 - x..(row - y) * width + x1 - x]
                });
                if !differs {
                    continue;
                }
                self.save_layer_tile(index, tx, ty);
                changed = true;
                for row in y0..y1 {
                    self.layers[index].pixels[row * self.width + x0..row * self.width + x1]
                        .copy_from_slice(
                            &pixels[(row - y) * width + x0 - x..(row - y) * width + x1 - x],
                        );
                }
            }
        }
        if changed {
            self.damage([x, y, x + width, y + height]);
        }
        changed
    }
    pub fn clear_layer(&mut self) {
        if self.alpha_locked(self.active) || self.pixel_locked(self.active) {
            return;
        }
        self.finish();
        self.stroke = Some(Edit {
            group: None,
            layer_change: None,
            layer: self.active,
            tiles: BTreeMap::new(),
            mask_tiles: BTreeMap::new(),
            mask_change: None,
            composition: None,
            metadata_bytes: 0,
        });
        for ty in 0..(self.height + TILE - 1) / TILE {
            for tx in 0..(self.width + TILE - 1) / TILE {
                self.save_tile(tx, ty);
            }
        }
        self.layers[self.active].pixels.fill(0);
        self.invalidate();
        self.finish();
    }
    pub fn save(&mut self, path: &Path) -> io::Result<()> {
        self.save_with_replay(path, None)
    }
    pub(crate) fn save_with_replay(
        &mut self,
        path: &Path,
        replay: Option<&[u8]>,
    ) -> io::Result<()> {
        self.finish();
        self.validate_mask_storage().map_err(io::Error::other)?;
        if replay.is_some_and(|bytes| bytes.len() > super::replay::MAX_ARCHIVE) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Project replay exceeds the supported limit",
            ));
        }
        let metadata = serde_json::to_vec(&self.composition()).map_err(io::Error::other)?;
        if metadata.len() > 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Project metadata exceeds the supported limit",
            ));
        }
        // Same-directory temp + rename keeps an existing project intact on write failure.
        let tmp = path.with_extension("mcp.tmp");
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(if self.mask_storage_used() {
            b"MICOPI04"
        } else if replay.is_some() {
            b"MICOPI03"
        } else {
            b"MICOPI02"
        })?;
        for v in [self.width, self.height, self.layers.len(), self.active] {
            f.write_all(&(v as u32).to_le_bytes())?;
        }
        for l in &self.layers {
            f.write_all(&[l.visible as u8, l.opacity])?;
            let mut bytes = Vec::with_capacity(l.pixels.len() * 4);
            for p in &l.pixels {
                bytes.extend_from_slice(&p.to_le_bytes());
            }
            f.write_all(&bytes)?;
        }
        f.write_all(&(metadata.len() as u32).to_le_bytes())?;
        f.write_all(&metadata)?;
        if self.mask_storage_used() {
            self.write_mask_project_suffix(&mut f, replay)?;
        } else if let Some(bytes) = replay {
            f.write_all(&(bytes.len() as u64).to_le_bytes())?;
            f.write_all(bytes)?;
        }
        f.sync_all()?;
        drop(f);
        crate::platform::replace_file(&tmp, path)?;
        self.dirty = false;
        Ok(())
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        let mut f = std::fs::File::open(path)?;
        let mut magic = [0; 8];
        f.read_exact(&mut magic)?;
        let bad = || {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid or oversized Micopi project",
            )
        };
        if &magic != b"MICOPI01"
            && &magic != b"MICOPI02"
            && &magic != b"MICOPI03"
            && &magic != b"MICOPI04"
        {
            return Err(bad());
        }
        let mut dims = [0usize; 4];
        for d in &mut dims {
            let mut b = [0; 4];
            f.read_exact(&mut b)?;
            *d = u32::from_le_bytes(b) as usize;
        }
        let [w, h, n, a] = dims;
        if w == 0
            || h == 0
            || w > MAX_SIDE
            || h > MAX_SIDE
            || n == 0
            || n > MAX_LAYERS
            || a >= n
            || w * h * n > 64 * 1024 * 1024
        {
            return Err(bad());
        }
        let base = 24 + (2 + w * h * 4) as u64 * n as u64;
        let size = f.metadata()?.len();
        if (&magic == b"MICOPI01" && size != base)
            || (&magic == b"MICOPI02" && (size < base + 4 || size > base + 4 + 1024 * 1024))
            || (&magic == b"MICOPI03"
                && (size < base + 4 + 8
                    || size > base + 4 + 1024 * 1024 + 8 + super::replay::MAX_ARCHIVE as u64))
            || (&magic == b"MICOPI04"
                && (size < base + 4 + 4 + 8
                    || size
                        > base
                            + 4
                            + 1024 * 1024
                            + 4
                            + 32 * 13
                            + super::raster_masks::MASK_SAMPLE_LIMIT as u64
                            + 8
                            + super::replay::MAX_ARCHIVE as u64))
        {
            return Err(bad());
        }
        let mut doc = Self::new(w, h);
        doc.layers.clear();
        doc.layer_metadata = (0..n)
            .map(|i| LayerMetadata {
                id: i as u64 + 1,
                name: String::new(),
                alpha_locked: false,
                ..Default::default()
            })
            .collect();
        doc.next_layer_id = n as u64 + 1;
        doc.active = a;
        for _ in 0..n {
            let mut flags = [0; 2];
            f.read_exact(&mut flags)?;
            if flags[0] > 1 {
                return Err(bad());
            }
            let mut bytes = vec![0; w * h * 4];
            f.read_exact(&mut bytes)?;
            let pixels = bytes
                .chunks_exact(4)
                .map(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]))
                .collect();
            doc.layers.push(Layer {
                pixels,
                visible: flags[0] != 0,
                opacity: flags[1],
                mode: BlendMode::Normal,
            });
        }
        // A legacy project did not opt into linked semantics. Preserve its exact metadata.
        doc.graph = Graph::default();
        doc.plan.clear();
        if &magic == b"MICOPI02" || &magic == b"MICOPI03" || &magic == b"MICOPI04" {
            let mut length = [0u8; 4];
            f.read_exact(&mut length)?;
            let length = u32::from_le_bytes(length) as usize;
            if length > 1024 * 1024 || (&magic == b"MICOPI02" && size != base + 4 + length as u64) {
                return Err(bad());
            }
            let mut bytes = vec![0; length];
            f.read_exact(&mut bytes)?;
            let state: Composition = serde_json::from_slice(&bytes).map_err(|_| bad())?;
            if state.layers.len() != n {
                return Err(bad());
            }
            state.validate_layer_metadata().map_err(|_| bad())?;
            state.validate_layer_selection(a).map_err(|_| bad())?;
            state.selection.validate(w, h).map_err(|_| bad())?;
            state
                .raster_masks
                .validate_dimensions(w, h)
                .map_err(|_| bad())?;
            if state.raster_masks.is_default() == (&magic == b"MICOPI04") {
                return Err(bad());
            }
            state.graph.compile(n).map_err(|_| bad())?;
            state.workflow.validate_structure().map_err(|_| bad())?;
            doc.restore_composition(state);
            if &magic == b"MICOPI04" {
                doc.read_mask_project_suffix(&mut f, size - base - 4 - length as u64)?;
            } else if &magic == b"MICOPI03" {
                let mut length_bytes = [0; 8];
                f.read_exact(&mut length_bytes)?;
                let replay_length = u64::from_le_bytes(length_bytes);
                if replay_length > super::replay::MAX_ARCHIVE as u64
                    || size != base + 4 + length as u64 + 8 + replay_length
                {
                    return Err(bad());
                }
                let mut bytes = vec![0; replay_length as usize];
                f.read_exact(&mut bytes)?;
                doc.replay_data = Some(bytes);
            }
        }
        doc.validate_mask_storage().map_err(|_| bad())?;
        Ok(doc)
    }
}
#[path = "blend_modes.rs"]
mod blend_modes;
#[path = "canvas_editing.rs"]
mod canvas_editing;
#[path = "group_editing.rs"]
mod group_editing;
#[path = "group_merge.rs"]
mod group_merge;
#[path = "layer_editing.rs"]
mod layer_editing;
#[path = "layer_merge.rs"]
mod layer_merge;
#[path = "layer_selection.rs"]
mod layer_selection;
#[path = "mask_apply.rs"]
mod mask_apply;
#[path = "mask_composition.rs"]
mod mask_composition;
#[path = "mask_effects.rs"]
mod mask_effects;
#[path = "mix.rs"]
mod mix;
#[path = "raster_mask_editing.rs"]
mod raster_mask_editing;
#[path = "raster_mask_project.rs"]
mod raster_mask_project;
#[path = "selected_merge.rs"]
mod selected_merge;
pub use layer_merge::MergeLayers;
pub use selected_merge::SelectedMergePlan;
#[cfg(test)]
#[path = "raster_mask_tests.rs"]
mod raster_mask_tests;
#[cfg(test)]
#[path = "document_tests.rs"]
mod tests;
