//! Cold raw-A8 staging. The native sampler owns all byte geometry and filtering.
use crate::{
    application::{ActionError, Session},
    engine::raster_masks::{PlaneView, SamplePlane},
};
use micopi_plugin_api as abi;

pub(crate) enum Sampling {
    Reframe(abi::A8Reframe),
    Geometry(abi::A8Geometry),
    Resample(abi::Resample),
}

pub(crate) fn require_idle(s: &Session) -> Result<(), ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before changing canvas geometry",
        ));
    }
    Ok(())
}

pub(crate) fn available(s: &Session) -> bool {
    s.registry
        .native_plugins
        .get("painting")
        .is_some_and(|plugin| plugin.supports_a8_sampler())
}

/// Stage every owner, including hidden, disabled and unlinked masks. Raw input
/// stays borrowed; effective caches and compressed pixel selections are separate.
pub(crate) fn stage(
    s: &Session,
    width: usize,
    height: usize,
    sampling: &Sampling,
) -> Result<Vec<(u64, SamplePlane)>, String> {
    s.doc.validate_canvas_geometry_frame(width, height)?;
    if !s.doc.has_raster_masks() {
        return Ok(Vec::new());
    }
    let plugin = s
        .registry
        .native_plugins
        .get("painting")
        .filter(|plugin| plugin.supports_a8_sampler())
        .ok_or("Painting DLL does not support raw raster-mask geometry")?;
    let mut planes = Vec::with_capacity(s.doc.mask_state().items.len());
    for metadata in &s.doc.mask_state().items {
        let source = match s.doc.mask_plane(metadata.id)? {
            PlaneView::Uniform(value) => abi::CompositeMask {
                data: std::ptr::null(),
                len: 0,
                stride: 0,
                constant: u32::from(value),
            },
            PlaneView::Dense(data) => abi::CompositeMask {
                data: data.as_ptr(),
                len: data.len(),
                stride: s.doc.width as u32,
                constant: 0,
            },
        };
        let mut data = vec![0; width * height];
        let mut output = abi::A8Pixels {
            data: data.as_mut_ptr(),
            len: data.len(),
            width: width as u32,
            height: height as u32,
            stride: width as u32,
            origin_x: 0,
            origin_y: 0,
        };
        let (sw, sh) = (s.doc.width as u32, s.doc.height as u32);
        match sampling {
            Sampling::Reframe(options) => {
                plugin.a8_reframe(options, &source, sw, sh, &mut output)?
            }
            Sampling::Geometry(options) => {
                plugin.a8_geometry(options, &source, sw, sh, &mut output)?
            }
            Sampling::Resample(options) => {
                plugin.a8_resample(options, &source, sw, sh, &mut output)?
            }
        }
        // Storage canonicalization does not change samples or run an image kernel.
        let samples = if data.iter().all(|value| *value == data[0]) {
            SamplePlane::Uniform(data[0])
        } else {
            SamplePlane::Dense(data)
        };
        planes.push((metadata.id, samples));
    }
    Ok(planes)
}
