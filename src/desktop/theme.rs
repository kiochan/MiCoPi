use super::{localize, settings::Appearance, tr};
use eframe::egui::{self, *};

#[derive(Clone, Copy)]
pub struct Colors {
    pub background: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    /// Accent tuned for text and thin strokes on panel surfaces.
    pub accent_text: Color32,
    /// Second identity hue (warm peach) used sparingly next to the accent.
    pub blush: Color32,
    /// Backdrop behind the artwork, slightly recessed from the workspace.
    pub stage: Color32,
    pub shadow: Color32,
    /// Group-row band, set apart from the panel body below it.
    pub header: Color32,
    pub dark: bool,
}
pub fn rgb(value: u32) -> Color32 {
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}
impl Colors {
    pub fn new(a: &Appearance) -> Self {
        let dark = a.theme == "graphite";
        let (background, panel, raised, border, text, muted, stage, blush) = match a.theme.as_str()
        {
            "light" => (
                0xe8e4ee, 0xfcfbfe, 0xf1eef6, 0xe2dcea, 0x2e2738, 0x7b7088, 0xdedae6, 0xf0a98f,
            ),
            "warm" => (
                0xebe3da, 0xfdf9f4, 0xf5eee6, 0xe8ddd0, 0x43362f, 0x8a776b, 0xe1d7cb, 0xe59a7e,
            ),
            _ => (
                0x18171d, 0x24222b, 0x2f2c37, 0x3a3644, 0xf1edf6, 0xa198ae, 0x131217, 0xf2b49c,
            ),
        };
        let accent =
            rgb(u32::from_str_radix(a.accent.trim_start_matches('#'), 16).unwrap_or(0xb8a2e4));
        Self {
            background: rgb(background),
            panel: rgb(panel),
            raised: rgb(raised),
            border: rgb(border),
            text: rgb(text),
            muted: rgb(muted),
            accent,
            accent_text: if dark {
                accent.lerp_to_gamma(rgb(text), 0.15)
            } else {
                accent.lerp_to_gamma(rgb(text), 0.42)
            },
            blush: rgb(blush),
            stage: rgb(stage),
            shadow: if dark {
                Color32::from_black_alpha(110)
            } else {
                Color32::from_rgba_unmultiplied(60, 40, 80, 30)
            },
            header: if dark {
                rgb(panel).lerp_to_gamma(rgb(background), 0.45)
            } else {
                rgb(panel).lerp_to_gamma(rgb(raised), 0.8)
            },
            dark,
        }
    }
    /// A soft tint of the accent over the panel surface.
    pub fn tint(&self, amount: f32) -> Color32 {
        self.panel.lerp_to_gamma(self.accent, amount)
    }
}
pub fn apply(ctx: &Context, a: &Appearance) {
    // Style is stored per egui theme. Pin the configured appearance before
    // installing it so a later OS-theme event cannot select an unstyled variant.
    ctx.set_theme(if a.theme == "graphite" {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    let c = Colors::new(a);
    let mut style = Style {
        visuals: if a.theme == "graphite" {
            Visuals::dark()
        } else {
            Visuals::light()
        },
        ..Style::default()
    };
    style.spacing.item_spacing = vec2(a.item_spacing, a.item_spacing);
    style.spacing.button_padding = vec2(
        a.panel_padding * 0.75,
        (a.control_height - a.font_size - 2.).max(0.) * 0.5,
    );
    style.spacing.interact_size = vec2(a.control_height, a.control_height);
    style.spacing.slider_width = 112.;
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.scroll.bar_width = (a.item_spacing * 0.75).clamp(3., 8.);
    style.spacing.slider_rail_height = 3.;
    style.spacing.combo_width = 120.;
    style.spacing.menu_margin = Margin::same(a.panel_padding as i8);
    style.spacing.window_margin = Margin::same(a.panel_padding as i8);
    style.spacing.icon_width = a.icon_size.min(22.);
    style.visuals.override_text_color = Some(c.text);
    style.visuals.weak_text_color = Some(c.muted);
    style.visuals.handle_shape = egui::style::HandleShape::Circle;
    style.visuals.slider_trailing_fill = true;
    style.visuals.panel_fill = c.panel;
    style.visuals.window_fill = c.panel;
    style.visuals.window_stroke = Stroke::new(1., c.border);
    let radius = CornerRadius::same(a.corner_radius as u8);
    let large = CornerRadius::same((a.corner_radius * 1.5) as u8);
    style.visuals.window_corner_radius = large;
    style.visuals.menu_corner_radius = large;
    style.visuals.window_shadow = Shadow {
        offset: [0, 10],
        blur: 32,
        spread: 0,
        color: c.shadow,
    };
    style.visuals.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: c.shadow,
    };
    style.visuals.extreme_bg_color = if c.dark { c.background } else { c.raised };
    style.visuals.text_edit_bg_color = Some(style.visuals.extreme_bg_color);
    style.visuals.faint_bg_color = c.raised;
    style.visuals.selection.bg_fill = c.tint(if c.dark { 0.3 } else { 0.24 });
    style.visuals.selection.stroke = Stroke::new(1., c.accent_text);
    style.visuals.hyperlink_color = c.accent_text;
    for v in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        v.corner_radius = radius;
        v.bg_stroke = Stroke::NONE;
        v.bg_fill = c.raised;
        v.weak_bg_fill = c.raised;
        v.fg_stroke = Stroke::new(1., c.text);
        v.expansion = 0.;
    }
    // Slider rails and checkbox wells read as quiet grooves; handles carry the accent.
    style.visuals.widgets.inactive.bg_fill = c.border;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.2, c.text);
    style.visuals.widgets.hovered.bg_fill = c.border.lerp_to_gamma(c.accent, 0.25);
    style.visuals.widgets.hovered.weak_bg_fill = c.raised.lerp_to_gamma(c.border, 0.6);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.4, c.text);
    style.visuals.widgets.active.bg_fill = c.tint(0.34);
    style.visuals.widgets.active.weak_bg_fill = c.tint(0.26);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.6, c.text);
    style.visuals.widgets.open.weak_bg_fill = c.tint(0.18);
    style.visuals.widgets.noninteractive.bg_fill = c.background;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1., c.border);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1., c.muted);
    style.visuals.widgets.noninteractive.corner_radius = radius;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(a.font_size));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(a.font_size));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(a.small_font_size));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(a.font_size + 3.));
    style.animation_time = 0.12;
    ctx.set_style(style);
}

