//! Finding vectors that lie exactly on top of one another.
//!
//! Two shapes are duplicates when every point of each one lies within the
//! tolerance of the other, whatever their direction, start node or node
//! count. Imported drawings often carry such copies, which then cut twice.

use super::flatten::flatten_path;
use super::metrics::{nearest_point_on_path, path_bbox, path_length};
use super::path::Path;
use super::point::{Point, Tolerance};

/// One shape to compare, with what the quick checks need worked out once.
struct Candidate<'a> {
    path: &'a Path,
    min: Point,
    max: Point,
    length: f64,
    samples: Vec<Point>,
}

/// Groups shapes that overlap exactly and answers, for every group, the
/// indices after the first one: the copies that can be removed.
///
/// `paths` is in z-order, so the one kept is the one drawn first. `tol` is
/// how far apart, in millimetres, two shapes may be and still count as the
/// same. Empty paths are never duplicates. The answer is sorted.
pub fn duplicate_indices(paths: &[Path], tol: Tolerance) -> Vec<usize> {
    let flat = Tolerance::new((tol.get() / 4.0).max(1e-4)).unwrap_or_default();
    let candidates: Vec<Option<Candidate>> = paths
        .iter()
        .map(|path| {
            let bbox = path_bbox(path)?;
            Some(Candidate {
                path,
                min: bbox.min,
                max: bbox.max,
                length: path_length(path, flat),
                samples: samples(path, flat),
            })
        })
        .collect();

    // Sort by the left edge so only neighbours within the tolerance are
    // compared in full.
    let mut order: Vec<usize> = (0..paths.len())
        .filter(|i| candidates[*i].is_some())
        .collect();
    order.sort_by(|a, b| {
        let ax = candidates[*a].as_ref().map_or(0.0, |c| c.min.x);
        let bx = candidates[*b].as_ref().map_or(0.0, |c| c.min.x);
        ax.total_cmp(&bx).then(a.cmp(b))
    });

    let t = tol.get();
    let mut copy_of: Vec<Option<usize>> = vec![None; paths.len()];
    for (pos, &i) in order.iter().enumerate() {
        let Some(a) = candidates[i].as_ref() else {
            continue;
        };
        for &j in &order[pos + 1..] {
            let Some(b) = candidates[j].as_ref() else {
                continue;
            };
            if b.min.x - a.min.x > t {
                break;
            }
            let (first, second) = if i < j { (i, j) } else { (j, i) };
            if copy_of[second].is_some() {
                continue;
            }
            if same_shape(a, b, t, flat) {
                // Chain to the earliest shape of the group.
                let root = copy_of[first].unwrap_or(first);
                copy_of[second] = Some(root);
            }
        }
    }
    (0..paths.len()).filter(|i| copy_of[*i].is_some()).collect()
}

/// Points spread along a path: every flattened vertex plus the midpoint of
/// each flattened edge, so long straight runs are checked in the middle too.
fn samples(path: &Path, flat: Tolerance) -> Vec<Point> {
    let mut out = Vec::new();
    for polyline in flatten_path(path, flat) {
        let pts = &polyline.points;
        for (k, p) in pts.iter().enumerate() {
            out.push(*p);
            if let Some(next) = pts.get(k + 1) {
                out.push(p.lerp(*next, 0.5));
            }
        }
    }
    out
}

fn same_shape(a: &Candidate, b: &Candidate, t: f64, flat: Tolerance) -> bool {
    if (a.min.x - b.min.x).abs() > t
        || (a.min.y - b.min.y).abs() > t
        || (a.max.x - b.max.x).abs() > t
        || (a.max.y - b.max.y).abs() > t
    {
        return false;
    }
    // Lengths of the same curve flattened two ways differ a little; allow for
    // that plus the tolerance along every sample.
    let slack = t * 2.0 + (a.length.max(b.length) * 1e-3);
    if (a.length - b.length).abs() > slack {
        return false;
    }
    covers(a, b.path, t, flat) && covers(b, a.path, t, flat)
}

