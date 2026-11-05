//! Adversarial tests written during the review of brief 002.
//!
//! These target degenerate and boundary inputs rather than the happy paths
//! the implementation tests already cover: zero length segments, full circles
//! built from two arcs, self touching loops, offsets that collapse a shape,
//! booleans on shapes that share an edge, holes nested three deep, arcs put
//! through a non uniform scale, empty and multi line text, joins that need a
//! reversal, corner treatments that do not fit, and points that land exactly
//! on a vertex.

use super::*;

const FLAT: Tolerance = Tolerance::FLATTEN;
const EQ: Tolerance = Tolerance::EQUALITY;

fn fine() -> Tolerance {
    Tolerance::new(0.002).expect("tolerance")
}

fn square(size: f64) -> SubPath {
    rect(Point::ORIGIN, Point::new(size, size))
        .expect("rect")
        .subpaths[0]
        .clone()
}

/// A closed square whose first node sits halfway along the bottom edge, so
/// the last and first segments are collinear across the start node.
fn square_started_mid_edge() -> SubPath {
    SubPath::new(
        Point::new(5.0, 0.0),
        vec![
            Segment::line(Point::new(10.0, 0.0)),
            Segment::line(Point::new(10.0, 10.0)),
            Segment::line(Point::new(0.0, 10.0)),
            Segment::line(Point::new(0.0, 0.0)),
            Segment::line(Point::new(5.0, 0.0)),
        ],
        true,
    )
}

// 1. Zero length segments.

#[test]
fn a_zero_length_segment_does_not_disturb_length_area_or_offset() {
    let mut sub = SubPath::open(Point::ORIGIN);
    sub.line_to(Point::new(10.0, 0.0))
        .line_to(Point::new(10.0, 0.0)) // zero length
        .line_to(Point::new(10.0, 10.0))
        .line_to(Point::new(0.0, 10.0))
        .line_to(Point::ORIGIN);
    sub.closed = true;
    let path = sub.clone().into_path();

    assert!((path_length(&path, FLAT) - 40.0).abs() < 1e-9);
    assert!((path_area(&path, FLAT) - 100.0).abs() < 1e-9);

    let issues = validate(&path, EQ);
    assert!(issues
        .iter()
        .any(|i| i.kind == ValidationKind::ZeroLengthSegment));
    assert!(issues
        .iter()
        .any(|i| i.kind == ValidationKind::DuplicateNode));

    let out = offset_path(&path, 1.0, OffsetSide::Inward, OffsetCorner::Sharp, FLAT)
        .expect("offset of a path with a zero length segment");
    assert_eq!(out.len(), 1, "one inward loop");
    let area = path_area(&out[0], FLAT);
    assert!((area - 64.0).abs() < 1e-6, "inward offset area {area}");
}

#[test]
fn corner_tools_report_an_error_on_a_closed_subpath_with_no_segments() {
    let empty = SubPath::new(Point::ORIGIN, Vec::new(), true);
    assert!(
        fillet_corner(&empty, 0, 1.0, FilletKind::Normal).is_err(),
        "filleting a closed subpath with no segments must be an error, not a panic"
    );
    assert!(
        chamfer_corner(&empty, 0, 1.0).is_err(),
        "chamfering a closed subpath with no segments must be an error, not a panic"
    );
}

// 2. A full circle built from two arcs.

#[test]
fn a_full_circle_of_two_arcs_measures_flattens_and_refits() {
    let path = circle(Point::ORIGIN, 5.0).expect("circle");
    let r: f64 = 5.0;

    assert!((path_area(&path, FLAT) - std::f64::consts::PI * r * r).abs() < 1e-9);
    assert!((path_length(&path, FLAT) - 2.0 * std::f64::consts::PI * r).abs() < 1e-9);

    let bbox = path_bbox(&path).expect("bbox");
    assert!((bbox.min.x + r).abs() < 1e-9, "left {}", bbox.min.x);
    assert!((bbox.max.y - r).abs() < 1e-9, "top {}", bbox.max.y);

    assert!(point_in_path(&path, Point::ORIGIN, FillRule::NonZero, FLAT));
    assert!(!point_in_path(
        &path,
        Point::new(6.0, 0.0),
        FillRule::NonZero,
        FLAT
    ));
    assert!(!path_self_intersects(&path, FLAT));

    let poly = flatten_subpath(&path.subpaths[0], fine());
    assert!(poly.closed, "a closed circle flattens to a closed polyline");
    let refit = fit_arcs(&poly, fine()).expect("refit");
    let refit_area = path_area(&refit.into_path(), FLAT);
    let want = std::f64::consts::PI * r * r;
    assert!(
        (refit_area - want).abs() / want < 0.01,
        "refit area {refit_area} against {want}"
    );

    let grown =
        offset_path(&path, 2.0, OffsetSide::Outward, OffsetCorner::Round, FLAT).expect("offset");
    assert_eq!(grown.len(), 1);
    let grown_area = path_area(&grown[0], FLAT);
    let want = std::f64::consts::PI * 49.0;
    assert!(
        (grown_area - want).abs() < 1e-6,
        "grown area {grown_area} against {want}"
    );
}

