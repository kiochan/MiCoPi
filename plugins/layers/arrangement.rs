//! Cold, explicitly targeted alignment. The shared operation owns pixel work.
use super::*;

const DRAFT: &str = "layer-arrangement-draft";
const MODES: [(&str, &str); 8] = [
    ("left", "Align Left"),
    ("horizontal_center", "Align Horizontal Centers"),
    ("right", "Align Right"),
    ("top", "Align Top"),
    ("vertical_center", "Align Vertical Centers"),
    ("bottom", "Align Bottom"),
    (
        "distribute_horizontal_centers",
        "Distribute Horizontal Centers",
    ),
    ("distribute_vertical_centers", "Distribute Vertical Centers"),
];
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reference {
    Targets,
    Canvas,
}
impl Reference {
    fn value(self) -> &'static str {
        if self == Self::Canvas {
            "canvas"
        } else {
            "targets"
        }
    }
    fn title(self) -> &'static str {
        if self == Self::Canvas {
            "Canvas"
        } else {
            "Selected Layers"
        }
    }
}
#[derive(Clone)]
struct Draft {
    document: u64,
    revision: u64,
    ids: Vec<u64>,
    reference: Reference,
}
fn eligible(count: usize, reference: Reference, index: usize) -> bool {
    if index >= 6 {
        reference == Reference::Targets && count >= 3
    } else {
        count >= if reference == Reference::Canvas { 1 } else { 2 }
    }
}
fn glyph(painter: &Painter, rect: Rect, index: usize, color: Color32) {
    let horizontal = matches!(index, 0 | 1 | 2 | 6);
    let start = if horizontal { rect.left() } else { rect.top() };
    let extent = if horizontal {
        rect.width()
    } else {
        rect.height()
    };
    let stroke = Stroke::new(1.25, color);
    let point = |along: f32, across: f32| {
        if horizontal {
            pos2(start + along * extent, rect.top() + across * rect.height())
        } else {
            pos2(rect.left() + across * rect.width(), start + along * extent)
        }
    };
    let centers = if index >= 6 {
        [0.15, 0.5, 0.85]
    } else if matches!(index, 0 | 3) {
        [0.3, 0.4, 0.25]
    } else if matches!(index, 2 | 5) {
        [0.7, 0.6, 0.75]
    } else {
        [0.5; 3]
    };
    for (i, (center, size)) in centers.into_iter().zip([0.4, 0.6, 0.3]).enumerate() {
        let across = 0.15 + i as f32 * 0.3;
        let shape = Rect::from_two_pos(
            point(center - size * 0.5, across),
            point(center + size * 0.5, across + 0.16),
        );
        painter.rect_stroke(shape, 1., stroke, StrokeKind::Inside);
    }
    if index < 6 {
        let guide = if matches!(index, 0 | 3) {
            0.1
        } else if matches!(index, 2 | 5) {
            0.9
        } else {
            0.5
        };
        painter.line_segment([point(guide, 0.), point(guide, 1.)], stroke);
    }
}

