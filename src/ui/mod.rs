//! Platform-independent workspace layout, themes and settings.
pub mod docking;
pub mod keymap;
pub mod palette;
mod settings;
pub mod shortcuts;
pub mod workspace;

pub use workspace::{
    layer_rows, Box2, Layout, Theme, ToolsGeometry, Workspace, ACTIVITY_WIDTH, CANVAS_TOOLBAR,
    CHROME_BOTTOM, CHROME_TOP, DOCUMENT_TABS, PANEL_HEADER,
};