// 3. A self touching polygon.

#[test]
fn two_squares_touching_at_one_corner_validate_and_weld() {
    let sub = SubPath::new(
        Point::ORIGIN,
        vec![
            Segment::line(Point::new(10.0, 0.0)),
            Segment::line(Point::new(10.0, 10.0)),
            Segment::line(Point::new(20.0, 10.0)),
            Segment::line(Point::new(20.0, 20.0)),
            Segment::line(Point::new(10.0, 20.0)),
            Segment::line(Point::new(10.0, 10.0)),
            Segment::line(Point::new(0.0, 10.0)),
            Segment::line(Point::ORIGIN),
        ],
        true,
    );
    let path = sub.into_path();
    assert!((path_area(&path, FLAT) - 200.0).abs() < 1e-9);

    let issues = validate(&path, EQ);
    assert!(
        issues
            .iter()
            .any(|i| i.kind == ValidationKind::SelfIntersecting),
        "a loop that revisits a node is a validator finding, issues {issues:?}"
    );

    let welded = weld(std::slice::from_ref(&path), fine()).expect("weld");
    let total: f64 = welded.iter().map(|p| path_area(p, FLAT)).sum();
    assert!((total - 200.0).abs() < 0.5, "welded area {total}");
    assert!(point_in_path(
        &path,
        Point::new(5.0, 5.0),
        FillRule::NonZero,
        FLAT
    ));
    assert!(point_in_path(
        &path,
        Point::new(15.0, 15.0),
        FillRule::NonZero,
        FLAT
    ));
    assert!(!point_in_path(
        &path,
        Point::new(5.0, 15.0),
        FillRule::NonZero,
        FLAT
    ));
    assert!(!point_in_path(
        &path,
        Point::new(15.0, 5.0),
        FillRule::NonZero,
        FLAT
    ));
}

// 4. An offset bigger than half the narrow dimension.

#[test]
fn an_inward_offset_past_the_half_width_drops_the_shape() {
    let path = rect(Point::ORIGIN, Point::new(20.0, 10.0)).expect("rect");

    let survives =
        offset_path(&path, 4.0, OffsetSide::Inward, OffsetCorner::Sharp, FLAT).expect("offset 4");
    assert_eq!(survives.len(), 1);
    let area = path_area(&survives[0], FLAT);
    assert!((area - 12.0 * 2.0).abs() < 1e-6, "area {area}");

    let collapsed =
        offset_path(&path, 6.0, OffsetSide::Inward, OffsetCorner::Sharp, FLAT).expect("offset 6");
    assert!(
        collapsed.is_empty(),
        "an inward offset past the half width leaves nothing, got {} loops",
        collapsed.len()
    );

    let exact =
        offset_path(&path, 5.0, OffsetSide::Inward, OffsetCorner::Sharp, FLAT).expect("offset 5");
    let exact_area: f64 = exact.iter().map(|p| path_area(p, FLAT).abs()).sum();
    assert!(
        exact_area < 1e-6,
        "an offset of exactly half the height encloses no area, got {exact_area}"
    );
}

// 5. A boolean on shapes that share an edge.