impl Editor {
    pub(super) fn dismiss_arrangement_ui(&self, ctx: &Context) {
        ctx.data_mut(|data| data.remove::<Draft>(Id::new(DRAFT)));
    }
    pub(super) fn arrangement_menu_open(&self, ctx: &Context) -> bool {
        ctx.data_mut(|data| data.get_temp::<Draft>(Id::new(DRAFT)))
            .is_some()
    }
    pub(super) fn layer_arrangement_ui(
        &self,
        ui: &mut Ui,
        pending: &mut Vec<crate::application::Action>,
    ) {
        if !self.session.registry.actions.contains_key("layer.arrange") {
            return;
        }
        let a = self.settings.ui.clone();
        let colors = theme::Colors::new(&a);
        let mut opened = false;
        let (response,_)=egui::containers::menu::MenuButton::new(tr("Arrange"))
            .config(egui::containers::menu::MenuConfig::new().close_behavior(PopupCloseBehavior::CloseOnClickOutside)).ui(ui,|ui|{
            opened=true;
            let key=Id::new(DRAFT);
            let mut draft=ui.data_mut(|data|data.get_temp::<Draft>(key)).unwrap_or_else(||Draft {
                document:self.session.doc.identity,revision:self.session.revision,ids:self.session.doc.selected_layer_ids(),reference:Reference::Targets,
            });
            let stale=(draft.document,draft.revision)!=(self.session.doc.identity,self.session.revision);
            ui.set_max_width((a.control_height*12.).min(ui.ctx().content_rect().width()-a.panel_padding*4.).max(a.control_height*4.));
            ui.strong(crate::i18n::text("Selected layers: {count}").replace("{count}",&draft.ids.len().to_string()));
            ui.add(Label::new(format!("{}: {}",localize("Painting Layer"),self.session.doc.layer_name(self.session.doc.active))).wrap());
            ui.add_space(a.item_spacing);
            ui.horizontal_wrapped(|ui|{
                ui.label(tr("Align To"));
                let _reference=ComboBox::from_id_salt("layer-arrangement-reference").width(ui.available_width()).truncate()
                    .selected_text(tr(draft.reference.title())).show_ui(ui,|ui|{
                        ui.selectable_value(&mut draft.reference,Reference::Targets,tr("Selected Layers"));
                        ui.selectable_value(&mut draft.reference,Reference::Canvas,tr("Canvas"));
                    });
                #[cfg(test)] ui.data_mut(|data|data.insert_temp(Id::new("layer-arrangement-reference"),_reference.response.rect));
            });
            ui.add_space(a.item_spacing);
            for row in [0..3,3..6,6..8] {
                if row.start==0 {ui.label(tr("Align"));}
                if row.start==6 {ui.separator();ui.label(tr("Distribute"));}
                ui.horizontal(|ui|{
                    for index in row {
                        let (mode,title)=MODES[index];
                        let enabled=!stale&&eligible(draft.ids.len(),draft.reference,index);
                        let side=a.control_height.max(a.icon_size+a.item_spacing*2.);
                        let (rect,response)=ui.add_enabled_ui(enabled,|ui|{
                            let (rect,response)=ui.allocate_exact_size(Vec2::splat(side),Sense::click());
                            let visuals=ui.style().interact(&response);
                            ui.painter().rect(rect,a.corner_radius*0.6,visuals.weak_bg_fill,visuals.bg_stroke,StrokeKind::Inside);
                            glyph(ui.painter(),rect.shrink((side-a.icon_size)*0.5),index,if enabled{visuals.text_color()}else{colors.muted});
                            response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,enabled,localize(title)));
                            (rect,response)
                        }).inner;
                        let hint=if index>=6 {"Distribute at least three selected layers by their centers. The two outer layers stay fixed."}else{"Align painted layer bounds. Masks and filters do not change these bounds."};
                        let tooltip=format!("{}\n{}",localize(title),localize(hint));
                        let response=response.on_hover_text(tooltip.as_str()).on_disabled_hover_text(tooltip);
                        #[cfg(test)] ui.data_mut(|data|data.insert_temp(Id::new(("layer-arrangement-control",mode)),(rect,enabled)));
                        if enabled&&response.clicked() {
                            let mut action=crate::application::Action::new("layer.arrange",json!({"layer_ids":draft.ids,"mode":mode,"reference":draft.reference.value()}));
                            action.expected_revision=Some(self.session.revision);pending.push(action);ui.close();
                        }
                    }
                });
            }
            ui.add_space(a.item_spacing);
            ui.add(Label::new(tr("Only the selected layers move. Their linked masks follow; other linked layers stay fixed.")).wrap());
            ui.add(Label::new(tr("Pixels beyond the canvas are clipped. Undo restores them.")).wrap());
            if stale {ui.add(Label::new(tr("The document changed. Reopen Arrange to use the current selection.")).wrap());}
            ui.data_mut(|data|data.insert_temp(key,draft));
        });
        response.on_hover_text(tr(
            "Align and distribute selected raster layers without changing the painting layer.",
        ));
        if !opened {
            ui.data_mut(|data| data.remove::<Draft>(Id::new(DRAFT)));
        }
    }
}

#[cfg(test)]
#[path = "arrangement_ui_tests.rs"]
mod tests;
