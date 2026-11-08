//! Reversible affine draft. Gestures update parameters; idle previews borrow source layers.
use super::*;
use crate::{
    application::MoveTarget,
    engine::transform::OwnerScope,
    plugins::affine::{self, Method, Parameters},
    plugins::owner_affine,
};
use std::time::Duration;
#[path = "transform_perspective.rs"]
mod perspective;
pub(super) struct Draft {
    source: (u64, u64, u64),
    pub(super) params: Parameters,
    bounds: [usize; 4],
    bounds_valid: bool,
    bounds_target: MoveTarget,
    scope: DraftScope,
    guarded_contact: bool,
    observed: Parameters,
    changed: Instant,
    attempt: Option<Parameters>,
    painted: Option<Parameters>,
    texture: Option<TextureHandle>,
    mask: Option<TextureHandle>,
    error: String,
    preview_error: String,
    preview: bool,
    locked: bool,
    drag: Option<Drag>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DraftScope {
    Legacy,
    Owners(OwnerScope),
}
fn capture_scope(
    s: &crate::application::Session,
    target: MoveTarget,
    ids: Option<&[u64]>,
    mask: Option<u64>,
    capture_links: bool,
) -> Result<DraftScope, String> {
    if ids.is_none() && mask.is_none() && capture_links {
        if let crate::plugins::movement::MoveDestination::Layer { layer_id, .. } =
            crate::plugins::movement::destination(s, target, None)
                .map_err(|error| error.to_string())?
        {
            let members = s.doc.linked_layer_ids(layer_id)?;
            if s.doc.has_raster_masks() || members.len() > 1 {
                return owner_affine::resolve(s, target, Some(&members), None)?
                    .map(DraftScope::Owners)
                    .ok_or("Owner transform scope is unavailable".into());
            }
        }
    }
    Ok(owner_affine::resolve(s, target, ids, mask)?.map_or(DraftScope::Legacy, DraftScope::Owners))
}
fn scope_plan(
    s: &crate::application::Session,
    scope: DraftScope,
    p: Parameters,
) -> Result<([usize; 4], String), String> {
    match scope {
        DraftScope::Legacy if s.doc.has_raster_masks() => {
            Err(crate::engine::MASK_EDITING_PENDING.into())
        }
        DraftScope::Legacy => {
            affine::source_bounds(s, p.target.unwrap()).map(|bounds| (bounds, String::new()))
        }
        DraftScope::Owners(scope) => owner_affine::plan(s, &scope, p)
            .map(|plan| (plan.bounds, plan.preview_error.unwrap_or_default())),
    }
}
fn target_title(target: MoveTarget, guarded: bool) -> &'static str {
    if guarded && target == MoveTarget::Auto {
        "Auto: Mask, Selection or Layer"
    } else {
        canvas_movement::target_title(target)
    }
}
#[derive(Clone, Copy)]
enum Handle {
    Move,
    Corner(usize),
    Skew(usize),
    Rotate,
    Pivot,
}
struct Drag {
    handle: Handle,
    start: Pos2,
    params: Parameters,
}
fn edge(q: [Pos2; 4], i: usize) -> Pos2 {
    q[i] + (q[(i + 1) % 4] - q[i]) * 0.5
}
/// Project the pointer delta onto the local shear direction, preserving scale/pivot.
fn skew_drag(
    p: Parameters,
    bounds: [usize; 4],
    i: usize,
    delta: Vec2,
    snap: bool,
) -> Option<Parameters> {
    let (_, pivot) = p.matrix(bounds).ok()?;
    let (sin, cos) = p.angle.to_radians().sin_cos();
    let local = [
        cos * delta.x as f64 + sin * delta.y as f64,
        -sin * delta.x as f64 + cos * delta.y as f64,
    ];
    let (hx, hy) = p.shears();
    let sx = p.scale_x * if p.flip_x { -1. } else { 1. };
    let sy = p.scale_y * if p.flip_y { -1. } else { 1. };
    let mut next = p;
    let (angle, value) = if i % 2 == 0 {
        let y = if i == 0 { bounds[1] } else { bounds[3] } as f64;
        let den = (y - pivot[1]) * sy;
        if den.abs() < 1e-6 {
            return None;
        }
        (
            &mut next.skew_x,
            (hx + (local[0] + hy * local[1]) / ((1. + hy * hy) * den))
                .atan()
                .to_degrees(),
        )
    } else {
        let x = if i == 1 { bounds[2] } else { bounds[0] } as f64;
        let y = (bounds[1] + bounds[3]) as f64 / 2.;
        let den = sx * (x - pivot[0]) + hx * sy * (y - pivot[1]);
        if den.abs() < 1e-6 {
            return None;
        }
        (&mut next.skew_y, (hy + local[1] / den).atan().to_degrees())
    };
    *angle = (if snap {
        (value / 5.).round() * 5.
    } else {
        value
    })
    .clamp(-80., 80.);
    next.matrix(bounds).ok()?;
    Some(next)
}
fn quad(p: Parameters, b: [usize; 4]) -> Option<[Pos2; 4]> {
    let (mapping, _) = p.mapping(b).ok()?;
    Some(
        mapping
            .corners(b)
            .ok()?
            .map(|[x, y]| pos2(x as f32, y as f32)),
    )
}