#[test]
fn welding_two_rectangles_that_share_an_edge_gives_one_shape() {
    let left = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
    let right = rect(Point::new(10.0, 0.0), Point::new(20.0, 10.0)).expect("rect");
    let out = weld(&[left.clone(), right.clone()], fine()).expect("weld");
    assert_eq!(out.len(), 1, "two rectangles sharing an edge weld into one");
    assert_eq!(out[0].subpaths.len(), 1, "no hole along the shared edge");
    let area = path_area(&out[0], FLAT);
    assert!((area - 200.0).abs() < 1e-6, "welded area {area}");
    let bbox = path_bbox(&out[0]).expect("bbox");
    assert!((bbox.width() - 20.0).abs() < 1e-6);
    assert!((bbox.height() - 10.0).abs() < 1e-6);

    let common = intersect_paths(&[left], &[right], fine()).expect("intersect");
    let shared: f64 = common.iter().map(|p| path_area(p, FLAT).abs()).sum();
    assert!(
        shared < 1e-6,
        "shapes that only share an edge have no common area, got {shared}"
    );
}

// 6. Holes nested three deep.

#[test]
fn holes_nested_three_deep_keep_their_areas() {
    let outer = rect(Point::new(-50.0, -50.0), Point::new(50.0, 50.0)).expect("rect");
    let hole = reverse(&rect(Point::new(-40.0, -40.0), Point::new(40.0, 40.0)).expect("rect"));
    let island = rect(Point::new(-30.0, -30.0), Point::new(30.0, 30.0)).expect("rect");
    let pocket = reverse(&rect(Point::new(-20.0, -20.0), Point::new(20.0, 20.0)).expect("rect"));

    let mut nested = Path::new();
    for part in [&outer, &hole, &island, &pocket] {
        nested.push(part.subpaths[0].clone());
    }
    let want = 10000.0 - 6400.0 + 3600.0 - 1600.0;
    assert!((path_area(&nested, FLAT) - want).abs() < 1e-9);

    assert!(point_in_path(
        &nested,
        Point::new(45.0, 0.0),
        FillRule::NonZero,
        FLAT
    ));
    assert!(!point_in_path(
        &nested,
        Point::new(35.0, 0.0),
        FillRule::NonZero,
        FLAT
    ));
    assert!(point_in_path(
        &nested,
        Point::new(25.0, 0.0),
        FillRule::NonZero,
        FLAT
    ));
    assert!(!point_in_path(
        &nested,
        Point::ORIGIN,
        FillRule::NonZero,
        FLAT
    ));

    let welded = weld(&[nested], fine()).expect("weld");
    let total: f64 = welded.iter().map(|p| path_area(p, FLAT)).sum();
    assert!(
        (total - want).abs() < 1.0,
        "a boolean over three levels of nesting keeps the area, got {total} against {want}"
    );
    let loops: usize = welded.iter().map(|p| p.subpaths.len()).sum();
    assert_eq!(loops, 4, "all four loops survive the boolean");
}

// 7. Arcs through a non uniform scale and back.

#[test]
fn a_non_uniform_scale_turns_arcs_into_cubics_that_scale_back() {
    let path = circle(Point::ORIGIN, 10.0).expect("circle");
    let squash = Mat3::scale(2.0, 1.0);
    let squashed = transform_path(&path, &squash);

    assert!(
        squashed.subpaths[0]
            .segments
            .iter()
            .all(|s| s.kind() == SegmentKind::Cubic),
        "a non uniform scale has to give up the arcs"
    );
    assert!(squashed.subpaths[0].closed);

    let bbox = path_bbox(&squashed).expect("bbox");
    assert!((bbox.width() - 40.0).abs() < 0.02, "width {}", bbox.width());
    assert!(
        (bbox.height() - 20.0).abs() < 0.02,
        "height {}",
        bbox.height()
    );

    let back = transform_path(&squashed, &squash.invert().expect("invert"));
    let bbox = path_bbox(&back).expect("bbox");
    assert!((bbox.width() - 20.0).abs() < 0.02, "width {}", bbox.width());
    assert!(
        (bbox.height() - 20.0).abs() < 0.02,
        "height {}",
        bbox.height()
    );
    let length = path_length(&back, fine());
    let want = 2.0 * std::f64::consts::PI * 10.0;
    assert!(
        (length - want).abs() / want < 0.001,
        "round trip length {length} against {want}"
    );
    let area = path_area(&back, FLAT);
    let want = std::f64::consts::PI * 100.0;
    assert!(
        (area - want).abs() / want < 0.001,
        "round trip area {area} against {want}"
    );
}

// 8. Text with nothing in it and text with a newline.

