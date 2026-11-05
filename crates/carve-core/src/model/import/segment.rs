//! 3D Segmenting: a model cut by planes into segments (a tree whose root is
//! the whole model and where every cut gives a segment two children),
//! segments merged back together, each segment turned its own way and laid
//! out side by side, and each projected into its own component.
//!
//! A segment is a list of leaves, each leaf the half spaces on its path
//! from the root. Nothing is rebuilt geometrically: the preview clips the
//! triangles, and the projection reads the solid along vertical lines
//! within the half spaces, so cut faces come out flat (capped) for free.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::{BBox, Point};
use crate::io::mesh::Triangle;

use super::super::field::Field;
use super::super::shape::Grid;
use super::raster::{window, z_range, HalfSpace, Hits};
use super::vec3::{self, V3};

/// A segmenting plane: a point on it and two tilts of the level plane.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CutPlane {
    /// A point on the plane, millimetres in the oriented model's space.
    pub point: V3,
    /// Tilt about X, degrees.
    pub angle_x: f64,
    /// Tilt about Y after the X tilt, degrees.
    pub angle_y: f64,
}

impl Default for CutPlane {
    fn default() -> Self {
        CutPlane {
            point: [0.0; 3],
            angle_x: 0.0,
            angle_y: 0.0,
        }
    }
}

impl CutPlane {
    /// The unit normal, pointing to the side the first child keeps.
    pub fn normal(&self) -> V3 {
        let n = vec3::rotate_x([0.0, 0.0, 1.0], self.angle_x.to_radians());
        vec3::rotate_y(n, self.angle_y.to_radians())
    }

    /// The side the first child keeps.
    pub fn above(&self) -> HalfSpace {
        HalfSpace::through(self.point, self.normal())
    }
}

/// A node of the segment tree: a leaf, or a cut with two children, the
/// first above the plane and the second below it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SegmentNode {
    /// The plane this node is cut along; none for a leaf.
    pub cut: Option<CutPlane>,
    /// Empty for a leaf, else the two halves.
    pub children: Vec<SegmentNode>,
}

/// How one segment is turned in the layout: quarter turns from the arrow
/// buttons plus free turns, degrees, about X, then Y, then Z.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SegmentLayout {
    /// Quarter turns about X.
    pub quarter_x: i32,
    /// Quarter turns about Y.
    pub quarter_y: i32,
    /// Quarter turns about Z.
    pub quarter_z: i32,
    /// Free turn about X, degrees.
    pub rot_x: f64,
    /// Free turn about Y, degrees.
    pub rot_y: f64,
    /// Free turn about Z, degrees.
    pub rot_z: f64,
}

impl SegmentLayout {
    /// Turns a point (a linear map, about the origin).
    pub fn turn(&self, v: V3) -> V3 {
        let q = std::f64::consts::FRAC_PI_2;
        let ax = f64::from(self.quarter_x) * q + self.rot_x.to_radians();
        let ay = f64::from(self.quarter_y) * q + self.rot_y.to_radians();
        let az = f64::from(self.quarter_z) * q + self.rot_z.to_radians();
        vec3::rotate_z(vec3::rotate_y(vec3::rotate_x(v, ax), ay), az)
    }
}

/// Everything the Segment form holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SegmentPlan {
    /// The tree; its root is the whole model.
    pub root: SegmentNode,
    /// Groups of leaf ids merged into one segment.
    pub merges: Vec<Vec<String>>,
    /// Turns per segment id (the first leaf id of a merged segment).
    pub layouts: BTreeMap<String, SegmentLayout>,
    /// Fill the cut faces flat (Create Backface Caps).
    pub caps: bool,
    /// Grow the material to hold the laid out segments.
    pub resize_material: bool,
    /// Draw each segment's top and bottom outlines as vectors.
    pub boundary_vectors: bool,
    /// Space between laid out segments, millimetres.
    pub gap: f64,
}

impl Default for SegmentPlan {
    fn default() -> Self {
        SegmentPlan {
            root: SegmentNode::default(),
            merges: Vec::new(),
            layouts: BTreeMap::new(),
            caps: true,
            resize_material: false,
            boundary_vectors: false,
            gap: 5.0,
        }
    }
}

