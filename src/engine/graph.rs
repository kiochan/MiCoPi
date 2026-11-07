//! Serializable image DAG. Validation/compilation stays host-owned; pixel math is DLL code.
use micopi_plugin_api as abi;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_NODES: usize = 64;
#[cfg(test)]
#[path = "mask_graph_tests.rs"]
mod mask_graph_tests;
pub fn schema() -> serde_json::Value {
    use serde_json::json;
    let id = json!({"type":"integer","minimum":0,"maximum":4294967295u64});
    json!({"type":"object","required":["enabled","nodes","edges"],"additionalProperties":false,"properties":{
        "enabled":{"type":"boolean"},"linked":{"type":"boolean","default":false,"description":"A validated layer-stack chain; structural layer edits keep its branches synchronized"},
        "nodes":{"type":"array","maxItems":64,"items":{"type":"object","required":["id","position","kind"],"additionalProperties":false,"properties":{
            "node_type":{"type":"string"},"id":id,"position":{"type":"array","minItems":2,"maxItems":2,"items":{"type":"number","minimum":-1000000,"maximum":1000000}},
            "kind":{"type":"string","enum":["layer","filter","levels","curves","hsl","blend","mix","output","transparent","mask_source","apply_mask","masked_mix"]},"index":{"type":"integer","minimum":0,"maximum":15},
            "parameters":super::levels::Levels::schema(),
            "curves":super::curves::Curves::schema(),
            "hsl":super::hsl::Hsl::schema(),
            "mask_id":{"type":"integer","minimum":1,"maximum":4294967295u64},
            "filter":{"type":"string","enum":Filter::ALL},"amount":{"type":"number","minimum":-1,"maximum":1},
            "mode":{"type":"string","enum":BlendMode::ALL},"opacity":{"type":"number","minimum":0,"maximum":1}
        }}},
        "edges":{"type":"array","maxItems":192,"items":{"type":"object","required":["from","to","input"],"additionalProperties":false,"properties":{"from":id,"to":id,"input":{"type":"integer","minimum":0,"maximum":2}}}}
    }})
}

#[cfg(test)]
mod transparent_tests {
    use super::*;

    #[test]
    fn transparent_source_compiles_without_layer_or_input_and_keeps_canonical_fields() {
        let graph = Graph {
            enabled: true,
            linked: false,
            nodes: vec![
                Node {
                    id: 3,
                    node_type: Some("compositing.transparent".into()),
                    position: [0., 0.],
                    data: NodeKind::Transparent,
                },
                Node {
                    id: 7,
                    node_type: Some("compositing.output".into()),
                    position: [100., 0.],
                    data: NodeKind::Output,
                },
            ],
            edges: vec![Edge {
                from: 3,
                to: 7,
                input: 0,
            }],
        };
        let decoded: Graph = serde_json::from_value(serde_json::to_value(&graph).unwrap()).unwrap();
        assert_eq!(decoded, graph);
        assert_eq!(NodeKind::Transparent.inputs(), 0);
        let compiled = graph.compile(16).unwrap();
        assert_eq!(compiled.len(), 2);
        let seed = compiled[0];
        assert_eq!(
            (seed.kind, seed.a, seed.b, seed.mode, seed.amount),
            (4, 0, 0, 0, 0.)
        );
        assert_eq!((compiled[1].kind, compiled[1].a), (3, 0));
        let mut invalid = graph.clone();
        invalid.edges.push(Edge {
            from: 3,
            to: 3,
            input: 0,
        });
        assert!(invalid.compile(16).is_err());
        invalid = graph;
        invalid.linked = true;
        assert!(invalid.compile(16).is_err());
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u32)]
pub enum BlendMode {
    #[default]
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Overlay = 3,
    Darken = 4,
    Lighten = 5,
    Add = 6,
    Difference = 7,
    ColorBurn = 8,
    LinearBurn = 9,
    DarkerColor = 10,
    ColorDodge = 11,
    LighterColor = 12,
    SoftLight = 13,
    HardLight = 14,
    VividLight = 15,
    LinearLight = 16,
    PinLight = 17,
    HardMix = 18,
    Exclusion = 19,
    Subtract = 20,
    Divide = 21,
    Hue = 22,
    Saturation = 23,
    Color = 24,
    Luminosity = 25,
}
impl BlendMode {
    pub const LEGACY: [Self; 8] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
        Self::Add,
        Self::Difference,
    ];
    pub const ALL: [Self; 26] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
        Self::Add,
        Self::Difference,
        Self::ColorBurn,
        Self::LinearBurn,
        Self::DarkerColor,
        Self::ColorDodge,
        Self::LighterColor,
        Self::SoftLight,
        Self::HardLight,
        Self::VividLight,
        Self::LinearLight,
        Self::PinLight,
        Self::HardMix,
        Self::Exclusion,
        Self::Subtract,
        Self::Divide,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];
    pub const MENU: [Self; 26] = [
        Self::Normal,
        Self::Darken,
        Self::Multiply,
        Self::ColorBurn,
        Self::LinearBurn,
        Self::DarkerColor,
        Self::Lighten,
        Self::Screen,
        Self::ColorDodge,
        Self::Add,
        Self::LighterColor,
        Self::Overlay,
        Self::SoftLight,
        Self::HardLight,
        Self::VividLight,
        Self::LinearLight,
        Self::PinLight,
        Self::HardMix,
        Self::Difference,
        Self::Exclusion,
        Self::Subtract,
        Self::Divide,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];
    pub fn starts_menu_group(self) -> bool {
        matches!(
            self,
            Self::Normal
                | Self::Darken
                | Self::Lighten
                | Self::Overlay
                | Self::Difference
                | Self::Hue
        )
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Multiply => "Multiply",
            Self::Screen => "Screen",
            Self::Overlay => "Overlay",
            Self::Darken => "Darken",
            Self::Lighten => "Lighten",
            Self::Add => "Linear Dodge",
            Self::Difference => "Difference",
            Self::ColorBurn => "Color Burn",
            Self::LinearBurn => "Linear Burn",
            Self::DarkerColor => "Darker Color",
            Self::ColorDodge => "Color Dodge",
            Self::LighterColor => "Lighter Color",
            Self::SoftLight => "Soft Light",
            Self::HardLight => "Hard Light",
            Self::VividLight => "Vivid Light",
            Self::LinearLight => "Linear Light",
            Self::PinLight => "Pin Light",
            Self::HardMix => "Hard Mix",
            Self::Exclusion => "Exclusion",
            Self::Subtract => "Subtract",
            Self::Divide => "Divide",
            Self::Hue => "Hue",
            Self::Saturation => "Saturation",
            Self::Color => "Color",
            Self::Luminosity => "Luminosity",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u32)]
