//! MiCoPi painting engine, workspace model and native desktop integration.
//!
//! The engine and workspace can be tested without creating a desktop window.
//! Windows-specific APIs live under [`platform`].
pub mod engine;
pub mod platform;
pub mod ui;

#[cfg(all(windows, feature = "desktop"))]
pub mod desktop;

pub mod application;
pub mod automation;
pub mod plugins;

pub mod configuration;
pub mod i18n;
