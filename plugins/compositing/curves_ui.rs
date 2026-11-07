//! Local curve drafts only. Native LUT previews are cold and bounded.
use super::*;
use crate::engine::{
    curves::{Curve, Curves, Interpolation, Point},
    graph::Renderer,
};
use std::collections::VecDeque;

const CACHE_LIMIT: usize = 96;
#[derive(Clone)]
struct Preview {
    provider: usize,
    curve: Curve,
    table: Result<[u8; 256], String>,
}
#[derive(Clone)]
struct State {
    id: Id,
    channel: usize,
    selected: usize,
    dragging: Option<usize>,
    preview: Option<Preview>,
}
impl State {
    fn new(id: Id) -> Self {
        Self {
            id,
            channel: 0,
            selected: 0,
            dragging: None,
            preview: None,
        }
    }
}
#[derive(Clone, Default)]
struct Cache {
    entries: VecDeque<State>,
}
impl Cache {
    fn take(&mut self, id: Id) -> State {
        self.entries
            .iter()
            .position(|s| s.id == id)
            .and_then(|i| self.entries.remove(i))
            .unwrap_or_else(|| State::new(id))
    }
    fn put(&mut self, state: State) {
        self.entries.retain(|entry| entry.id != state.id);
        self.entries.push_back(state);
        while self.entries.len() > CACHE_LIMIT {
            self.entries.pop_front();
        }
    }
}
fn text(message: &str) -> RichText {
    tr_for(Some("compositing"), message)
}
fn is_default(curve: &Curve) -> bool {
    curve.interpolation == Interpolation::Smooth
        && curve.points.as_slice()
            == [
                Point {
                    input: 0,
                    output: 0,
                },
                Point {
                    input: 255,
                    output: 255,
                },
            ]
}
fn channel(value: &mut Curves, index: usize) -> &mut Curve {
    match index {
        1 => &mut value.red,
        2 => &mut value.green,
        3 => &mut value.blue,
        _ => &mut value.master,
    }
}
fn point_position(rect: Rect, point: Point) -> Pos2 {
    pos2(
        rect.left() + rect.width() * f32::from(point.input) / 255.,
        rect.bottom() - rect.height() * f32::from(point.output) / 255.,
    )
}
fn position_point(rect: Rect, position: Pos2) -> Point {
    Point {
        input: ((position.x - rect.left()) / rect.width() * 255.)
            .round()
            .clamp(0., 255.) as u8,
        output: ((rect.bottom() - position.y) / rect.height() * 255.)
            .round()
            .clamp(0., 255.) as u8,
    }
}
fn move_point(curve: &mut Curve, index: usize, mut point: Point) -> bool {
    if index >= curve.points.len() || curve.validate().is_err() {
        return false;
    }
    let low = if index == 0 {
        0
    } else {
        curve.points[index - 1].input + 1
    };
    let high = curve.points.get(index + 1).map_or(255, |p| p.input - 1);
    point.input = point.input.clamp(low, high);
    let changed = curve.points[index] != point;
    curve.points[index] = point;
    changed
}
fn insert_point(curve: &mut Curve, point: Point) -> Option<usize> {
    if curve.points.len() >= 32 || curve.validate().is_err() {
        return None;
    }
    match curve.points.binary_search_by_key(&point.input, |p| p.input) {
        Ok(_) => None,
        Err(index) => {
            curve.points.insert(index, point);
            Some(index)
        }
    }
}
fn update_preview(state: &mut State, curve: &Curve, renderer: Option<&dyn Renderer>, paused: bool) {
    if paused || curve.validate().is_err() {
        return;
    }
    // The cache owns only data, never callbacks or DLL references. Package/DLL
    // activation requires restart; caller IDs include document identity.
    let provider = renderer.map_or(0, |r| r as *const dyn Renderer as *const () as usize);
    if state
        .preview
        .as_ref()
        .is_some_and(|p| p.provider == provider && p.curve == *curve)
    {
        return;
    }
    let table = renderer
        .filter(|r| r.supports_curves())
        .ok_or_else(|| "Curves renderer unavailable".to_string())
        .and_then(|r| curve.parameters().and_then(|p| r.prepare_curves(&p)))
        .map(|t| t.values);
    state.preview = Some(Preview {
        provider,
        curve: curve.clone(),
        table,
    });
}
/// Fallback for standalone viewer fixtures without initialized UI fonts.
pub(crate) fn fallback_height(a: &crate::configuration::Appearance, width: f32) -> f32 {
    width.min(a.control_height * 8.)
        + 12. * a.control_height
        + 24. * a.font_size
        + 24. * a.item_spacing
}
/// Measure wrapping with the actual font and current catalog. Only graph Fit
/// needs this budget; normal frames do not run the text-layout preparation.
pub(crate) fn estimated_height(ui: &Ui, width: f32) -> f32 {
    let width = width.max(1.);
    let spacing = ui.spacing();
    let body = TextStyle::Body.resolve(ui.style());
    let button = TextStyle::Button.resolve(ui.style());
    let measure = |message: &str, font: FontId, available: f32| {
        ui.painter()
            .layout(message.to_owned(), font, Color32::WHITE, available.max(1.))
            .size()
            .y
    };
    let label = |message: &str| {
        measure(
            &crate::i18n::translated_for(Some("compositing"), message),
            body.clone(),
            width,
        )
    };
    let line = measure("255", body.clone(), f32::INFINITY);
    let control = spacing
        .interact_size
        .y
        .max(measure("255", button.clone(), f32::INFINITY) + 2. * spacing.button_padding.y);
    let labels = ["Channel", "Interpolation", "Input", "Output"]
        .iter()
        .map(|s| label(s))
        .sum::<f32>();
    let count = measure(
        &format!(
            "{} 32/32",
            crate::i18n::translated_for(Some("compositing"), "Control Points")
        ),
        body.clone(),
        width,
    );
    let status = [
        "Invalid Control Points",
        "Curve preview pauses while drawing.",
        "Curve Preview Unavailable",
    ]
    .iter()
    .map(|s| label(s))
    .fold(0., f32::max);
    let buttons = ["Remove Point", "Reset Curve", "Reset All Curves"]
        .iter()
        .map(|s| {
            let title = crate::i18n::translated_for(Some("compositing"), s);
            control.max(
                measure(
                    &title,
                    button.clone(),
                    width - 2. * spacing.button_padding.x,
                ) + 2. * spacing.button_padding.y,
            )
        })
        .sum::<f32>();
    (width.min(spacing.interact_size.y * 8.)
        + 4. * control
        + labels
        + count
        + status
        + buttons
        + line
        + label("Click to add a point. Drag to move. Right-click to remove.")
        + 20. * spacing.item_spacing.y)
        .ceil()
}