pub enum Filter {
    Invert,
    Grayscale,
    Brightness,
    Contrast,
    Saturation,
}
impl Filter {
    pub const ALL: [Self; 5] = [
        Self::Invert,
        Self::Grayscale,
        Self::Brightness,
        Self::Contrast,
        Self::Saturation,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Invert => "Invert",
            Self::Grayscale => "Grayscale",
            Self::Brightness => "Brightness",
            Self::Contrast => "Contrast",
            Self::Saturation => "Saturation",
        }
    }
    pub fn minimum(self) -> f32 {
        if matches!(self, Self::Invert | Self::Grayscale) {
            0.
        } else {
            -1.
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeKind {
    Transparent,
    Layer { index: usize },
    Filter { filter: Filter, amount: f32 },
    Levels { parameters: super::levels::Levels },
    Curves { curves: super::curves::Curves },
    Hsl { hsl: super::hsl::Hsl },
    Blend { mode: BlendMode, opacity: f32 },
    Mix { opacity: f32 },
    MaskSource { mask_id: u64 },
    ApplyMask { opacity: f32 },
    MaskedMix { opacity: f32 },
    Output,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortDomain {
    Image,
    Coverage,
}
impl NodeKind {
    pub fn inputs(&self) -> usize {
        match self {
            Self::Layer { .. } | Self::Transparent | Self::MaskSource { .. } => 0,
            Self::Blend { .. } | Self::Mix { .. } | Self::ApplyMask { .. } => 2,
            Self::MaskedMix { .. } => 3,
            _ => 1,
        }
    }
    pub fn output_domain(&self) -> PortDomain {
        if matches!(self, Self::MaskSource { .. }) {
            PortDomain::Coverage
        } else {
            PortDomain::Image
        }
    }
    pub fn input_domain(&self, port: usize) -> Option<PortDomain> {
        if port >= self.inputs() {
            return None;
        }
        Some(
            if matches!(self, Self::ApplyMask { .. }) && port == 1
                || matches!(self, Self::MaskedMix { .. }) && port == 2
            {
                PortDomain::Coverage
            } else {
                PortDomain::Image
            },
        )
    }
    pub fn title(&self) -> String {
        match self {
            Self::Transparent => "Transparent".into(),
            Self::Layer { index } => format!("Layer {}", index + 1),
            Self::Filter { filter, .. } => filter.title().into(),
            Self::Levels { .. } => "Levels".into(),
            Self::Curves { .. } => "Curves".into(),
            Self::Hsl { .. } => "Hue/Saturation".into(),
            Self::Blend { mode, .. } => format!("Blend · {}", mode.title()),
            Self::Mix { .. } => "Backdrop Mix".into(),
            Self::MaskSource { mask_id } => format!("Mask #{mask_id}"),
            Self::ApplyMask { .. } => "Apply Mask".into(),
            Self::MaskedMix { .. } => "Masked Backdrop Mix".into(),
            Self::Output => "Canvas Output".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    pub id: u32,
    pub position: [f32; 2],
    #[serde(flatten)]
    pub data: NodeKind,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub from: u32,
    pub to: u32,
    pub input: u32,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub linked: bool,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}
fn is_false(value: &bool) -> bool {
    !*value
}
pub trait Renderer {
    fn supports_histogram_plan(&self) -> bool {
        false
    }
    fn histogram_plan(
        &self,
        _nodes: &[abi::CompositeNode],
        _layers: &[abi::CompositeLayer],
        _masks: &[abi::CompositeMask],
        _luts: &[abi::CompositeLut],
        _request: abi::HistogramRequest,
    ) -> Result<abi::HistogramResult, String> {
        Err("The compositor does not support histogram plan sampling".into())
    }
    fn supports_hsl(&self) -> bool {
        false
    }
    fn render_hsl(
        &self,
        _nodes: &[abi::CompositeNode],
        _layers: &[abi::CompositeLayer],
        _masks: &[abi::CompositeMask],
        _luts: &[abi::CompositeLut],
        _hsl: &[abi::HslParameters],
        _width: u32,
        _height: u32,
        _output: &mut abi::Pixels,
    ) -> Result<(), String> {
        Err("The compositor does not support HSL rendering".into())
    }
    fn histogram_hsl(
        &self,
        _nodes: &[abi::CompositeNode],
        _layers: &[abi::CompositeLayer],
        _masks: &[abi::CompositeMask],
        _luts: &[abi::CompositeLut],
        _hsl: &[abi::HslParameters],
        _request: abi::HistogramRequest,
    ) -> Result<abi::HistogramResult, String> {
        Err("The compositor does not support HSL histogram sampling".into())
    }
    fn supports_levels_channels(&self) -> bool {
        false
    }
    fn prepare_levels_channels(
        &self,
        _parameters: &abi::LevelsChannelsParameters,
    ) -> Result<[abi::CompositeLut; 3], String> {
        Err("The compositor does not support channel Levels preparation".into())
    }
    fn supports_curves(&self) -> bool {
        false
    }
    fn prepare_curves(
        &self,
        _parameters: &abi::CurvesParameters,
    ) -> Result<abi::CompositeLut, String> {
        Err("The compositor does not support Curves preparation".into())
    }
    fn supports_levels(&self) -> bool {
        false
    }
    fn prepare_levels(
        &self,
        _parameters: &abi::LevelsParameters,
    ) -> Result<abi::CompositeLut, String> {
        Err("The compositor does not support Levels preparation".into())
    }
    fn render_color(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        masks: &[abi::CompositeMask],
        luts: &[abi::CompositeLut],
        hsl: &[abi::HslParameters],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        if hsl.is_empty() {
            self.render_prepared(nodes, layers, masks, luts, width, height, output)
        } else {
            self.render_hsl(nodes, layers, masks, luts, hsl, width, height, output)
        }
    }
    /// Borrow prepared tables without modifying or retaining the document plan.
    fn render_prepared(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        masks: &[abi::CompositeMask],
        luts: &[abi::CompositeLut],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        if !luts.is_empty() || nodes.iter().any(|n| n.kind == 9 || n.kind == 10) {
            return Err("The compositor does not support Levels rendering".into());
        }
        self.render_masked(nodes, layers, masks, width, height, output)
    }
    fn supports_mask_effects(&self) -> bool {
        false
    }
    /// Reserve reusable native halo capacity at the cold mask-contact boundary,
    /// without evaluating a full-frame feather kernel or changing any pixels.
    fn reserve_mask_effects(&self, _width: u32, _height: u32, _radius: u32) -> Result<(), String> {
        Err("The compositor does not support raster mask effect reservation".into())
    }
    /// Evaluate native feather/density against immutable full-frame raw A8.
    /// The output is a disjoint global-coordinate region in a reusable cache.
    fn mask_effect_region(
        &self,
        _request: &abi::MaskEffectRequest,
        _source: &abi::CompositeMask,
        _width: u32,
        _height: u32,
        _output: &mut abi::A8Pixels,
    ) -> Result<(), String> {
        Err("The compositor does not support raster mask effects".into())
    }
    fn supports_masks(&self) -> bool {
        false
    }
    fn supports_mix(&self) -> bool {
        false
    }
    fn supports_blend_mode(&self, mode: u32) -> bool {
        mode < 8
    }
    fn supports_transparent(&self) -> bool {
        false
    }
    fn render(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String>;
    /// Optional native prerequisite. Document mask ownership and graph syntax
    /// remain separate contracts; old renderers keep the unmasked path.
    fn render_masked(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        masks: &[abi::CompositeMask],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        if !masks.is_empty() || nodes.iter().any(|node| node.kind >= 6) {
            return Err("The compositor does not support mask coverage".into());
        }
        self.render(nodes, layers, width, height, output)
    }
}
impl Graph {
    pub fn color_intents(&self) -> Vec<super::curves::ColorIntent> {
        self.nodes
            .iter()
            .filter_map(|n| match &n.data {
                NodeKind::Levels { parameters } => {
                    Some(super::curves::ColorIntent::Levels(*parameters))
                }
                NodeKind::Curves { curves } => {
                    Some(super::curves::ColorIntent::Curves(curves.clone()))
                }
                NodeKind::Hsl { hsl } => Some(super::curves::ColorIntent::Hsl(*hsl)),
                _ => None,
            })
            .collect()
    }
    pub fn has_hsl(&self) -> bool {
        self.nodes
            .iter()
            .any(|n| matches!(n.data, NodeKind::Hsl { .. }))
    }
    pub fn has_curves(&self) -> bool {
        self.nodes
            .iter()
            .any(|n| matches!(n.data, NodeKind::Curves { .. }))
    }
    pub fn has_channel_levels(&self) -> bool {
        self.nodes.iter().any(|n|matches!(n.data,NodeKind::Levels {parameters} if !parameters.channels.is_identity()))
    }
    pub fn has_prepared_color(&self) -> bool {
        self.has_levels() || self.has_curves() || self.has_hsl()
    }
    fn table_index(&self, index: usize) -> u32 {
        self.nodes[..index]
            .iter()
            .map(|n| match n.data {
                NodeKind::Levels { parameters } => parameters.table_count() as u32,
                NodeKind::Curves { .. } => 3,
                _ => 0,
            })
            .sum()
    }
    pub fn levels_intents(&self) -> Vec<super::levels::Levels> {
        self.nodes
            .iter()
            .filter_map(|node| match node.data {
                NodeKind::Levels { parameters } => Some(parameters),
                _ => None,
            })
            .collect()
    }
    pub fn has_levels(&self) -> bool {
        self.nodes
            .iter()
            .any(|n| matches!(n.data, NodeKind::Levels { .. }))
    }
    pub fn from_layers(layers: &[super::Layer]) -> Self {
        let mut graph = Self {
            enabled: true,
            ..Self::default()
        };
        let mut last = 0;
        for (index, layer) in layers.iter().enumerate() {
            let id = graph.nodes.len() as u32;
            graph.nodes.push(Node {
                node_type: Some("compositing.layer".into()),
                id,
                position: [index as f32 * 240., if index == 0 { 20. } else { 260. }],
                data: NodeKind::Layer { index },
            });
            if index == 0 {
                last = id;
            } else {
                let blend = graph.nodes.len() as u32;
                graph.nodes.push(Node {
                    node_type: Some("compositing.blend".into()),
                    id: blend,
                    position: [index as f32 * 240., 0.],
                    data: NodeKind::Blend {
                        mode: layer.mode,
                        opacity: 1.,
                    },
                });
                graph.edges.extend([
                    Edge {
                        from: last,
                        to: blend,
                        input: 0,
                    },
                    Edge {
                        from: id,
                        to: blend,
                        input: 1,
                    },
                ]);
                last = blend;
            }
        }
        let id = graph.nodes.len() as u32;
        graph.nodes.push(Node {
            node_type: Some("compositing.output".into()),
            id,
            position: [layers.len() as f32 * 240., 0.],
            data: NodeKind::Output,
        });
        graph.edges.push(Edge {
            from: last,
            to: id,
            input: 0,
        });
        graph
    }
    pub fn compile(&self, layer_count: usize) -> Result<Vec<abi::CompositeNode>, String> {
        self.compile_with_masks(layer_count, &[])
    }
    /// Stable IDs resolve to the supplied descriptor order only at cold compilation.
    /// The editable linked Layer Filters model does not contain generated mask gates.
    pub fn compile_with_masks(
        &self,
        layer_count: usize,
        mask_ids: &[u64],
    ) -> Result<Vec<abi::CompositeNode>, String> {
        if mask_ids.is_empty()
            && self.nodes.iter().any(|n| {
                matches!(
                    n.data,
                    NodeKind::MaskSource { .. }
                        | NodeKind::ApplyMask { .. }
                        | NodeKind::MaskedMix { .. }
                )
            })
        {
            return Err("Mask nodes require a document mask descriptor context".into());
        }
        if mask_ids.len() > super::raster_masks::MAX_MASKS
            || mask_ids
                .iter()
                .enumerate()
                .any(|(i, id)| *id == 0 || *id > u32::MAX as u64 || mask_ids[..i].contains(id))
        {
            return Err("Invalid mask descriptor identities".into());
        }
        let edge_limit = if self
            .nodes
            .iter()
            .any(|n| matches!(n.data, NodeKind::MaskedMix { .. }))
        {
            MAX_NODES * 3
        } else {
            MAX_NODES * 2
        };
        if self.nodes.len() > MAX_NODES || self.edges.len() > edge_limit {
            return Err("Node or edge limit exceeded".into());
        }
        if self.nodes.is_empty() {
            return if self.enabled || self.linked {
                Err("An enabled image graph requires an output node".into())
            } else {
                Ok(vec![])
            };
        }
        if self
            .nodes
            .iter()
            .map(|n| match n.data {
                NodeKind::Levels { parameters } => parameters.table_count(),
                NodeKind::Curves { .. } => 3,
                _ => 0,
            })
            .sum::<usize>()
            > 64
        {
            return Err("Color table limit exceeded".into());
        }
        let mut ids = BTreeMap::new();
        let mut output = None;
        for (i, n) in self.nodes.iter().enumerate() {
            if ids.insert(n.id, i).is_some()
                || n.position.iter().any(|v| !v.is_finite() || v.abs() > 1e6)
            {
                return Err("Duplicate node ID or invalid position".into());
            }
            match n.data {
                NodeKind::Levels { parameters } => parameters.validate()?,
                NodeKind::Curves { ref curves } => curves.validate()?,
                NodeKind::Hsl { hsl } => hsl.validate()?,
                NodeKind::Layer { index } if index >= layer_count => {
                    return Err("Node references a missing layer".into())
                }
                NodeKind::MaskSource { mask_id } if !mask_ids.contains(&mask_id) => {
                    return Err("Node references a missing raster mask".into())
                }
                NodeKind::Filter { filter, amount }
                    if !amount.is_finite() || !(filter.minimum()..=1.).contains(&amount) =>
                {
                    return Err("Invalid filter parameters".into())
                }
                NodeKind::Blend { opacity, .. }
                | NodeKind::Mix { opacity }
                | NodeKind::ApplyMask { opacity }
                | NodeKind::MaskedMix { opacity }
                    if !opacity.is_finite() || !(0. ..=1.).contains(&opacity) =>
                {
                    return Err("Invalid blend opacity".into())
                }
                NodeKind::Output => {
                    if output.replace(i).is_some() {
                        return Err("Only one canvas output is allowed".into());
                    }
                }
                _ => {}
            }
        }
        let output = output.ok_or("Missing canvas output")?;
        let mut inputs = vec![[None; 3]; self.nodes.len()];
        for e in &self.edges {
            let from = *ids.get(&e.from).ok_or("Edge source does not exist")?;
            let to = *ids.get(&e.to).ok_or("Edge destination does not exist")?;
            if matches!(self.nodes[from].data, NodeKind::Output)
                || e.input as usize >= self.nodes[to].data.inputs()
                || inputs[to][e.input as usize].replace(from).is_some()
            {
                return Err("Invalid edge port or multiple outputs connected to one input".into());
            }
            if self.nodes[to].data.input_domain(e.input as usize)
                != Some(self.nodes[from].data.output_domain())
            {
                return Err("Image and mask coverage ports are not interchangeable".into());
            }
        }
        fn visit(
            i: usize,
            nodes: &[Node],
            inputs: &[[Option<usize>; 3]],
            visiting: &mut BTreeSet<usize>,
            done: &mut BTreeSet<usize>,
            order: &mut Vec<usize>,
            required: bool,
        ) -> Result<(), String> {
            if done.contains(&i) {
                return Ok(());
            }
            if !visiting.insert(i) {
                return Err("The graph contains a cycle".into());
            }
            for port in 0..nodes[i].data.inputs() {
                match inputs[i][port] {
                    Some(j) => visit(j, nodes, inputs, visiting, done, order, required)?,
                    None if required => {
                        return Err(format!(
                            "Input of {} is not connected",
                            nodes[i].data.title()
                        ))
                    }
                    None => {}
                }
            }
            visiting.remove(&i);
            done.insert(i);
            order.push(i);
            Ok(())
        }
        let mut order = vec![];
        let mut done = BTreeSet::new();
        let mut visiting = BTreeSet::new();
        visit(
            output,
            &self.nodes,
            &inputs,
            &mut visiting,
            &mut done,
            &mut order,
            true,
        )?;
        // Disconnected drafts may remain, but cycles anywhere are rejected.
        for i in 0..self.nodes.len() {
            visit(
                i,
                &self.nodes,
                &inputs,
                &mut visiting,
                &mut done,
                &mut vec![],
                false,
            )?;
        }
        let slots: BTreeMap<_, _> = order
            .iter()
            .enumerate()
            .map(|(slot, id)| (*id, slot as u32))
            .collect();
        if self.linked {
            self.stack_branches(layer_count)?;
        }
        Ok(order
            .iter()
            .map(|&i| {
                let input = |port: usize| slots[&inputs[i][port].unwrap()];
                match self.nodes[i].data {
                    NodeKind::Transparent => abi::CompositeNode {
                        kind: 4,
                        a: 0,
                        b: 0,
                        mode: 0,
                        amount: 0.,
                    },
                    NodeKind::Layer { index } => abi::CompositeNode {
                        kind: 0,
                        a: index as u32,
                        b: 0,
                        mode: 0,
                        amount: 1.,
                    },
                    NodeKind::Filter { filter, amount } => abi::CompositeNode {
                        kind: 1,
                        a: input(0),
                        b: 0,
                        mode: filter as u32,
                        amount,
                    },
                    NodeKind::Levels { parameters } => abi::CompositeNode {
                        kind: if parameters.channels.is_identity() {
                            9
                        } else {
                            10
                        },
                        a: input(0),
                        b: 0,
                        mode: self.table_index(i),
                        amount: 1.,
                    },
                    NodeKind::Curves { .. } => abi::CompositeNode {
                        kind: 10,
                        a: input(0),
                        b: 0,
                        mode: self.table_index(i),
                        amount: 1.,
                    },
                    NodeKind::Hsl { .. } => abi::CompositeNode {
                        kind: 11,
                        a: input(0),
                        b: 0,
                        mode: self.nodes[..i]
                            .iter()
                            .filter(|n| matches!(n.data, NodeKind::Hsl { .. }))
                            .count() as u32,
                        amount: 1.,
                    },
                    NodeKind::Blend { mode, opacity } => abi::CompositeNode {
                        kind: 2,
                        a: input(0),
                        b: input(1),
                        mode: mode as u32,
                        amount: opacity,
                    },
                    NodeKind::Mix { opacity } => abi::CompositeNode {
                        kind: 5,
                        a: input(0),
                        b: input(1),
                        mode: 0,
                        amount: opacity,
                    },
                    NodeKind::MaskSource { mask_id } => abi::CompositeNode {
                        kind: 6,
                        a: mask_ids.iter().position(|id| *id == mask_id).unwrap() as u32,
                        b: 0,
                        mode: 0,
                        amount: 0.,
                    },
                    NodeKind::ApplyMask { opacity } => abi::CompositeNode {
                        kind: 7,
                        a: input(0),
                        b: input(1),
                        mode: 0,
                        amount: opacity,
                    },
                    NodeKind::MaskedMix { opacity } => abi::CompositeNode {
                        kind: 8,
                        a: input(0),
                        b: input(1),
                        mode: input(2),
                        amount: opacity,
                    },
                    NodeKind::Output => abi::CompositeNode {
                        kind: 3,
                        a: input(0),
                        b: 0,
                        mode: 0,
                        amount: 1.,
                    },
                }
            })
            .collect())
    }
}