#[test]
fn text_copes_with_an_empty_string_and_with_a_newline() {
    let library = FontLibrary::bundled();

    let blank = TextSpec {
        text: String::new(),
        ..TextSpec::default()
    };
    let out = layout_text(&library, &blank).expect("empty text must not be an error");
    assert!(out.is_empty(), "an empty string draws nothing");
    assert!(outlines_bbox(&out).is_none());

    let spaces = TextSpec {
        text: "   ".to_string(),
        ..TextSpec::default()
    };
    assert!(layout_text(&library, &spaces).expect("spaces").is_empty());

    let one_line = TextSpec {
        text: "HI".to_string(),
        ..TextSpec::default()
    };
    let two_lines = TextSpec {
        text: "H\nI".to_string(),
        ..TextSpec::default()
    };
    let flat = layout_text(&library, &one_line).expect("one line");
    let split = layout_text(&library, &two_lines).expect("two lines");
    assert_eq!(flat.len(), split.len(), "a newline keeps every glyph");

    let flat_box = outlines_bbox(&flat).expect("bbox");
    let split_box = outlines_bbox(&split).expect("bbox");
    assert!(
        split_box.height() > flat_box.height() * 1.5,
        "the second line sits below the first, heights {} and {}",
        split_box.height(),
        flat_box.height()
    );
    assert!(
        split_box.width() < flat_box.width(),
        "the second line starts again at the left, widths {} and {}",
        split_box.width(),
        flat_box.width()
    );
    assert!(
        split_box.min.y < -one_line.size_mm * 0.5,
        "the second baseline is below y zero, got {}",
        split_box.min.y
    );

    let leading = TextSpec {
        text: "\nH".to_string(),
        ..TextSpec::default()
    };
    let dropped = layout_text(&library, &leading).expect("leading newline");
    assert_eq!(dropped.len(), 1, "a leading newline still draws the glyph");
    let dropped_box = outlines_bbox(&dropped).expect("bbox");
    assert!(
        dropped_box.max.y < 0.0,
        "a leading newline pushes the text below the baseline, got {}",
        dropped_box.max.y
    );
}

// 9. Joining two open paths that run towards each other.

#[test]
fn two_open_paths_that_meet_end_to_end_join_after_a_reversal() {
    let a = line(Point::ORIGIN, Point::new(10.0, 0.0));
    let b = line(Point::new(20.0, 0.0), Point::new(10.0, 0.0));
    let joined = join_paths(&[a, b], EQ);
    assert_eq!(joined.len(), 1, "the two runs make one vector");
    let sub = &joined[0].subpaths[0];
    assert!(!sub.closed, "the joined run is still open");
    assert_eq!(sub.segments.len(), 2);
    assert!((path_length(&joined[0], FLAT) - 20.0).abs() < 1e-9);
    assert!(sub.start.approx_eq(Point::ORIGIN, EQ));
    assert!(sub.end().approx_eq(Point::new(20.0, 0.0), EQ));

    let head_to_head = join_paths(
        &[
            line(Point::new(10.0, 0.0), Point::ORIGIN),
            line(Point::new(10.0, 0.0), Point::new(20.0, 0.0)),
        ],
        EQ,
    );
    assert_eq!(head_to_head.len(), 1, "two runs sharing a start also join");
    assert!((path_length(&head_to_head[0], FLAT) - 20.0).abs() < 1e-9);

    let apart = join_paths(
        &[
            line(Point::ORIGIN, Point::new(10.0, 0.0)),
            line(Point::new(10.5, 0.0), Point::new(20.0, 0.0)),
        ],
        EQ,
    );
    assert_eq!(apart.len(), 2, "ends outside the tolerance stay apart");
}

// 10. A corner treatment that does not fit the segments.

