//! Exact bounded union of admitted dirty rectangles; no pixel or heap storage.
use micopi_plugin_api::Region;

const EMPTY: Region = Region {
    x: 0,
    y: 0,
    width: 0,
    height: 0,
};
// Four rectangles have at most eight distinct Y edges (seven bands), each
// containing at most four disjoint X intervals. Adjacent equal runs coalesce.
const MAX_REGIONS: usize = 7 * 4;
pub(super) struct Disjoint {
    items: [Region; MAX_REGIONS],
    len: usize,
}
impl Disjoint {
    pub(super) fn iter(&self) -> impl ExactSizeIterator<Item = &Region> {
        self.items[..self.len].iter()
    }
    fn add(&mut self, x: u32, right: u32, y: u32, bottom: u32) {
        let width = right - x;
        if let Some(previous) = self.items[..self.len]
            .iter_mut()
            .find(|r| r.x == x && r.width == width && r.y + r.height == y)
        {
            previous.height += bottom - y;
            return;
        }
        self.items[self.len] = Region {
            x,
            y,
            width,
            height: bottom - y,
        };
        self.len += 1;
    }
}
/// Sources have already passed the native dimensions/ROI admission checks.
/// Partition their exact union, preserving distant arms without sampling overlap.
pub(super) fn disjoint(source: &[Option<Region>; 4]) -> Disjoint {
    let mut edges = [0; 8];
    let mut count = 0;
    for r in source
        .iter()
        .flatten()
        .filter(|r| r.width != 0 && r.height != 0)
    {
        edges[count] = r.y;
        edges[count + 1] = r.y + r.height;
        count += 2;
    }
    edges[..count].sort_unstable();
    let mut output = Disjoint {
        items: [EMPTY; MAX_REGIONS],
        len: 0,
    };
    for pair in edges[..count].windows(2) {
        let (y, bottom) = (pair[0], pair[1]);
        if y == bottom {
            continue;
        }
        let mut spans = [(0, 0); 4];
        let mut length = 0;
        for r in source
            .iter()
            .flatten()
            .filter(|r| r.width != 0 && r.y <= y && r.y + r.height >= bottom)
        {
            spans[length] = (r.x, r.x + r.width);
            length += 1;
        }
        spans[..length].sort_unstable();
        let mut run: Option<(u32, u32)> = None;
        for (x, right) in spans[..length].iter().copied() {
            run = match run {
                None => Some((x, right)),
                Some((left, end)) if x <= end => Some((left, end.max(right))),
                Some((left, end)) => {
                    output.add(left, end, y, bottom);
                    Some((x, right))
                }
            };
        }
        if let Some((left, right)) = run {
            output.add(left, right, y, bottom);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    fn r(x: u32, y: u32, width: u32, height: u32) -> Option<Region> {
        Some(Region {
            x,
            y,
            width,
            height,
        })
    }
    fn contains(r: &Region, x: u32, y: u32) -> bool {
        x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
    }
    fn check(source: [Option<Region>; 4], width: u32, height: u32) {
        let output = disjoint(&source);
        assert!(output.len <= MAX_REGIONS);
        assert!(output.iter().all(|r| r.width > 0
            && r.height > 0
            && r.x + r.width <= width
            && r.y + r.height <= height));
        for y in 0..height {
            for x in 0..width {
                let covered = source.iter().flatten().any(|r| contains(r, x, y));
                let visits = output.iter().filter(|r| contains(r, x, y)).count();
                assert_eq!(visits, usize::from(covered), "pixel {x},{y}");
            }
        }
    }
    #[test]
    fn exact_union_handles_empty_duplicates_nested_touching_and_holes() {
        for source in [
            [None; 4],
            [r(1, 1, 8, 6); 4],
            [r(1, 1, 8, 6), r(2, 2, 1, 1), r(3, 3, 2, 2), None],
            [r(0, 0, 5, 3), r(5, 0, 5, 3), r(0, 3, 10, 3), None],
            [r(1, 1, 8, 1), r(1, 6, 8, 1), r(1, 2, 1, 4), r(8, 2, 1, 4)],
            [r(1, 1, 7, 5), r(4, 0, 5, 4), r(2, 4, 8, 3), r(6, 2, 5, 5)],
        ] {
            check(source, 12, 8);
        }
        assert_eq!(disjoint(&[r(1, 1, 8, 6); 4]).len, 1);
        assert_eq!(
            disjoint(&[r(0, 0, 5, 3), r(5, 0, 5, 3), r(0, 3, 10, 3), None]).len,
            1
        );
    }
    #[test]
    fn distant_arms_keep_sparse_extents_without_enclosing_span() {
        let output = disjoint(&[r(0, 0, 4, 4), r(16000, 16000, 4, 4), None, None]);
        assert_eq!(output.len, 2);
        assert_eq!(
            output
                .iter()
                .map(|r| u64::from(r.width) * u64::from(r.height))
                .sum::<u64>(),
            32
        );
    }
    #[test]
    fn bounded_generated_rectangles_match_independent_per_pixel_coverage_once() {
        let mut seed = 0x41d8_193fu32;
        for _ in 0..512 {
            let mut source = [None; 4];
            for slot in &mut source {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let x = seed % 12;
                let y = (seed >> 8) % 8;
                let width = 1 + (seed >> 16) % (12 - x);
                let height = 1 + (seed >> 24) % (8 - y);
                *slot = r(x, y, width, height);
            }
            check(source, 12, 8);
        }
    }
}
