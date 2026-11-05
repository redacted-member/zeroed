//! Snapping: what the cursor can latch on to near a point.
//!
//! `entities.probe` is asked on every mouse move while a tool is live, so it
//! works from bounding boxes first and only looks at geometry that could be
//! within the radius. Hits are sorted by kind priority and then by distance,
//! which is the order the 2D view shows them in: a node beats an
//! intersection, which beats a midpoint, and the grid comes last.

use serde::{Deserialize, Serialize};

use super::Document;
use crate::geom::{
    nearest_point_on_path, path_intersections, ArcGeom, BBox, Path, Point, Segment, Tolerance,
};

/// What kinds of snap the user has switched on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SnapOptions {
    /// Snap to the grid.
    pub grid: bool,
    /// Snap to guide lines.
    pub guides: bool,
    /// Snap to path nodes.
    pub nodes: bool,
    /// Snap to segment midpoints.
    pub midpoints: bool,
    /// Snap to arc and circle centres, and to their quadrant points.
    pub centres: bool,
    /// Snap to where two vectors cross.
    pub intersections: bool,
    /// Snap to the job corners and centre.
    pub job_points: bool,
    /// Snap to alignment lines inferred from nearby nodes.
    pub smart: bool,
}

impl Default for SnapOptions {
    fn default() -> Self {
        SnapOptions {
            grid: true,
            guides: true,
            nodes: true,
            midpoints: true,
            centres: true,
            intersections: true,
            job_points: true,
            smart: false,
        }
    }
}

/// A guide line the frontend is holding for the user.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guide {
    /// Guide id, echoed back on a hit.
    pub id: u64,
    /// `x` for a vertical guide at `position`, `y` for a horizontal one.
    pub axis: GuideAxis,
    /// Where the guide sits, in millimetres.
    pub position: f64,
    /// Whether the user has locked the guide in place.
    #[serde(default)]
    pub locked: bool,
}

/// Which way a guide runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GuideAxis {
    /// A vertical guide, fixed in X.
    X,
    /// A horizontal guide, fixed in Y.
    Y,
}

/// What `entities.probe` was asked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeArgs {
    /// Cursor X in millimetres.
    pub x: f64,
    /// Cursor Y in millimetres.
    pub y: f64,
    /// How far from the cursor a snap may sit, in millimetres.
    pub radius: f64,
    /// Which snaps are switched on.
    #[serde(default)]
    pub snap: SnapOptions,
    /// Grid pitch in millimetres. Zero switches the grid snap off.
    #[serde(default)]
    pub grid_spacing: f64,
    /// Guides the frontend is holding.
    #[serde(default)]
    pub guides: Vec<Guide>,
    /// Entities to ignore, usually the one being dragged.
    #[serde(default)]
    pub exclude: Vec<u64>,
}

/// What kind of thing a snap landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapKind {
    /// A node of a vector.
    Node,
    /// The midpoint of a segment.
    Mid,
    /// The centre of an arc or circle.
    Centre,
    /// A quadrant point of an arc or circle.
    Quadrant,
    /// The nearest point on a vector.
    On,
    /// Where two vectors cross.
    Intersection,
    /// A guide line.
    Guide,
    /// A grid point.
    Grid,
    /// A corner of the material.
    JobCorner,
    /// The centre of the material.
    JobCentre,
}

impl SnapKind {
    /// Sort order. Lower wins when two hits are the same distance away.
    pub fn priority(self) -> u8 {
        match self {
            SnapKind::Node => 0,
            SnapKind::Intersection => 1,
            SnapKind::Mid => 2,
            SnapKind::Centre => 3,
            SnapKind::Quadrant => 4,
            SnapKind::JobCorner | SnapKind::JobCentre => 5,
            SnapKind::On => 6,
            SnapKind::Guide => 7,
            SnapKind::Grid => 8,
        }
    }
}

