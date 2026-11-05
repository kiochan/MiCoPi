//! All point filtering, pressure dynamics, dab spacing and brush rasterization live
//! in this DLL. The host supplies only scoped pixels, history and color sampling.
mod geometry;
mod input;
mod raster;
mod resample;
mod selection;
use input::{Dynamics, Sample, Stabilizer};
use micopi_plugin_api as abi;
use raster::Raster;
use std::ffi::c_void;
#[derive(Clone, Copy)]
pub struct Brush {
    pub radius: f32,
    pub opacity: f32,
    pub hardness: f32,
    pub color: u32,
    pub erase: bool,
}
struct State {
    selection: abi::Coverage,
    alpha_locked: bool,
    filter: Stabilizer,
    contact: Option<Contact>,
    spacing: f32,
    minimum_opacity: f32,
}
impl Default for State {
    fn default() -> Self {
        Self {
            selection: Default::default(),
            alpha_locked: false,
            filter: Default::default(),
            contact: None,
            spacing: 0.11,
            minimum_opacity: 0.,
        }
    }
}
struct Contact {
    handler: u32,
    parameters: abi::ToolParameters,
    pen: bool,
    erase: bool,
    last: abi::ToolSample,
    raster: Raster,
    minimum_opacity: f32,
}
impl Contact {
    fn brush(&self, pressure: f32) -> Brush {
        let p = self.parameters;
        let brush = Brush {
            radius: p.radius,
            opacity: p.opacity,
            hardness: p.hardness,
            color: p.color,
            erase: self.erase,
        };
        if self.pen {
            let mut brush = Dynamics {
                size: p.flags & abi::PRESSURE_SIZE != 0,
                opacity: p.flags & abi::PRESSURE_OPACITY != 0,
                gamma: p.pressure_gamma,
                minimum: p.minimum_radius,
            }
            .apply(brush, pressure);
            if p.flags & abi::PRESSURE_OPACITY != 0 && pressure > 0. && self.minimum_opacity > 0. {
                let curve = pressure
                    .clamp(0., 1.)
                    .powf(p.pressure_gamma.clamp(0.25, 4.));
                brush.opacity =
                    p.opacity * (self.minimum_opacity + (1. - self.minimum_opacity) * curve);
            }
            brush
        } else {
            brush
        }
    }
}
impl State {
    fn input(
        &mut self,
        handler: u32,
        p: abi::ToolSample,
        parameters: Option<abi::ToolParameters>,
        host: &abi::ToolHost,
        out: &mut abi::ToolOutput,
    ) -> i32 {
        match p.phase {
            abi::DOWN => {
                if self.contact.is_some() || !(1..=5).contains(&handler) {
                    return abi::ERROR;
                }
                if (handler == 1 || handler == 2)
                    && !self.selection.data.is_null()
                    && (p.width, p.height) != (self.selection.width, self.selection.height)
                {
                    return abi::ERROR;
                }
                let Some(parameters) = parameters else {
                    return abi::ERROR;
                };
                if handler == 3 {
                    let mut color = parameters.color;
                    let status = unsafe { (host.pick)(host.context, p.x, p.y, &mut color) };
                    if status == abi::OK {
                        out.flags = abi::COLOR_CHANGED;
                        out.color = color;
                    }
                    return status;
                }
                if handler == 5 {
                    let zoom = (p.zoom
                        * if p.flags & abi::ERASER != 0 {
                            1. / 1.25
                        } else {
                            1.25
                        })
                    .clamp(0.02, 32.);
                    *out = abi::ToolOutput {
                        flags: abi::VIEW_CHANGED,
                        zoom,
                        pan_dx: (p.width as f32 / 2. - p.x) * (zoom - p.zoom),
                        pan_dy: (p.height as f32 / 2. - p.y) * (zoom - p.zoom),
                        ..Default::default()
                    };
                    return abi::OK;
                }
                let mut contact = Contact {
                    handler,
                    parameters,
                    pen: p.flags & abi::PEN != 0,
                    erase: handler == 2 || p.flags & abi::ERASER != 0,
                    last: p,
                    minimum_opacity: self.minimum_opacity,
                    raster: Raster {
                        selection: self.selection,
                        alpha_locked: self.alpha_locked,
                        spacing: self.spacing,
                        width: p.width,
                        height: p.height,
                        ..Default::default()
                    },
                };
                if handler != 4 {
                    let sample = self.filter.reset(Sample {
                        x: p.x,
                        y: p.y,
                        pressure: p.pressure,
                        time: p.time,
                    });
                    contact
                        .raster
                        .begin(host, sample.x, sample.y, contact.brush(sample.pressure));
                } else {
                    out.flags |= abi::VIEW_CHANGED;
                    out.zoom = p.zoom;
                }
                self.contact = Some(contact);
            }
            abi::MOVE | abi::UP => {
                if let Some(contact) = &mut self.contact {
                    if contact.handler != handler {
                        return abi::ERROR;
                    }
                    if handler == 4 {
                        out.flags = abi::VIEW_CHANGED;
                        out.zoom = p.zoom;
                        out.pan_dx = p.view_x - contact.last.view_x;
                        out.pan_dy = p.view_y - contact.last.view_y;
                    } else {
                        let pressure = if p.phase == abi::UP && p.pressure == 0. {
                            contact.last.pressure
                        } else {
                            p.pressure
                        };
                        let sample = self.filter.push(
                            Sample {
                                x: p.x,
                                y: p.y,
                                pressure,
                                time: p.time,
                            },
                            contact.parameters.stabilizer,
                        );
                        contact.raster.line(
                            host,
                            sample.x,
                            sample.y,
                            contact.brush(sample.pressure),
                        );
                    }
                    contact.last = p;
                }
                if p.phase == abi::UP {
                    self.finish(host);
                }
            }
            abi::CANCEL => self.finish(host),
            _ => return abi::ERROR,
        }
        if let Some(contact) = &self.contact {
            if contact.raster.failed {
                return abi::ERROR;
            }
            out.flags |= if contact.handler == 4 {
                abi::PANNING
            } else {
                abi::DRAWING
            };
        }
        abi::OK
    }
    fn finish(&mut self, host: &abi::ToolHost) {
        if let Some(mut contact) = self.contact.take() {
            if contact.handler != 4 {
                if let Some(sample) = self.filter.finish() {
                    contact
                        .raster
                        .line(host, sample.x, sample.y, contact.brush(sample.pressure));
                }
            }
        }
        self.filter.cancel();
    }
}
unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(State::default())).cast()
}
unsafe extern "C" fn destroy(state: *mut c_void) {
    if !state.is_null() {
        drop(Box::from_raw(state.cast::<State>()));
    }
}
unsafe extern "C" fn pointer(
    state: *mut c_void,
    handler: u32,
    sample: *const abi::ToolSample,
    parameters: *const abi::ToolParameters,
    host: *const abi::ToolHost,
    out: *mut abi::ToolOutput,
) -> i32 {
    if state.is_null() || sample.is_null() || host.is_null() || out.is_null() {
        return abi::ERROR;
    }
    let host = &*host;
    if host.abi_version != abi::TOOL_ABI_VERSION
        || host.struct_size < std::mem::size_of::<abi::ToolHost>() as u32
    {
        return abi::ERROR;
    }
    let state = &mut *state.cast::<State>();
    *out = abi::ToolOutput::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        state.input(
            handler,
            *sample,
            parameters.as_ref().copied(),
            host,
            &mut *out,
        )
    }));
    match result {
        Ok(abi::OK) => abi::OK,
        _ => {
            state.contact = None;
            state.filter.cancel();
            abi::ERROR
        }
    }
}
unsafe extern "C" fn describe() -> abi::Bytes {
    abi::Bytes::borrowed(b"{\"id\":\"painting\"}")
}
static PLUGIN: abi::PluginApi = abi::PluginApi {
    abi_version: abi::ABI_VERSION,
    struct_size: std::mem::size_of::<abi::PluginApi>() as u32,
    describe: Some(describe),
    create: None,
    destroy: None,
    invoke: None,
};
static TOOLS: abi::ToolApi = abi::ToolApi {
    abi_version: abi::TOOL_ABI_VERSION,
    struct_size: std::mem::size_of::<abi::ToolApi>() as u32,
    create: Some(create),
    destroy: Some(destroy),
    pointer: Some(pointer),
};
#[no_mangle]
pub extern "C" fn micopi_plugin_v1() -> *const abi::PluginApi {
    &PLUGIN
}
#[no_mangle]
pub extern "C" fn micopi_tools_v1() -> *const abi::ToolApi {
    &TOOLS
}

