//! Regression tests for the findings of review 033 on the geometry kernel.

use super::*;

fn fine() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

fn reversed(path: &Path) -> Path {
    Path::from_subpaths(path.subpaths.iter().map(edit::reverse_subpath).collect())
}

fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Path {
    rect(Point::new(x0, y0), Point::new(x1, y1)).expect("rect")
}

fn bbox_size(paths: &[Path]) -> (f64, f64) {
    let mut all = Path::new();
    for p in paths {
        all.extend(p.clone());
    }
    let b = path_bbox(&all).expect("bbox");
    (b.max.x - b.min.x, b.max.y - b.min.y)
}

fn net_area(paths: &[Path]) -> f64 {
    paths
        .iter()
        .flat_map(|p| p.subpaths.iter())
        .map(metrics::subpath_signed_area)
        .sum()
}

// Row 1: a clockwise outline offsets outward and inward the right way.
#[test]
fn a_clockwise_rectangle_grows_when_offset_outward() {
    let cw = reversed(&square(0.0, 0.0, 10.0, 20.0));
    let out =
        offset_path(&cw, 2.0, OffsetSide::Outward, OffsetCorner::Sharp, fine()).expect("offset");
    let (w, h) = bbox_size(&out);
    assert!(
        (w - 14.0).abs() < 1e-6 && (h - 24.0).abs() < 1e-6,
        "{w} x {h}"
    );
    let inn =
        offset_path(&cw, 2.0, OffsetSide::Inward, OffsetCorner::Sharp, fine()).expect("offset");
    let (w, h) = bbox_size(&inn);
    assert!(
        (w - 6.0).abs() < 1e-6 && (h - 16.0).abs() < 1e-6,
        "{w} x {h}"
    );
}

// Row 1: an SVG rectangle (clockwise once the Y axis is flipped) grows.
#[test]
fn an_imported_svg_rectangle_grows_when_offset_outward() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100"><rect width="10" height="20"/></svg>"#;
    let (paths, _) = crate::io::svg::import(svg, 1.0).expect("svg");
    let out = offset_path(
        &paths[0].path,
        2.0,
        OffsetSide::Outward,
        OffsetCorner::Sharp,
        fine(),
    )
    .expect("offset");
    let (w, h) = bbox_size(&out);
    assert!(
        (w - 14.0).abs() < 1e-6 && (h - 24.0).abs() < 1e-6,
        "{w} x {h}"
    );
}

// Row 1: a donut with both loops drawn the same way keeps its hole in a weld.
#[test]
fn a_donut_drawn_one_way_keeps_its_hole_when_welded() {
    let mut donut = square(0.0, 0.0, 80.0, 80.0);
    donut.extend(square(20.0, 20.0, 60.0, 60.0));
    let bar = square(70.0, 0.0, 90.0, 10.0);
    // 6400 - 1600 + 200 - 100 overlap.
    for d in [donut.clone(), reversed(&donut)] {
        let out = weld(&[d.clone(), bar.clone()], fine()).expect("weld");
        assert!((net_area(&out) - 4900.0).abs() < 1e-6, "{}", net_area(&out));
        let out = subtract(std::slice::from_ref(&d), std::slice::from_ref(&bar), fine())
            .expect("subtract");
        assert!((net_area(&out) - 4700.0).abs() < 1e-6, "{}", net_area(&out));
    }
}

// Row 7: a ring shrunk past half its wall leaves nothing.
#[test]
fn a_ring_shrunk_past_half_its_wall_is_gone() {
    let mut ring = square(0.0, 0.0, 40.0, 40.0);
    ring.extend(reversed(&square(10.0, 10.0, 30.0, 30.0)));
    let out =
        offset_path(&ring, 6.0, OffsetSide::Inward, OffsetCorner::Round, fine()).expect("offset");
    assert!(
        out.is_empty(),
        "{} loops, net {}",
        out.len(),
        net_area(&out)
    );
    // Just inside half the wall the ring stays: 2 mm wide, 14 to 26 hole.
    let out =
        offset_path(&ring, 4.0, OffsetSide::Inward, OffsetCorner::Sharp, fine()).expect("offset");
    assert!(
        (net_area(&out) - (32.0 * 32.0 - 28.0 * 28.0)).abs() < 1e-6,
        "{}",
        net_area(&out)
    );
}