#[test]
fn a_fillet_radius_bigger_than_the_segment_is_refused() {
    let sub = square(10.0);
    assert!(
        fillet_corner(&sub, 1, 20.0, FilletKind::Normal).is_err(),
        "a 20 mm fillet cannot fit a 10 mm side"
    );
    assert!(
        chamfer_corner(&sub, 1, 20.0).is_err(),
        "a 20 mm chamfer cannot fit a 10 mm side"
    );

    let filleted = fillet_corner(&sub, 1, 2.0, FilletKind::Normal).expect("fillet");
    let arcs = filleted
        .segments
        .iter()
        .filter(|s| s.kind() == SegmentKind::Arc)
        .count();
    assert_eq!(arcs, 1, "one corner becomes one arc");
    let area = path_area(&filleted.into_path(), FLAT);
    let removed = 4.0 * (1.0 - std::f64::consts::FRAC_PI_4);
    assert!(
        (area - (100.0 - removed)).abs() < 1e-6,
        "filleted area {area}"
    );

    // A dog bone reaches further than the radius, so the limit is tighter.
    assert!(
        fillet_corner(&sub, 1, 11.0, FilletKind::DogBone).is_err(),
        "a dog bone bigger than the side is refused"
    );
    assert!(
        fillet_corner(&sub, 1, 6.0, FilletKind::TBone).is_err(),
        "a T bone cuts back twice the radius, so 6 mm does not fit 10 mm"
    );
}

// 11. A point exactly on a vertex.

#[test]
fn point_in_path_on_a_vertex_is_stable() {
    let path = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
    let probes = [
        Point::ORIGIN,
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
        Point::new(5.0, 0.0),
        Point::new(10.0, 5.0),
    ];
    for probe in probes {
        for rule in [FillRule::EvenOdd, FillRule::NonZero] {
            let first = point_in_path(&path, probe, rule, FLAT);
            let again = point_in_path(&path, probe, rule, FLAT);
            assert_eq!(
                first, again,
                "a boundary point must answer the same way every time at {probe:?}"
            );
        }
        assert_eq!(
            point_in_path(&path, probe, FillRule::EvenOdd, FLAT),
            point_in_path(&path, probe, FillRule::NonZero, FLAT),
            "the two fill rules agree on a simple loop at {probe:?}"
        );
    }
    // A point a whisker inside is inside under both rules.
    assert!(point_in_path(
        &path,
        Point::new(1e-9, 1e-9),
        FillRule::NonZero,
        FLAT
    ));
}

// 12. Node reduction on a closed loop.

#[test]
fn reducing_nodes_keeps_a_closed_loop_closed() {
    let sub = square_started_mid_edge();
    let before = sub.clone().into_path();
    assert!((path_area(&before, FLAT) - 100.0).abs() < 1e-9);

    let after = reduce_nodes(&before, FLAT);
    let reduced = &after.subpaths[0];
    assert!(reduced.closed, "the loop is still closed");
    assert!(
        reduced.end().approx_eq(reduced.start, EQ),
        "the last segment lands back on the start, start {:?} end {:?}",
        reduced.start,
        reduced.end()
    );
    assert_eq!(reduced.segments.len(), 4, "the split bottom edge merges");
    let area = path_area(&after, FLAT);
    assert!(
        (area - 100.0).abs() < 1e-9,
        "node reduction must not change the area, got {area}"
    );
    assert!(!path_self_intersects(&after, FLAT));
    let bbox = path_bbox(&after).expect("bbox");
    assert!((bbox.width() - 10.0).abs() < 1e-9);
    assert!((bbox.height() - 10.0).abs() < 1e-9);
}

// Timing measurements.

#[test]
#[ignore = "timing check, run with cargo test -- --ignored --nocapture"]
fn timing_offset_of_a_thousand_rectangles() {
    let paths: Vec<Path> = (0..1000)
        .map(|i| {
            let w = 10.0 + (i % 17) as f64 * 0.5;
            let h = 8.0 + (i % 11) as f64 * 0.5;
            rect(Point::ORIGIN, Point::new(w, h)).expect("rect")
        })
        .collect();
    let start = std::time::Instant::now();
    let mut loops = 0usize;
    for path in &paths {
        let out =
            offset_path(path, 1.0, OffsetSide::Outward, OffsetCorner::Round, FLAT).expect("offset");
        loops += out.len();
    }
    let elapsed = start.elapsed();
    eprintln!("offset of 1000 rectangles: {elapsed:?} for {loops} loops");
    assert_eq!(loops, 1000);
    assert!(elapsed.as_secs_f64() < 1.0, "took {elapsed:?}");
}