#[cfg(test)]
pub(crate) fn edit(
    ui: &mut Ui,
    id: Id,
    value: &mut Curves,
    renderer: Option<&dyn Renderer>,
    paused: bool,
) -> bool {
    edit_with_histogram(ui, id, value, renderer, paused, None)
}
pub(crate) fn edit_with_histogram(
    ui: &mut Ui,
    id: Id,
    value: &mut Curves,
    renderer: Option<&dyn Renderer>,
    paused: bool,
    histogram: Option<&super::histogram_ui::Snapshot>,
) -> bool {
    let id = ui.make_persistent_id(id);
    let cache_id = Id::new("micopi.curve-editors.v1");
    let mut state = ui.data_mut(|d| d.get_temp_mut_or_default::<Cache>(cache_id).take(id));
    let mut changed = false;
    ui.push_id(id, |ui| {
        ui.style_mut().wrap_mode = Some(TextWrapMode::Wrap);
        ui.label(text("Channel"));
        let previous_channel = state.channel;
        let titles = ["Master RGB", "Red", "Green", "Blue"];
        ComboBox::from_id_salt("channel")
            .width(ui.available_width())
            .truncate()
            .selected_text(text(titles[state.channel]))
            .show_ui(ui, |ui| {
                for (index, title) in titles.iter().enumerate() {
                    ui.selectable_value(&mut state.channel, index, text(title));
                }
            });
        if previous_channel != state.channel {
            state.selected = 0;
            state.dragging = None;
        }
        let curve = channel(value, state.channel);
        state.selected = state.selected.min(curve.points.len().saturating_sub(1));
        ui.label(text("Interpolation"));
        let previous = curve.interpolation;
        ComboBox::from_id_salt("interpolation")
            .width(ui.available_width())
            .truncate()
            .selected_text(text(match curve.interpolation {
                Interpolation::Linear => "Linear",
                Interpolation::Smooth => "Smooth Curve",
            }))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut curve.interpolation,
                    Interpolation::Linear,
                    text("Linear"),
                );
                ui.selectable_value(
                    &mut curve.interpolation,
                    Interpolation::Smooth,
                    text("Smooth Curve"),
                );
            });
        changed |= curve.interpolation != previous;
        ui.add_space(ui.spacing().item_spacing.y);
        let width = ui.available_width().max(1.);
        let height = width.min(ui.spacing().interact_size.y * 8.).max(1.);
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
        let margin = (ui.spacing().interact_size.y * 0.18)
            .min(width * 0.1)
            .min(height * 0.1);
        let plot = rect.shrink(margin);
        #[cfg(test)]
        ui.data_mut(|d| d.insert_temp(id.with("plot"), plot));
        if curve.validate().is_ok() {
            state.selected = state.selected.min(curve.points.len() - 1);
            let hit_radius = (ui.spacing().interact_size.y * 0.35).max(5.);
            if let Some(position) = response.interact_pointer_pos() {
                let nearest = curve
                    .points
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        point_position(plot, **a)
                            .distance_sq(position)
                            .total_cmp(&point_position(plot, **b).distance_sq(position))
                    })
                    .filter(|(_, p)| point_position(plot, **p).distance(position) <= hit_radius)
                    .map(|(i, _)| i);
                if ui.input(|i| i.pointer.button_pressed(PointerButton::Primary))
                    && (response.hovered() || response.is_pointer_button_down_on())
                {
                    state.dragging = nearest;
                    if let Some(index) = nearest {
                        state.selected = index;
                    }
                }
                if response.dragged_by(PointerButton::Primary) {
                    if let Some(index) = state.dragging {
                        changed |= move_point(curve, index, position_point(plot, position));
                    }
                }
                if response.clicked_by(PointerButton::Primary) {
                    if let Some(index) = nearest {
                        state.selected = index;
                    } else if let Some(index) = insert_point(curve, position_point(plot, position))
                    {
                        state.selected = index;
                        changed = true;
                    }
                }
                if response.clicked_by(PointerButton::Secondary) && curve.points.len() > 2 {
                    if let Some(index) = nearest {
                        curve.points.remove(index);
                        state.selected = index.min(curve.points.len() - 1);
                        state.dragging = None;
                        changed = true;
                    }
                }
            }
            if response.drag_stopped_by(PointerButton::Primary)
                || !ui.input(|i| i.pointer.primary_down())
            {
                state.dragging = None;
            }
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(if state.dragging.is_some() {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Crosshair
            });
        }
        if ui.is_rect_visible(rect) {
            update_preview(&mut state, curve, renderer, paused);
            let painter = ui.painter();
            painter.rect_filled(
                rect,
                ui.visuals().widgets.inactive.corner_radius,
                ui.visuals().extreme_bg_color,
            );
            if let Some(snapshot) = histogram {
                super::histogram_ui::paint(ui, plot, snapshot, [3, 0, 1, 2][state.channel]);
            }
            for step in 0..=4 {
                let t = step as f32 / 4.;
                painter.line_segment(
                    [
                        pos2(plot.left() + plot.width() * t, plot.top()),
                        pos2(plot.left() + plot.width() * t, plot.bottom()),
                    ],
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
                painter.line_segment(
                    [
                        pos2(plot.left(), plot.top() + plot.height() * t),
                        pos2(plot.right(), plot.top() + plot.height() * t),
                    ],
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
            }
            painter.line_segment(
                [plot.left_bottom(), plot.right_top()],
                Stroke::new(1., ui.visuals().weak_text_color()),
            );
            if let Some(table) = state
                .preview
                .as_ref()
                .filter(|p| p.curve == *curve)
                .and_then(|p| p.table.as_ref().ok())
            {
                let points = table
                    .iter()
                    .enumerate()
                    .map(|(x, y)| {
                        point_position(
                            plot,
                            Point {
                                input: x as u8,
                                output: *y,
                            },
                        )
                    })
                    .collect();
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(1.5, ui.visuals().selection.stroke.color),
                ));
            }
            if curve.validate().is_ok() {
                let radius = (ui.spacing().interact_size.y * 0.12).max(2.);
                for (index, point) in curve.points.iter().enumerate() {
                    let position = point_position(plot, *point);
                    painter.circle_filled(position, radius + 1., ui.visuals().extreme_bg_color);
                    painter.circle_filled(
                        position,
                        radius,
                        if index == state.selected {
                            ui.visuals().selection.stroke.color
                        } else {
                            ui.visuals().text_color()
                        },
                    );
                }
            }
        }
        if let Some(snapshot) = histogram {
            super::histogram_ui::label(ui, snapshot);
        }
        ui.weak(format!(
            "{} {}/32",
            crate::i18n::translated_for(Some("compositing"), "Control Points"),
            curve.points.len()
        ));
        if let Some(mut point) = curve.points.get(state.selected).copied() {
            ui.label(text("Input"));
            let input = ui.add_sized(
                [ui.available_width(), ui.spacing().interact_size.y],
                DragValue::new(&mut point.input).range(0..=255),
            );
            ui.label(text("Output"));
            let output = ui.add_sized(
                [ui.available_width(), ui.spacing().interact_size.y],
                DragValue::new(&mut point.output).range(0..=255),
            );
            if input.changed() || output.changed() {
                changed |= move_point(curve, state.selected, point);
            }
        }
        let remove = ui.add_enabled(curve.points.len() > 2, Button::new(text("Remove Point")));
        if remove.clicked() {
            curve.points.remove(state.selected);
            state.selected = state.selected.min(curve.points.len() - 1);
            changed = true;
        }
        let reset = ui.add_enabled(!is_default(curve), Button::new(text("Reset Curve")));
        #[cfg(test)]
        ui.data_mut(|d| d.insert_temp(id.with("reset"), reset.rect));
        if reset.clicked() {
            *curve = Curve::default();
            state.selected = 0;
            state.dragging = None;
            changed = true;
        }
        if curve.validate().is_err() {
            ui.colored_label(ui.visuals().warn_fg_color, text("Invalid Control Points"));
        } else if paused {
            ui.weak(text("Curve preview pauses while drawing."));
        } else if let Some(error) = state.preview.as_ref().and_then(|p| p.table.as_ref().err()) {
            ui.weak(text("Curve Preview Unavailable"))
                .on_hover_text(error);
        }
        let reset_all = ui.add_enabled(
            ![&value.master, &value.red, &value.green, &value.blue]
                .into_iter()
                .all(is_default),
            Button::new(text("Reset All Curves")),
        );
        if reset_all.clicked() {
            *value = Curves::default();
            state.selected = 0;
            state.dragging = None;
            changed = true;
        }
        ui.add(
            Label::new(text(
                "Click to add a point. Drag to move. Right-click to remove.",
            ))
            .wrap(),
        );
    });
    ui.data_mut(|d| d.get_temp_mut_or_default::<Cache>(cache_id).put(state));
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use micopi_plugin_api as abi;
    use std::cell::Cell;
    #[derive(Default)]
    struct Counter {
        calls: Cell<usize>,
    }
    impl Renderer for Counter {
        fn supports_curves(&self) -> bool {
            true
        }
        fn prepare_curves(&self, _: &abi::CurvesParameters) -> Result<abi::CompositeLut, String> {
            self.calls.set(self.calls.get() + 1);
            Ok(abi::CompositeLut {
                values: std::array::from_fn(|i| i as u8),
            })
        }
        fn render(
            &self,
            _: &[abi::CompositeNode],
            _: &[abi::CompositeLayer],
            _: u32,
            _: u32,
            _: &mut abi::Pixels,
        ) -> Result<(), String> {
            panic!("Local curve controls must never render or mutate a document")
        }
    }
    fn frame(
        ctx: &Context,
        value: &mut Curves,
        width: f32,
        events: Vec<Event>,
        time: f64,
        renderer: Option<&dyn Renderer>,
        paused: bool,
    ) {
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 1800.))),
                events,
                time: Some(time),
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    edit(ui, Id::new("curves-test"), value, renderer, paused);
                });
            },
        );
    }
    fn rect(ctx: &Context, key: &str) -> Rect {
        ctx.data_mut(|d| {
            let cache = d
                .get_temp::<Cache>(Id::new("micopi.curve-editors.v1"))
                .unwrap();
            d.get_temp::<Rect>(cache.entries.back().unwrap().id.with(key))
                .unwrap()
        })
    }
    fn click(ctx: &Context, value: &mut Curves, pos: Pos2, button: PointerButton, time: f64) {
        for (offset, pressed) in [true, false].into_iter().enumerate() {
            frame(
                ctx,
                value,
                240.,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                time + offset as f64 * 0.02,
                None,
                false,
            );
        }
    }
    #[test]
    fn actual_click_drag_remove_and_reset_change_only_the_selected_local_curve() {
        let ctx = Context::default();
        let mut value = Curves::default();
        value.red.points[0].output = 255;
        value.red.points[1].output = 0;
        let red = value.red.clone();
        frame(&ctx, &mut value, 240., vec![], 0., None, false);
        frame(&ctx, &mut value, 240., vec![], 0.1, None, false);
        let plot = rect(&ctx, "plot");
        let center = plot.center();
        click(&ctx, &mut value, center, PointerButton::Primary, 1.);
        assert_eq!(value.master.points.len(), 3);
        assert!((i16::from(value.master.points[1].input) - 128).abs() <= 1);
        let target = pos2(
            plot.left() + plot.width() * 0.25,
            plot.top() + plot.height() * 0.25,
        );
        frame(
            &ctx,
            &mut value,
            240.,
            vec![
                Event::PointerMoved(center),
                Event::PointerButton {
                    pos: center,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            2.,
            None,
            false,
        );
        frame(
            &ctx,
            &mut value,
            240.,
            vec![Event::PointerMoved(target)],
            2.1,
            None,
            false,
        );
        frame(
            &ctx,
            &mut value,
            240.,
            vec![Event::PointerButton {
                pos: target,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
            2.2,
            None,
            false,
        );
        assert!((i16::from(value.master.points[1].input) - 64).abs() <= 1);
        assert!((i16::from(value.master.points[1].output) - 191).abs() <= 1);
        click(&ctx, &mut value, target, PointerButton::Secondary, 3.);
        assert_eq!(value.master.points.len(), 2);
        click(&ctx, &mut value, center, PointerButton::Primary, 4.);
        assert_eq!(value.master.points.len(), 3);
        click(
            &ctx,
            &mut value,
            rect(&ctx, "reset").center(),
            PointerButton::Primary,
            5.,
        );
        assert_eq!(value.master, Curve::default());
        assert_eq!(value.red, red);
    }
    #[test]
    fn point_edits_preserve_strict_order_count_limit_and_endpoint_ranges() {
        let mut curve = Curve::default();
        insert_point(
            &mut curve,
            Point {
                input: 128,
                output: 80,
            },
        )
        .unwrap();
        assert!(move_point(
            &mut curve,
            1,
            Point {
                input: 255,
                output: 255
            }
        ));
        assert_eq!(curve.points[1].input, 254);
        curve.validate().unwrap();
        assert!(move_point(
            &mut curve,
            1,
            Point {
                input: 0,
                output: 0
            }
        ));
        assert_eq!(curve.points[1].input, 1);
        assert!(insert_point(
            &mut curve,
            Point {
                input: 1,
                output: 9
            }
        )
        .is_none());
        curve.points = (0..31)
            .map(|input| Point {
                input,
                output: input,
            })
            .collect();
        assert!(insert_point(
            &mut curve,
            Point {
                input: 255,
                output: 255
            }
        )
        .is_some());
        assert_eq!(curve.points.len(), 32);
        assert!(insert_point(
            &mut curve,
            Point {
                input: 64,
                output: 64
            }
        )
        .is_none());
        curve.validate().unwrap();
    }
    #[test]
    fn preview_is_cached_for_idle_frames_and_pauses_during_contacts() {
        let ctx = Context::default();
        let provider = Counter::default();
        let mut value = Curves::default();
        for frame_index in 0..8 {
            frame(
                &ctx,
                &mut value,
                240.,
                vec![],
                frame_index as f64,
                Some(&provider),
                true,
            );
        }
        assert_eq!(provider.calls.get(), 0);
        for frame_index in 8..16 {
            frame(
                &ctx,
                &mut value,
                240.,
                vec![],
                frame_index as f64,
                Some(&provider),
                false,
            );
        }
        assert_eq!(provider.calls.get(), 1);
        value.master.points[0].output = 22;
        for frame_index in 16..24 {
            frame(
                &ctx,
                &mut value,
                240.,
                vec![],
                frame_index as f64,
                Some(&provider),
                true,
            );
        }
        assert_eq!(provider.calls.get(), 1);
        frame(&ctx, &mut value, 240., vec![], 24., Some(&provider), false);
        assert_eq!(provider.calls.get(), 2);
        let replacement = Counter::default();
        frame(
            &ctx,
            &mut value,
            240.,
            vec![],
            25.,
            Some(&replacement),
            false,
        );
        assert_eq!(replacement.calls.get(), 1);
    }
    #[test]
    fn preview_state_is_bounded_across_removed_nodes_and_documents() {
        let mut cache = Cache::default();
        for i in 0..CACHE_LIMIT * 3 {
            let mut state = cache.take(Id::new(i));
            state.channel = 3;
            cache.put(state);
        }
        assert_eq!(cache.entries.len(), CACHE_LIMIT);
        assert_eq!(cache.take(Id::new(0)).channel, 0);
        assert_eq!(cache.take(Id::new(CACHE_LIMIT * 3 - 1)).channel, 3);
    }
    #[test]
    fn controls_fit_four_languages_narrow_wide_and_large_metrics() {
        for language in crate::i18n::Language::ALL {
            crate::i18n::set_language(language);
            for width in [176., 360., 680.] {
                for large in [false, true] {
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
                    let mut value = Curves::default();
                    let _ = ctx.run(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 1800.))),
                            ..Default::default()
                        },
                        |ctx| {
                            CentralPanel::default().show(ctx, |ui| {
                                let bounds = ui.max_rect();
                                let response = ui.scope(|ui| {
                                    let snapshot = super::super::histogram_ui::test_snapshot();
                                    edit_with_histogram(
                                        ui,
                                        Id::new("curve-layout"),
                                        &mut value,
                                        None,
                                        false,
                                        Some(&snapshot),
                                    );
                                });
                                let actual = response.response.rect;
                                assert!(
                                    actual.left() >= bounds.left() - 1.
                                        && actual.right() <= bounds.right() + 1.,
                                    "{language:?}/{width}/{large}: {actual:?}"
                                );
                                assert!(
                                    actual.height() <= estimated_height(ui, bounds.width()),
                                    "{language:?}/{width}/{large}: {} vs {}",
                                    actual.height(),
                                    estimated_height(ui, bounds.width())
                                );
                            });
                        },
                    );
                }
            }
        }
    }
}
