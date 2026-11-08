//! Bounded inverse deformation field for shared-owner Liquify. No host pointers.
//! The native sampler/Session/UI integration is a subsequent delivery stage.
const MAX_NODES: usize = 2_097_152;
const MAX_BYTES: usize = 64 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Push {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub radius: f32,
    pub strength: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Regions {
    pub items: [[u32; 4]; 4],
    pub len: usize,
}
pub(crate) struct Field {
    width: u32,
    height: u32,
    step: u32,
    cols: usize,
    rows: usize,
    values: Vec<[f32; 2]>,
    scratch: Vec<(usize, [f32; 2])>,
}
impl Field {
    pub fn new(width: u32, height: u32, step: u32, budget: usize) -> Result<Self, &'static str> {
        if width == 0 || height == 0 || width > 16384 || height > 16384 || !(1..=64).contains(&step)
        {
            return Err("Invalid Liquify field dimensions or cell spacing");
        }
        let cols = width.div_ceil(step) as usize + 1;
        let rows = height.div_ceil(step) as usize + 1;
        let nodes = cols
            .checked_mul(rows)
            .ok_or("Liquify field dimensions overflow")?;
        let bytes = nodes
            .checked_mul(std::mem::size_of::<[f32; 2]>() + std::mem::size_of::<(usize, [f32; 2])>())
            .ok_or("Liquify field budget overflow")?;
        if nodes > MAX_NODES || bytes > budget.min(MAX_BYTES) {
            return Err("Liquify field exceeds the configured memory ceiling");
        }
        let mut values = Vec::new();
        values
            .try_reserve_exact(nodes)
            .map_err(|_| "Cannot allocate Liquify field")?;
        values.resize(nodes, [0.; 2]);
        let mut scratch = Vec::new();
        scratch
            .try_reserve_exact(nodes)
            .map_err(|_| "Cannot allocate Liquify update workspace")?;
        Ok(Self {
            width,
            height,
            step,
            cols,
            rows,
            values,
            scratch,
        })
    }
    pub fn clear(&mut self) {
        self.values.fill([0.; 2]);
        self.scratch.clear();
    }
    /// Bilinear displacement in document-edge coordinates; the exterior is identity.
    pub fn displacement(&self, x: f64, y: f64) -> [f64; 2] {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.
            || y < 0.
            || x > f64::from(self.width)
            || y > f64::from(self.height)
        {
            return [0.; 2];
        }
        let col = ((x / f64::from(self.step)).floor() as usize).min(self.cols - 2);
        let row = ((y / f64::from(self.step)).floor() as usize).min(self.rows - 2);
        let x0 = (col as u32 * self.step).min(self.width);
        let y0 = (row as u32 * self.step).min(self.height);
        let x1 = ((col as u32 + 1) * self.step).min(self.width);
        let y1 = ((row as u32 + 1) * self.step).min(self.height);
        let u = (x - f64::from(x0)) / f64::from(x1 - x0);
        let v = (y - f64::from(y0)) / f64::from(y1 - y0);
        let weights = [(1. - u) * (1. - v), u * (1. - v), (1. - u) * v, u * v];
        let indices = [
            row * self.cols + col,
            row * self.cols + col + 1,
            (row + 1) * self.cols + col,
            (row + 1) * self.cols + col + 1,
        ];
        let mut result = [0.; 2];
        for (index, weight) in indices.into_iter().zip(weights) {
            for axis in 0..2 {
                result[axis] += f64::from(self.values[index][axis]) * weight;
            }
        }
        result
    }
    /// Compose one joint brush sample over the immutable old field. At most four
    /// arms permit X/Y symmetry without evaluating intermediate asymmetric states.
    /// Duplicate requests deposit once; overlap normalizes summed influence above 1.
    pub fn push(&mut self, pushes: &[Push]) -> Result<Regions, &'static str> {
        if pushes.is_empty() || pushes.len() > 4 {
            return Err("Liquify accepts one to four joint pushes");
        }
        for p in pushes {
            if [p.x, p.y, p.dx, p.dy, p.radius, p.strength]
                .iter()
                .any(|v| !v.is_finite())
                || p.x.abs() > 1_048_576.
                || p.y.abs() > 1_048_576.
                || p.radius < self.step as f32
                || p.radius > 2048.
                || !(0. ..=1.).contains(&p.strength)
                || f64::from(p.dx).hypot(f64::from(p.dy)) > f64::from(p.radius)
            {
                return Err("Invalid Liquify push geometry or strength");
            }
        }
        let mut boxes = [[0usize; 4]; 4];
        for (i, p) in pushes.iter().enumerate() {
            if p.strength == 0. || p.dx == 0. && p.dy == 0. || pushes[..i].contains(p) {
                continue;
            }
            let left = (f64::from(p.x) - f64::from(p.radius)).max(0.);
            let top = (f64::from(p.y) - f64::from(p.radius)).max(0.);
            let right = (f64::from(p.x) + f64::from(p.radius)).min(f64::from(self.width));
            let bottom = (f64::from(p.y) + f64::from(p.radius)).min(f64::from(self.height));
            if left > right || top > bottom {
                continue;
            }
            boxes[i] = [
                (left / f64::from(self.step)).floor() as usize,
                (top / f64::from(self.step)).floor() as usize,
                ((right / f64::from(self.step)).ceil() as usize + 1).min(self.cols),
                ((bottom / f64::from(self.step)).ceil() as usize + 1).min(self.rows),
            ];
        }
        self.scratch.clear();
        for (box_index, b) in boxes.iter().enumerate().take(pushes.len()) {
            for row in b[1]..b[3] {
                for col in b[0]..b[2] {
                    if boxes[..box_index]
                        .iter()
                        .any(|a| col >= a[0] && col < a[2] && row >= a[1] && row < a[3])
                    {
                        continue;
                    }
                    let x = f64::from((col as u32 * self.step).min(self.width));
                    let y = f64::from((row as u32 * self.step).min(self.height));
                    let mut total = 0.;
                    let mut delta = [0.; 2];
                    for (i, p) in pushes.iter().enumerate() {
                        if pushes[..i].contains(p) {
                            continue;
                        }
                        let r = f64::from(p.radius);
                        let d =
                            ((x - f64::from(p.x)).powi(2) + (y - f64::from(p.y)).powi(2)) / (r * r);
                        let weight = (1. - d).max(0.).powi(2) * f64::from(p.strength);
                        total += weight;
                        delta[0] += weight * f64::from(p.dx);
                        delta[1] += weight * f64::from(p.dy);
                    }
                    if delta == [0.; 2] {
                        continue;
                    }
                    let divisor = total.max(1.);
                    delta[0] /= divisor;
                    delta[1] /= divisor;
                    let old = self.displacement(x - delta[0], y - delta[1]);
                    let next = [(old[0] - delta[0]) as f32, (old[1] - delta[1]) as f32];
                    if next.iter().any(|v| !v.is_finite() || v.abs() > 4_194_304.) {
                        self.scratch.clear();
                        return Err("Liquify displacement exceeds the field range");
                    }
                    let index = row * self.cols + col;
                    if next != self.values[index] {
                        self.scratch.push((index, next));
                    }
                }
            }
        }
        if self.scratch.is_empty() {
            return Ok(Regions::default());
        }
        for (index, next) in &self.scratch {
            self.values[*index] = *next;
        }
        let mut regions = Regions::default();
        for b in boxes.into_iter().take(pushes.len()) {
            if b[0] == b[2] || b[1] == b[3] {
                continue;
            }
            let left = (b[0] as u32).saturating_sub(1) * self.step;
            let top = (b[1] as u32).saturating_sub(1) * self.step;
            let right = ((b[2] as u32 + 1) * self.step).min(self.width);
            let bottom = ((b[3] as u32 + 1) * self.step).min(self.height);
            regions.items[regions.len] =
                [left.min(self.width), top.min(self.height), right, bottom];
            regions.len += 1;
        }
        Ok(regions)
    }
}

#[cfg(test)]
#[path = "liquify_field_tests.rs"]
mod tests;