#[test]
#[ignore = "timing check, run with cargo test -- --ignored --nocapture"]
fn timing_union_of_two_hundred_overlapping_circles() {
    // Two hundred circles evenly spaced round a 60 mm ring, each overlapping
    // its neighbours, so the union is one annulus.
    let count = 200;
    let paths: Vec<Path> = (0..count)
        .map(|i| {
            let t = i as f64 / count as f64 * std::f64::consts::TAU;
            circle(Point::new(t.cos() * 60.0, t.sin() * 60.0), 10.0).expect("circle")
        })
        .collect();
    let start = std::time::Instant::now();
    let out = weld(&paths, FLAT).expect("weld");
    let elapsed = start.elapsed();
    let area: f64 = out.iter().map(|p| path_area(p, FLAT)).sum();
    let loops: usize = out.iter().map(|p| p.subpaths.len()).sum();
    eprintln!(
        "union of 200 overlapping circles: {elapsed:?} into {} shape(s), {loops} loop(s), area {area:.1} mm2",
        out.len()
    );
    assert_eq!(out.len(), 1, "the ring of circles unions into one shape");
    assert_eq!(loops, 2, "an outer rim and the hole in the middle");
    let rim = std::f64::consts::PI * 70.0 * 70.0;
    let hole = std::f64::consts::PI * 50.0 * 50.0;
    assert!(
        (area - (rim - hole)).abs() / (rim - hole) < 0.01,
        "annulus area {area} against {}",
        rim - hole
    );
    assert!(elapsed.as_secs_f64() < 1.0, "took {elapsed:?}");
}

// Second pass: more degenerate and boundary probes.

#[test]
fn offsetting_an_open_line_gives_a_stadium_outline() {
    let path = line(Point::ORIGIN, Point::new(10.0, 0.0));
    let out = offset_path(&path, 2.0, OffsetSide::Outward, OffsetCorner::Round, FLAT)
        .expect("offset of an open vector");
    assert_eq!(out.len(), 1, "an open vector offsets to one closed outline");
    assert!(out[0].subpaths[0].closed);
    let area = path_area(&out[0], FLAT);
    let want = 10.0 * 4.0 + std::f64::consts::PI * 4.0;
    assert!(
        (area - want).abs() < 1e-6,
        "outline area {area} against {want}"
    );
    let length = path_length(&out[0], FLAT);
    let want = 20.0 + 2.0 * std::f64::consts::PI * 2.0;
    assert!(
        (length - want).abs() < 1e-6,
        "outline length {length} against {want}"
    );
    let bbox = path_bbox(&out[0]).expect("bbox");
    assert!((bbox.min.x + 2.0).abs() < 1e-6, "left {}", bbox.min.x);
    assert!((bbox.max.x - 12.0).abs() < 1e-6, "right {}", bbox.max.x);
}

#[test]
fn an_exclusive_or_of_a_shape_with_itself_leaves_nothing() {
    let a = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
    let b = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
    let out = xor(&[a], &[b], fine()).expect("xor");
    let area: f64 = out.iter().map(|p| path_area(p, FLAT).abs()).sum();
    assert!(area < 1e-6, "xor of identical shapes leaves {area} mm2");
}

#[test]
fn subtracting_through_a_shape_that_already_has_a_hole_keeps_both() {
    let outer = rect(Point::ORIGIN, Point::new(40.0, 40.0)).expect("rect");
    let hole = rect(Point::new(5.0, 5.0), Point::new(15.0, 15.0)).expect("rect");
    let ring = subtract(&[outer], &[hole], fine()).expect("subtract");
    assert_eq!(ring.len(), 1);
    let second = rect(Point::new(25.0, 25.0), Point::new(35.0, 35.0)).expect("rect");
    let out = subtract(&ring, &[second], fine()).expect("subtract again");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].subpaths.len(), 3, "outer plus two holes");
    let area = path_area(&out[0], FLAT);
    assert!((area - (1600.0 - 100.0 - 100.0)).abs() < 1.0, "area {area}");
}

#[test]
fn cropping_keeps_the_piece_of_an_open_vector_inside_the_boundary() {
    let boundary = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
    let run = line(Point::new(-5.0, 5.0), Point::new(15.0, 5.0));
    let out = crop(&[run], &[boundary], fine()).expect("crop");
    let kept: f64 = out.iter().map(|p| path_length(p, FLAT)).sum();
    assert!(
        (kept - 10.0).abs() < 0.05,
        "the 10 mm inside the boundary is kept, got {kept}"
    );
}