// Row 9: a cutter through a node cuts there.
#[test]
fn a_trim_through_a_node_cuts_at_the_node() {
    let square = SubPath::new(
        Point::new(5.0, 0.0),
        vec![
            Segment::line(Point::new(10.0, 0.0)),
            Segment::line(Point::new(10.0, 10.0)),
            Segment::line(Point::new(0.0, 10.0)),
            Segment::line(Point::new(0.0, 0.0)),
            Segment::line(Point::new(5.0, 0.0)),
        ],
        true,
    );
    let mut cut = SubPath::open(Point::new(5.0, -5.0));
    cut.line_to(Point::new(5.0, 15.0));
    let cutter = cut.into_path();
    let left = trim_to_intersection(&square, &cutter, Point::new(8.0, 5.0), fine()).expect("trim");
    assert_eq!(left.len(), 1, "{left:?}");
    let piece = left[0].clone().into_path();
    let b = path_bbox(&piece).expect("bbox");
    assert!(
        b.min.x.abs() < 1e-9 && (b.max.x - 5.0).abs() < 1e-9,
        "{b:?}"
    );
    assert!((path_length(&piece, fine()) - 20.0).abs() < 1e-9);
    // A cut line snapped to a corner of a square that starts at a corner.
    let corner = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0))
        .expect("rect")
        .subpaths[0]
        .clone();
    let mut diagonal = SubPath::open(Point::new(-1.0, -1.0));
    diagonal.line_to(Point::new(11.0, 11.0));
    let rest = trim_to_intersection(&corner, &diagonal.into_path(), Point::new(9.0, 1.0), fine())
        .expect("trim");
    assert_eq!(rest.len(), 1, "{rest:?}");
    let piece = rest[0].clone().into_path();
    assert!((path_length(&piece, fine()) - 20.0).abs() < 1e-9);
    let b = path_bbox(&piece).expect("bbox");
    assert!(
        (b.max.y - 10.0).abs() < 1e-9 && b.min.x.abs() < 1e-9,
        "{b:?}"
    );
    assert!(geom_point_on(&piece, Point::new(0.0, 5.0)));
    assert!(!geom_point_on(&piece, Point::new(10.0, 5.0)));
}

fn geom_point_on(path: &Path, p: Point) -> bool {
    contain::point_on_path(path, p, 1e-6, fine())
}

// Row 10: an open vector whose ends meet outlines into a ring.
#[test]
fn an_open_vector_whose_ends_meet_outlines_into_a_ring() {
    let mut open = square(0.0, 0.0, 20.0, 20.0).subpaths[0].clone();
    open.closed = false;
    if open.start.distance(open.end()) > 1e-12 {
        open.line_to(open.start);
    }
    let out = offset_path(
        &open.into_path(),
        2.0,
        OffsetSide::Outward,
        OffsetCorner::Sharp,
        fine(),
    )
    .expect("outline");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].subpaths.len(), 2, "outer loop and hole");
    // Round outside corners: 24 x 24 less the corner pieces, less the
    // 16 x 16 hole.
    let corners = 4.0 * 4.0 - std::f64::consts::PI * 4.0;
    let want = 24.0 * 24.0 - corners - 16.0 * 16.0;
    assert!(
        (net_area(&out) - want).abs() < 1e-3,
        "{} want {want}",
        net_area(&out)
    );
    for sub in &out[0].subpaths {
        assert!(!intersect::subpath_self_intersects(sub, fine()));
    }
}