/// Quiet chrome actions share the configured hit size, with a subtle hover surface.
pub fn chrome_button(ui: &mut Ui, rect: Rect) -> Response {
    let response = ui.put(rect, Button::new("").frame(false));
    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(
            rect,
            ui.visuals().widgets.inactive.corner_radius,
            ui.visuals().widgets.hovered.bg_fill,
        );
    }
    response
}
pub fn fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let paths: &[&str] = match crate::i18n::current() {
        crate::i18n::Language::Japanese => &[
            "C:/Windows/Fonts/YuGothM.ttc",
            "C:/Windows/Fonts/meiryo.ttc",
            "C:/Windows/Fonts/msyh.ttc",
        ],
        crate::i18n::Language::Korean => &[
            "C:/Windows/Fonts/malgun.ttf",
            "C:/Windows/Fonts/gulim.ttc",
            "C:/Windows/Fonts/msyh.ttc",
        ],
        _ => &[
            "C:/Windows/Fonts/msyh.ttc",
            "C:/Windows/Fonts/msyh.ttf",
            "C:/Windows/Fonts/simhei.ttf",
        ],
    };
    for path in paths {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("Chinese".into(), FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .push("Chinese".into());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("Chinese".into());
            break;
        }
    }
    // Segoe UI gives Latin text a calmer, native rhythm; CJK falls through to the
    // localized face above. A semibold face carries titles and the active tab.
    let latin = |name: &str| std::fs::read(format!("C:/Windows/Fonts/{name}")).ok();
    let proportional = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut strong = proportional.clone();
    if let Some(bytes) = latin("segoeui.ttf") {
        fonts
            .font_data
            .insert("Latin".into(), FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "Latin".into());
        strong.insert(0, "Latin".into());
    }
    if let Some(bytes) = latin("seguisb.ttf") {
        fonts
            .font_data
            .insert("LatinStrong".into(), FontData::from_owned(bytes).into());
        strong.insert(0, "LatinStrong".into());
    }
    fonts
        .families
        .insert(FontFamily::Name(STRONG.into()), strong);
    fonts.font_data.insert(
        "Material".into(),
        FontData::from_static(include_bytes!(
            "../../assets/material-symbols/MiCoPiSymbols.ttf"
        ))
        .into(),
    );
    fonts
        .families
        .insert(FontFamily::Name("Material".into()), vec!["Material".into()]);
    ctx.set_fonts(fonts);
}
const STRONG: &str = "Strong";
/// One radius for every panel card, its group row and the canvas stage.
pub fn card_radius(a: &Appearance) -> u8 {
    (a.corner_radius + 2.).clamp(0., 255.) as u8
}
/// Semibold text for titles; falls back to the body face before fonts are installed.
pub fn strong(ctx: &Context, size: f32) -> FontId {
    let family = FontFamily::Name(STRONG.into());
    if ctx.fonts(|f| f.families().contains(&family)) {
        FontId::new(size, family)
    } else {
        FontId::proportional(size)
    }
}
/// A rounded rectangle filled with a diagonal two-color gradient.
pub fn gradient_rect(painter: &Painter, rect: Rect, radius: f32, from: Color32, to: Color32) {
    let radius = radius
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5)
        .max(0.);
    let mut points = vec![];
    for (corner, start) in [
        (pos2(rect.right() - radius, rect.bottom() - radius), 0.),
        (pos2(rect.left() + radius, rect.bottom() - radius), 90.),
        (pos2(rect.left() + radius, rect.top() + radius), 180.),
        (pos2(rect.right() - radius, rect.top() + radius), 270.),
    ] {
        for step in 0..=6 {
            let angle = (start + step as f32 * 15f32).to_radians();
            points.push(corner + vec2(angle.cos(), angle.sin()) * radius);
        }
    }
    let span = (rect.width() + rect.height()).max(1.);
    let color = |p: Pos2| {
        let t = ((p.x - rect.left()) + (p.y - rect.top())) / span;
        from.lerp_to_gamma(to, t.clamp(0., 1.))
    };
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.center(), color(rect.center()));
    for p in &points {
        mesh.colored_vertex(*p, color(*p));
    }
    let n = points.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(Shape::mesh(mesh));
}
pub fn icon(code: char, size: f32) -> RichText {
    RichText::new(localize(code.to_string()))
        .font(FontId::new(size, FontFamily::Name("Material".into())))
}
pub fn action_icon(id: u32) -> char {
    match id {
        1 => '\u{e89c}',
        2 => '\u{eaf3}',
        3 => '\u{e161}',
        7 => '\u{eb60}',
        4 => '\u{f090}',
        5 => '\u{e166}',
        6 | 10166 | 10168 | 10171 => '\u{e15a}',
        10167 | 10172 => '\u{e166}',
        10169 | 10170 | 10173 => '\u{e3c2}',
        10 => '\u{e3ae}',
        11 => '\u{e6d0}',
        12 => '\u{e3b8}',
        13 => '\u{e764}',
        14 | 85 => '\u{e8ff}',
        86 => '\u{e900}',
        40 => '\u{ea10}',
        42 => '\u{e145}',
        43 => '\u{e14a}',
        93 => '\u{e312}',
        95 => '\u{ef7a}',
        10110 => '\u{e40a}',
        61 => '\u{e53b}',
        _ => '\u{e429}',
    }
}
pub fn keycaps(ui: &mut Ui, label: &str) {
    use crate::ui::keymap::{Chord, ALT, CTRL, SHIFT, WHEEL};
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.;
        for (i, alias) in label
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .enumerate()
        {
            if i > 0 {
                ui.weak(tr("/"));
            }
            let Ok(chord) = Chord::parse(alias) else {
                ui.small(tr(alias));
                continue;
            };
            let mut parts: Vec<RichText> = vec![];
            if chord.mods & CTRL != 0 {
                parts.push(RichText::new(localize("Ctrl")).small());
            }
            if chord.mods & ALT != 0 {
                parts.push(RichText::new(localize("Alt")).small());
            }
            if chord.mods & SHIFT != 0 {
                parts.push(icon('\u{e5f2}', 14.));
            }
            let sym = match chord.key {
                13 => Some('\u{e31b}'),
                9 => Some('\u{e31c}'),
                32 => Some('\u{e256}'),
                8 => Some('\u{e14a}'),
                37 => Some('\u{e5c4}'),
                38 => Some('\u{e5d8}'),
                39 => Some('\u{e5c8}'),
                40 => Some('\u{e5db}'),
                WHEEL => Some('\u{e323}'),
                _ => None,
            };
            if let Some(sym) = sym {
                parts.push(icon(sym, 14.));
            } else if chord.key != 0 {
                parts.push(
                    RichText::new(localize(
                        Chord {
                            key: chord.key,
                            mods: 0,
                        }
                        .text(),
                    ))
                    .small(),
                );
            }
            for (index, part) in parts.into_iter().enumerate() {
                if index > 0 {
                    ui.weak(tr("+"));
                }
                Frame::new()
                    .fill(ui.visuals().faint_bg_color)
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .corner_radius(3)
                    .inner_margin(Margin::symmetric(5, 2))
                    .show(ui, |ui| {
                        ui.label(tr(part));
                    });
            }
        }
    });
}