#[test]
fn text_stands_within_five_percent_of_its_nominal_cap_height() {
    let library = FontLibrary::bundled();
    let font = library
        .load(FontLibrary::FALLBACK_FAMILY, false, false)
        .expect("font");
    let nominal = cap_height(&font, 20.0).expect("cap height");
    let spec = TextSpec {
        text: "HELLO".to_string(),
        size_mm: 20.0,
        ..TextSpec::default()
    };
    let out = layout_text(&library, &spec).expect("layout");
    assert_eq!(out.len(), 5, "five glyphs");
    let bbox = outlines_bbox(&out).expect("bbox");
    assert!(
        (bbox.height() - nominal).abs() / nominal < 0.05,
        "drawn height {} against nominal cap height {nominal}",
        bbox.height()
    );
    // Round letters overshoot the baseline a little, which is normal, but no
    // capital should sit meaningfully below it.
    assert!(
        bbox.min.y > -spec.size_mm * 0.02,
        "capitals sit on the baseline, min y {}",
        bbox.min.y
    );
}

#[test]
fn layout_is_the_same_every_time_for_the_bundled_face() {
    let library = FontLibrary::bundled();
    let spec = TextSpec {
        text: "Carve 123".to_string(),
        size_mm: 12.0,
        ..TextSpec::default()
    };
    let a = layout_text(&library, &spec).expect("layout");
    let b = layout_text(&library, &spec).expect("layout");
    assert_eq!(a, b, "text layout has to be deterministic");
}

#[test]
fn an_ellipse_from_arcs_has_the_right_bounding_box_and_area() {
    let path = ellipse(Point::ORIGIN, 20.0, 5.0, EllipseForm::Cubic).expect("ellipse");
    let bbox = path_bbox(&path).expect("bbox");
    assert!((bbox.width() - 40.0).abs() < 0.02, "width {}", bbox.width());
    assert!(
        (bbox.height() - 10.0).abs() < 0.02,
        "height {}",
        bbox.height()
    );
    let area = path_area(&path, fine());
    let want = std::f64::consts::PI * 20.0 * 5.0;
    assert!(
        (area - want).abs() / want < 0.005,
        "ellipse area {area} against {want}"
    );
}

#[test]
fn a_degenerate_two_point_polyline_simplifies_without_losing_its_ends() {
    let poly = Polyline::new(vec![Point::ORIGIN, Point::new(1.0, 0.0)], false);
    let out = simplify_polyline(&poly, FLAT);
    assert_eq!(out.points.len(), 2);
    let single = Polyline::new(vec![Point::ORIGIN], false);
    assert_eq!(simplify_polyline(&single, FLAT).points.len(), 1);
    let none = Polyline::new(Vec::new(), true);
    assert!(simplify_polyline(&none, FLAT).points.is_empty());
    assert!(fit_arcs(&none, FLAT).is_none());
}

#[test]
fn nearest_point_on_a_full_circle_lands_on_the_rim() {
    let path = circle(Point::ORIGIN, 5.0).expect("circle");
    let near = nearest_point_on_path(&path, Point::new(0.0, 100.0), FLAT).expect("nearest");
    assert!(
        near.point
            .approx_eq(Point::new(0.0, 5.0), Tolerance::new(1e-6).expect("tol")),
        "nearest point {:?}",
        near.point
    );
    assert!(
        (near.distance - 95.0).abs() < 1e-6,
        "distance {}",
        near.distance
    );
    let centre = nearest_point_on_path(&path, Point::ORIGIN, FLAT).expect("nearest");
    assert!(
        (centre.distance - 5.0).abs() < 1e-6,
        "distance {}",
        centre.distance
    );
}

#[test]
fn an_inward_offset_of_a_shape_with_a_hole_eats_into_both_loops() {
    let outer = rect(Point::ORIGIN, Point::new(40.0, 40.0)).expect("rect");
    let hole = reverse(&rect(Point::new(10.0, 10.0), Point::new(30.0, 30.0)).expect("rect"));
    let mut shape = Path::new();
    shape.push(outer.subpaths[0].clone());
    shape.push(hole.subpaths[0].clone());
    assert!((path_area(&shape, FLAT) - (1600.0 - 400.0)).abs() < 1e-9);

    let out = offset_path(&shape, 2.0, OffsetSide::Inward, OffsetCorner::Sharp, FLAT)
        .expect("inward offset");
    assert_eq!(out.len(), 2, "the outer loop and the hole both come back");
    let mut areas: Vec<f64> = out.iter().map(|p| path_area(p, FLAT)).collect();
    areas.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    assert!(
        (areas[1] - 36.0 * 36.0).abs() < 1e-6,
        "the outer loop shrinks to {}",
        areas[1]
    );
    assert!(
        (areas[0] + 24.0 * 24.0).abs() < 1e-6,
        "the hole grows and stays wound the other way, got {}",
        areas[0]
    );
    let total: f64 = areas.iter().sum();
    assert!(
        (total - (1296.0 - 576.0)).abs() < 1e-6,
        "material left {total}"
    );
}

