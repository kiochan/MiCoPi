//! OS integration. The desktop shell currently supports Windows only.
pub(crate) mod clipboard;
mod file;
pub(crate) use file::replace_file;

#[cfg(windows)]
pub mod windows;
