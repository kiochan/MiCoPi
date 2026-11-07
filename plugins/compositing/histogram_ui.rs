//! Explicit observations share the registered query; idle frames never sample.
use super::*;
use crate::application::{Action, ActionError, Session};
use serde::Deserialize;
use std::sync::Arc;
#[path = "histogram_nodes_ui.rs"]
mod nodes;
const STATE_KEY: &str = "micopi.histogram-ui.v1";
#[derive(Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Source {
    #[default]
    Composite,
    Layer,
    Node,
}
#[derive(Clone)]
pub(crate) struct Snapshot {
    document: u64,
    revision: u64,
    source: Source,
    layer_id: Option<u64>,
    node: Option<nodes::Observation>,
    budget: u32,
    sampled: u64,
    total: u64,
    nontransparent: u64,
    exact: bool,
    bins: [[u64; 256]; 4],
}
#[derive(Deserialize)]
struct Wire {
    document_identity: u64,
    revision: u64,
    source: Source,
    layer_id: Option<u64>,
    #[serde(flatten)]
    node: nodes::Wire,
    max_samples: u32,
    sampled_pixels: u64,
    total_pixels: u64,
    nontransparent_pixels: u64,
    exact: bool,
    red: Vec<u64>,
    green: Vec<u64>,
    blue: Vec<u64>,
    rgb: Vec<u64>,
}
impl Snapshot {
    fn decode(value: serde_json::Value) -> Result<Self, ActionError> {
        let w: Wire = serde_json::from_value(value)?;
        if w.document_identity == 0
            || !(1..=65536).contains(&w.max_samples)
            || w.sampled_pixels == 0
            || w.sampled_pixels > u64::from(w.max_samples)
            || w.total_pixels < w.sampled_pixels
            || w.nontransparent_pixels > w.sampled_pixels
            || w.exact != (w.sampled_pixels == w.total_pixels)
            || match w.source {
                Source::Layer => w.layer_id.is_none_or(|id| id == 0 || id > u32::MAX as u64),
                Source::Composite | Source::Node => w.layer_id.is_some(),
            }
        {
            return Err(ActionError::new(
                "histogram_result",
                "Invalid histogram snapshot metadata",
            ));
        }
        let node = if w.source == Source::Node {
            Some(w.node.decode()?)
        } else {
            if w.node.present() {
                return Err(ActionError::new(
                    "histogram_result",
                    "Unexpected node scope metadata",
                ));
            }
            None
        };
        let mut bins = [[0; 256]; 4];
        for (target, source) in bins.iter_mut().zip([w.red, w.green, w.blue, w.rgb]) {
            *target = source.try_into().map_err(|_| {
                ActionError::new(
                    "histogram_result",
                    "Histogram requires 256 bins per channel",
                )
            })?;
        }
        Ok(Self {
            document: w.document_identity,
            revision: w.revision,
            source: w.source,
            layer_id: w.layer_id,
            node,
            budget: w.max_samples,
            sampled: w.sampled_pixels,
            total: w.total_pixels,
            nontransparent: w.nontransparent_pixels,
            exact: w.exact,
            bins,
        })
    }
}
#[derive(Clone)]
struct State {
    document: u64,
    source: Source,
    budget: u32,
    nodes: nodes::Model,
    snapshot: Option<Arc<Snapshot>>,
    error: String,
}
impl Default for State {
    fn default() -> Self {
        Self {
            document: 0,
            source: Source::Composite,
            budget: 65536,
            nodes: nodes::Model::default(),
            snapshot: None,
            error: String::new(),
        }
    }
}
fn text(message: &str) -> RichText {
    tr_for(Some("compositing"), message)
}
fn translated(message: &str) -> std::borrow::Cow<'_, str> {
    crate::i18n::translated_for(Some("compositing"), message)
}
fn active(s: &Session) -> bool {
    s.stroke.active() || s.pointer_checkpoint.is_some()
}
impl State {
    fn attach(&mut self, document: u64) {
        if self.document != document {
            self.document = document;
            self.snapshot = None;
            self.nodes.reset();
            self.error.clear();
        }
    }
    fn current(&self, s: &Session) -> bool {
        !active(s)
            && self.document == s.doc.identity
            && self.snapshot.as_ref().is_some_and(|p| {
                p.document == s.doc.identity
                    && p.revision == s.revision
                    && p.source == self.source
                    && p.budget == self.budget
                    && match p.source {
                        Source::Composite => true,
                        Source::Layer => p.layer_id == Some(s.doc.layer_id(s.doc.active)),
                        Source::Node => p
                            .node
                            .is_some_and(|o| Some(o.request) == self.nodes.request(s)),
                    }
            })
    }
    fn refresh(&mut self, s: &mut Session) {
        self.attach(s.doc.identity);
        if self.source == Source::Node {
            self.nodes.refresh_choices(s);
        }
        let expected_node = if self.source == Source::Node {
            self.nodes.request(s)
        } else {
            None
        };
        if self.source == Source::Node && expected_node.is_none() {
            return;
        }
        let document = s.doc.identity;
        let revision = s.revision;
        let source = self.source;
        let budget = self.budget;
        let expected_layer = (source == Source::Layer).then(|| s.doc.layer_id(s.doc.active));
        let mut args = serde_json::json!({"max_samples":self.budget,"source":match self.source {Source::Composite=>"composite",Source::Layer=>"layer",Source::Node=>"node"}});
        if let Some(request) = expected_node {
            nodes::Model::inject(request, s, &mut args);
        }

        if self.source == Source::Layer {
            args["layer_id"] = serde_json::json!(s.doc.layer_id(s.doc.active));
        }
        let result = s
            .apply(Action::new("histogram.query", args))
            .and_then(|mut response| {
                Snapshot::decode(
                    response
                        .as_object_mut()
                        .and_then(|o| o.remove("result"))
                        .ok_or_else(|| {
                            ActionError::new("histogram_result", "Missing histogram result")
                        })?,
                )
            });
        match result {
            Ok(snapshot)
                if snapshot.document == document
                    && snapshot.revision == revision
                    && snapshot.source == source
                    && snapshot.budget == budget
                    && snapshot.layer_id == expected_layer
                    && snapshot.node.map(|o| o.request) == expected_node =>
            {
                self.snapshot = Some(Arc::new(snapshot));
                self.error.clear();
            }
            Ok(_) => {
                self.snapshot = None;
                self.error = "Histogram response does not match the selected scope".into();
            }
            Err(error) => {
                self.snapshot = None;
                self.error = error.to_string();
            }
        }
    }
}
pub(crate) fn cached(ctx: &Context, s: &Session) -> Option<Arc<Snapshot>> {
    ctx.data_mut(|d| d.get_temp::<State>(Id::new(STATE_KEY)))
        .filter(|state| state.current(s))
        .and_then(|state| state.snapshot)
}
pub(crate) fn caption(snapshot: &Snapshot) -> String {
    match snapshot.source {
        Source::Composite => translated("Visible Composite Histogram").into_owned(),
        Source::Layer => format!(
            "{} #{}",
            translated("Raw Layer Histogram"),
            snapshot.layer_id.unwrap_or(0)
        ),
        Source::Node => snapshot.node.map_or_else(
            || translated("Histogram Unavailable").into_owned(),
            nodes::Observation::caption,
        ),
    }
}
/// Presentation-only grouping/normalization of native bins, never pixel reads.
pub(crate) fn paint(ui: &Ui, rect: Rect, snapshot: &Snapshot, channel: usize) {
    let Some(bins) = snapshot.bins.get(channel) else {
        return;
    };
    let peak = bins.iter().copied().max().unwrap_or(0).max(1) as f32;
    let bars = (rect.width().ceil() as usize).clamp(1, 256);
    let mut mesh = egui::epaint::Mesh::default();
    let color = ui.visuals().weak_text_color().gamma_multiply(0.20);
    for bar in 0..bars {
        let start = bar * 256 / bars;
        let end = (bar + 1) * 256 / bars;
        let value = bins[start..end].iter().copied().max().unwrap_or(0) as f32;
        if value == 0. {
            continue;
        }
        mesh.add_colored_rect(
            Rect::from_min_max(
                pos2(
                    rect.left() + rect.width() * bar as f32 / bars as f32,
                    rect.bottom() - rect.height() * value / peak,
                ),
                pos2(
                    rect.left() + rect.width() * (bar + 1) as f32 / bars as f32,
                    rect.bottom(),
                ),
            ),
            color,
        );
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}
pub(crate) fn band(ui: &mut Ui, snapshot: &Snapshot, channel: usize) {
    label(ui, snapshot);
    let (rect, _) = ui.allocate_exact_size(
        vec2(
            ui.available_width().max(1.),
            ui.spacing().interact_size.y * 2.,
        ),
        Sense::hover(),
    );
    if ui.is_rect_visible(rect) {
        paint(ui, rect, snapshot, channel);
    }
}
pub(super) fn label_height(ui: &Ui, width: f32) -> f32 {
    let font = TextStyle::Body.resolve(ui.style());
    [
        "Source Input",
        "Source Output",
        "Actual Input",
        "Actual Output",
    ]
    .into_iter()
    .map(|title| {
        ui.painter()
            .layout(
                format!(
                    "{} · {} #{}",
                    translated("Sampled"),
                    translated(title),
                    u32::MAX
                ),
                font.clone(),
                Color32::WHITE,
                width.max(1.),
            )
            .size()
            .y
    })
    .fold(0., f32::max)
}
pub(crate) fn label(ui: &mut Ui, snapshot: &Snapshot) {
    let scope = match snapshot.source {
        Source::Composite => translated("Visible Composite").into_owned(),
        Source::Layer => format!(
            "{} #{}",
            translated("Raw Layer"),
            snapshot.layer_id.unwrap_or(0)
        ),
        Source::Node => snapshot.node.map_or_else(
            || translated("Histogram Unavailable").into_owned(),
            nodes::Observation::short,
        ),
    };
    let label = format!(
        "{} · {}",
        translated(if snapshot.exact { "Exact" } else { "Sampled" }),
        scope
    );
    let tooltip = format!(
        "{} · {}/{} · {} {}",
        caption(snapshot),
        snapshot.sampled,
        snapshot.total,
        translated("Revision"),
        snapshot.revision
    );
    let label = Label::new(RichText::new(label).color(ui.visuals().weak_text_color()));
    ui.add(if snapshot.source == Source::Node {
        label.wrap()
    } else {
        label.truncate()
    })
    .on_hover_text(tooltip);
}
#[cfg(test)]
pub(super) fn test_snapshot() -> Snapshot {
    let mut bins = [[0; 256]; 4];
    for channel in 0..3 {
        bins[channel][64] = 50000;
    }
    bins[3][64] = 150000;
    Snapshot {
        document: 1,
        revision: 1,
        source: Source::Layer,
        layer_id: Some(u32::MAX as u64),
        node: None,
        budget: 65536,
        sampled: 65536,
        total: 16777216,
        nontransparent: 50000,
        exact: false,
        bins,
    }
}
#[cfg(test)]
pub(super) fn test_node_snapshot() -> Snapshot {
    let mut snapshot = test_snapshot();
    snapshot.source = Source::Node;
    snapshot.layer_id = None;
    snapshot.node = Some(nodes::Observation {
        request: nodes::Request {
            view: nodes::View::Actual,
            node: u32::MAX,
            direction: nodes::Direction::Input,
            input: Some(0),
        },
        resolved: 0,
        enabled: true,
    });
    snapshot
}
fn body(ui: &mut Ui, s: &mut Session, state: &mut State) {
    ui.style_mut().wrap_mode = Some(TextWrapMode::Wrap);
    ui.label(text("Histogram Source"));
    let previous = state.source;
    let source_combo = ComboBox::from_id_salt(("histogram-source", s.doc.identity))
        .width(ui.available_width())
        .truncate()
        .selected_text(text(match state.source {
            Source::Composite => "Visible Composite",
            Source::Layer => "Raw Active Layer",
            Source::Node => "Published Graph Node",
        }))
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut state.source,
                Source::Composite,
                text("Visible Composite"),
            );
            ui.selectable_value(&mut state.source, Source::Layer, text("Raw Active Layer"));
            let node_option = ui.selectable_value(
                &mut state.source,
                Source::Node,
                text("Published Graph Node"),
            );
            #[cfg(test)]
            ui.data_mut(|d| {
                d.insert_temp(
                    Id::new(("histogram-source-node", s.doc.identity)),
                    node_option.rect,
                )
            });
            #[cfg(not(test))]
            let _ = node_option;
        });
    #[cfg(test)]
    ui.data_mut(|d| {
        d.insert_temp(
            Id::new(("histogram-source", s.doc.identity)),
            source_combo.response.rect,
        )
    });
    #[cfg(not(test))]
    let _ = source_combo;
    if previous != state.source {
        state.snapshot = None;
        state.error.clear();
    }
    if state.source == Source::Node && state.nodes.controls(ui, s) {
        state.snapshot = None;
        state.error.clear();
    }
    ui.label(text("Maximum Samples"));
    ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        DragValue::new(&mut state.budget).range(1..=65536),
    );
    let available = if state.source == Source::Node {
        s.doc
            .renderer
            .as_ref()
            .is_some_and(|r| r.supports_histogram_plan())
    } else {
        s.registry
            .native_plugins
            .get("compositing")
            .is_some_and(|p| p.supports_histogram())
    };
    let ready = state.source != Source::Node || state.nodes.request(s).is_some();
    let refresh = ui.add_enabled(
        !active(s) && available && ready,
        Button::new(text("Refresh Histogram")),
    );
    #[cfg(test)]
    ui.data_mut(|d| d.insert_temp(Id::new(("histogram-refresh", s.doc.identity)), refresh.rect));
    if refresh.clicked() {
        state.refresh(s);
    }
    if active(s) {
        ui.weak(text("Histogram refresh pauses while drawing."));
    } else if !available {
        ui.weak(text("Histogram Unavailable"));
    } else if state.snapshot.is_some() && !state.current(s) {
        ui.weak(text("Histogram changed. Refresh to update."));
    }
    if !state.error.is_empty() {
        ui.colored_label(ui.visuals().warn_fg_color, text("Histogram Unavailable"))
            .on_hover_text(&state.error);
    }
    if let Some(snapshot) = &state.snapshot {
        ui.add(Label::new(caption(snapshot)).wrap());
        ui.add(
            Label::new(format!(
                "{} · {}/{}",
                translated(if snapshot.exact {
                    "Exact Histogram"
                } else {
                    "Sampled Histogram"
                }),
                snapshot.sampled,
                snapshot.total
            ))
            .wrap(),
        )
        .on_hover_text(format!(
            "{}: {} · {}: {}",
            translated("Nontransparent Pixels"),
            snapshot.nontransparent,
            translated("Ignored Transparent Pixels"),
            snapshot.sampled - snapshot.nontransparent
        ));
        ui.weak(format!("{} {}", translated("Revision"), snapshot.revision));
        if state.current(s) {
            let (rect, _) = ui.allocate_exact_size(
                vec2(
                    ui.available_width().max(1.),
                    ui.spacing().interact_size.y * 3.,
                ),
                Sense::hover(),
            );
            if ui.is_rect_visible(rect) {
                paint(ui, rect, snapshot, 3);
            }
        }
    }
    ui.add(
        Label::new(text(
            "Snapshots use published data; local drafts are not sampled.",
        ))
        .wrap(),
    );
}
pub(crate) fn controls(ui: &mut Ui, s: &mut Session) {
    let key = Id::new(STATE_KEY);
    let mut state = ui.data_mut(|d| std::mem::take(d.get_temp_mut_or_default::<State>(key)));
    state.attach(s.doc.identity);
    let title = if active(s) {
        "Histogram (Paused)"
    } else if state.snapshot.is_some() && !state.current(s) {
        "Histogram (Refresh)"
    } else {
        "Histogram"
    };
    let response = CollapsingHeader::new(text(title))
        .id_salt(("histogram", s.doc.identity))
        .default_open(false)
        .show(ui, |ui| body(ui, s, &mut state));
    #[cfg(test)]
    ui.data_mut(|d| {
        d.insert_temp(
            Id::new(("histogram-header", s.doc.identity)),
            response.header_response.rect,
        )
    });
    #[cfg(not(test))]
    let _ = response;
    ui.data_mut(|d| d.insert_temp(key, state));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{Pointer, PointerPhase},
        plugins::Registry,
    };
    use serde_json::json;
    fn call(s: &mut Session, id: &str, args: serde_json::Value) -> serde_json::Value {
        s.apply(Action::new(id, args)).unwrap()["result"].clone()
    }
    fn session() -> Session {
        let mut s = Session::new(
            Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")).unwrap(),
        )
        .unwrap();
        call(&mut s, "canvas.new", json!({"width":4,"height":4}));
        call(&mut s, "layer.fill_color", json!({"color":"#204060"}));
        s
    }
    fn frame(ctx: &Context, s: &mut Session, events: Vec<Event>, time: f64) {
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(320., 1400.))),
                events,
                time: Some(time),
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| controls(ui, s));
            },
        );
    }
    fn click(ctx: &Context, s: &mut Session, pos: Pos2, time: f64) {
        for (offset, pressed) in [true, false].into_iter().enumerate() {
            frame(
                ctx,
                s,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                time + offset as f64 * 0.02,
            );
        }
    }
    fn rect(ctx: &Context, s: &Session, kind: &str) -> Rect {
        ctx.data_mut(|d| d.get_temp::<Rect>(Id::new((kind, s.doc.identity))).unwrap())
    }
    fn open(ctx: &Context, s: &mut Session) {
        frame(ctx, s, vec![], 0.);
        frame(ctx, s, vec![], 0.1);
        let position = rect(ctx, s, "histogram-header").center();
        click(ctx, s, position, 1.);
        frame(ctx, s, vec![], 2.);
    }
    fn refresh(ctx: &Context, s: &mut Session, time: f64) {
        let position = rect(ctx, s, "histogram-refresh").center();
        click(ctx, s, position, time);
    }
    fn node_graph() -> crate::engine::graph::Graph {
        use crate::engine::graph::{Edge, Filter, Graph, Node, NodeKind};
        Graph {
            enabled: true,
            linked: true,
            nodes: vec![
                Node {
                    id: 11,
                    node_type: None,
                    position: [0., 0.],
                    data: NodeKind::Layer { index: 0 },
                },
                Node {
                    id: 22,
                    node_type: Some("compositing.filter".into()),
                    position: [200., 0.],
                    data: NodeKind::Filter {
                        filter: Filter::Invert,
                        amount: 1.,
                    },
                },
                Node {
                    id: 33,
                    node_type: None,
                    position: [400., 0.],
                    data: NodeKind::Output,
                },
            ],
            edges: vec![
                Edge {
                    from: 11,
                    to: 22,
                    input: 0,
                },
                Edge {
                    from: 22,
                    to: 33,
                    input: 0,
                },
            ],
        }
    }
    #[test]
    fn published_node_input_output_clicks_are_explicit_cached_and_read_only() {
        let mut s = session();
        call(&mut s, "graph.set", json!({"graph":node_graph()}));
        let before = s.state();
        let history = s.doc.history_bytes();
        let events = s.journal.as_ref().unwrap().entries.len();
        let ctx = Context::default();
        open(&ctx, &mut s);
        let position = rect(&ctx, &s, "histogram-source").center();
        click(&ctx, &mut s, position, 3.);
        let position = rect(&ctx, &s, "histogram-source-node").center();
        click(&ctx, &mut s, position, 4.);
        frame(&ctx, &mut s, vec![], 5.);
        assert!(cached(&ctx, &s).is_none());
        let position = rect(&ctx, &s, "histogram-node").center();
        click(&ctx, &mut s, position, 6.);
        let position = ctx
            .data_mut(|d| {
                d.get_temp::<Rect>(Id::new(("histogram-node-option", s.doc.identity, 22u32)))
                    .unwrap()
            })
            .center();
        click(&ctx, &mut s, position, 7.);
        refresh(&ctx, &mut s, 8.);
        let input = cached(&ctx, &s).unwrap();
        assert_eq!(input.bins[0][32], 16);
        assert_eq!(
            input.node.unwrap().request.direction,
            nodes::Direction::Input
        );
        for time in 9..12 {
            frame(&ctx, &mut s, vec![], time as f64);
        }
        assert!(Arc::ptr_eq(&input, &cached(&ctx, &s).unwrap()));
        let position = rect(&ctx, &s, "histogram-port").center();
        click(&ctx, &mut s, position, 12.);
        let position = rect(&ctx, &s, "histogram-port-output").center();
        click(&ctx, &mut s, position, 13.);
        assert!(cached(&ctx, &s).is_none());
        refresh(&ctx, &mut s, 14.);
        let output = cached(&ctx, &s).unwrap();
        assert_eq!(output.bins[0][223], 16);
        assert_eq!(
            output.node.unwrap().request.direction,
            nodes::Direction::Output
        );
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
        call(&mut s, "layer.rename", json!({"name":"Changed"}));
        assert!(cached(&ctx, &s).is_none());
    }
    #[test]
    fn actual_refresh_is_read_only_and_idle_frames_share_one_snapshot() {
        let mut s = session();
        let ctx = Context::default();
        let state = s.state();
        let history = s.doc.history_bytes();
        let events = s.journal.as_ref().unwrap().entries.len();
        open(&ctx, &mut s);
        assert!(cached(&ctx, &s).is_none());
        refresh(&ctx, &mut s, 3.);
        let original = cached(&ctx, &s).unwrap();
        assert_eq!(original.bins[0][32], 16);
        assert_eq!(original.bins[1][64], 16);
        assert_eq!(original.bins[2][96], 16);
        assert!(original.exact);
        for i in 4..12 {
            frame(&ctx, &mut s, vec![], i as f64);
        }
        assert!(Arc::ptr_eq(&original, &cached(&ctx, &s).unwrap()));
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    }
    #[test]
    fn revisions_budget_source_active_layer_and_replacement_invalidate_without_resampling() {
        let mut s = session();
        let ctx = Context::default();
        open(&ctx, &mut s);
        refresh(&ctx, &mut s, 3.);
        call(&mut s, "layer.rename", json!({"name":"Changed"}));
        assert!(cached(&ctx, &s).is_none());
        frame(&ctx, &mut s, vec![], 4.);
        assert!(cached(&ctx, &s).is_none());
        refresh(&ctx, &mut s, 5.);
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<State>(Id::new(STATE_KEY))
                .budget = 4
        });
        assert!(cached(&ctx, &s).is_none());
        frame(&ctx, &mut s, vec![], 6.);
        refresh(&ctx, &mut s, 7.);
        assert!(!cached(&ctx, &s).unwrap().exact);
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<State>(Id::new(STATE_KEY))
                .source = Source::Layer
        });
        assert!(cached(&ctx, &s).is_none());
        frame(&ctx, &mut s, vec![], 8.);
        refresh(&ctx, &mut s, 9.);
        assert_eq!(cached(&ctx, &s).unwrap().layer_id, Some(1));
        call(&mut s, "layer.add", json!({}));
        assert!(cached(&ctx, &s).is_none());
        call(&mut s, "canvas.new", json!({"width":2,"height":2}));
        frame(&ctx, &mut s, vec![], 10.);
        let data = ctx
            .data_mut(|d| d.get_temp::<State>(Id::new(STATE_KEY)))
            .unwrap();
        assert_eq!(data.document, s.doc.identity);
        assert!(data.snapshot.is_none());
    }
    #[test]
    fn active_contact_hides_snapshot_and_disables_refresh_without_closing_the_contact() {
        let mut s = session();
        let ctx = Context::default();
        open(&ctx, &mut s);
        refresh(&ctx, &mut s, 3.);
        let snapshot = cached(&ctx, &s).unwrap();
        let p = Pointer {
            phase: PointerPhase::Down,
            x: 1.5,
            y: 1.5,
            pressure: 1.,
            time: 1,
            pen: false,
            eraser: false,
            tool_override: None,
            view_x: None,
            view_y: None,
        };
        s.pointer(p).unwrap();
        let revision = s.revision;
        frame(&ctx, &mut s, vec![], 4.);
        assert!(cached(&ctx, &s).is_none());
        refresh(&ctx, &mut s, 5.);
        assert!(s.stroke.active());
        assert_eq!(s.revision, revision);
        let data = ctx
            .data_mut(|d| d.get_temp::<State>(Id::new(STATE_KEY)))
            .unwrap();
        assert!(data.error.is_empty());
        assert!(Arc::ptr_eq(&snapshot, data.snapshot.as_ref().unwrap()));
        s.pointer(Pointer {
            phase: PointerPhase::Up,
            time: 2,
            ..p
        })
        .unwrap();
        assert!(cached(&ctx, &s).is_none());
    }
    #[test]
    fn malformed_snapshot_metadata_and_bin_lengths_are_not_published() {
        let mut s = session();
        let value = call(&mut s, "histogram.query", json!({}));
        for case in 0..4 {
            let mut bad = value.clone();
            match case {
                0 => bad["red"] = json!([1]),
                1 => bad["nontransparent_pixels"] = json!(999),
                2 => bad["exact"] = json!(false),
                _ => bad["source"] = json!("layer"),
            }
            assert!(Snapshot::decode(bad).is_err());
        }
    }
    #[test]
    fn expanded_controls_fit_four_languages_and_appearance_metrics() {
        let mut s = session();
        let mut graph = node_graph();
        graph.nodes.iter_mut().find(|n| n.id == 22).unwrap().id = u32::MAX;
        for edge in &mut graph.edges {
            if edge.from == 22 {
                edge.from = u32::MAX;
            }
            if edge.to == 22 {
                edge.to = u32::MAX;
            }
        }
        call(&mut s, "graph.set", json!({"graph":graph}));
        for language in crate::i18n::Language::ALL {
            crate::i18n::set_language(language);
            for width in [176., 360., 680.] {
                for large in [false, true] {
                    for node_scope in [false, true] {
                        let ctx = Context::default();
                        theme::fonts(&ctx);
                        let mut a = crate::configuration::Appearance::default();
                        if large {
                            a.font_size = 22.;
                            a.control_height = 48.;
                            a.title_height = 60.;
                            a.toolbar_height = 60.;
                            a.tab_height = 52.;
                        }
                        theme::apply(&ctx, &a);
                        let mut snapshot = if node_scope {
                            test_node_snapshot()
                        } else {
                            test_snapshot()
                        };
                        snapshot.document = s.doc.identity;
                        snapshot.revision = s.revision;
                        snapshot.source = if node_scope {
                            Source::Node
                        } else {
                            Source::Composite
                        };
                        snapshot.layer_id = None;
                        let mut state = State {
                            document: s.doc.identity,
                            source: snapshot.source,
                            snapshot: Some(Arc::new(snapshot)),
                            ..Default::default()
                        };
                        if node_scope {
                            state.nodes.view = nodes::View::Actual;
                            state.nodes.refresh_choices(&s);
                            state.nodes.node = Some(u32::MAX);
                        }
                        let _ = ctx.run(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    Pos2::ZERO,
                                    vec2(width, 1800.),
                                )),
                                ..Default::default()
                            },
                            |ctx| {
                                CentralPanel::default().show(ctx, |ui| {
                                    let bounds = ui.max_rect();
                                    let scope = ui.scope(|ui| body(ui, &mut s, &mut state));
                                    let actual = scope.response.rect;
                                    assert!(
                                        actual.left() >= bounds.left() - 1.
                                            && actual.right() <= bounds.right() + 1.,
                                        "{language:?}/{width}/{large}: {actual:?}"
                                    );
                                    assert!(
                                        actual.height()
                                            <= 24. * a.control_height
                                                + 16. * a.font_size
                                                + 20. * a.item_spacing
                                    );
                                });
                            },
                        );
                    }
                }
            }
        }
    }
}