/// One segment: leaves that each keep the points inside all their half
/// spaces; the segment is their union.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// The id of its first leaf: "r" for the root, then one digit per
    /// level ("r01" is the root's first child's second child).
    pub id: String,
    /// Ids of every leaf in it.
    pub leaf_ids: Vec<String>,
    /// Each leaf's half spaces.
    pub leaves: Vec<Vec<HalfSpace>>,
}

fn collect_leaves(
    node: &SegmentNode,
    id: String,
    path: &[HalfSpace],
    out: &mut Vec<(String, Vec<HalfSpace>)>,
) {
    match (&node.cut, node.children.as_slice()) {
        (Some(cut), [a, b]) => {
            let above = cut.above();
            let mut left = path.to_vec();
            left.push(above);
            collect_leaves(a, format!("{id}0"), &left, out);
            let mut right = path.to_vec();
            right.push(above.flipped());
            collect_leaves(b, format!("{id}1"), &right, out);
        }
        _ => out.push((id, path.to_vec())),
    }
}

/// The segments of `plan`, in tree order; merged leaves make one segment.
/// Merge ids that name no leaf are ignored, as is a leaf's second group.
pub fn segments(plan: &SegmentPlan) -> Vec<Segment> {
    let mut leaves = Vec::new();
    collect_leaves(&plan.root, "r".to_string(), &[], &mut leaves);
    let mut group_of: BTreeMap<&str, usize> = BTreeMap::new();
    for (g, group) in plan.merges.iter().enumerate() {
        for id in group {
            group_of.entry(id.as_str()).or_insert(g);
        }
    }
    let mut out: Vec<Segment> = Vec::new();
    let mut placed_group: BTreeMap<usize, usize> = BTreeMap::new();
    for (id, spaces) in &leaves {
        if let Some(&g) = group_of.get(id.as_str()) {
            if let Some(&at) = placed_group.get(&g) {
                out[at].leaf_ids.push(id.clone());
                out[at].leaves.push(spaces.clone());
                continue;
            }
            placed_group.insert(g, out.len());
        }
        out.push(Segment {
            id: id.clone(),
            leaf_ids: vec![id.clone()],
            leaves: vec![spaces.clone()],
        });
    }
    out
}

/// Clips one triangle to a half space: the kept part, as up to two
/// triangles.
fn clip_one(t: &Triangle, s: &HalfSpace, out: &mut Vec<Triangle>) {
    let d = [s.distance(t[0]), s.distance(t[1]), s.distance(t[2])];
    if d.iter().all(|v| *v >= 0.0) {
        out.push(*t);
        return;
    }
    if d.iter().all(|v| *v <= 0.0) {
        return;
    }
    let mut poly: Vec<V3> = Vec::with_capacity(4);
    for k in 0..3 {
        let (a, b) = (t[k], t[(k + 1) % 3]);
        let (da, db) = (d[k], d[(k + 1) % 3]);
        if da >= 0.0 {
            poly.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            let f = da / (da - db);
            poly.push(vec3::add_scaled(a, vec3::sub(b, a), f));
        }
    }
    for k in 1..poly.len().saturating_sub(1) {
        let tri = [poly[0], poly[k], poly[k + 1]];
        if vec3::length(vec3::normal(&tri)) > 1e-12 {
            out.push(tri);
        }
    }
}

/// The parts of `triangles` inside every half space of `spaces`.
pub fn clip(triangles: &[Triangle], spaces: &[HalfSpace]) -> Vec<Triangle> {
    let mut current = triangles.to_vec();
    let mut next = Vec::with_capacity(current.len());
    for s in spaces {
        next.clear();
        for t in &current {
            clip_one(t, s, &mut next);
        }
        std::mem::swap(&mut current, &mut next);
    }
    current
}

/// The triangles of a segment (the union of its leaves' parts).
pub fn segment_triangles(triangles: &[Triangle], segment: &Segment) -> Vec<Triangle> {
    segment
        .leaves
        .iter()
        .flat_map(|spaces| clip(triangles, spaces))
        .collect()
}

/// Where one segment goes in the layout.
#[derive(Debug, Clone, PartialEq)]
pub struct Laid {
    /// Its turn.
    pub layout: SegmentLayout,
    /// Added after the turn, millimetres, so that it sits on Z zero at its
    /// place in the layout.
    pub offset: V3,
    /// Extent after the turn, millimetres.
    pub size: V3,
}

