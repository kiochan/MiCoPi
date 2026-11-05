//! Compiled Canvas UI adapter for provisional four-corner geometry.
use super::*;

pub(super) fn mode(ui: &mut Ui, d: &mut Draft) {
    ui.horizontal_wrapped(|ui| {
        if d.params.quad.is_none() {
            if ui.button(tr("Edit Perspective Corners")).clicked() {
                match bake(d.params, d.bounds) {
                    Ok(next) => {
                        d.params = next;
                        d.drag = None;
                        d.error.clear();
                    }
                    Err(e) => d.error = e,
                }
            }
        } else {
            ui.label(tr("Perspective Corners"));
        }
        if ui.button(tr("Reset Transform")).clicked() {
            d.params = Parameters {
                target: d.params.target,
                method: d.params.method,
                ..Default::default()
            };
            d.drag = None;
            d.attempt = None;
            if d.bounds_valid {
                d.error.clear();
            }
        }
    });
}
pub(super) fn controls(ui: &mut Ui, quad: &mut [[f64; 2]; 4], spacing: f32) {
    Grid::new("perspective-corners")
        .spacing(vec2(spacing * 2., spacing))
        .show(ui, |ui| {
            for (point, label) in
                quad.iter_mut()
                    .zip(["Top Left", "Top Right", "Bottom Right", "Bottom Left"])
            {
                ui.label(tr(label));
                ui.label("X");
                ui.add(
                    DragValue::new(&mut point[0])
                        .speed(0.5)
                        .range(-8192. ..=8192.),
                );
                ui.label("Y");
                ui.add(
                    DragValue::new(&mut point[1])
                        .speed(0.5)
                        .range(-8192. ..=8192.),
                );
                ui.end_row();
            }
        });
}
pub(super) fn drag(
    mut p: Parameters,
    bounds: [usize; 4],
    handle: Handle,
    start: Pos2,
    end: Pos2,
    shift: bool,
) -> Option<Parameters> {
    let mut q = p.quad?;
    let mut delta = end - start;
    if shift {
        if delta.x.abs() > delta.y.abs() {
            delta.y = 0.;
        } else {
            delta.x = 0.;
        }
    }
    for (i, point) in q.iter_mut().enumerate() {
        let include = match handle {
            Handle::Corner(n) => i == n,
            Handle::Skew(n) => i == n || i == (n + 1) % 4,
            Handle::Move => true,
            _ => false,
        };
        if include {
            point[0] += delta.x as f64;
            point[1] += delta.y as f64;
        }
    }
    p.quad = Some(q);
    p.mapping(bounds).ok()?;
    Some(p)
}

pub(super) fn bake(p: Parameters, bounds: [usize; 4]) -> Result<Parameters, String> {
    let quad = p.mapping(bounds)?.0.corners(bounds)?;
    let next = Parameters {
        target: p.target,
        method: p.method,
        quad: Some(quad),
        ..Default::default()
    };
    next.mapping(bounds)?;
    Ok(next)
}
