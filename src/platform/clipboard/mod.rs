//! Bounded image interchange; document pixel algorithms remain in native plugins.
mod image;
pub(crate) use image::{decode_dib, decode_png, encode_dib, encode_png, Image, View, MAX_BYTES};
pub(crate) struct ReadImage {
    pub image: Image,
    pub metadata: Option<Vec<u8>>,
    pub format: &'static str,
}
/// External desktop resource. The Session owns an adapter, never OS clipboard storage.
pub(crate) trait SystemClipboard: Send {
    fn status(&self) -> Result<serde_json::Value, String>;
    fn read(&self) -> Result<ReadImage, String>;
    fn write(&self, image: &View<'_>, metadata: &[u8]) -> Result<serde_json::Value, String>;
}
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) fn for_window(window: isize) -> Box<dyn SystemClipboard> {
    Box::new(windows::Clipboard(window))
}