impl Laid {
    /// A model point moved to its place in the layout.
    pub fn place(&self, v: V3) -> V3 {
        let t = self.layout.turn(v);
        [
            t[0] + self.offset[0],
            t[1] + self.offset[1],
            t[2] + self.offset[2],
        ]
    }

    /// A half space of the model moved with the segment.
    pub fn place_space(&self, s: HalfSpace) -> HalfSpace {
        let turned = s.turned(|v| self.layout.turn(v));
        HalfSpace {
            normal: turned.normal,
            offset: turned.offset + vec3::dot(turned.normal, self.offset),
        }
    }
}

/// Lays the segments out in rows across `area`, left to right and then
/// upward, `gap` apart, the whole arrangement centred on the area. Answers
/// each segment's placement and the extent of the arrangement.
pub fn lay_out(
    triangles: &[Triangle],
    segs: &[Segment],
    plan: &SegmentPlan,
    area: BBox,
) -> (Vec<Laid>, BBox) {
    let gap = if plan.gap.is_finite() && plan.gap >= 0.0 {
        plan.gap
    } else {
        5.0
    };
    let mut boxes = Vec::with_capacity(segs.len());
    for seg in segs {
        let layout = plan.layouts.get(&seg.id).copied().unwrap_or_default();
        let mut lo = [f64::MAX; 3];
        let mut hi = [f64::MIN; 3];
        for v in segment_triangles(triangles, seg).iter().flatten() {
            let t = layout.turn(*v);
            for a in 0..3 {
                lo[a] = lo[a].min(t[a]);
                hi[a] = hi[a].max(t[a]);
            }
        }
        if lo[0] > hi[0] {
            lo = [0.0; 3];
            hi = [0.0; 3];
        }
        boxes.push((layout, lo, hi));
    }
    let widest = boxes
        .iter()
        .map(|(_, lo, hi)| hi[0] - lo[0])
        .fold(0.0, f64::max);
    let row_limit = area.width().max(widest);
    // Positions of each box's low corner, before centring.
    let mut spots = Vec::with_capacity(boxes.len());
    let (mut x, mut y, mut row_depth, mut extent_x) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for (_, lo, hi) in &boxes {
        let (w, d) = (hi[0] - lo[0], hi[1] - lo[1]);
        if x > 0.0 && x + w > row_limit {
            y += row_depth + gap;
            x = 0.0;
            row_depth = 0.0;
        }
        spots.push((x, y));
        extent_x = extent_x.max(x + w);
        x += w + gap;
        row_depth = row_depth.max(d);
    }
    let extent_y = y + row_depth;
    let centre = area.centre();
    let (x0, y0) = (centre.x - extent_x / 2.0, centre.y - extent_y / 2.0);
    let laid = boxes
        .iter()
        .zip(&spots)
        .map(|((layout, lo, hi), (sx, sy))| Laid {
            layout: *layout,
            offset: [x0 + sx - lo[0], y0 + sy - lo[1], -lo[2]],
            size: vec3::sub(*hi, *lo),
        })
        .collect();
    let extent = BBox::new(Point::new(x0, y0), Point::new(x0 + extent_x, y0 + extent_y));
    (laid, extent)
}

/// A segment projected onto the model grid.
#[derive(Debug, Clone, PartialEq)]
pub struct Projected {
    /// Heights from Z zero, cropped; `None` when it misses the grid.
    pub top: Option<Field>,
    /// Cells where the segment's underside rests on Z zero (1) or not
    /// (empty), on the same grid as `top`, for its bottom outline.
    pub base: Option<Field>,
}