unsafe extern "C" fn configure(state: *mut c_void, options: *const abi::ToolOptions) -> i32 {
    if state.is_null() || options.is_null() {
        return abi::ERROR;
    }
    let header = std::ptr::read_unaligned(options.cast::<abi::Header>());
    if header.abi_version != 1
        || header.struct_size < std::mem::size_of::<abi::ToolOptions>() as u32
    {
        return abi::ERROR;
    }
    let flags = if header.struct_size >= std::mem::size_of::<abi::ToolOptionsExt>() as u32 {
        std::ptr::read_unaligned(options.cast::<abi::ToolOptionsExt>()).flags
    } else {
        0
    };
    if (header.struct_size > std::mem::size_of::<abi::ToolOptions>() as u32
        && header.struct_size < std::mem::size_of::<abi::ToolOptionsExt>() as u32)
        || flags & !(abi::TOOL_OPTION_ALPHA_LOCK | abi::TOOL_OPTION_SELECTION) != 0
    {
        return abi::ERROR;
    }
    let mask = if flags & abi::TOOL_OPTION_SELECTION != 0 {
        if header.struct_size < std::mem::size_of::<abi::ToolOptionsSelection>() as u32 {
            return abi::ERROR;
        }
        let mask = std::ptr::read_unaligned(options.cast::<abi::ToolOptionsSelection>()).selection;
        if !selection::valid(&mask) || mask.data.is_null() {
            return abi::ERROR;
        }
        mask
    } else {
        abi::Coverage::default()
    };
    let options = std::ptr::read_unaligned(options);
    if !options.spacing.is_finite()
        || !(0.01..=2.).contains(&options.spacing)
        || !options.minimum_opacity.is_finite()
        || !(0. ..=1.).contains(&options.minimum_opacity)
    {
        return abi::ERROR;
    }
    let state = &mut *state.cast::<State>();
    if state.contact.is_some() {
        return abi::ERROR;
    }
    state.spacing = options.spacing;
    state.minimum_opacity = options.minimum_opacity;
    state.alpha_locked = flags & abi::TOOL_OPTION_ALPHA_LOCK != 0;
    state.selection = mask;
    abi::OK
}
static OPTIONS: abi::ToolOptionsApiExt = abi::ToolOptionsApiExt {
    base: abi::ToolOptionsApi {
        abi_version: 1,
        struct_size: std::mem::size_of::<abi::ToolOptionsApiExt>() as u32,
        configure: Some(configure),
    },
    features: abi::TOOL_OPTION_ALPHA_LOCK | abi::TOOL_OPTION_SELECTION,
};
#[no_mangle]
pub extern "C" fn micopi_tool_options_v1() -> *const abi::ToolOptionsApi {
    &OPTIONS.base
}