#[test]
fn offsetting_both_ways_returns_the_inward_loop_first() {
    let path = rect(Point::ORIGIN, Point::new(20.0, 20.0)).expect("rect");
    let out =
        offset_path(&path, 2.0, OffsetSide::Both, OffsetCorner::Sharp, FLAT).expect("offset both");
    assert_eq!(out.len(), 2);
    let inward = path_area(&out[0], FLAT);
    let outward = path_area(&out[1], FLAT);
    assert!((inward - 16.0 * 16.0).abs() < 1e-6, "inward area {inward}");
    assert!(
        (outward - 24.0 * 24.0).abs() < 1e-6,
        "outward area {outward}"
    );
}

#[test]
fn boolean_and_offset_refuse_input_they_cannot_work_with() {
    assert!(weld(&[], FLAT).is_err(), "nothing to weld");
    assert!(intersect_paths(&[], &[], FLAT).is_err());
    let path = rect(Point::ORIGIN, Point::new(10.0, 10.0)).expect("rect");
    assert!(offset_path(&path, 0.0, OffsetSide::Inward, OffsetCorner::Round, FLAT).is_err());
    assert!(offset_path(&path, -1.0, OffsetSide::Inward, OffsetCorner::Round, FLAT).is_err());
    assert!(offset_path(
        &path,
        f64::NAN,
        OffsetSide::Inward,
        OffsetCorner::Round,
        FLAT
    )
    .is_err());
    assert!(Tolerance::new(0.0).is_err());
    assert!(Tolerance::new(f64::INFINITY).is_err());
    assert!(Mat3::scale(0.0, 1.0).invert().is_err());
}

#[test]
fn the_path_model_survives_a_json_round_trip() {
    let mut sub = SubPath::open(Point::new(-1.5, 2.25));
    sub.line_to(Point::new(3.0, 2.25))
        .arc_to(Point::new(3.0, 8.25), 1.0)
        .cubic_to(
            Point::new(1.0, 9.0),
            Point::new(-1.0, 9.0),
            Point::new(-1.5, 2.25),
        );
    sub.closed = true;
    let path = sub.into_path();
    let text = serde_json::to_string(&path).expect("encode");
    let back: Path = serde_json::from_str(&text).expect("decode");
    assert_eq!(path, back);

    let bbox = path_bbox(&path).expect("bbox");
    let back: BBox =
        serde_json::from_str(&serde_json::to_string(&bbox).expect("encode")).expect("decode");
    assert_eq!(bbox, back);

    let m = Mat3::rotate_about(0.7, Point::new(2.0, 3.0));
    let back: Mat3 =
        serde_json::from_str(&serde_json::to_string(&m).expect("encode")).expect("decode");
    assert_eq!(m, back);

    let tol = Tolerance::new(0.05).expect("tolerance");
    let back: Tolerance =
        serde_json::from_str(&serde_json::to_string(&tol).expect("encode")).expect("decode");
    assert_eq!(tol, back);
}

#[test]
fn a_tolerance_that_arrives_as_json_is_checked() {
    let good: Tolerance = serde_json::from_str("0.05").expect("a good tolerance decodes");
    assert!((good.get() - 0.05).abs() < 1e-12);
    assert!(
        serde_json::from_str::<Tolerance>("0.0").is_err(),
        "a zero tolerance from JSON must be refused"
    );
    assert!(
        serde_json::from_str::<Tolerance>("-5.0").is_err(),
        "a negative tolerance from JSON must be refused"
    );
    assert!(
        serde_json::from_str::<Tolerance>("null").is_err(),
        "a tolerance that is not a number must be refused"
    );
}