fn inside(p: Pos2, q: [Pos2; 4]) -> bool {
    let cross: Vec<_> = (0..4)
        .map(|i| {
            let a = q[(i + 1) % 4] - q[i];
            let b = p - q[i];
            a.x * b.y - a.y * b.x
        })
        .collect();
    cross.iter().all(|v| *v >= 0.) || cross.iter().all(|v| *v <= 0.)
}
fn relocate_pivot(
    p: Parameters,
    bounds: [usize; 4],
    point: Option<[f64; 2]>,
) -> Option<Parameters> {
    let (m, _) = p.matrix(bounds).ok()?;
    let pivot = point.unwrap_or([
        (bounds[0] + bounds[2]) as f64 / 2.,
        (bounds[1] + bounds[3]) as f64 / 2.,
    ]);
    let mut next = p;
    next.pivot_x = point.map(|p| p[0]);
    next.pivot_y = point.map(|p| p[1]);
    next.translate_x = m[2] - pivot[0] + m[0] * pivot[0] + m[1] * pivot[1];
    next.translate_y = m[5] - pivot[1] + m[3] * pivot[0] + m[4] * pivot[1];
    next.matrix(bounds).ok()?;
    Some(next)
}
impl Draft {
    fn source_matches(&self, s: &crate::application::Session) -> bool {
        self.source == (s.doc.identity, s.revision, s.doc.layer_id(s.doc.active))
    }
    fn valid(&self, s: &crate::application::Session) -> bool {
        self.source_matches(s)
            && !s.is_drawing()
            && !(self.guarded_contact && (s.stroke.active() || s.pointer_checkpoint.is_some()))
    }
    fn ready(&self, s: &crate::application::Session) -> bool {
        self.valid(s)
            && self.bounds_valid
            && self.preview
            && self.painted == Some(self.params)
            && self.error.is_empty()
            && self.preview_error.is_empty()
    }
    fn rotate_point(&self, zoom: f32, size: f32) -> Option<Pos2> {
        if self.params.quad.is_some() {
            return None;
        }
        let q = quad(self.params, self.bounds)?;
        let top = q[0] + (q[1] - q[0]) * 0.5;
        let center = q[0] + (q[2] - q[0]) * 0.5;
        Some(top + (top - center).normalized() * size / zoom)
    }
}
impl Editor {
    pub(super) fn guarded_transform_modal(&self) -> bool {
        self.transform
            .as_ref()
            .is_some_and(|draft| draft.guarded_contact)
    }
    pub(super) fn open_transform(&mut self) {
        self.open_transform_draft(
            Parameters {
                target: Some(self.session.movement_options().target),
                ..Default::default()
            },
            None,
            None,
            true,
        );
    }
    pub(super) fn open_transform_parameters(
        &mut self,
        p: Parameters,
        ids: Option<&[u64]>,
        mask: Option<u64>,
    ) {
        self.open_transform_draft(p, ids, mask, false);
    }
    fn open_transform_draft(
        &mut self,
        mut params: Parameters,
        ids: Option<&[u64]>,
        mask: Option<u64>,
        capture_links: bool,
    ) {
        let requested = if mask.is_some() {
            MoveTarget::Auto
        } else {
            params
                .target
                .unwrap_or(self.session.movement_options().target)
        };
        let captured = capture_scope(&self.session, requested, ids, mask, capture_links);
        let guarded_contact = self.session.doc.has_raster_masks()
            || matches!(&captured, Ok(DraftScope::Owners(_)))
            || captured.is_err();
        if guarded_contact {
            self.command_sync_suspended = true;
            if !self.backend_contact_active() {
                self.clear_local_contact();
                self.pen_active = false;
            }
        } else if self
            .transform
            .as_ref()
            .is_none_or(|draft| draft.guarded_contact)
        {
            self.discard_command_sync();
            self.finish_stroke();
            self.pen_active = false;
            self.push_gui_parameters();
            self.push_gui_view();
        }
        let scope = captured.as_ref().copied().unwrap_or(DraftScope::Legacy);
        let target = match scope {
            DraftScope::Owners(scope) if scope.is_mask_only() => MoveTarget::Auto,
            DraftScope::Owners(_) => MoveTarget::Layer,
            DraftScope::Legacy => {
                crate::plugins::movement::resolved_target(&self.session, requested)
            }
        };
        params.target = Some(target);
        let planned = captured.and_then(|_| scope_plan(&self.session, scope, params));
        let bounds = planned
            .as_ref()
            .map(|(bounds, _)| *bounds)
            .map_err(Clone::clone);
        let preview_error = planned
            .as_ref()
            .map(|(_, error)| error.clone())
            .unwrap_or_default();
        let error = bounds.as_ref().err().cloned().unwrap_or_default();
        let bounds_valid = bounds.is_ok();
        self.transform = Some(Draft {
            source: (
                self.session.doc.identity,
                self.session.revision,
                self.session.doc.layer_id(self.session.doc.active),
            ),
            params,
            bounds: bounds.unwrap_or([0, 0, self.session.doc.width, self.session.doc.height]),
            bounds_valid,
            bounds_target: target,
            scope,
            guarded_contact,
            observed: params,
            changed: Instant::now(),
            attempt: None,
            painted: None,
            texture: None,
            mask: None,
            error,
            preview_error,
            preview: true,
            locked: true,
            drag: None,
        });
    }
    pub(super) fn transform_texture(&self) -> Option<&TextureHandle> {
        self.transform
            .as_ref()
            .filter(|d| d.ready(&self.session))?
            .texture
            .as_ref()
    }
    pub(super) fn transform_mask(&self) -> Option<&TextureHandle> {
        let d = self.transform.as_ref().filter(|d| d.ready(&self.session));
        if let Some(d) = d {
            d.mask.as_ref()
        } else {
            self.selection_texture.as_ref().map(|(_, t)| t)
        }
    }
    pub(super) fn transform_input(
        &mut self,
        ui: &Ui,
        bounds: Rect,
        layer: LayerId,
        events: &[Event],
        mods: Modifiers,
    ) {
        if self
            .transform
            .as_ref()
            .is_none_or(|d| !d.valid(&self.session) || !d.bounds_valid)
        {
            return;
        }
        let ctx = ui.ctx();
        let ppp = ctx.pixels_per_point();
        if let Some(p) = self.packets.last() {
            self.pen_hover = match p.phase {
                crate::platform::windows::desktop_bridge::PenPhase::Cancel
                | crate::platform::windows::desktop_bridge::PenPhase::Leave => None,
                _ => Some(pos2(p.x, p.y) / ppp),
            };
        }
        let mut input: Vec<(u8, Pos2)> = self
            .packets
            .iter()
            .filter_map(|p| match p.phase {
                crate::platform::windows::desktop_bridge::PenPhase::Down => {
                    Some((0, pos2(p.x, p.y) / ppp))
                }
                crate::platform::windows::desktop_bridge::PenPhase::Move => {
                    Some((1, pos2(p.x, p.y) / ppp))
                }
                crate::platform::windows::desktop_bridge::PenPhase::Up => {
                    Some((2, pos2(p.x, p.y) / ppp))
                }
                crate::platform::windows::desktop_bridge::PenPhase::Cancel
                | crate::platform::windows::desktop_bridge::PenPhase::Leave => {
                    Some((3, Pos2::ZERO))
                }
                _ => None,
            })
            .collect();
        if input.is_empty() {
            input.extend(events.iter().filter_map(|e| match e {
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    ..
                } => Some((if *pressed { 0 } else { 2 }, *pos)),
                Event::PointerMoved(p) => Some((1, *p)),
                Event::WindowFocused(false) | Event::PointerGone => Some((3, Pos2::ZERO)),
                _ => None,
            }));
        }
        for (phase, p) in input {
            let on_canvas = bounds.contains(p) && ctx.layer_id_at(p) == Some(layer);
            let q = self.document_position(p);
            let size = self.settings.ui.icon_size;
            let d = self.transform.as_mut().unwrap();
            if phase == 3 {
                self.pen_hover = None;
                if let Some(drag) = d.drag.take() {
                    d.params = drag.params;
                }
                continue;
            }
            if !q.is_finite() {
                continue;
            }
            if phase == 0 && on_canvas {
                let Some(corners) = quad(d.params, d.bounds) else {
                    continue;
                };
                let handle = if let Some(i) = corners
                    .iter()
                    .position(|v| v.distance(q) * self.zoom < size * 0.5)
                {
                    Some(Handle::Corner(i))
                } else if let Some(i) =
                    (0..4).find(|i| edge(corners, *i).distance(q) * self.zoom < size * 0.4)
                {
                    Some(Handle::Skew(i))
                } else if d
                    .rotate_point(self.zoom, size)
                    .is_some_and(|v| v.distance(q) * self.zoom < size * 0.5)
                {
                    Some(Handle::Rotate)
                } else if d.params.quad.is_none()
                    && d.params.matrix(d.bounds).is_ok_and(|(_, pivot)| {
                        pos2(
                            (pivot[0] + d.params.translate_x) as f32,
                            (pivot[1] + d.params.translate_y) as f32,
                        )
                        .distance(q)
                            * self.zoom
                            < size * 0.4
                    })
                {
                    Some(Handle::Pivot)
                } else if inside(q, corners) {
                    Some(Handle::Move)
                } else {
                    None
                };
                d.drag = handle.map(|handle| Drag {
                    handle,
                    start: q,
                    params: d.params,
                });
            } else if phase == 1 || phase == 2 {
                if let Some(drag) = &d.drag {
                    let mut p = drag.params;
                    if p.quad.is_some() {
                        p = perspective::drag(p, d.bounds, drag.handle, drag.start, q, mods.shift)
                            .unwrap_or(d.params);
                    } else {
                        let (_, pivot) = p.matrix(d.bounds).unwrap();
                        let center = pos2(
                            (pivot[0] + p.translate_x) as f32,
                            (pivot[1] + p.translate_y) as f32,
                        );
                        match drag.handle {
                            Handle::Pivot => {
                                let (m, _) = drag.params.matrix(d.bounds).unwrap();
                                let det = m[0] * m[4] - m[1] * m[3];
                                let (x, y) = (q.x as f64 - m[2], q.y as f64 - m[5]);
                                let point =
                                    [(m[4] * x - m[1] * y) / det, (-m[3] * x + m[0] * y) / det];
                                if let Some(next) =
                                    relocate_pivot(drag.params, d.bounds, Some(point))
                                {
                                    p = next;
                                }
                            }
                            Handle::Move => {
                                let mut delta = q - drag.start;
                                if mods.shift {
                                    if delta.x.abs() > delta.y.abs() {
                                        delta.y = 0.;
                                    } else {
                                        delta.x = 0.;
                                    }
                                }
                                p.translate_x =
                                    (p.translate_x + delta.x as f64).clamp(-8192., 8192.);
                                p.translate_y =
                                    (p.translate_y + delta.y as f64).clamp(-8192., 8192.);
                            }
                            Handle::Rotate => {
                                let a = drag.start - center;
                                let b = q - center;
                                let mut angle =
                                    p.angle + (b.y.atan2(b.x) - a.y.atan2(a.x)).to_degrees() as f64;
                                if mods.shift {
                                    angle = (angle / 15.).round() * 15.;
                                }
                                p.angle = (angle + 180.).rem_euclid(360.) - 180.;
                            }
                            Handle::Skew(i) => {
                                if let Some(next) =
                                    skew_drag(drag.params, d.bounds, i, q - drag.start, mods.shift)
                                {
                                    p = next;
                                }
                            }
                            Handle::Corner(i) => {
                                let corner = [
                                    [d.bounds[0], d.bounds[1]],
                                    [d.bounds[2], d.bounds[1]],
                                    [d.bounds[2], d.bounds[3]],
                                    [d.bounds[0], d.bounds[3]],
                                ][i];
                                let v = q - center;
                                let den =
                                    [corner[0] as f64 - pivot[0], corner[1] as f64 - pivot[1]];
                                let local = p.unskew([v.x as f64, v.y as f64]);
                                if den[0].abs() > 1e-6 {
                                    let s = local[0] / den[0];
                                    p.scale_x = s.abs().clamp(0.01, 100.);
                                    p.flip_x = s < 0.;
                                }
                                if den[1].abs() > 1e-6 {
                                    let s = local[1] / den[1];
                                    p.scale_y = s.abs().clamp(0.01, 100.);
                                    p.flip_y = s < 0.;
                                }
                                if d.locked || mods.shift {
                                    let factors = [
                                        p.scale_x / drag.params.scale_x,
                                        p.scale_y / drag.params.scale_y,
                                    ];
                                    let factor =
                                        if (factors[0] - 1.).abs() >= (factors[1] - 1.).abs() {
                                            factors[0]
                                        } else {
                                            factors[1]
                                        };
                                    let factor = factor.clamp(
                                        (0.01 / drag.params.scale_x)
                                            .max(0.01 / drag.params.scale_y),
                                        (100. / drag.params.scale_x)
                                            .min(100. / drag.params.scale_y),
                                    );
                                    p.scale_x = drag.params.scale_x * factor;
                                    p.scale_y = drag.params.scale_y * factor;
                                }
                            }
                        }
                    }
                    if p.mapping(d.bounds).is_ok() {
                        d.params = p;
                    }
                }
                if phase == 2 {
                    d.drag = None;
                }
            }
            if on_canvas {
                ctx.set_cursor_icon(if d.drag.is_some() {
                    CursorIcon::Grabbing
                } else {
                    CursorIcon::Grab
                });
            }
        }
        if let Some(p) = ctx.pointer_hover_pos().or(self.pen_hover) {
            if bounds.contains(p) && ctx.layer_id_at(p) == Some(layer) {
                let d = self.transform.as_ref().unwrap();
                let q = self.document_position(p);
                let size = self.settings.ui.icon_size;
                let cursor = if d.drag.is_some() {
                    CursorIcon::Grabbing
                } else if quad(d.params, d.bounds).is_some_and(|corners| {
                    corners
                        .iter()
                        .any(|c| c.distance(q) * self.zoom < size * 0.5)
                }) {
                    if d.params.quad.is_some() {
                        CursorIcon::Crosshair
                    } else {
                        if self.session.view.mirror_x {
                            CursorIcon::ResizeNeSw
                        } else {
                            CursorIcon::ResizeNwSe
                        }
                    }
                } else if quad(d.params, d.bounds).is_some_and(|corners| {
                    (0..4).any(|i| edge(corners, i).distance(q) * self.zoom < size * 0.4)
                }) {
                    CursorIcon::Move
                } else if d
                    .rotate_point(self.zoom, size)
                    .is_some_and(|r| r.distance(q) * self.zoom < size * 0.5)
                    || (d.params.quad.is_none()
                        && d.params.matrix(d.bounds).is_ok_and(|(_, p)| {
                            pos2(
                                (p[0] + d.params.translate_x) as f32,
                                (p[1] + d.params.translate_y) as f32,
                            )
                            .distance(q)
                                * self.zoom
                                < size * 0.4
                        }))
                {
                    CursorIcon::Crosshair
                } else {
                    CursorIcon::Grab
                };
                ctx.set_cursor_icon(cursor);
            }
        }
    }
    pub(super) fn transform_outline(&self, painter: &Painter, color: Color32) {
        let Some(d) = self
            .transform
            .as_ref()
            .filter(|d| d.valid(&self.session) && d.bounds_valid)
        else {
            return;
        };
        let Some(q) = quad(d.params, d.bounds) else {
            return;
        };
        let screen = |p: Pos2| self.document_screen_position(p);
        let size = self.settings.ui.icon_size * 0.3;
        for i in 0..4 {
            painter.line_segment(
                [screen(q[i]), screen(q[(i + 1) % 4])],
                Stroke::new(1.5, color),
            );
            painter.rect_filled(
                Rect::from_center_size(screen(q[i]), Vec2::splat(size * 2.)),
                1.,
                color,
            );
        }
        for i in 0..4 {
            let p = screen(edge(q, i));
            painter.add(Shape::convex_polygon(
                vec![
                    p - vec2(0., size),
                    p + vec2(size, 0.),
                    p + vec2(0., size),
                    p - vec2(size, 0.),
                ],
                color,
                Stroke::NONE,
            ));
        }
        if let Some(r) = d.rotate_point(self.zoom, self.settings.ui.icon_size) {
            painter.line_segment(
                [screen(q[0] + (q[1] - q[0]) * 0.5), screen(r)],
                Stroke::new(1., color),
            );
            painter.circle_filled(screen(r), size, color);
        }
        if let Some((_, p)) = d
            .params
            .quad
            .is_none()
            .then(|| d.params.matrix(d.bounds).ok())
            .flatten()
        {
            let p = screen(pos2(
                (p[0] + d.params.translate_x) as f32,
                (p[1] + d.params.translate_y) as f32,
            ));
            painter.circle_stroke(p, size, Stroke::new(1., color));
            painter.line_segment(
                [p - vec2(size * 1.5, 0.), p + vec2(size * 1.5, 0.)],
                Stroke::new(1., color),
            );
            painter.line_segment(
                [p - vec2(0., size * 1.5), p + vec2(0., size * 1.5)],
                Stroke::new(1., color),
            );
        }
    }
    pub(super) fn transform_window(&mut self, ctx: &Context) {
        let Some(mut d) = self.transform.take() else {
            return;
        };
        if !d.guarded_contact {
            let before = self.session.revision;
            self.push_gui_parameters();
            self.push_gui_view();
            if d.source.1 == before {
                d.source.1 = self.session.revision;
            }
        } else if d.valid(&self.session) {
            // Navigation alone cannot alter captured source pixels or bounds.
            // Rebase only this known local view publication, never a stale draft.
            self.push_gui_view_now();
            if d.source.0 == self.session.doc.identity {
                d.source.1 = self.session.revision;
            }
        }
        let stale = !d.source_matches(&self.session);
        let contact = d.guarded_contact && self.backend_contact_active();
        let a = self.settings.ui.clone();
        let c = theme::Colors::new(&a);
        let frame = Frame::window(&ctx.style()).inner_margin(Margin::same(a.panel_padding as i8));
        let width = a.control_height * 14.;
        let width = width.min(
            (ctx.content_rect().width() - a.outer_padding * 2. - frame.total_margin().sum().x)
                .max(a.control_height * 6.),
        );
        // Reserve both regions before egui measures a previous-frame window.
        // Otherwise a longer translated inspector can clip the fixed footer.
        let scroll_height = (ctx.content_rect().height()
            - a.control_height * 5.
            - a.panel_padding * 4.
            - a.outer_padding * 2.)
            .max(a.control_height * 2.);
        let footer_height = a.control_height + a.panel_padding + a.item_spacing * 3. + 1.;
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        Window::new(tr("Free Transform"))
            .id(Id::new("free-transform"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .frame(frame)
            .default_width(width)
            .min_width(width)
            .max_width(width)
            .min_height(scroll_height+footer_height)
            .default_pos(pos2(
                ctx.content_rect().right() - width - frame.total_margin().sum().x - a.outer_padding,
                ctx.content_rect().center().y - a.control_height * 9.,
            ))
            .show(ctx, |ui| {
                let mut allowed = false;
                ScrollArea::vertical()
                    .id_salt("transform-controls")
                    .max_height(scroll_height)
                    .auto_shrink([false,true])
                    .show(ui, |ui| {
                        if d.params.quad.is_some() {
                            ui.label(tr("Drag corners independently, edges to move a side, or inside to move the quadrilateral."));
                            ui.weak(tr("Shift constrains perspective handle movement to one axis."));
                        }else {
                        ui.label(tr(
                            "Drag inside to move, corners to scale, or the round handle to rotate.",
                        ));
                        ui.weak(tr(
                            "Shift constrains movement or snaps rotation to 15 degrees.",
                        ));
                        ui.weak(tr(
                            "Drag the crosshair to move the pivot without moving the artwork.",
                        ));
                        ui.weak(tr(
                            "Drag edge diamonds to skew. Shift snaps skew to 5 degrees.",
                        ));
                        }
                        ui.add_space(a.item_spacing);
                        let mut target = d.params.target.unwrap();
                        ui.add_enabled_ui(!stale && !contact, |ui| ui.horizontal_wrapped(|ui| {
                            ui.label(tr("Target"));
                            ComboBox::from_id_salt("transform-target")
                                .selected_text(tr(target_title(target,d.guarded_contact)))
                                .show_ui(ui, |ui| {
                                    for t in canvas_movement::TARGETS {
                                        if t != MoveTarget::Auto || d.guarded_contact {
                                            ui.selectable_value(
                                                &mut target,
                                                t,
                                                tr(target_title(t,d.guarded_contact)),
                                            );
                                        }
                                    }
                                });
                        }));
                        d.params.target = Some(target);
                        if target != d.bounds_target {
                            d.bounds_target = target;
                            let scope=capture_scope(&self.session,target,None,None,true);
                            d.guarded_contact |= self.session.doc.has_raster_masks() || matches!(&scope,Ok(DraftScope::Owners(_))) || scope.is_err();
                            if d.guarded_contact {self.command_sync_suspended=true;}
                            match scope.and_then(|scope|scope_plan(&self.session,scope,d.params).map(|plan|(scope,plan))) {
                                Ok((scope,(b,preview_error))) => {
                                    d.scope=scope;
                                    d.bounds = b;
                                    d.bounds_valid = true;
                                    d.error.clear();
                                    d.preview_error=preview_error;
                                }
                                Err(e) => {
                                    d.error = e;
                                    d.bounds_valid = false;
                                }
                            }
                            d.attempt=None;
                            d.painted=None;
                            d.changed=Instant::now();
                        }
                        match d.scope {
                            DraftScope::Owners(scope) if scope.is_mask_only()=>{
                                if let Some(index)=scope.mask_owner_layer_id().and_then(|id|self.session.doc.layer_index(id)) {
                                    ui.weak(format!("{}: {}",crate::i18n::text("Raster mask"),self.session.doc.layer_name(index)));
                                }
                                ui.weak(tr("Mask only; layer pixels stay fixed."));
                                ui.weak(tr("Exposed mask areas reveal the layer."));
                            }
                            DraftScope::Owners(scope)=>{
                                ui.weak(crate::i18n::text("Transforming {count} layers and their linked masks.").replace("{count}",&scope.layer_ids().len().to_string()));
                                if !scope.mask_ids().is_empty() {ui.weak(tr("Exposed mask areas reveal the layer."));}
                                if self.session.doc.mask_state().items.iter().any(|mask|matches!(mask.owner,crate::engine::raster_masks::MaskOwner::Layer{id} if scope.layer_ids().contains(&id)) && !mask.linked) {
                                    ui.weak(tr("Unlinked mask stays fixed."));
                                }
                            }
                            DraftScope::Legacy=>{}
                        }
                        perspective::mode(ui,&mut d);
                        if let Some(quad)=&mut d.params.quad {
                            perspective::controls(ui,quad,a.item_spacing);
                        }else {
                        Grid::new("transform-values")
                            .spacing(vec2(a.item_spacing * 2., a.item_spacing))
                            .show(ui, |ui| {
                                for (label, x, y) in [(
                                    "Translate",
                                    &mut d.params.translate_x,
                                    &mut d.params.translate_y,
                                )] {
                                    ui.label(tr(label));
                                    ui.label("X");
                                    ui.add(DragValue::new(x).speed(1.).range(-8192. ..=8192.));
                                    ui.label("Y");
                                    ui.add(DragValue::new(y).speed(1.).range(-8192. ..=8192.));
                                    ui.end_row();
                                }
                                let old = [d.params.scale_x, d.params.scale_y];
                                let mut scale = [old[0] * 100., old[1] * 100.];
                                ui.label(tr("Scale"));
                                ui.label("X");
                                let x = ui.add(
                                    DragValue::new(&mut scale[0])
                                        .speed(1.)
                                        .range(1. ..=10000.)
                                        .suffix("%"),
                                );
                                ui.label("Y");
                                let y = ui.add(
                                    DragValue::new(&mut scale[1])
                                        .speed(1.)
                                        .range(1. ..=10000.)
                                        .suffix("%"),
                                );
                                ui.end_row();
                                if d.locked {
                                    if x.changed() {
                                        scale[1] = scale[0] * old[1] / old[0];
                                    } else if y.changed() {
                                        scale[0] = scale[1] * old[0] / old[1];
                                    }
                                }
                                d.params.scale_x = scale[0] / 100.;
                                d.params.scale_y = scale[1] / 100.;
                                ui.label(tr("Skew"));
                                ui.label("X");
                                ui.add(
                                    DragValue::new(&mut d.params.skew_x)
                                        .speed(0.5)
                                        .range(-80. ..=80.)
                                        .suffix("°"),
                                );
                                ui.label("Y");
                                ui.add(
                                    DragValue::new(&mut d.params.skew_y)
                                        .speed(0.5)
                                        .range(-80. ..=80.)
                                        .suffix("°"),
                                );
                                ui.end_row();
                                ui.label(tr("Angle"));
                                ui.add(
                                    DragValue::new(&mut d.params.angle)
                                        .speed(1.)
                                        .range(-360. ..=360.)
                                        .suffix("°"),
                                );
                                ui.end_row();
                                let pivot = d
                                    .params
                                    .matrix(d.bounds)
                                    .map(|(_, p)| p)
                                    .unwrap_or([0., 0.]);
                                let (mut px, mut py) = (pivot[0], pivot[1]);
                                ui.label(tr("Pivot"));
                                ui.label("X");
                                let x = ui
                                    .add(DragValue::new(&mut px).speed(0.5).range(-8192. ..=8192.));
                                ui.label("Y");
                                let y = ui
                                    .add(DragValue::new(&mut py).speed(0.5).range(-8192. ..=8192.));
                                ui.end_row();
                                if x.changed() || y.changed() {
                                    if let Some(next) =
                                        relocate_pivot(d.params, d.bounds, Some([px, py]))
                                    {
                                        d.params = next;
                                        d.attempt = None;
                                        if d.bounds_valid {
                                            d.error.clear();
                                        }
                                    } else {
                                        d.error = "Pivot change exceeds transform limits".into();
                                    }
                                }
                            });
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut d.locked, tr("Lock Aspect Ratio"));
                            ui.checkbox(&mut d.params.flip_x, tr("Flip Horizontal"));
                            ui.checkbox(&mut d.params.flip_y, tr("Flip Vertical"));
                            if ui.button(tr("Center Pivot")).clicked() {
                                if let Some(next) = relocate_pivot(d.params, d.bounds, None) {
                                    d.params = next;
                                    d.attempt = None;
                                    if d.bounds_valid {
                                        d.error.clear();
                                    }
                                } else {
                                    d.error = "Pivot change exceeds transform limits".into();
                                }
                            }
                        });
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.label(tr("Sampling"));
                            ComboBox::from_id_salt("transform-method")
                                .selected_text(tr(if d.params.method == Method::Nearest {
                                    "Nearest Neighbor"
                                } else {
                                    "Bilinear"
                                }))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut d.params.method,
                                        Method::Nearest,
                                        tr("Nearest Neighbor"),
                                    );
                                    ui.selectable_value(
                                        &mut d.params.method,
                                        Method::Bilinear,
                                        tr("Bilinear"),
                                    );
                                });
                            ui.checkbox(&mut d.preview, tr("Preview"));
                        });
                        ui.weak(tr(
                            "Pixels beyond the canvas are clipped. Undo restores them.",
                        ));
                        if stale {
                            ui.colored_label(
                                c.accent,
                                tr("Document changed; reopen Free Transform."),
                            );
                        }
                        if contact { ui.colored_label(c.accent,tr("Finish the current stroke or pan before applying Free Transform.")); }
                        let matrix = d.params.mapping(d.bounds);
                        if let Err(e) = &matrix {
                            ui.colored_label(c.accent, tr(e));
                        }
                        if !d.error.is_empty() {
                            ui.colored_label(c.accent, tr(&d.error));
                        }
                        if !d.preview_error.is_empty() {
                            let message=if d.preview_error=="Transform preview exceeds its separate composite or mask-effect memory limit; disable Preview to apply" {
                                "Preview is unavailable for this document. Turn off Preview to apply the transform."
                            } else {&d.preview_error};
                            ui.colored_label(c.muted,tr(message));
                        }
                        allowed = !stale && !contact
                            && d.bounds_valid
                            && d.error.is_empty()
                            && matrix.is_ok_and(|(m, _)| !m.identity());
                    });
                ui.add_space(a.item_spacing);
                ui.horizontal(|ui| {
                    if ui.button(tr("Cancel")).clicked() {
                        cancel = true;
                    }
                    if ui.add_enabled(allowed, Button::new(tr("Apply"))).clicked() {
                        apply = true;
                    }
                });
                cancel |= ctx.input(|i| i.key_pressed(Key::Escape));
                apply |= allowed
                    && !ctx.wants_keyboard_input()
                    && ctx.input(|i| i.key_pressed(Key::Enter));
            });
        if d.params != d.observed {
            d.observed = d.params;
            d.changed = Instant::now();
            d.attempt = None;
            if d.bounds_valid {
                d.error.clear();
            }
        }
        if !stale
            && !contact
            && d.bounds_valid
            && d.preview
            && d.attempt != Some(d.params)
            && d.drag.is_none()
        {
            if d.changed.elapsed() >= Duration::from_millis(150) {
                d.attempt = Some(d.params);
                let preview = match d.scope {
                    DraftScope::Legacy => affine::preview(&self.session, d.params),
                    DraftScope::Owners(scope) => owner_affine::preview_cached(
                        &self.session,
                        &scope,
                        d.params,
                        (d.source.0, d.source.1),
                        d.bounds,
                    ),
                };
                match preview {
                    Ok((pixels, selection)) => {
                        let w = self.session.doc.width;
                        let h = self.session.doc.height;
                        let colors = pixels
                            .into_iter()
                            .enumerate()
                            .map(|(i, p)| {
                                let (x, y) = (i % w, i / w);
                                let bg = if (x / 12 + y / 12) % 2 == 0 {
                                    0xfffdfdfd
                                } else {
                                    0xfff3f5f7
                                };
                                let p = crate::engine::over(bg, p);
                                Color32::from_rgb((p >> 16) as u8, (p >> 8) as u8, p as u8)
                            })
                            .collect();
                        let image = ColorImage::new([w, h], colors);
                        if let Some(t) = &mut d.texture {
                            t.set(image, TextureOptions::LINEAR);
                        } else {
                            d.texture = Some(ctx.load_texture(
                                "transform-preview",
                                image,
                                TextureOptions::LINEAR,
                            ));
                        }
                        d.mask = selection.current.as_ref().map(|m| {
                            ctx.load_texture(
                                "transform-selection",
                                canvas_selection::outline_image(m),
                                TextureOptions::NEAREST,
                            )
                        });
                        d.painted = Some(d.params);
                        d.error.clear();
                        d.preview_error.clear();
                        ctx.request_repaint();
                    }
                    Err(e) => {
                        if matches!(d.scope, DraftScope::Owners(_)) {
                            d.preview_error = e;
                        } else {
                            d.error = e;
                        }
                        d.painted = None;
                        ctx.request_repaint();
                    }
                }
            } else {
                ctx.request_repaint_after(
                    Duration::from_millis(150).saturating_sub(d.changed.elapsed()),
                );
            }
        }
        if apply {
            let mut action = crate::application::Action::new(
                "canvas.transform",
                match d.scope {
                    DraftScope::Legacy => serde_json::to_value(d.params).unwrap(),
                    DraftScope::Owners(scope) => owner_affine::action_args(&scope, &d.params),
                },
            );
            action.expected_revision = Some(d.source.1);
            match self.session.apply(action) {
                Ok(_) => {
                    self.sync_from_session();
                    self.release_command_sync();
                    return;
                }
                Err(e) => d.error = e.to_string(),
            }
        }
        if open && !cancel {
            self.transform = Some(d);
        } else {
            self.release_command_sync();
        }
    }
}

#[cfg(test)]
#[path = "transform_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "transform_perspective_tests.rs"]
mod perspective_tests;

#[cfg(test)]
#[path = "transform_owner_ui_tests.rs"]
mod owner_ui_tests;