/// Projects one laid out segment onto `grid`: over each cell, the highest
/// point of the segment (of its solid when `plan.caps`, else of its
/// surface).
pub fn project(
    triangles: &[Triangle],
    segment: &Segment,
    laid: &Laid,
    grid: &Grid,
    caps: bool,
) -> Projected {
    let moved: Vec<Triangle> = triangles.iter().map(|t| t.map(|v| laid.place(v))).collect();
    let leaves: Vec<Vec<HalfSpace>> = segment
        .leaves
        .iter()
        .map(|l| l.iter().map(|s| laid.place_space(*s)).collect())
        .collect();
    let mut lo = Point::new(f64::MAX, f64::MAX);
    let mut hi = Point::new(f64::MIN, f64::MIN);
    for spaces in &leaves {
        for v in clip(&moved, spaces).iter().flatten() {
            lo = Point::new(lo.x.min(v[0]), lo.y.min(v[1]));
            hi = Point::new(hi.x.max(v[0]), hi.y.max(v[1]));
        }
    }
    let Some(part) = (if lo.x <= hi.x {
        window(grid, lo, hi)
    } else {
        None
    }) else {
        return Projected {
            top: None,
            base: None,
        };
    };
    let hits = Hits::cast(&moved, &part);
    let mut top = Field::empty(part.width, part.height, part.cell, part.origin);
    let mut base = top.clone();
    let rest = (part.cell * 0.5).max(1e-6);
    for cell in 0..part.width * part.height {
        if !hits.any(cell) {
            continue;
        }
        let c = hits.centre(cell);
        let (mut high, mut low): (Option<f64>, Option<f64>) = (None, None);
        for spaces in &leaves {
            let Some(range) = z_range(spaces, c.x, c.y, hits.z_lo, hits.z_hi) else {
                continue;
            };
            if let Some(h) = hits.highest(cell, range, caps) {
                high = Some(high.map_or(h, |x| x.max(h)));
            }
            if let Some(h) = hits.lowest(cell, range, caps) {
                low = Some(low.map_or(h, |x| x.min(h)));
            }
        }
        if let Some(h) = high {
            top.z[cell] = h.max(0.0) as f32;
        }
        if low.is_some_and(|l| l <= rest) {
            base.z[cell] = 1.0;
        }
    }
    Projected {
        top: top.cropped(),
        base: base.cropped(),
    }
}

/// Checks the plan's planes are numbers.
///
/// # Errors
/// Returns [`CoreError::Validation`] naming the problem.
pub fn check(plan: &SegmentPlan) -> Result<(), CoreError> {
    fn walk(node: &SegmentNode) -> Result<(), CoreError> {
        if let Some(cut) = &node.cut {
            let ok = cut.point.iter().all(|v| v.is_finite())
                && cut.angle_x.is_finite()
                && cut.angle_y.is_finite();
            if !ok {
                return Err(CoreError::validation("a segmenting plane must be numbers"));
            }
            if node.children.len() != 2 {
                return Err(CoreError::validation("a cut segment has two children"));
            }
        } else if !node.children.is_empty() {
            return Err(CoreError::validation(
                "a segment with children needs a plane",
            ));
        }
        node.children.iter().try_for_each(walk)
    }
    walk(&plan.root)
}

#[cfg(test)]
mod tests {
    use super::super::raster::tests::cube;
    use super::*;

    fn level_cut(z: f64) -> SegmentNode {
        SegmentNode {
            cut: Some(CutPlane {
                point: [0.0, 0.0, z],
                ..CutPlane::default()
            }),
            children: vec![SegmentNode::default(), SegmentNode::default()],
        }
    }

    fn area() -> BBox {
        BBox::new(Point::new(0.0, 0.0), Point::new(200.0, 100.0))
    }

    fn grid() -> Grid {
        Grid {
            width: 400,
            height: 200,
            cell: 0.5,
            origin: Point::new(0.25, 0.25),
        }
    }