/// One snap candidate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeHit {
    /// What was hit.
    pub kind: SnapKind,
    /// Snap X in millimetres.
    pub x: f64,
    /// Snap Y in millimetres.
    pub y: f64,
    /// Distance from the cursor in millimetres.
    pub dist: f64,
    /// Entity or guide the hit belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
}

/// What `entities.probe` answers with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeReply {
    /// Candidates, best first.
    pub hits: Vec<ProbeHit>,
    /// Top level entity the cursor is nearest to, when it is within radius.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nearest_entity: Option<u64>,
}

/// How many snap candidates one probe answers with. The 2D view only draws
/// the best one, the rest are there for the "cycle snaps" key.
const MAX_HITS: usize = 64;

struct Collector {
    at: Point,
    radius: f64,
    hits: Vec<ProbeHit>,
}

impl Collector {
    fn push(&mut self, kind: SnapKind, p: Point, id: Option<u64>) {
        let dist = self.at.distance(p);
        if dist <= self.radius && p.is_finite() {
            self.hits.push(ProbeHit {
                kind,
                x: p.x,
                y: p.y,
                dist,
                id,
            });
        }
    }
}

/// Finds everything the cursor could snap to.
pub fn probe(doc: &Document, args: &ProbeArgs) -> ProbeReply {
    let at = Point::new(args.x, args.y);
    let radius = if args.radius.is_finite() && args.radius > 0.0 {
        args.radius
    } else {
        0.0
    };
    let mut collector = Collector {
        at,
        radius,
        hits: Vec::new(),
    };
    let box_around = BBox::new(
        Point::new(at.x - radius, at.y - radius),
        Point::new(at.x + radius, at.y + radius),
    );
    let tol = Tolerance::default();

    // Only geometry whose box reaches the cursor can produce a hit.
    let mut nearby: Vec<(u64, u64, Path)> = Vec::new();
    for top in doc.selectable() {
        if args.exclude.contains(&top) {
            continue;
        }
        for leaf in doc.leaves(top) {
            if args.exclude.contains(&leaf) {
                continue;
            }
            let Some(entity) = doc.entities().get(&leaf) else {
                continue;
            };
            let Some(path) = entity.path() else {
                continue;
            };
            let Some(bbox) = entity.bbox() else {
                continue;
            };
            if bbox.expand(radius).intersects(box_around) {
                nearby.push((top, leaf, path));
            }
        }
    }

    let mut nearest: Option<(u64, f64)> = None;
    for (top, leaf, path) in &nearby {
        for sub in &path.subpaths {
            let mut from = sub.start;
            if args.snap.nodes {
                collector.push(SnapKind::Node, sub.start, Some(*leaf));
            }
            let last = sub.segments.len().saturating_sub(1);
            for (index, segment) in sub.segments.iter().enumerate() {
                let to = segment.end();
                // A closed subpath ends on its own start, so its last node is
                // already in the list.
                if args.snap.nodes && !(sub.closed && index == last) {
                    collector.push(SnapKind::Node, to, Some(*leaf));
                }
                if args.snap.midpoints {
                    collector.push(SnapKind::Mid, midpoint(from, segment), Some(*leaf));
                }
                if args.snap.centres {
                    if let Segment::Arc { to, bulge } = *segment {
                        if let Some(arc) = ArcGeom::from_bulge(from, to, bulge) {
                            collector.push(SnapKind::Centre, arc.centre, Some(*leaf));
                            for quadrant in quadrants(&arc) {
                                collector.push(SnapKind::Quadrant, quadrant, Some(*leaf));
                            }
                        }
                    }
                }
                from = to;
            }
        }
        if let Some(on) = nearest_point_on_path(path, at, tol) {
            if on.distance <= radius {
                collector.push(SnapKind::On, on.point, Some(*leaf));
                if nearest.is_none_or(|(_, d)| on.distance < d) {
                    nearest = Some((*top, on.distance));
                }
            }
        }
    }

    if args.snap.intersections {
        for i in 0..nearby.len() {
            for j in (i + 1)..nearby.len() {
                for hit in path_intersections(&nearby[i].2, &nearby[j].2, tol) {
                    collector.push(SnapKind::Intersection, hit.point, Some(nearby[i].1));
                }
            }
        }
    }

    if args.snap.smart {
        // Alignment lines inferred from nodes that line up with the cursor.
        // These reach further than the snap radius along their own axis, so
        // the search is a band through the cursor rather than a disc.
        let mut inferred: Vec<Point> = Vec::new();
        for top in doc.selectable() {
            if args.exclude.contains(&top) {
                continue;
            }
            for leaf in doc.leaves(top) {
                if args.exclude.contains(&leaf) {
                    continue;
                }
                let Some(entity) = doc.entities().get(&leaf) else {
                    continue;
                };
                let (Some(path), Some(bbox)) = (entity.path(), entity.bbox()) else {
                    continue;
                };
                let in_x = bbox.min.x - radius <= at.x && at.x <= bbox.max.x + radius;
                let in_y = bbox.min.y - radius <= at.y && at.y <= bbox.max.y + radius;
                if !in_x && !in_y {
                    continue;
                }
                for sub in &path.subpaths {
                    for node in sub.nodes() {
                        inferred.push(Point::new(node.x, at.y));
                        inferred.push(Point::new(at.x, node.y));
                    }
                }
            }
        }
        for point in inferred {
            collector.push(SnapKind::Guide, point, None);
        }
    }

    if args.snap.guides {
        for guide in &args.guides {
            let point = match guide.axis {
                GuideAxis::X => Point::new(guide.position, at.y),
                GuideAxis::Y => Point::new(at.x, guide.position),
            };
            collector.push(SnapKind::Guide, point, Some(guide.id));
        }
    }

    if args.snap.job_points {
        let bounds = doc.job_bounds();
        for corner in [
            Point::new(bounds.min.x, bounds.min.y),
            Point::new(bounds.max.x, bounds.min.y),
            Point::new(bounds.min.x, bounds.max.y),
            Point::new(bounds.max.x, bounds.max.y),
        ] {
            collector.push(SnapKind::JobCorner, corner, None);
        }
        collector.push(SnapKind::JobCentre, bounds.centre(), None);
    }

    if args.snap.grid && args.grid_spacing > 0.0 && args.grid_spacing.is_finite() {
        let step = args.grid_spacing;
        let point = Point::new((at.x / step).round() * step, (at.y / step).round() * step);
        collector.push(SnapKind::Grid, point, None);
    }

    let mut hits = collector.hits;
    hits.sort_by(|a, b| {
        a.kind.priority().cmp(&b.kind.priority()).then(
            a.dist
                .partial_cmp(&b.dist)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    hits.dedup_by(|a, b| a.kind == b.kind && (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
    hits.truncate(MAX_HITS);
    ProbeReply {
        hits,
        nearest_entity: nearest.map(|(id, _)| id),
    }
}

/// Middle of a segment, by parameter.
fn midpoint(start: Point, segment: &Segment) -> Point {
    crate::geom::segment_point_at(start, segment, 0.5)
}

/// What `entities.hitTest` was asked: a click or a rubber band.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum HitTestArgs {
    /// A click at a point, with a pick radius in millimetres.
    Point {
        /// Click X in millimetres.
        x: f64,
        /// Click Y in millimetres.
        y: f64,
        /// How far from the outline still counts as a hit.
        tolerance: f64,
    },
    /// A rubber band box.
    Rect {
        /// `[minX, minY, maxX, maxY]` in millimetres.
        rect: [f64; 4],
        /// Whether a vector must be wholly inside or only touch the box.
        mode: RectMode,
    },
}

/// How a rubber band decides what it caught.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RectMode {
    /// Only vectors that fit entirely inside the box.
    Inside,
    /// Any vector the box touches.
    Touch,
}

/// Finds the top level entities a click or a rubber band caught.
///
/// A click answers with the entities whose outline is within the tolerance,
/// nearest first, which is what lets repeated clicks cycle through stacked
/// vectors. Hidden and locked layers, and sheets other than the active one,
/// are left out.
pub fn hit_test(doc: &Document, args: &HitTestArgs) -> Vec<u64> {
    let tol = Tolerance::default();
    match *args {
        HitTestArgs::Point { x, y, tolerance } => {
            let at = Point::new(x, y);
            let reach = tolerance.max(0.0);
            let mut hits: Vec<(u64, f64)> = Vec::new();
            for top in doc.selectable() {
                let mut best = f64::INFINITY;
                for (_, path) in doc.paths_of(top) {
                    let Some(bbox) = crate::geom::path_bbox(&path) else {
                        continue;
                    };
                    if !bbox.expand(reach).contains(at) {
                        continue;
                    }
                    if let Some(near) = nearest_point_on_path(&path, at, tol) {
                        if near.distance <= reach {
                            best = best.min(near.distance);
                        }
                    }
                }
                // A bitmap is picked anywhere over its picture, after vectors.
                if !best.is_finite() && super::bitmap_cmd::bitmap_contains(doc, top, at) {
                    best = reach;
                }
                if best.is_finite() {
                    hits.push((top, best));
                }
            }
            hits.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            hits.into_iter().map(|(id, _)| id).collect()
        }
        HitTestArgs::Rect { rect, mode } => {
            let box_of = BBox::new(
                Point::new(rect[0].min(rect[2]), rect[1].min(rect[3])),
                Point::new(rect[0].max(rect[2]), rect[1].max(rect[3])),
            );
            doc.selectable()
                .into_iter()
                .filter(|top| {
                    let paths = doc.paths_of(*top);
                    if paths.is_empty() {
                        return false;
                    }
                    match mode {
                        RectMode::Inside => paths.iter().all(|(_, path)| {
                            crate::geom::path_bbox(path).is_some_and(|b| box_of.contains_bbox(b))
                        }),
                        RectMode::Touch => paths.iter().any(|(_, path)| touches(path, box_of, tol)),
                    }
                })
                .collect()
        }
    }
}

/// Whether any part of a path lies inside a box.
fn touches(path: &Path, box_of: BBox, tol: Tolerance) -> bool {
    let Some(bbox) = crate::geom::path_bbox(path) else {
        return false;
    };
    if !bbox.intersects(box_of) {
        return false;
    }
    for polyline in crate::geom::flatten_path(path, tol) {
        let points = &polyline.points;
        if points.iter().any(|p| box_of.contains(*p)) {
            return true;
        }
        let count = points.len();
        if count < 2 {
            continue;
        }
        let steps = if polyline.closed { count } else { count - 1 };
        for index in 0..steps {
            let a = points[index];
            let b = points[(index + 1) % count];
            if segment_crosses(a, b, box_of) {
                return true;
            }
        }
    }
    false
}

/// Whether the straight segment from `a` to `b` crosses a box.
fn segment_crosses(a: Point, b: Point, box_of: BBox) -> bool {
    let span = BBox::new(a, b);
    if !span.intersects(box_of) {
        return false;
    }
    let corners = [
        Point::new(box_of.min.x, box_of.min.y),
        Point::new(box_of.max.x, box_of.min.y),
        Point::new(box_of.max.x, box_of.max.y),
        Point::new(box_of.min.x, box_of.max.y),
    ];
    let tol = Tolerance::default();
    for index in 0..4 {
        let c = corners[index];
        let d = corners[(index + 1) % 4];
        let hits = crate::geom::segment_intersections(
            a,
            &Segment::Line { to: b },
            c,
            &Segment::Line { to: d },
            tol,
        );
        if !hits.is_empty() {
            return true;
        }
    }
    false
}

/// The quadrant points of an arc that the arc actually passes through.
fn quadrants(arc: &ArcGeom) -> Vec<Point> {
    let quarter = std::f64::consts::FRAC_PI_2;
    (0..4)
        .map(|i| quarter * i as f64)
        .filter(|angle| arc.contains_angle(*angle))
        .map(|angle| {
            Point::new(
                arc.centre.x + arc.radius * angle.cos(),
                arc.centre.y + arc.radius * angle.sin(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Entity, EntityKind};
    use crate::geom::{circle, rect};

    fn doc_with_a_rectangle() -> (Document, u64) {
        let mut doc = Document::new();
        doc.begin("Create");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: rect(Point::new(0.0, 0.0), Point::new(100.0, 50.0)).expect("rect"),
            },
        });
        doc.commit();
        (doc, id)
    }

    fn args(x: f64, y: f64, radius: f64) -> ProbeArgs {
        ProbeArgs {
            x,
            y,
            radius,
            snap: SnapOptions::default(),
            grid_spacing: 0.0,
            guides: Vec::new(),
            exclude: Vec::new(),
        }
    }

    #[test]
    fn a_corner_comes_back_as_a_node() {
        let (doc, id) = doc_with_a_rectangle();
        let reply = probe(&doc, &args(0.4, 0.3, 2.0));
        let best = reply.hits.first().expect("a corner is within reach");
        assert_eq!(best.kind, SnapKind::Node);
        assert!((best.x).abs() < 1e-9 && (best.y).abs() < 1e-9, "{best:?}");
        assert_eq!(best.id, Some(id));
        assert_eq!(reply.nearest_entity, Some(id));
    }

    #[test]
    fn a_midpoint_is_found_along_an_edge() {
        let (doc, _) = doc_with_a_rectangle();
        let reply = probe(&doc, &args(50.0, 0.2, 2.0));
        assert!(
            reply.hits.iter().any(|h| h.kind == SnapKind::Mid),
            "{:?}",
            reply.hits
        );
    }

    #[test]
    fn nodes_beat_on_vector_hits_at_the_same_place() {
        let (doc, _) = doc_with_a_rectangle();
        let reply = probe(&doc, &args(0.0, 0.0, 5.0));
        let kinds: Vec<SnapKind> = reply.hits.iter().map(|h| h.kind).collect();
        let node = kinds.iter().position(|k| *k == SnapKind::Node);
        let on = kinds.iter().position(|k| *k == SnapKind::On);
        assert!(node < on || on.is_none(), "{kinds:?}");
    }

    #[test]
    fn a_circle_offers_its_centre_and_quadrants() {
        let mut doc = Document::new();
        doc.begin("Create");
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: circle(Point::new(20.0, 20.0), 10.0).expect("circle"),
            },
        });
        doc.commit();
        let reply = probe(&doc, &args(20.0, 20.0, 30.0));
        assert!(reply.hits.iter().any(|h| h.kind == SnapKind::Centre));
        assert!(reply.hits.iter().any(|h| h.kind == SnapKind::Quadrant));
    }

    #[test]
    fn intersections_between_two_vectors_are_offered() {
        let mut doc = Document::new();
        doc.begin("Create");
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: rect(Point::new(0.0, 0.0), Point::new(20.0, 20.0)).expect("rect"),
            },
        });
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: rect(Point::new(10.0, 10.0), Point::new(30.0, 30.0)).expect("rect"),
            },
        });
        doc.commit();
        let reply = probe(&doc, &args(20.0, 10.0, 3.0));
        assert!(
            reply.hits.iter().any(|h| h.kind == SnapKind::Intersection),
            "{:?}",
            reply.hits
        );
    }

    #[test]
    fn the_grid_snaps_to_the_nearest_multiple() {
        let doc = Document::new();
        let mut a = args(10.4, 19.6, 5.0);
        a.grid_spacing = 10.0;
        let reply = probe(&doc, &a);
        let grid = reply
            .hits
            .iter()
            .find(|h| h.kind == SnapKind::Grid)
            .expect("the grid is on");
        assert!(
            (grid.x - 10.0).abs() < 1e-9 && (grid.y - 20.0).abs() < 1e-9,
            "{grid:?}"
        );
    }

    #[test]
    fn guides_snap_on_their_own_axis_only() {
        let doc = Document::new();
        let mut a = args(48.0, 5.0, 5.0);
        a.guides = vec![Guide {
            id: 9,
            axis: GuideAxis::X,
            position: 50.0,
            locked: false,
        }];
        let reply = probe(&doc, &a);
        let guide = reply
            .hits
            .iter()
            .find(|h| h.kind == SnapKind::Guide)
            .expect("the guide is in range");
        assert!(
            (guide.x - 50.0).abs() < 1e-9 && (guide.y - 5.0).abs() < 1e-9,
            "{guide:?}"
        );
        assert_eq!(guide.id, Some(9));
    }

    #[test]
    fn job_corners_are_offered_when_asked_for() {
        let doc = Document::new();
        let reply = probe(&doc, &args(0.5, 0.5, 3.0));
        assert!(reply.hits.iter().any(|h| h.kind == SnapKind::JobCorner));
    }

    #[test]
    fn excluded_entities_are_left_out() {
        let (doc, id) = doc_with_a_rectangle();
        let mut a = args(0.2, 0.2, 2.0);
        a.exclude = vec![id];
        let reply = probe(&doc, &a);
        assert!(
            reply.hits.iter().all(|h| h.id != Some(id)),
            "{:?}",
            reply.hits
        );
        assert!(reply.nearest_entity.is_none());
    }

    #[test]
    fn switching_a_snap_off_drops_its_hits() {
        let (doc, _) = doc_with_a_rectangle();
        let mut a = args(0.2, 0.2, 2.0);
        a.snap.nodes = false;
        let reply = probe(&doc, &a);
        assert!(reply.hits.iter().all(|h| h.kind != SnapKind::Node));
    }

    #[test]
    fn a_hidden_layer_offers_nothing() {
        let (mut doc, _) = doc_with_a_rectangle();
        doc.begin("Edit Layer");
        doc.update_layer(1, None, None, Some(false), None)
            .expect("hides");
        doc.commit();
        let reply = probe(&doc, &args(0.2, 0.2, 2.0));
        assert!(
            reply.hits.iter().all(|h| h.id.is_none()),
            "{:?}",
            reply.hits
        );
    }

    #[test]
    fn smart_snapping_projects_alignment_lines() {
        let (doc, _) = doc_with_a_rectangle();
        let mut a = args(150.0, 0.2, 5.0);
        a.snap.smart = true;
        a.snap.job_points = false;
        let reply = probe(&doc, &a);
        assert!(
            reply
                .hits
                .iter()
                .any(|h| h.kind == SnapKind::Guide && (h.y).abs() < 1e-9),
            "{:?}",
            reply.hits
        );
    }

    #[test]
    fn probe_args_parse_from_the_frontend_shape() {
        let text = r#"{"x":1,"y":2,"radius":3,"snap":{"grid":true,"guides":true,
            "nodes":true,"midpoints":false,"centres":true,"intersections":false,
            "jobPoints":true,"smart":false},"gridSpacing":5,
            "guides":[{"id":1,"axis":"x","position":10,"locked":false}],"exclude":[7]}"#;
        let args: ProbeArgs = serde_json::from_str(text).expect("the frontend shape parses");
        assert!(!args.snap.midpoints);
        assert!(args.snap.job_points);
        assert_eq!(args.grid_spacing, 5.0);
        assert_eq!(args.guides[0].axis, GuideAxis::X);
        assert_eq!(args.exclude, vec![7]);
    }
}
