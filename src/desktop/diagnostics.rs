//! Cached presentation observations exposed through the ordinary Session registry.
use super::*;
use serde_json::json;

impl Studio {
    pub(super) fn report_diagnostics(&mut self) {
        let material = |native: Option<&Native>| {
            native
                .and_then(Native::material_status)
                .map(|s| s.diagnostics())
        };
        let windows: Vec<_> = self.native_windows.iter().map(|(index, window)| {
            json!({"surface":index,"material":material(window.native.as_ref())})
        }).collect();
        self.editor.session.desktop_diagnostics = Some(json!({
            "renderer":"egui_glow",
            "appearance":{"theme":self.editor.settings.ui.theme,
                "window_blur":self.editor.settings.ui.window_blur,
                "surface_opacity":self.editor.settings.ui.surface_opacity},
            "settings_path":self.editor.settings.path,
            "root_material":material(self.editor.native.as_ref()),
            "native_windows":windows,
            "capture":{"source":"opengl_framebuffer","includes_desktop_backdrop":false,
                "root_action":"ui.capture","panel_action":"window.capture",
                "status_action":"window.list"},
            "note":"Native negotiation does not verify visible Acrylic. Windows may apply accessibility, battery or hardware fallbacks. Renderer PNGs exclude the desktop compositor."
        }));
    }
}
