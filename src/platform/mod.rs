//! OS integration. The desktop shell currently supports Windows only.
mod file;
pub(crate) use file::replace_file;

#[cfg(windows)]
pub mod windows;
