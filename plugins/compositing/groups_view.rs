//! Read-only view of the backend's actual nested composition graph.
use super::*;
use serde_json::json;
use std::hash::Hasher;
#[derive(Default)]
pub(super) struct EffectiveView {
    fingerprint: Option<u64>,
    source: Option<Graph>,
    snarl: Snarl<Node>,
    fit: bool,
    error: String,
    names: BTreeMap<u32, Gate>,
    renderer: bool,
    mix_available: bool,
}
#[derive(Clone)]
struct Gate {
    name: String,
    visible: bool,
    opacity: f32,
    compositing: crate::engine::groups::GroupPolicy,
}
fn fingerprint(editor: &Editor) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    let doc = &editor.session.doc;
    hash.write_u64(doc.identity);
    hash.write_u64(doc.groups_revision());
    for (i, layer) in doc.layers.iter().enumerate() {
        hash.write_u64(doc.layer_id(i));
        hash.write_u8(layer.visible as u8);
        hash.write_u8(layer.opacity);
        hash.write_u8(layer.mode as u8);
    }
    hash.finish()
}
impl EffectiveView {
    fn refresh(&mut self, editor: &mut Editor) {
        let key = fingerprint(editor);
        if self.fingerprint == Some(key) && self.source.as_ref() == Some(&editor.session.doc.graph)
        {
            return;
        }
        self.fingerprint = Some(key);
        self.source = Some(editor.session.doc.graph.clone());
        let result = editor
            .session
            .apply(crate::application::Action::new("graph.read", json!({})))
            .map_err(|error| error.message)
            .and_then(|response| {
                let state = &response["result"];
                let graph = serde_json::from_value::<Graph>(state["effective_graph"].clone())
                    .map_err(|error| error.to_string())?;
                let names = state["groups"]["groups"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|group| {
                        Some((
                            group["node_id"].as_u64()? as u32,
                            Gate {
                                name: group["name"].as_str()?.to_owned(),
                                visible: group["visible"].as_bool()?,
                                opacity: group["opacity"].as_f64()? as f32,
                                compositing: serde_json::from_value(group["compositing"].clone())
                                    .unwrap_or_default(),
                            },
                        ))
                    })
                    .collect();
                Ok((
                    graph,
                    names,
                    state["renderer_transparent"].as_bool().unwrap_or(false),
                    state["renderer_mix"].as_bool().unwrap_or(false),
                ))
            });
        match result {
            Ok((graph, names, renderer, mix_available)) => {
                self.names = names;
                self.renderer = renderer;
                self.mix_available = mix_available;
                let previous: BTreeMap<_, _> = self
                    .snarl
                    .nodes_pos_ids()
                    .map(|(_, position, node)| (node.id, (node.data.clone(), position)))
                    .collect();
                self.snarl = Snarl::new();
                let mut depth: BTreeMap<u32, usize> =
                    graph.nodes.iter().map(|node| (node.id, 0)).collect();
                for _ in 0..graph.nodes.len() {
                    let mut changed = false;
                    for edge in &graph.edges {
                        let next = depth[&edge.from] + 1;
                        if depth[&edge.to] < next {
                            depth.insert(edge.to, next);
                            changed = true;
                        }
                    }
                    if !changed {
                        break;
                    }
                }
                let mut rows = BTreeMap::<usize, usize>::new();
                let mut ids = BTreeMap::new();
                for mut node in graph.nodes {
                    let column = depth[&node.id];
                    let row = rows.entry(column).or_default();
                    let position = previous
                        .get(&node.id)
                        .filter(|(kind, _)| {
                            std::mem::discriminant(kind) == std::mem::discriminant(&node.data)
                        })
                        .map(|(_, position)| *position)
                        .unwrap_or_else(|| {
                            pos2(
                                column as f32 * editor.settings.ui.control_height * 10.,
                                *row as f32 * editor.settings.ui.control_height * 6.,
                            )
                        });
                    *row += 1;
                    node.position = [position.x, position.y];
                    ids.insert(node.id, self.snarl.insert_node(position, node));
                }
                for edge in graph.edges {
                    self.snarl.connect(
                        OutPinId {
                            node: ids[&edge.from],
                            output: 0,
                        },
                        InPinId {
                            node: ids[&edge.to],
                            input: edge.input as usize,
                        },
                    );
                }
                self.fit = previous.is_empty();
                self.error.clear();
            }
            Err(error) => {
                self.error = error;
                self.snarl = Snarl::new();
            }
        }
    }
    pub(super) fn show(&mut self, editor: &mut Editor, ui: &mut Ui) {
        self.refresh(editor);
        ui.horizontal_wrapped(|ui| {
            ui.strong(tr("Actual Composition"));
            if ui.button(tr("Fit Graph")).clicked() {
                self.fit = true;
            }
        });
        ui.weak(tr("Groups and layer filters are shown as they are composed. Edit layer filters in Layer Filters; edit groups in Layers."));
        if !self.renderer && !self.names.is_empty() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Group rendering requires a compatible compositor."),
            );
        }
        if !self.mix_available
            && self
                .names
                .values()
                .any(|group| group.compositing == crate::engine::groups::GroupPolicy::PassThrough)
        {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Pass Through requires a compatible compositor."),
            );
        }
        if let Some(error) = &editor.session.doc.render_error {
            ui.colored_label(ui.visuals().error_fg_color, localize(error));
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, localize(&self.error));
            return;
        }
        let mut viewer = EffectiveViewer {
            document: &editor.session.doc,
            names: &self.names,
            width: editor.settings.ui.font_size * 12.,
            viewport: ui.available_rect_before_wrap(),
            fit: self.fit,
        };
        let style = SnarlStyle {
            node_layout: Some(NodeLayout::sandwich()),
            crisp_magnified_text: Some(false),
            ..Default::default()
        };
        self.snarl.show(
            &mut viewer,
            &style,
            ("effective-composition", editor.session.doc.identity),
            ui,
        );
        self.fit = false;
    }
}
struct EffectiveViewer<'a> {
    document: &'a crate::engine::Document,
    names: &'a BTreeMap<u32, Gate>,
    width: f32,
    viewport: Rect,
    fit: bool,
}
#[allow(refining_impl_trait)]
impl SnarlViewer<Node> for EffectiveViewer<'_> {
    fn current_transform(
        &mut self,
        transform: &mut egui::emath::TSTransform,
        snarl: &mut Snarl<Node>,
    ) {
        if !self.fit {
            return;
        }
        let mut bounds = Rect::NOTHING;
        for (_, position, _) in snarl.nodes_pos_ids() {
            bounds = bounds.union(Rect::from_min_size(
                position,
                vec2(self.width + 24., self.width + 24.),
            ));
        }
        if bounds.is_finite() {
            let view = self.viewport.shrink(16.);
            let scaling = (view.width() / bounds.width())
                .min(view.height() / bounds.height())
                .clamp(0.05, 1.);
            *transform = egui::emath::TSTransform {
                scaling,
                translation: view.center().to_vec2() - bounds.center().to_vec2() * scaling,
            };
        }
    }
    fn title(&mut self, node: &Node) -> String {
        if let Some(group) = self.names.get(&node.id) {
            return group.name.clone();
        }
        if let NodeKind::Layer { index } = node.data {
            return self.document.layer_name(index);
        }
        if let NodeKind::MaskSource { mask_id } = node.data {
            return format!("{} #{mask_id}", localize("Mask"));
        }
        localize(node.data.title())
    }
    fn show_header(
        &mut self,
        id: NodeId,
        _: &[InPin],
        _: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<Node>,
    ) {
        let title = self.title(&snarl[id]);
        ui.add_sized(
            [self.width, ui.text_style_height(&TextStyle::Body)],
            Label::new(&title).truncate(),
        )
        .on_hover_text(title);
    }
    fn inputs(&mut self, node: &Node) -> usize {
        node.data.inputs()
    }
    fn outputs(&mut self, node: &Node) -> usize {
        usize::from(!matches!(node.data, NodeKind::Output))
    }
    fn show_input(&mut self, pin: &InPin, ui: &mut Ui, snarl: &mut Snarl<Node>) -> PinInfo {
        ui.label(tr(
            if matches!(snarl[pin.id.node].data, NodeKind::MaskedMix { .. }) {
                match pin.id.input {
                    0 => "Before",
                    1 => "After",
                    _ => "Coverage",
                }
            } else if matches!(snarl[pin.id.node].data, NodeKind::ApplyMask { .. }) {
                if pin.id.input == 0 {
                    "Image"
                } else {
                    "Coverage"
                }
            } else if matches!(snarl[pin.id.node].data, NodeKind::Mix { .. }) {
                if pin.id.input == 0 {
                    "Before"
                } else {
                    "After"
                }
            } else if matches!(snarl[pin.id.node].data, NodeKind::Blend { .. }) {
                if pin.id.input == 0 {
                    "Background"
                } else {
                    "Foreground"
                }
            } else {
                "Image"
            },
        ));
        PinInfo::circle().with_fill(ui.visuals().selection.stroke.color)
    }
    fn show_output(&mut self, pin: &OutPin, ui: &mut Ui, snarl: &mut Snarl<Node>) -> PinInfo {
        ui.label(tr(
            if matches!(snarl[pin.id.node].data, NodeKind::MaskSource { .. }) {
                "Coverage"
            } else {
                "Image"
            },
        ));
        PinInfo::circle().with_fill(ui.visuals().selection.stroke.color)
    }
    fn has_body(&mut self, _: &Node) -> bool {
        true
    }
    fn show_body(
        &mut self,
        id: NodeId,
        _: &[InPin],
        _: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<Node>,
    ) {
        // Snarl supplies a horizontal body UI; report the full stacked height before outputs.
        ui.vertical(|ui| {
            ui.set_width(self.width);
            ui.style_mut().wrap_mode = Some(TextWrapMode::Wrap);
            if let Some(group) = self.names.get(&snarl[id].id) {
                ui.weak(tr(
                    if group.compositing == crate::engine::groups::GroupPolicy::PassThrough {
                        "Pass Through Group"
                    } else {
                        "Group Composite"
                    },
                ));
                ui.label(format!(
                    "{}: {:.0}%",
                    localize("Opacity"),
                    group.opacity * 100.
                ));
                if !group.visible {
                    ui.weak(tr("Hidden Group"));
                }
            }
            match snarl[id].data {
                NodeKind::Transparent => {
                    ui.weak(tr("Transparent base"));
                }
                NodeKind::Layer { index } => {
                    let layer = &self.document.layers[index];
                    ui.weak(tr("Layer Source"));
                    ui.label(format!(
                        "{} · {:.0}%",
                        localize(layer.mode.title()),
                        layer.opacity as f32 / 255. * 100.
                    ));
                    if !layer.visible {
                        ui.weak(tr("Hidden Layers"));
                    }
                }
                NodeKind::Filter { filter, amount } => {
                    ui.label(tr(filter.title()));
                    ui.label(format!("{amount:.2}"));
                }
                NodeKind::Blend { mode, opacity } => {
                    ui.label(tr(mode.title()));
                    if !self.names.contains_key(&snarl[id].id) {
                        ui.label(format!("{:.0}%", opacity * 100.));
                    }
                }
                NodeKind::Mix { opacity } => {
                    ui.weak(tr("Backdrop Mix"));
                    if !self.names.contains_key(&snarl[id].id) {
                        ui.label(format!("{:.0}%", opacity * 100.));
                    }
                }
                NodeKind::MaskSource { .. } => {
                    ui.weak(tr("Raw A8 Coverage"));
                }
                NodeKind::ApplyMask { opacity } | NodeKind::MaskedMix { opacity } => {
                    if !self.names.contains_key(&snarl[id].id) {
                        ui.label(format!("{}: {:.0}%", localize("Opacity"), opacity * 100.));
                    }
                }
                NodeKind::Output => {
                    ui.weak(tr("Canvas Output"));
                }
            }
        });
    }
    fn connect(&mut self, _: &OutPin, _: &InPin, _: &mut Snarl<Node>) {}
    fn disconnect(&mut self, _: &OutPin, _: &InPin, _: &mut Snarl<Node>) {}
    fn drop_inputs(&mut self, _: &InPin, _: &mut Snarl<Node>) {}
    fn drop_outputs(&mut self, _: &OutPin, _: &mut Snarl<Node>) {}
    fn has_graph_menu(&mut self, _: Pos2, _: &mut Snarl<Node>) -> bool {
        false
    }
    fn has_node_menu(&mut self, _: &Node) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::studio;
    fn ungrouped_fixture() -> Studio {
        let mut app = studio();
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(app
            .editor
            .core("canvas.new", json!({"width":12,"height":8})));
        assert!(app.editor.core("layer.add", json!({})));
        app
    }
    fn fixture() -> (Studio, u64) {
        let mut app = ungrouped_fixture();
        let ids = app.editor.session.doc.selected_layer_ids();
        let group = app
            .editor
            .session
            .apply(crate::application::Action::new(
                "group.create",
                json!({"members":[{"kind":"layer","id":ids[0]}],"name":"Character"}),
            ))
            .unwrap()["result"]["group_id"]
            .as_u64()
            .unwrap();
        (app, group)
    }
    #[test]
    fn grouped_nodes_default_to_actual_readonly_topology_and_keep_source_graph_and_pixels() {
        let (mut app, _) = fixture();
        let ctx = Context::default();
        theme::fonts(&ctx);
        theme::apply(&ctx, &app.editor.settings.ui);
        let state = app.editor.session.state();
        let pixels = app
            .editor
            .session
            .doc
            .layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect::<Vec<_>>();
        let history = app.editor.session.doc.history_bytes();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        for n in 0..4 {
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 800.))),
                    time: Some(n as f64),
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| app.editor.nodes_ui(ui));
                },
            );
        }
        assert!(!app.editor.nodes.layer_filters);
        let effective = app.editor.session.doc.effective_graph().unwrap();
        assert_eq!(
            app.editor.nodes.effective.snarl.nodes().count(),
            effective.nodes.len()
        );
        assert_eq!(
            app.editor.nodes.effective.snarl.wires().count(),
            effective.edges.len()
        );
        assert!(app
            .editor
            .nodes
            .effective
            .snarl
            .nodes()
            .any(|node| matches!(node.data, NodeKind::Transparent)));
        for node in effective.nodes {
            assert!(app
                .editor
                .nodes
                .effective
                .snarl
                .nodes()
                .any(|shown| shown.id == node.id && shown.data == node.data));
        }
        assert!(app
            .editor
            .nodes
            .effective
            .names
            .values()
            .any(|group| group.name == "Character"));
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
        assert_eq!(
            app.editor
                .session
                .doc
                .layers
                .iter()
                .map(|layer| layer.pixels.clone())
                .collect::<Vec<_>>(),
            pixels
        );
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            events
        );
    }
    #[test]
    fn composition_group_titles_body_rows_and_output_pins_do_not_overlap() {
        fn text_rect(shape: &Shape, value: &str) -> Option<Rect> {
            match shape {
                Shape::Text(text) if text.galley.text() == value => {
                    Some(Rect::from_min_size(text.pos, text.galley.size()))
                }
                Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_rect(shape, value)),
                _ => None,
            }
        }
        let old = crate::i18n::current();
        let (mut app, group) = fixture();
        assert!(app.editor.core(
            "group.set",
            json!({"group_id":group,"visible":false,"opacity":0.5})
        ));
        let mut view = EffectiveView::default();
        view.refresh(&mut app.editor);
        let gate = *view.names.keys().next().unwrap();
        let node = view
            .snarl
            .nodes()
            .find(|node| node.id == gate)
            .unwrap()
            .clone();
        let long_name = "Character colors / 色彩 / キャラクター / 캐릭터".repeat(8);
        view.names.get_mut(&gate).unwrap().name = long_name.clone();
        let state = app.editor.session.state();
        for language in crate::i18n::Language::ALL {
            crate::i18n::set_language(language);
            for font_size in [12., 20.] {
                let ctx = Context::default();
                let mut appearance = app.editor.settings.ui.clone();
                appearance.font_size = font_size;
                theme::fonts(&ctx);
                theme::apply(&ctx, &appearance);
                let width = font_size * 12.;
                let mut snarl = Snarl::new();
                snarl.insert_node(pos2(40., 40.), node.clone());
                let mut output = None;
                for frame in 0..5 {
                    output = Some(ctx.run(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 900.))),
                            time: Some(frame as f64),
                            ..Default::default()
                        },
                        |ctx| {
                            CentralPanel::default().show(ctx, |ui| {
                                let mut viewer = EffectiveViewer {
                                    document: &app.editor.session.doc,
                                    names: &view.names,
                                    width,
                                    viewport: ui.available_rect_before_wrap(),
                                    fit: false,
                                };
                                snarl.show(
                                    &mut viewer,
                                    &SnarlStyle {
                                        node_layout: Some(NodeLayout::sandwich()),
                                        crisp_magnified_text: Some(false),
                                        // Compare painted screen rects to graph-space width at 1:1.
                                        // Snarl otherwise auto-fits this single node up to 2x.
                                        min_scale: Some(1.),
                                        max_scale: Some(1.),
                                        ..Default::default()
                                    },
                                    "group-node-layout",
                                    ui,
                                );
                            });
                        },
                    ));
                }
                let output = output.unwrap();
                let labels = [
                    long_name.clone(),
                    crate::i18n::text_in(language, "Background").into_owned(),
                    crate::i18n::text_in(language, "Foreground").into_owned(),
                    crate::i18n::text_in(language, "Group Composite").into_owned(),
                    format!("{}: 50%", crate::i18n::text_in(language, "Opacity")),
                    crate::i18n::text_in(language, "Hidden Group").into_owned(),
                    crate::i18n::text_in(language, "Normal").into_owned(),
                    crate::i18n::text_in(language, "Image").into_owned(),
                ];
                let rects = labels
                    .iter()
                    .map(|label| {
                        output
                            .shapes
                            .iter()
                            .find_map(|shape| text_rect(&shape.shape, label))
                            .unwrap_or_else(|| panic!("{language:?}/{font_size}: missing {label}"))
                    })
                    .collect::<Vec<_>>();
                for (i, rect) in rects.iter().enumerate() {
                    assert!(
                        rect.width() <= width + 1.,
                        "{language:?}/{font_size}: {} exceeds node width: {rect:?}",
                        labels[i]
                    );
                    if let Some(next) = rects.get(i + 1) {
                        assert!(
                            rect.bottom() <= next.top(),
                            "{language:?}/{font_size}: {} overlaps {}: {rect:?}, {next:?}",
                            labels[i],
                            labels[i + 1]
                        );
                    }
                }
            }
        }
        assert_eq!(app.editor.session.state(), state);
        crate::i18n::set_language(old);
    }
    #[test]
    fn group_tree_changes_block_unapplied_layer_filter_drafts_without_discarding_them() {
        let (mut app, group) = fixture();
        let mut draft = GraphEditor::default();
        draft.load(&app.editor.session.doc, false, &app.editor.plugin_nodes);
        draft.enabled = !draft.enabled;
        let pending = draft.graph();
        assert!(app
            .editor
            .core("group.set", json!({"group_id":group,"opacity":0.5})));
        let applied = app.editor.session.doc.graph.clone();
        draft.apply(&mut app.editor);
        assert_eq!(app.editor.session.doc.graph, applied);
        assert_eq!(draft.graph(), pending);
        assert_eq!(
            draft.message,
            "Graph or Groups Changed Externally; Use Load Current"
        );
    }
    #[test]
    fn pass_through_actual_composition_shows_real_mix_gate_and_before_after_ports() {
        let (mut app, group) = fixture();
        assert!(app.editor.core(
            "group.set",
            json!({"group_id":group,"compositing":"pass_through","opacity":0.5})
        ));
        let ctx = Context::default();
        theme::fonts(&ctx);
        theme::apply(&ctx, &app.editor.settings.ui);
        let state = app.editor.session.state();
        let history = app.editor.session.doc.history_bytes();
        let mut output = None;
        for n in 0..4 {
            output = Some(ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1300., 900.))),
                    time: Some(n as f64),
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| app.editor.nodes_ui(ui));
                },
            ));
        }
        let effective = app.editor.session.doc.effective_graph().unwrap();
        assert_eq!(
            app.editor.nodes.effective.snarl.nodes().count(),
            effective.nodes.len()
        );
        assert_eq!(
            app.editor.nodes.effective.snarl.wires().count(),
            effective.edges.len()
        );
        let gate = app
            .editor
            .nodes
            .effective
            .names
            .iter()
            .find(|(_, gate)| gate.name == "Character")
            .unwrap();
        assert_eq!(
            gate.1.compositing,
            crate::engine::groups::GroupPolicy::PassThrough
        );
        assert!(app
            .editor
            .nodes
            .effective
            .snarl
            .nodes()
            .any(|node| node.id == *gate.0
                && matches!(node.data,NodeKind::Mix {opacity} if opacity>0.49 && opacity<0.51)));
        fn has(shape: &Shape, title: &str) -> bool {
            match shape {
                Shape::Text(value) => value.galley.text() == title,
                Shape::Vec(values) => values.iter().any(|value| has(value, title)),
                _ => false,
            }
        }
        let output = output.unwrap();
        for title in ["Before", "After", "Pass Through Group", "Backdrop Mix"] {
            assert!(
                output.shapes.iter().any(|shape| has(&shape.shape, title)),
                "{title}"
            );
        }
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
    }
    #[test]
    fn generated_mask_gates_keep_typed_coverage_wires_and_refresh_without_editing() {
        use crate::engine::raster_masks::MaskOwner;
        let (mut app, group) = fixture();
        assert!(app.editor.core(
            "group.set",
            json!({"group_id":group,"compositing":"pass_through","opacity":0.5})
        ));
        let layer_id = app.editor.session.doc.layer_id(1);
        let layer_mask = app
            .editor
            .session
            .doc
            .create_mask(MaskOwner::Layer { id: layer_id }, 192)
            .unwrap();
        let group_mask = app
            .editor
            .session
            .doc
            .create_mask(MaskOwner::Group { id: group }, 128)
            .unwrap();
        let state = app.editor.session.state();
        let source = app.editor.session.doc.graph.clone();
        let history = app.editor.session.doc.history_bytes();
        let mut view = EffectiveView::default();
        view.refresh(&mut app.editor);
        assert!(view.error.is_empty(), "{}", view.error);
        let effective = app.editor.session.doc.effective_graph().unwrap();
        assert_eq!(view.snarl.nodes().count(), effective.nodes.len());
        assert_eq!(view.snarl.wires().count(), effective.edges.len());
        let ids: BTreeMap<_, _> = view
            .snarl
            .node_ids()
            .map(|(id, node)| (node.id, id))
            .collect();
        let mut viewer = EffectiveViewer {
            document: &app.editor.session.doc,
            names: &view.names,
            width: 160.,
            viewport: Rect::EVERYTHING,
            fit: false,
        };
        for node in &effective.nodes {
            assert_eq!(viewer.inputs(node), node.data.inputs());
            assert_eq!(
                viewer.outputs(node),
                usize::from(!matches!(node.data, NodeKind::Output))
            );
            if let NodeKind::MaskSource { mask_id } = node.data {
                assert_eq!(
                    viewer.title(node),
                    format!("{} #{mask_id}", localize("Mask"))
                );
            }
        }
        for (kind, input) in [
            (NodeKind::ApplyMask { opacity: 1. }, 1),
            (NodeKind::MaskedMix { opacity: 1. }, 2),
        ] {
            let target = effective
                .nodes
                .iter()
                .find(|node| std::mem::discriminant(&node.data) == std::mem::discriminant(&kind))
                .unwrap();
            let edge = effective
                .edges
                .iter()
                .find(|edge| edge.to == target.id && edge.input == input)
                .unwrap();
            assert!(view.snarl.wires().any(|(from, to)| {
                from.node == ids[&edge.from]
                    && to.node == ids[&target.id]
                    && to.input == input as usize
            }));
            assert!(matches!(
                effective
                    .nodes
                    .iter()
                    .find(|node| node.id == edge.from)
                    .unwrap()
                    .data,
                NodeKind::MaskSource { .. }
            ));
        }
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(app.editor.session.doc.graph, source);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
        for id in [layer_mask, group_mask] {
            let mut metadata = app
                .editor
                .session
                .doc
                .mask_state()
                .items
                .iter()
                .find(|mask| mask.id == id)
                .unwrap()
                .clone();
            metadata.enabled = false;
            app.editor
                .session
                .doc
                .update_mask_metadata(metadata)
                .unwrap();
        }
        view.refresh(&mut app.editor);
        assert!(view.error.is_empty(), "{}", view.error);
        assert!(!view.snarl.nodes().any(|node| matches!(
            node.data,
            NodeKind::MaskSource { .. } | NodeKind::ApplyMask { .. } | NodeKind::MaskedMix { .. }
        )));
        assert!(view
            .snarl
            .nodes()
            .any(|node| matches!(node.data, NodeKind::Mix { .. })));
        assert_eq!(
            app.editor
                .session
                .doc
                .mask_plane(layer_mask)
                .unwrap()
                .sample(0),
            192
        );
        assert_eq!(
            app.editor
                .session
                .doc
                .mask_plane(group_mask)
                .unwrap()
                .sample(0),
            128
        );
    }
    #[test]
    fn flat_masks_open_actual_composition_without_replacing_the_editable_source() {
        let mut app = ungrouped_fixture();
        app.editor
            .session
            .doc
            .create_mask(crate::engine::raster_masks::MaskOwner::Layer { id: 1 }, 128)
            .unwrap();
        assert!(!app.editor.session.doc.has_groups());
        let state = app.editor.session.state();
        let source = app.editor.session.doc.graph.clone();
        let history = app.editor.session.doc.history_bytes();
        let ctx = Context::default();
        theme::fonts(&ctx);
        theme::apply(&ctx, &app.editor.settings.ui);
        for frame in 0..4 {
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 800.))),
                    time: Some(frame as f64),
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| app.editor.nodes_ui(ui));
                },
            );
        }
        assert!(!app.editor.nodes.layer_filters);
        let shown = &app.editor.nodes.effective.snarl;
        assert!(shown
            .nodes()
            .any(|node| matches!(node.data, NodeKind::MaskSource { mask_id: 1 })));
        assert!(shown
            .nodes()
            .any(|node| matches!(node.data, NodeKind::ApplyMask { .. })));
        assert!(!shown
            .nodes()
            .any(|node| matches!(node.data, NodeKind::Transparent)));
        let effective = app.editor.session.doc.effective_graph().unwrap();
        assert_eq!(shown.nodes().count(), effective.nodes.len());
        assert_eq!(shown.wires().count(), effective.edges.len());
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(app.editor.session.doc.graph, source);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
    }
}
