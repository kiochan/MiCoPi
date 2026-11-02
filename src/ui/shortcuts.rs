//! Photoshop-style bindings for the actions MiCoPi currently supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Action {
    Brush,
    Eraser,
    Picker,
    Hand,
    Zoom,
    Undo,
    Redo,
    New,
    Open,
    Save,
    SaveAs,
    Export,
    NewLayer,
    Fit,
    ActualPixels,
    ZoomIn,
    ZoomOut,
    Preferences,
    ShowShortcuts,
    TogglePanels,
    ToggleLayers,
    SmallerBrush,
    LargerBrush,
    SofterBrush,
    HarderBrush,
    OpacityDigit(u8),
    DefaultColors,
    SwapColors,
    OpenPalette,
    ReloadBindings,
    ResetBindings,
    Cancel,
    UiCommand(u32),
}
#[cfg(test)]
pub fn resolve(key: usize, ctrl: bool, shift: bool, alt: bool) -> Option<Action> {
    super::keymap::Keymap::default()
        .resolve(key, ctrl as u8 | ((shift as u8) << 1) | ((alt as u8) << 2))
}
pub fn brush_diameter(current: f32, increase: bool) -> f32 {
    const SIZES: [f32; 27] = [
        1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 15., 20., 25., 30., 35., 40., 45., 50., 60., 70.,
        80., 90., 100., 125., 150., 175., 200.,
    ];
    if increase {
        SIZES
            .iter()
            .copied()
            .find(|n| *n > current + 0.01)
            .unwrap_or(200.)
    } else {
        SIZES
            .iter()
            .rev()
            .copied()
            .find(|n| *n < current - 0.01)
            .unwrap_or(1.)
    }
}
#[derive(Default)]
pub struct OpacityInput {
    last: Option<(u8, u32)>,
}
impl OpacityInput {
    pub fn digit(&mut self, digit: u8, time: u32) -> f32 {
        if let Some((first, then)) = self.last {
            if time.wrapping_sub(then) <= 500 {
                self.last = None;
                return (first * 10 + digit) as f32 / 100.;
            }
        }
        self.last = Some((digit, time));
        if digit == 0 {
            1.
        } else {
            digit as f32 / 10.
        }
    }
    pub fn reset(&mut self) {
        self.last = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn size_steps_and_ps_tool_and_export_keys() {
        assert_eq!(brush_diameter(12., true), 15.);
        assert_eq!(brush_diameter(12., false), 10.);
        assert_eq!(brush_diameter(200., true), 200.);
        assert_eq!(brush_diameter(1., false), 1.);
        assert_eq!(resolve(0x48, false, false, false), Some(Action::Hand));
        assert_eq!(
            resolve(0x4a, true, false, false),
            Some(Action::UiCommand(10140))
        );
        assert_eq!(resolve(0x5a, false, false, false), Some(Action::Zoom));
        assert_eq!(resolve(0x57, true, true, true), Some(Action::Export));
        assert_eq!(resolve(0x4b, true, true, true), Some(Action::ShowShortcuts));
    }
    #[test]
    fn photoshop_keys_do_not_reuse_unimplemented_ps_commands() {
        assert_eq!(resolve(0x59, true, false, false), None);
        assert_eq!(resolve(0x46, false, false, false), None);
        assert_eq!(resolve(0x71, false, false, false), None);
        assert_eq!(resolve(0x74, false, false, false), None);
        assert_eq!(resolve(0x5a, true, true, false), Some(Action::Redo));
        assert_eq!(
            resolve(0x31, true, false, false),
            Some(Action::ActualPixels)
        );
        assert_eq!(resolve(0xdb, false, true, false), Some(Action::SofterBrush));
    }
    #[test]
    fn opacity_digits_support_two_digits_timeout_and_clock_wrap() {
        let mut i = OpacityInput::default();
        assert_eq!(i.digit(2, 10), 0.2);
        assert_eq!(i.digit(5, 50), 0.25);
        assert_eq!(i.digit(0, 1000), 1.);
        assert_eq!(i.digit(0, 1050), 0.);
        i.digit(1, 2000);
        assert_eq!(i.digit(5, 3000), 0.5);
        i.reset();
        i.digit(3, u32::MAX - 5);
        assert_eq!(i.digit(5, 5), 0.35);
    }
}
