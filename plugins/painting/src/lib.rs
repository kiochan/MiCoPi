//! All point filtering, pressure dynamics, dab spacing and brush rasterization live
//! in this DLL. The host supplies only scoped pixels, history and color sampling.
mod input;
mod raster;
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
#[derive(Default)]
struct State {
    filter: Stabilizer,
    contact: Option<Contact>,
}
struct Contact {
    handler: u32,
    parameters: abi::ToolParameters,
    pen: bool,
    erase: bool,
    last: abi::ToolSample,
    raster: Raster,
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
            Dynamics {
                size: p.flags & abi::PRESSURE_SIZE != 0,
                opacity: p.flags & abi::PRESSURE_OPACITY != 0,
                gamma: p.pressure_gamma,
                minimum: p.minimum_radius,
            }
            .apply(brush, pressure)
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
                    raster: Raster {
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