#[cfg(test)]
mod option_tests {
    use super::*;
    fn options() -> abi::ToolOptions {
        abi::ToolOptions {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::ToolOptions>() as u32,
            spacing: 0.5,
            minimum_opacity: 0.4,
        }
    }
    fn contact() -> Contact {
        Contact {
            handler: 1,
            parameters: abi::ToolParameters {
                radius: 10.,
                opacity: 0.8,
                hardness: 1.,
                color: 0xff0000,
                stabilizer: 0.,
                pressure_gamma: 1.,
                minimum_radius: 0.08,
                flags: abi::PRESSURE_OPACITY,
            },
            pen: true,
            erase: false,
            last: unsafe { std::mem::zeroed() },
            raster: Default::default(),
            minimum_opacity: 0.4,
        }
    }
    #[test]
    fn pressure_floor_respects_zero_full_and_mouse_pressure() {
        let mut c = contact();
        assert_eq!(c.brush(0.).opacity, 0.);
        assert!((c.brush(0.5).opacity - 0.56).abs() < 1e-6);
        assert_eq!(c.brush(1.).opacity, 0.8);
        c.pen = false;
        assert_eq!(c.brush(0.1).opacity, 0.8);
    }
    #[test]
    fn option_abi_validates_header_ranges_and_contact_ownership() {
        let mut state = State::default();
        let pointer = (&mut state as *mut State).cast();
        let valid = options();
        assert_eq!(unsafe { configure(pointer, &valid) }, abi::OK);
        assert_eq!(state.spacing, 0.5);
        for i in 0..5 {
            let mut bad = valid;
            match i {
                0 => bad.struct_size = 8,
                1 => bad.abi_version = 2,
                2 => bad.spacing = f32::NAN,
                3 => bad.spacing = 0.,
                _ => bad.minimum_opacity = 1.1,
            }
            assert_eq!(unsafe { configure(pointer, &bad) }, abi::ERROR);
            assert_eq!(state.spacing, 0.5);
        }
        assert_eq!(unsafe { configure(pointer, std::ptr::null()) }, abi::ERROR);
        state.contact = Some(contact());
        assert_eq!(unsafe { configure(pointer, &valid) }, abi::ERROR);
    }
    #[test]
    fn additive_lock_options_accept_legacy_prefix_and_reject_unknown_flags() {
        let mut state = State::default();
        let ptr = (&mut state as *mut State).cast();
        let mut extended = abi::ToolOptionsExt {
            base: abi::ToolOptions {
                struct_size: std::mem::size_of::<abi::ToolOptionsExt>() as u32,
                ..options()
            },
            flags: abi::TOOL_OPTION_ALPHA_LOCK,
        };
        assert_eq!(unsafe { configure(ptr, &extended.base) }, abi::OK);
        assert!(state.alpha_locked);
        extended.flags = 4;
        assert_eq!(unsafe { configure(ptr, &extended.base) }, abi::ERROR);
        assert!(state.alpha_locked);
        extended.flags = abi::TOOL_OPTION_SELECTION;
        assert_eq!(unsafe { configure(ptr, &extended.base) }, abi::ERROR);
        assert_eq!(unsafe { configure(ptr, &options()) }, abi::OK);
        assert!(!state.alpha_locked);
        extended.base.struct_size = 17;
        assert_eq!(unsafe { configure(ptr, &extended.base) }, abi::ERROR);
    }
    #[test]
    fn selection_options_validate_complete_tail_and_reset_legacy_calls() {
        let mut state = State::default();
        let ptr = (&mut state as *mut State).cast();
        let coverage = [255u8, 128, 0, 255];
        let mut options = abi::ToolOptionsSelection {
            base: abi::ToolOptionsExt {
                base: abi::ToolOptions {
                    struct_size: std::mem::size_of::<abi::ToolOptionsSelection>() as u32,
                    ..options()
                },
                flags: abi::TOOL_OPTION_SELECTION,
            },
            selection: abi::Coverage {
                data: coverage.as_ptr(),
                len: 4,
                width: 2,
                height: 2,
                stride: 2,
            },
        };
        assert_eq!(unsafe { configure(ptr, &options.base.base) }, abi::OK);
        assert_eq!(state.selection.data, coverage.as_ptr());
        options.selection.len = 3;
        assert_eq!(unsafe { configure(ptr, &options.base.base) }, abi::ERROR);
        assert_eq!(state.selection.len, 4);
        assert_eq!(unsafe { configure(ptr, &self::options()) }, abi::OK);
        assert!(state.selection.data.is_null());
    }
}