/// True when every sample of `a` lies within `t` of `other`.
fn covers(a: &Candidate, other: &Path, t: f64, flat: Tolerance) -> bool {
    a.samples
        .iter()
        .all(|p| nearest_point_on_path(other, *p, flat).is_some_and(|near| near.distance <= t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::path::SubPath;

    fn square(x: f64, y: f64, size: f64) -> Path {
        let mut sub = SubPath::open(Point::new(x, y));
        sub.line_to(Point::new(x + size, y))
            .line_to(Point::new(x + size, y + size))
            .line_to(Point::new(x, y + size))
            .line_to(Point::new(x, y));
        sub.closed = true;
        Path::from_subpath(sub)
    }

    /// The same square, drawn clockwise from a different corner, with an
    /// extra node half way along one side.
    fn square_reworked(x: f64, y: f64, size: f64) -> Path {
        let mut sub = SubPath::open(Point::new(x + size, y + size));
        sub.line_to(Point::new(x + size, y))
            .line_to(Point::new(x + size / 2.0, y))
            .line_to(Point::new(x, y))
            .line_to(Point::new(x, y + size))
            .line_to(Point::new(x + size, y + size));
        sub.closed = true;
        Path::from_subpath(sub)
    }

    fn line(a: (f64, f64), b: (f64, f64)) -> Path {
        let mut sub = SubPath::open(Point::new(a.0, a.1));
        sub.line_to(Point::new(b.0, b.1));
        Path::from_subpath(sub)
    }

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    #[test]
    fn exact_copies_keep_the_first_and_report_the_rest() {
        let paths = vec![
            square(0.0, 0.0, 10.0),
            square(20.0, 0.0, 10.0),
            square(0.0, 0.0, 10.0),
            square(0.0, 0.0, 10.0),
        ];
        assert_eq!(duplicate_indices(&paths, tol()), vec![2, 3]);
    }

    #[test]
    fn direction_start_node_and_extra_nodes_do_not_matter() {
        let paths = vec![square(5.0, 5.0, 10.0), square_reworked(5.0, 5.0, 10.0)];
        assert_eq!(duplicate_indices(&paths, tol()), vec![1]);
        let lines = vec![line((0.0, 0.0), (10.0, 0.0)), line((10.0, 0.0), (0.0, 0.0))];
        assert_eq!(duplicate_indices(&lines, tol()), vec![1]);
    }

    #[test]
    fn shapes_that_only_partly_overlap_are_not_duplicates() {
        // Same box, different shape: a square against its diagonal-cut twin.
        let mut sub = SubPath::open(Point::new(0.0, 0.0));
        sub.line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0))
            .line_to(Point::new(0.0, 0.0));
        sub.closed = true;
        let paths = vec![
            square(0.0, 0.0, 10.0),
            Path::from_subpath(sub),
            line((0.0, 0.0), (10.0, 0.0)),
            line((0.0, 0.0), (5.0, 0.0)),
            // A line that doubles back over itself covers the same ground
            // but is twice as long.
            {
                let mut sub = SubPath::open(Point::new(0.0, 0.0));
                sub.line_to(Point::new(10.0, 0.0))
                    .line_to(Point::new(0.0, 0.0));
                Path::from_subpath(sub)
            },
        ];
        assert!(duplicate_indices(&paths, tol()).is_empty());
    }

    #[test]
    fn the_tolerance_decides_how_close_counts() {
        let paths = vec![square(0.0, 0.0, 10.0), square(0.005, 0.0, 10.0)];
        assert_eq!(duplicate_indices(&paths, tol()), vec![1]);
        let tight = Tolerance::new(0.001).expect("tolerance");
        assert!(duplicate_indices(&paths, tight).is_empty());
    }

    #[test]
    fn empty_paths_are_ignored() {
        let paths = vec![Path::new(), Path::new(), square(0.0, 0.0, 1.0)];
        assert!(duplicate_indices(&paths, tol()).is_empty());
    }
}
