//! Typed native tool routing. Filtering, pressure, spacing and rasterization are
//! implemented by DLLs; the host retains only contact ownership and document undo.
use super::native::ToolInstance;
use crate::application::{
    parse_color, ActionError, Parameters, Pointer, PointerPhase, Session, ToolKind,
};
use micopi_plugin_api as abi;
#[derive(Default)]
pub struct StrokeEngine {
    state: Option<Contact>,
}
struct Contact {
    instance: ToolInstance,
    owner: String,
    handler: u32,
    kind: ToolKind,
    write_pixels: bool,
    flags: u32,
    last: Pointer,
}
impl StrokeEngine {
    pub fn last_sample(&self) -> Option<Pointer> {
        self.state.as_ref().map(|s| s.last)
    }
    pub fn active(&self) -> bool {
        self.state.is_some()
    }
    pub fn drawing(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(|s| s.flags & abi::DRAWING != 0)
    }
    pub fn panning(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(|s| s.flags & abi::PANNING != 0)
    }
    pub fn input(&mut self, s: &mut Session, p: Pointer) -> Result<(), ActionError> {
        if p.phase == PointerPhase::Down {
            if self.active() {
                return Err(ActionError::new(
                    "stroke_active",
                    "Current tool has not received up/cancel",
                ));
            }
            let kind = p.tool_override.unwrap_or(s.tool);
            let base = match kind {
                ToolKind::Brush => "brush",
                ToolKind::Eraser => "eraser",
                ToolKind::Picker => "picker",
                ToolKind::Hand => "hand",
                ToolKind::Zoom => "zoom",
            };
            let id = if p.tool_override.is_some() {
                base
            } else {
                &s.selected_tool
            };
            let (owner, handler) = s
                .registry
                .native_tools
                .get(id)
                .or_else(|| s.registry.native_tools.get(base))
                .cloned()
                .ok_or_else(|| {
                    ActionError::new(
                        "tool_plugin_missing",
                        "Painting plugin is not loaded; check the painting DLL and plugin settings",
                    )
                })?;
            let parameters = parameters(&s.parameters)?;
            let painting = matches!(kind, ToolKind::Brush | ToolKind::Eraser);
            if painting {
                super::PixelAccess::Pixels.require(&s.doc, s.doc.active)?;
                if s.doc.has_raster_masks() {
                    s.doc
                        .ensure_graph_renderer()
                        .map_err(|error| ActionError::new("mask_renderer", error))?;
                }
                if s.doc.alpha_locked(s.doc.active) && (kind == ToolKind::Eraser || p.eraser) {
                    return Err(ActionError::new(
                        "alpha_locked",
                        "Unlock layer transparency before erasing",
                    ));
                }
                if p.x < 0. || p.y < 0. || p.x >= s.doc.width as f32 || p.y >= s.doc.height as f32 {
                    return Err(ActionError::new(
                        "outside_canvas",
                        "Stroke starts outside the canvas",
                    ));
                }
                if !s.doc.layer_visible(s.doc.active) {
                    return Err(ActionError::new("hidden_layer", "Current layer is hidden"));
                }
            }
            let instance = match s.tool_instances.remove(&owner) {
                Some(instance) => instance,
                None => {
                    ToolInstance::new(s.registry.native_plugins.get(&owner).cloned().ok_or_else(
                        || ActionError::new("tool_plugin_missing", "Tool DLL is not loaded"),
                    )?)?
                }
            };
            if painting {
                s.doc.begin_plugin_stroke();
            }
            let contact = Contact {
                instance,
                owner,
                handler,
                kind,
                write_pixels: painting,
                flags: 0,
                last: p,
            };
            return self.deliver(s, contact, p, Some(&parameters));
        }
        if let Some(contact) = self.state.take() {
            self.deliver(s, contact, p, None)
        } else {
            Ok(())
        }
    }
    fn deliver(
        &mut self,
        s: &mut Session,
        mut contact: Contact,
        p: Pointer,
        parameters: Option<&abi::ToolParameters>,
    ) -> Result<(), ActionError> {
        let sample = abi::ToolSample {
            phase: match p.phase {
                PointerPhase::Down => abi::DOWN,
                PointerPhase::Move => abi::MOVE,
                PointerPhase::Up => abi::UP,
                PointerPhase::Cancel => abi::CANCEL,
            },
            flags: if p.pen { abi::PEN } else { 0 } | if p.eraser { abi::ERASER } else { 0 },
            x: p.x,
            y: p.y,
            pressure: p.pressure,
            time: p.time,
            view_x: p.view_x.unwrap_or(p.x),
            view_y: p.view_y.unwrap_or(p.y),
            width: s.doc.width as u32,
            height: s.doc.height as u32,
            zoom: s.view.zoom,
        };
        let painting = contact.write_pixels;
        let ended = matches!(p.phase, PointerPhase::Up | PointerPhase::Cancel);
        let result = contact
            .instance
            .pointer(s, contact.handler, sample, parameters, painting)
            .and_then(|output| {
                if (ended && output.flags & (abi::DRAWING | abi::PANNING) != 0)
                    || (!painting && output.flags & abi::DRAWING != 0)
                    || (contact.kind != ToolKind::Hand && output.flags & abi::PANNING != 0)
                {
                    return Err(ActionError::new(
                        "tool_plugin",
                        "Tool returned an invalid contact state",
                    ));
                }
                Ok(output)
            });
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                if painting {
                    s.doc.rollback_plugin_stroke();
                }
                return Err(error);
            }
        };
        if output.flags & abi::COLOR_CHANGED != 0 {
            s.parameters.color = format!("#{:06X}", output.color);
        }
        if output.flags & abi::VIEW_CHANGED != 0 {
            s.view.zoom = output.zoom;
            s.view.pan_x += output.pan_dx;
            s.view.pan_y += output.pan_dy;
            s.view.fit = false;
        }
        contact.flags = output.flags;
        contact.last = p;
        if !ended && output.flags & (abi::DRAWING | abi::PANNING) != 0 {
            self.state = Some(contact);
        } else {
            if painting {
                s.doc.finish();
            }
            s.tool_instances.insert(contact.owner, contact.instance);
        }
        Ok(())
    }
}
fn parameters(p: &Parameters) -> Result<abi::ToolParameters, ActionError> {
    Ok(abi::ToolParameters {
        radius: p.radius,
        opacity: p.opacity,
        hardness: p.hardness,
        color: parse_color(&p.color)?,
        stabilizer: p.stabilizer,
        pressure_gamma: p.pressure_gamma,
        minimum_radius: p.minimum_radius,
        flags: if p.pressure_size {
            abi::PRESSURE_SIZE
        } else {
            0
        } | if p.pressure_opacity {
            abi::PRESSURE_OPACITY
        } else {
            0
        },
    })
}
