//! Editor-style shell: activity rail, title command center and canvas toolbar.
use super::*;
use crate::ui::{Layout, ACTIVITY_WIDTH};
impl App {
    pub(super) unsafe fn draw_shell(&mut self, dc: Dc, l: &Layout) {
        let t = self.workspace.theme();
        Self::fill(dc, rect(0, 0, self.size.0, TOP), t.panel);
        SelectObject(dc, self.title_font);
        Self::centered(dc, rect(0, 0, 48, TOP), "M", t.accent);
        SelectObject(dc, self.font);
        for (x, label, id) in [
            (56, "File", 90),
            (108, "Edit", 91),
            (160, "View", 92),
            (212, "Workspace", 50),
        ] {
            let r = rect(x, 4, if id == 50 { 68 } else { 48 }, 28);
            if self.hovered.map_or(false, |h| h.0 == id) {
                self.rounded(dc, r, t.selected, 4);
            }
            Self::centered(dc, r, label, t.muted);
            self.hit(r, id);
        }
        self.draw_chrome(dc);
        Self::fill(dc, rect(0, TOP - 1, self.size.0, 1), self.border_color());
        if l.activity.w > 0 {
            Self::fill(
                dc,
                rect(0, l.activity.y, l.activity.w, l.activity.h),
                t.panel,
            );
            Self::fill(
                dc,
                rect(ACTIVITY_WIDTH - 1, l.activity.y, 1, l.activity.h),
                self.border_color(),
            );
            for (y, id, icon, active) in [
                (
                    TOP + 8,
                    10110,
                    0,
                    self.workspace.tools_visible && self.workspace.tools_page == 0,
                ),
                (
                    TOP + 56,
                    10111,
                    1,
                    self.workspace.tools_visible && self.workspace.tools_page == 1,
                ),
                (TOP + 104, 61, 2, self.workspace.layers_visible),
                (TOP + 152, 95, 3, false),
                (self.size.1 - BOTTOM - 96, 93, 4, false),
                (self.size.1 - BOTTOM - 48, 50, 5, false),
            ] {
                let r = rect(0, y, 48, 44);
                let hovered = self.hovered.map_or(false, |h| h.0 == id);
                if hovered {
                    self.rounded(dc, rect(5, y + 2, 38, 40), t.button, 5);
                }
                if active {
                    Self::fill(dc, rect(0, y + 9, 2, 26), t.accent);
                }
                let symbol = match icon {
                    0 => icons::PALETTE,
                    1 => icons::BRUSH,
                    2 => icons::LAYERS,
                    3 => icons::SEARCH,
                    4 => icons::KEYBOARD,
                    _ => icons::SETTINGS,
                };
                self.icons.draw(
                    dc,
                    rect(12, y + 10, 24, 24),
                    symbol,
                    24,
                    if active || hovered { t.text } else { t.muted },
                );
                self.hit(r, id);
            }
        }
        let p = l.toolbar;
        Self::fill(dc, rect(p.x, p.y, p.w, p.h), t.panel);
        let saved = SaveDC(dc);
        IntersectClipRect(dc, p.x, p.y, p.x + p.w, p.y + p.h);
        self.widget_clip = Some(rect(p.x, p.y, p.w, p.h));
        let tools = if p.w >= 420 { 5 } else { 2 };
        for (i, (label, id, tool)) in [
            ("Brush Tool", 10, Tool::Brush),
            ("Eraser", 11, Tool::Eraser),
            ("Eyedropper", 12, Tool::Picker),
            ("Hand", 13, Tool::Hand),
            ("Zoom", 14, Tool::Zoom),
        ]
        .iter()
        .enumerate()
        .take(tools)
        {
            self.button(
                dc,
                rect(p.x + 8 + i as i32 * 42, p.y + 5, 38, 24),
                label,
                *id,
                self.tool == *tool,
            );
        }
        SelectObject(dc, self.small_font);
        if p.w >= 650 {
            Self::text(
                dc,
                rect(p.x + 226, p.y + 3, p.w - 420, 28),
                &format!("{:.0} px  ·  {:.0}%", self.radius * 2., self.opacity * 100.),
                t.muted,
            );
        }
        if p.w >= 360 {
            self.button(
                dc,
                rect(p.x + p.w - 174, p.y + 5, 64, 24),
                "Fit to Window",
                40,
                false,
            );
            self.button(dc, rect(p.x + p.w - 104, p.y + 5, 36, 24), "1:1", 41, false);
            self.button(
                dc,
                rect(p.x + p.w - 62, p.y + 5, 54, 24),
                "Export",
                4,
                false,
            );
        } else if p.w >= 180 {
            self.button(dc, rect(p.x + p.w - 62, p.y + 5, 54, 24), "Fit", 40, false);
        }
        self.widget_clip = None;
        RestoreDC(dc, saved);
        Self::fill(dc, rect(p.x, p.y + p.h - 1, p.w, 1), self.border_color());
        SelectObject(dc, self.font);
    }
}