    #[test]
    fn a_cut_splits_the_triangles_between_the_children() {
        let mesh = cube([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
        let plan = SegmentPlan {
            root: level_cut(4.0),
            ..SegmentPlan::default()
        };
        let segs = segments(&plan);
        assert_eq!(
            segs.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["r0", "r1"]
        );
        let above = segment_triangles(&mesh, &segs[0]);
        let below = segment_triangles(&mesh, &segs[1]);
        let z_range_of = |ts: &[Triangle]| {
            ts.iter()
                .flatten()
                .fold((f64::MAX, f64::MIN), |(lo, hi), v| {
                    (lo.min(v[2]), hi.max(v[2]))
                })
        };
        assert_eq!(z_range_of(&above), (4.0, 10.0));
        assert_eq!(z_range_of(&below), (0.0, 4.0));
        // Side areas add up to the uncut side: 4 walls of 10 by 10.
        let area = |ts: &[Triangle]| {
            ts.iter()
                .filter(|t| vec3::normal(t)[2].abs() < 1e-9)
                .map(|t| vec3::length(vec3::normal(t)) / 2.0)
                .sum::<f64>()
        };
        assert!((area(&above) + area(&below) - 400.0).abs() < 1e-6);
    }

    #[test]
    fn merging_leaves_makes_one_segment_again() {
        let mut root = level_cut(6.0);
        root.children[1] = level_cut(3.0);
        let plan = SegmentPlan {
            root,
            merges: vec![vec!["r0".into(), "r11".into()], vec!["nope".into()]],
            ..SegmentPlan::default()
        };
        let segs = segments(&plan);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].leaf_ids, ["r0", "r11"]);
        assert_eq!(segs[1].id, "r10");
        let mesh = cube([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
        let merged = segment_triangles(&mesh, &segs[0]);
        assert!(merged.iter().flatten().all(|v| v[2] >= 6.0 || v[2] <= 3.0));
    }

    #[test]
    fn caps_fill_the_cut_face_flat() {
        let mesh = cube([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
        let plan = SegmentPlan {
            root: level_cut(4.0),
            ..SegmentPlan::default()
        };
        let segs = segments(&plan);
        let (laid, _) = lay_out(&mesh, &segs, &plan, area());
        // The lower part keeps its orientation: its open side faces up.
        let capped = project(&mesh, &segs[1], &laid[1], &grid(), true);
        let open = project(&mesh, &segs[1], &laid[1], &grid(), false);
        let top = capped.top.expect("covers cells");
        assert_eq!(top.max(), Some(4.0));
        assert_eq!(top.min(), Some(4.0));
        // Without a cap the line sees down to the bottom face.
        assert_eq!(open.top.expect("covers cells").max(), Some(0.0));
        let base = capped.base.expect("rests on the bed");
        assert_eq!(base.width, top.width);
    }

    #[test]
    fn segments_are_laid_out_apart_and_turned() {
        let mesh = cube([0.0, 0.0, 0.0], [10.0, 20.0, 10.0]);
        let mut plan = SegmentPlan {
            root: level_cut(5.0),
            ..SegmentPlan::default()
        };
        plan.layouts.insert(
            "r1".into(),
            SegmentLayout {
                quarter_x: 2,
                ..SegmentLayout::default()
            },
        );
        let segs = segments(&plan);
        let (laid, extent) = lay_out(&mesh, &segs, &plan, area());
        assert!((extent.width() - 25.0).abs() < 1e-9, "two 10 wide, 5 apart");
        assert!((extent.centre().x - 100.0).abs() < 1e-9);
        assert!((laid[1].size[2] - 5.0).abs() < 1e-9);
        // Turned over, the lower part's cut face is now at its bottom and
        // its flat bottom on top, 5 high.
        let p = project(&mesh, &segs[1], &laid[1], &grid(), false);
        let top = p.top.expect("covers cells");
        assert!((f64::from(top.max().unwrap_or(0.0)) - 5.0).abs() < 1e-6);
        assert!(laid[0].offset[0] + 10.0 < laid[1].offset[0] + 1e-9 || laid[1].size[0] > 0.0);
        // A half space moved with its segment still separates the same
        // points.
        let s = segs[1].leaves[0][0];
        let p0 = [5.0, 5.0, 2.0];
        assert!(s.distance(p0) > 0.0);
        assert!(laid[1].place_space(s).distance(laid[1].place(p0)) > 0.0);
    }

    #[test]
    fn bad_trees_are_refused() {
        let mut root = level_cut(1.0);
        root.children.pop();
        assert!(check(&SegmentPlan {
            root,
            ..SegmentPlan::default()
        })
        .is_err());
        let mut root = level_cut(f64::NAN);
        assert!(check(&SegmentPlan {
            root: root.clone(),
            ..SegmentPlan::default()
        })
        .is_err());
        root.cut = None;
        assert!(check(&SegmentPlan {
            root,
            ..SegmentPlan::default()
        })
        .is_err());
    }
}
