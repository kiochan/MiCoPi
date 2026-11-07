//! Layer-link controls only dispatch registered metadata actions.
use super::*;

pub(super) fn chain(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let stroke = Stroke::new((size * 0.085).max(1.), color);
    for offset in [-0.19, 0.19] {
        painter.rect_stroke(
            Rect::from_center_size(
                center + vec2(size * offset, 0.),
                vec2(size * 0.55, size * 0.38),
            ),
            size * 0.18,
            stroke,
            StrokeKind::Inside,
        );
    }
    painter.line_segment(
        [
            center - vec2(size * 0.16, 0.),
            center + vec2(size * 0.16, 0.),
        ],
        stroke,
    );
}

fn control(
    ui: &mut Ui,
    appearance: &crate::configuration::Appearance,
    title: &'static str,
    unlink: bool,
) -> Response {
    let icon = appearance.icon_size;
    let padding = appearance.panel_padding * 0.65;
    let natural = ui.painter().layout_no_wrap(
        localize(title).into_owned(),
        FontId::proportional(appearance.font_size),
        Color32::PLACEHOLDER,
    );
    let prefix = icon + appearance.item_spacing + padding * 2.;
    let width = (prefix + natural.size().x)
        .min(ui.available_width())
        .max(1.);
    let galley = WidgetText::from(
        RichText::new(localize(title))
            .font(FontId::proportional(appearance.font_size))
            .color(Color32::PLACEHOLDER),
    )
    .into_galley(
        ui,
        Some(TextWrapMode::Truncate),
        (width - prefix).max(0.),
        TextStyle::Body,
    );
    let size = vec2(width, appearance.control_height.max(icon + padding));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let full_title = if unlink {
        "Unlink Selected"
    } else {
        "Link Selected"
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            localize(full_title),
        )
    });
    let visuals = ui.style().interact(&response);
    ui.painter().rect(
        rect,
        appearance.corner_radius,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        StrokeKind::Inside,
    );
    let center = pos2(rect.left() + padding + icon * 0.5, rect.center().y);
    chain(ui.painter(), center, icon, visuals.text_color());
    if unlink {
        ui.painter().line_segment(
            [
                center + vec2(-icon * 0.38, icon * 0.38),
                center + vec2(icon * 0.38, -icon * 0.38),
            ],
            Stroke::new(1.3, visuals.text_color()),
        );
    }
    ui.painter().galley(
        pos2(
            rect.left() + padding + icon + appearance.item_spacing,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        visuals.text_color(),
    );
    response.on_hover_text(format!("{}\n{}", localize(full_title), localize("Links apply to Move and nudges. Painting stays on the active layer. Linked Free Transform is not available yet.")))
}

impl Editor {
    pub(super) fn layer_link_controls_ui(
        &self,
        ui: &mut Ui,
        pending: &mut Vec<crate::application::Action>,
        enabled: bool,
    ) {
        let selected = self.session.doc.selected_layer_ids();
        let any_linked = selected.iter().any(|id| {
            self.session
                .doc
                .linked_layer_ids(*id)
                .is_ok_and(|ids| ids.len() > 1)
        });
        // Child enabled scopes cannot announce their intrinsic width to a
        // wrapping parent in advance. Choose the row/column before entering them.
        let required = ["Link", "Unlink"]
            .into_iter()
            .map(|title| {
                let text = ui.painter().layout_no_wrap(
                    localize(title).into_owned(),
                    FontId::proportional(self.settings.ui.font_size),
                    Color32::PLACEHOLDER,
                );
                self.settings.ui.icon_size
                    + self.settings.ui.item_spacing
                    + self.settings.ui.panel_padding * 1.3
                    + text.size().x
            })
            .sum::<f32>()
            + ui.spacing().item_spacing.x;
        let inline = required <= ui.available_width();
        let mut buttons = |ui: &mut Ui| {
            for (linked, title, allowed) in [
                (true, "Link", selected.len() > 1),
                (false, "Unlink", any_linked),
            ] {
                let response = ui
                    .add_enabled_ui(enabled && allowed, |ui| {
                        control(ui, &self.settings.ui, title, !linked)
                    })
                    .inner;
                #[cfg(test)]
                ui.data_mut(|data| {
                    data.insert_temp(
                        Id::new(("layer-links-control", self.session.doc.identity, linked)),
                        response.rect,
                    )
                });
                if response.clicked() {
                    pending.push(crate::application::Action::new(
                        "layer.links.set",
                        json!({"layer_ids":selected,"linked":linked}),
                    ));
                }
            }
        };
        if inline {
            ui.horizontal(&mut buttons);
        } else {
            ui.vertical(&mut buttons);
        }
        let active = self.session.doc.layer_id(self.session.doc.active);
        if let Ok(ids) = self.session.doc.linked_layer_ids(active) {
            if ids.len() > 1 {
                ui.add(
                    Label::new(tr(format!("{} linked layers move together.", ids.len())).weak())
                        .wrap(),
                );
                ui.add(Label::new(tr("Painting stays on the active layer.").weak()).wrap());
            }
        }
    }
}

#[cfg(test)]
#[path = "links_ui_tests.rs"]
mod tests;
