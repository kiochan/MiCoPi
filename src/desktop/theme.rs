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
}
pub fn rgb(value: u32) -> Color32 {
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}
impl Colors {
    pub fn new(a: &Appearance) -> Self {
        let (background, panel, raised, border, text, muted) = match a.theme.as_str() {
            "light" => (0xe9edf2, 0xf8fafc, 0xffffff, 0xd9e0e8, 0x273544, 0x697c8f),
            "warm" => (0xe9e5de, 0xf8f5ef, 0xfffdf8, 0xded8cd, 0x383b36, 0x777c70),
            _ => (0x161b21, 0x20262e, 0x2a333e, 0x333e49, 0xe0e7ee, 0x91a1b2),
        };
        Self {
            background: rgb(background),
            panel: rgb(panel),
            raised: rgb(raised),
            border: rgb(border),
            text: rgb(text),
            muted: rgb(muted),
            accent: rgb(
                u32::from_str_radix(a.accent.trim_start_matches('#'), 16).unwrap_or(0x76c7ba)
            ),
        }
    }
}
pub fn apply(ctx: &Context, a: &Appearance) {
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
        (a.control_height - a.font_size) * 0.35,
    );
    style.spacing.interact_size = vec2(a.control_height, a.control_height);
    style.spacing.slider_width = 112.;
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.combo_width = 120.;
    style.spacing.menu_margin = Margin::same(a.panel_padding as i8);
    style.spacing.window_margin = Margin::same(a.panel_padding as i8);
    style.spacing.icon_width = a.icon_size.min(22.);
    style.visuals.override_text_color = Some(c.text);
    style.visuals.handle_shape = egui::style::HandleShape::Circle;
    style.visuals.slider_trailing_fill = true;
    style.visuals.panel_fill = c.panel;
    style.visuals.window_fill = c.panel;
    style.visuals.window_stroke = Stroke::new(1., c.border);
    style.visuals.window_corner_radius = CornerRadius::same(a.corner_radius as u8);
    style.visuals.menu_corner_radius = CornerRadius::same(a.corner_radius as u8);
    style.visuals.extreme_bg_color = c.background;
    style.visuals.faint_bg_color = c.raised;
    style.visuals.selection.bg_fill = c.accent.gamma_multiply(0.24);
    style.visuals.selection.stroke = Stroke::new(1., c.accent);
    for v in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        v.corner_radius = CornerRadius::same(a.corner_radius as u8);
        v.bg_stroke = Stroke::NONE;
        v.bg_fill = c.raised;
        v.weak_bg_fill = c.raised;
        v.fg_stroke = Stroke::new(1., c.text);
    }
    style.visuals.widgets.hovered.bg_fill = c.border;
    style.visuals.widgets.active.bg_fill = c.accent.gamma_multiply(0.32);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1., c.border);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1., c.muted);
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
    style.animation_time = 0.1;
    ctx.set_style(style);
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
        6 => '\u{e15a}',
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
