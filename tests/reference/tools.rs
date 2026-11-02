//! Native tool backend for startup-loaded brush/tool definitions. No UI dependency.
use crate::{
    application::{ActionError, Parameters, Pointer, PointerPhase, Session, ToolKind},
    engine::input::{Sample, Stabilizer},
};
pub trait ToolBackend {
    fn input(&mut self, session: &mut Session, sample: Pointer) -> Result<(), ActionError>;
    fn finish(&mut self, session: &mut Session);
}
#[derive(Default)]
pub struct StrokeEngine {
    filter: Stabilizer,
    state: Option<Contact>,
}
struct Contact {
    kind: ToolKind,
    parameters: Parameters,
    pen: bool,
    erase: bool,
    last: Pointer,
}
#[allow(dead_code)] // Retained baseline also exposes methods used by the old GUI.
impl StrokeEngine {
    pub fn last_sample(&self) -> Option<Pointer> {
        self.state.as_ref().map(|s| s.last)
    }
    pub fn active(&self) -> bool {
        self.state.is_some()
    }
    pub fn panning(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(|s| s.kind == ToolKind::Hand)
    }
    pub fn drawing(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(|s| matches!(s.kind, ToolKind::Brush | ToolKind::Eraser))
    }
    pub fn input(&mut self, s: &mut Session, p: Pointer) -> Result<(), ActionError> {
        <Self as ToolBackend>::input(self, s, p)
    }
    pub fn finish(&mut self, s: &mut Session) {
        <Self as ToolBackend>::finish(self, s)
    }
}
impl ToolBackend for StrokeEngine {
    fn input(&mut self, s: &mut Session, p: Pointer) -> Result<(), ActionError> {
        match p.phase {
            PointerPhase::Down => {
                if self.active() {
                    return Err(ActionError::new(
                        "stroke_active",
                        "当前工具尚未收到 up/cancel",
                    ));
                }
                let kind = p.tool_override.unwrap_or(s.tool);
                match kind {
                    ToolKind::Brush | ToolKind::Eraser => {
                        if p.x < 0.
                            || p.y < 0.
                            || p.x >= s.doc.width as f32
                            || p.y >= s.doc.height as f32
                        {
                            return Err(ActionError::new("outside_canvas", "起笔位置在画布外"));
                        }
                        if !s.doc.layers[s.doc.active].visible {
                            return Err(ActionError::new("hidden_layer", "当前图层已隐藏"));
                        }
                        let filtered = self.filter.reset(Sample {
                            x: p.x,
                            y: p.y,
                            pressure: p.pressure,
                            time: p.time,
                        });
                        s.doc.begin(
                            filtered.x,
                            filtered.y,
                            s.parameters.brush(
                                kind == ToolKind::Eraser || p.eraser,
                                p.pressure,
                                p.pen,
                            ),
                        );
                    }
                    ToolKind::Picker => {
                        s.pick(p.x, p.y)?;
                        return Ok(());
                    }
                    ToolKind::Zoom => {
                        let old = s.view.zoom;
                        s.view.zoom =
                            (old * if p.eraser { 1. / 1.25 } else { 1.25 }).clamp(0.02, 32.);
                        s.view.pan_x += (s.doc.width as f32 / 2. - p.x) * (s.view.zoom - old);
                        s.view.pan_y += (s.doc.height as f32 / 2. - p.y) * (s.view.zoom - old);
                        s.view.fit = false;
                        return Ok(());
                    }
                    ToolKind::Hand => s.view.fit = false,
                }
                self.state = Some(Contact {
                    kind,
                    parameters: s.parameters.clone(),
                    pen: p.pen,
                    erase: kind == ToolKind::Eraser || p.eraser,
                    last: p,
                });
            }
            PointerPhase::Move | PointerPhase::Up => {
                if let Some(contact) = &mut self.state {
                    if contact.kind == ToolKind::Hand {
                        s.view.pan_x +=
                            p.view_x.unwrap_or(p.x) - contact.last.view_x.unwrap_or(contact.last.x);
                        s.view.pan_y +=
                            p.view_y.unwrap_or(p.y) - contact.last.view_y.unwrap_or(contact.last.y);
                    } else {
                        let pressure = if p.phase == PointerPhase::Up && p.pressure == 0. {
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
                        s.doc.line(
                            sample.x,
                            sample.y,
                            contact
                                .parameters
                                .brush(contact.erase, sample.pressure, contact.pen),
                        );
                    }
                    contact.last = p;
                }
                if p.phase == PointerPhase::Up {
                    self.finish(s);
                }
            }
            PointerPhase::Cancel => self.finish(s),
        }
        Ok(())
    }
    fn finish(&mut self, s: &mut Session) {
        if let Some(contact) = self.state.take() {
            if matches!(contact.kind, ToolKind::Brush | ToolKind::Eraser) {
                if let Some(sample) = self.filter.finish() {
                    s.doc.line(
                        sample.x,
                        sample.y,
                        contact
                            .parameters
                            .brush(contact.erase, sample.pressure, contact.pen),
                    );
                }
                s.doc.finish();
            }
        }
        self.filter.cancel();
    }
}
