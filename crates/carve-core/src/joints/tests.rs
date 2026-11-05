//! Joint Creator tests through the commands: geometry of both halves, exact
//! restore on remove, settings that remake every joint, and the clearance
//! per side measured by Verify Fit on the toolpaths' exact solids.

use serde_json::{json, Value};

use super::geometry::{area2, inside_rings, path_rings, P2};
use crate::assembly::pieces::tests::add as add_toolpath;
use crate::assembly::pieces::PartsCache;
use crate::assembly::pieces_cmd;
use crate::cam::compute::{calculate, Request, Strategy};
use crate::cam::tool::{Tool, ToolGeometry};
use crate::doc::{Document, Entity, EntityKind, JobSetup};
use crate::geom::{Path, Point, SubPath};
use crate::kernel::KernelReply;

pub(crate) fn job(width: f64, height: f64, thickness: f64) -> Document {
    let mut doc = Document::new();
    doc.begin("job");
    doc.set_job(JobSetup {
        width,
        height,
        thickness,
        ..JobSetup::default()
    })
    .expect("job");
    doc.commit();
    doc
}

pub(crate) fn run(doc: &mut Document, name: &str, args: Value) -> Value {
    try_run(doc, name, args).unwrap_or_else(|e| panic!("{name}: {e}"))
}

pub(crate) fn try_run(doc: &mut Document, name: &str, args: Value) -> Result<Value, String> {
    let mut cache = PartsCache::default();
    let reply = if name.starts_with("joints.") {
        super::commands::dispatch(doc, &mut cache, name, &args.to_string())
    } else {
        pieces_cmd::dispatch(doc, &mut cache, name, &args.to_string())
    };
    match reply.map_err(|e| e.to_string())? {
        KernelReply::Json(text) => {
            Ok(serde_json::from_str::<Value>(&text).expect("json")["result"].clone())
        }
        KernelReply::Bytes(bytes) => Ok(json!({ "bytes": bytes.len() })),
    }
}

/// A closed polygon vector on `sheet`.
pub(crate) fn polygon(doc: &mut Document, sheet: u64, points: &[P2]) -> u64 {
    let mut sub = SubPath::open(Point::new(points[0][0], points[0][1]));
    for p in &points[1..] {
        sub.line_to(Point::new(p[0], p[1]));
    }
    sub.line_to(Point::new(points[0][0], points[0][1]));
    sub.closed = true;
    let layer = doc.active_layer();
    doc.begin("vector");
    let id = doc.insert_entity(Entity {
        rotation: 0.0,
        id: 0,
        layer,
        sheet,
        revision: 0,
        name: None,
        parent: None,
        kind: EntityKind::Path {
            path: Path {
                subpaths: vec![sub],
            },
        },
    });
    doc.commit();
    id
}

pub(crate) fn rect(doc: &mut Document, sheet: u64, x0: f64, y0: f64, x1: f64, y1: f64) -> u64 {
    polygon(doc, sheet, &[[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
}

pub(crate) fn path(doc: &Document, id: u64) -> Path {
    match &doc.entity(id).expect("entity").kind {
        EntityKind::Path { path } => path.clone(),
        _ => panic!("not a path"),
    }
}

pub(crate) fn rings(doc: &Document, id: u64) -> Vec<Vec<P2>> {
    path_rings(&path(doc, id))
}

pub(crate) fn area(doc: &Document, id: u64) -> f64 {
    area2(&rings(doc, id)) / 2.0
}

pub(crate) fn edge(vector: u64, from: P2, to: P2) -> Value {
    json!({ "vector": vector, "from": from, "to": to })
}

pub(crate) fn machine(doc: &mut Document, id: u64, strategy: Strategy, params: Value) {
    let sheet = doc.entity(id).expect("entity").sheet;
    let tool = Tool {
        name: "1/4 in".into(),
        tool_type: ToolGeometry::EndMill { diameter: 6.35 },
        ..Tool::default()
    };
    let request = Request {
        strategy,
        name: None,
        tool,
        params,
        ids: vec![id],
    };
    let (mut toolpath, _) = calculate(doc, &request).expect("calculates");
    toolpath.sheet = sheet;
    add_toolpath(doc, toolpath);
}

/// The 6.35 mm end mill the joints are cut with, as the panel sends it.
pub(crate) fn end_mill() -> Value {
    serde_json::to_value(Tool {
        name: "1/4 in".into(),
        tool_type: ToolGeometry::EndMill { diameter: 6.35 },
        ..Tool::default()
    })
    .expect("tool")
}

pub(crate) fn cut_out(doc: &mut Document, id: u64, depth: f64) {
    machine(
        doc,
        id,
        Strategy::Profile,
        json!({ "side": "outside", "cutDepth": depth }),
    );
}

pub(crate) fn number(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

/// Verify Fit's joints.
pub(crate) fn verify(doc: &mut Document, target: f64, tolerance: f64) -> Vec<Value> {
    let reply = run(
        doc,
        "assembly.verifyFit",
        json!({ "target": target, "tolerance": tolerance }),
    );
    reply["joints"].as_array().cloned().unwrap_or_default()
}

/// The fit sides of every joint (walls facing a wall), smallest and
/// largest gaps.
pub(crate) fn fit_gaps(joints: &[Value]) -> Vec<(f64, f64)> {
    joints
        .iter()
        .flat_map(|j| j["sides"].as_array().cloned().unwrap_or_default())
        .filter(|s| s["fit"] == true)
        .map(|s| (number(&s["min"]), number(&s["max"])))
        .collect()
}

/// Two 100 x 50 panels on two sheets, 12 mm thick.
fn two_panels() -> (Document, u64, u64) {
    let mut doc = job(300.0, 200.0, 12.0);
    let first = doc.active_sheet();
    doc.begin("sheet");
    let second = doc.add_sheet(Some("Second".into()));
    doc.commit();
    let a = rect(&mut doc, first, 10.0, 10.0, 110.0, 60.0);
    let b = rect(&mut doc, second, 10.0, 100.0, 110.0, 150.0);
    (doc, a, b)
}

fn finger_spec(a: u64, b: u64) -> Value {
    json!({
        "kind": "finger",
        // A's top edge and B's bottom edge, each with its part on the left.
        "a": edge(a, [110.0, 60.0], [10.0, 60.0]),
        "b": { "kind": "edge", "vector": b, "from": [10.0, 100.0], "to": [110.0, 100.0] },
        "params": {},
    })
}

#[test]
fn a_finger_joint_notches_both_edges_in_turn_with_dog_bones() {
    let (mut doc, a, b) = two_panels();
    let before_a = area(&doc, a);
    let before_b = area(&doc, b);
    let reply = run(&mut doc, "joints.add", finger_spec(a, b));
    let outcome = &reply["outcome"];
    // 100 mm edge, 12 mm stock: five fingers of 20 mm.
    assert_eq!(outcome["resolved"]["count"], 5);
    // A keeps fingers 1, 3 and 5 (its corners) and loses 2 and 4: two
    // notches 20 + 0.1 mm wide, 12 deep, and four dog-bones.
    let lost_a = before_a - area(&doc, a);
    assert!(
        lost_a > 2.0 * 20.1 * 12.0 && lost_a < 2.0 * 20.1 * 12.0 + 4.0 * 40.0,
        "{lost_a}"
    );
    // B loses the three A fingers sit in, the end ones run out past its
    // corners: 2 x 20.05 + 20.1 wide.
    let lost_b = before_b - area(&doc, b);
    let bare = (2.0 * 20.05 + 20.1) * 12.0;
    assert!(lost_b > bare && lost_b < bare + 4.0 * 40.0, "{lost_b}");
    // A's notch floor lies 12 mm in from the edge; its corners are
    // relieved, so a point just inside the notch wall is gone.
    let ra = rings(&doc, a);
    assert!(!inside_rings(&ra, [50.0, 55.0]), "notch 2 is cut");
    assert!(inside_rings(&ra, [20.0, 55.0]), "finger 1 stays");
    assert!(
        !inside_rings(&ra, [29.9, 49.0]),
        "the corner at the notch floor is relieved"
    );
    // B: the corners are A's, so B's ends are notched.
    let rb = rings(&doc, b);
    assert!(!inside_rings(&rb, [15.0, 105.0]));
    assert!(inside_rings(&rb, [40.0, 105.0]));
}

#[test]
fn removing_a_joint_restores_the_edges_exactly_and_undo_brings_it_back() {
    let (mut doc, a, b) = two_panels();
    let (pa, pb) = (path(&doc, a), path(&doc, b));
    let id = run(&mut doc, "joints.add", finger_spec(a, b))["id"].clone();
    let (ja, jb) = (path(&doc, a), path(&doc, b));
    assert_ne!(ja, pa);
    run(&mut doc, "joints.remove", json!({ "id": id }));
    assert_eq!(path(&doc, a), pa);
    assert_eq!(path(&doc, b), pb);
    assert!(doc.assembly().joint_set.joints.is_empty());
    assert!(doc.assembly().joint_set.originals.is_empty());
    doc.undo();
    assert_eq!(path(&doc, a), ja);
    assert_eq!(path(&doc, b), jb);
    doc.undo();
    assert_eq!(path(&doc, a), pa);
}

#[test]
fn changing_the_fit_remakes_every_joint_in_one_step() {
    let mut doc = job(400.0, 300.0, 12.0);
    let sheet = doc.active_sheet();
    let a = rect(&mut doc, sheet, 10.0, 10.0, 110.0, 60.0);
    let b = rect(&mut doc, sheet, 10.0, 100.0, 110.0, 150.0);
    let c = rect(&mut doc, sheet, 200.0, 10.0, 250.0, 60.0);
    run(&mut doc, "joints.add", finger_spec(a, b));
    // A's right edge and C's left edge, both 50 mm.
    run(
        &mut doc,
        "joints.add",
        json!({
            "kind": "finger",
            "a": edge(a, [110.0, 10.0], [110.0, 60.0]),
            "b": { "kind": "edge", "vector": c, "from": [200.0, 60.0], "to": [200.0, 10.0] },
        }),
    );
    let press = [area(&doc, a), area(&doc, b), area(&doc, c)];
    let depth = doc.journal().undo_depth();
    let mut settings = doc.assembly().joint_set.settings.clone();
    settings.fit = super::Fit::KnockDown;
    run(
        &mut doc,
        "joints.setSettings",
        json!({ "settings": settings }),
    );
    assert_eq!(doc.journal().undo_depth(), depth + 1);
    let loose = [area(&doc, a), area(&doc, b), area(&doc, c)];
    for (p, l) in press.iter().zip(loose) {
        assert!(
            l < *p - 0.5,
            "every part lost more to the looser fit: {p} {l}"
        );
    }
    let list = run(&mut doc, "joints.list", json!({}));
    assert_eq!(list["clearance"], 0.25);
    assert_eq!(list["joints"].as_array().map(Vec::len), Some(2));
}

#[test]
fn wrong_picks_say_what_to_change() {
    let (mut doc, a, b) = two_panels();
    let short = json!({
        "kind": "finger",
        "a": edge(a, [110.0, 60.0], [10.0, 60.0]),
        "b": { "kind": "edge", "vector": b, "from": [110.0, 100.0], "to": [110.0, 150.0] },
    });
    let error = try_run(&mut doc, "joints.add", short).expect_err("refused");
    assert!(error.contains("pick edges of the same length"), "{error}");
    let mut tiny = finger_spec(a, b);
    tiny["params"] = json!({ "count": 25 });
    let error = try_run(&mut doc, "joints.add", tiny).expect_err("refused");
    assert!(error.contains("narrower than the tool"), "{error}");
    let same = json!({
        "kind": "finger",
        "a": edge(a, [110.0, 60.0], [10.0, 60.0]),
        "b": { "kind": "edge", "vector": a, "from": [10.0, 10.0], "to": [110.0, 10.0] },
    });
    let error = try_run(&mut doc, "joints.add", same).expect_err("refused");
    assert!(error.contains("same part"), "{error}");
    // Nothing was left half made.
    assert!(doc.assembly().joint_set.joints.is_empty());
}

#[test]
fn picking_finds_the_edge_with_its_part_on_the_left() {
    let mut doc = job(300.0, 200.0, 12.0);
    let sheet = doc.active_sheet();
    let a = rect(&mut doc, sheet, 10.0, 10.0, 110.0, 60.0);
    let reply = run(
        &mut doc,
        "joints.pickEdge",
        json!({ "x": 50.0, "y": 60.4, "tolerance": 1.0 }),
    );
    assert_eq!(reply["edge"]["vector"], a);
    assert_eq!(reply["edge"]["from"], json!([110.0, 60.0]));
    assert_eq!(reply["edge"]["to"], json!([10.0, 60.0]));
    let inside = run(
        &mut doc,
        "joints.pickEdge",
        json!({ "x": 50.0, "y": 30.0, "tolerance": 1.0 }),
    );
    assert!(inside["edge"].is_null());
    assert!(inside["reason"]
        .as_str()
        .is_some_and(|r| r.contains("not inside")));
    let face = run(&mut doc, "joints.pickFace", json!({ "x": 50.0, "y": 25.0 }));
    assert!((number(&face["face"]["offset"]) - 15.0).abs() < 1e-9);
    // A click goes to the nearest half millimetre, from the edge and along
    // it, so two parts drawn alike can be picked at the same place.
    let near = run(
        &mut doc,
        "joints.pickFace",
        json!({ "x": 49.79, "y": 25.13 }),
    );
    assert_eq!(near["face"]["offset"], json!(15.0), "{near}");
    assert_eq!(near["face"]["along"], json!(40.0), "{near}");
    let quarter = run(&mut doc, "joints.pickFace", json!({ "x": 50.3, "y": 24.7 }));
    assert_eq!(quarter["face"]["offset"], json!(14.5), "{quarter}");
    assert_eq!(quarter["face"]["along"], json!(40.5), "{quarter}");
}

#[test]
fn a_finger_joint_cut_out_fits_with_the_clearance_on_each_side() {
    let (mut doc, a, b) = two_panels();
    let id = run(&mut doc, "joints.add", finger_spec(a, b))["id"].clone();
    let made = run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    // One outline toolpath per sheet.
    assert_eq!(made["ids"].as_array().map(Vec::len), Some(2), "{made}");
    run(&mut doc, "joints.assemble", json!({ "id": id }));
    let joints = verify(&mut doc, 0.1, 0.05);
    // One joint per contact region, every one exact and clear.
    assert!(!joints.is_empty());
    for joint in &joints {
        assert_eq!(joint["source"], "exact", "{joint}");
        assert_eq!(
            joint["overlapDepth"], 0.0,
            "the dog-bones clear the corners: {joint}"
        );
        assert_ne!(joint["status"], "interference", "{joint}");
    }
    assert_fit(&joints, 0.1);
}

#[test]
fn a_finger_joint_with_dog_bones_reads_ok_at_its_own_clearance() {
    // As the panel makes it by default: 18 mm stock, a 3 mm end mill,
    // dog-bones, Press (0.1 a side). Verify Fit at 0.1 +/- 0.02 calls every
    // joint OK: the relief's facets belong to the fit they face, and a gap
    // cut at exactly the target limit is not tight.
    let mut doc = job(300.0, 200.0, 18.0);
    let sheet = doc.active_sheet();
    let a = rect(&mut doc, sheet, 10.0, 10.0, 150.0, 90.0);
    let b = rect(&mut doc, sheet, 10.0, 110.0, 150.0, 190.0);
    let spec = json!({
        "kind": "finger",
        "a": edge(a, [150.0, 90.0], [10.0, 90.0]),
        "b": { "kind": "edge", "vector": b, "from": [10.0, 110.0], "to": [150.0, 110.0] },
        "params": {},
    });
    let id = run(&mut doc, "joints.add", spec)["id"].clone();
    let small = serde_json::to_value(Tool {
        name: "3 mm".into(),
        tool_type: ToolGeometry::EndMill { diameter: 3.0 },
        ..Tool::default()
    })
    .expect("tool");
    run(&mut doc, "joints.makeToolpaths", json!({ "tool": small }));
    run(&mut doc, "joints.assemble", json!({ "id": id }));
    let joints = verify(&mut doc, 0.1, 0.02);
    assert!(!joints.is_empty());
    for joint in &joints {
        assert_eq!(joint["status"], "ok", "{joint}");
    }
    let edge_only = verify(&mut doc, 0.2, 0.1);
    assert!(
        edge_only.iter().all(|j| j["status"] == "ok"),
        "0.1 is inside 0.2 +/- 0.1: {edge_only:?}"
    );
}

/// Every wall facing a wall is `clearance` away, to 0.01 mm.
pub(crate) fn assert_fit(joints: &[Value], clearance: f64) {
    let gaps = fit_gaps(joints);
    assert!(!gaps.is_empty(), "no fit sides: {joints:?}");
    for (min, max) in gaps {
        assert!(
            (min - clearance).abs() < 0.01 && (max - clearance).abs() < 0.01,
            "{min} {max} against {clearance}"
        );
    }
}

#[test]
fn without_relief_square_fingers_run_into_the_round_notch_corners() {
    let (mut doc, a, b) = two_panels();
    let mut settings = doc.assembly().joint_set.settings.clone();
    settings.relief = super::Relief::None;
    run(
        &mut doc,
        "joints.setSettings",
        json!({ "settings": settings }),
    );
    let id = run(&mut doc, "joints.add", finger_spec(a, b))["id"].clone();
    cut_out(&mut doc, a, 12.5);
    cut_out(&mut doc, b, 12.5);
    run(&mut doc, "joints.assemble", json!({ "id": id }));
    let joints = verify(&mut doc, 0.1, 0.05);
    assert!(
        joints.iter().any(|j| j["status"] == "interference"),
        "{joints:?}"
    );
}

#[test]
fn a_box_pairs_its_panels_edges_and_each_corner_has_one_owner() {
    let mut doc = job(600.0, 400.0, 12.0);
    let sheet = doc.active_sheet();
    // An open box 200 x 100 x 80 laid out flat: bottom, front, back, two
    // ends.
    let bottom = rect(&mut doc, sheet, 100.0, 100.0, 300.0, 200.0);
    let front = rect(&mut doc, sheet, 100.0, 10.0, 300.0, 90.0);
    let back = rect(&mut doc, sheet, 100.0, 210.0, 300.0, 290.0);
    let left = rect(&mut doc, sheet, 10.0, 100.0, 90.0, 200.0);
    let right = rect(&mut doc, sheet, 310.0, 100.0, 390.0, 200.0);
    let ids = [bottom, front, back, left, right];
    let reply = run(&mut doc, "joints.box", json!({ "ids": ids }));
    // Bottom to each of four sides, and each end to front and back.
    assert_eq!(reply["ids"].as_array().map(Vec::len), Some(8), "{reply}");
    let set = &doc.assembly().joint_set;
    let pair = |x: u64, y: u64| {
        set.joints
            .iter()
            .filter(|j| {
                (j.a.vector == x && j.b.vector() == y) || (j.a.vector == y && j.b.vector() == x)
            })
            .count()
    };
    for side in [front, back, left, right] {
        assert_eq!(pair(bottom, side), 1);
    }
    for end in [left, right] {
        assert_eq!(pair(end, front), 1);
        assert_eq!(pair(end, back), 1);
    }
    assert_eq!(pair(front, back), 0);
    // The bottom's matched edges are the ones next to each panel.
    let bottom_front = set
        .joints
        .iter()
        .find(|j| j.a.vector == bottom && j.b.vector() == front)
        .expect("joint");
    assert!((bottom_front.a.from[1] - 100.0).abs() < 1e-9);
    // The bottom is listed first, so it keeps its corners.
    assert!(inside_rings(&rings(&doc, bottom), [101.0, 101.0]));
    assert!(!inside_rings(&rings(&doc, front), [101.0, 89.0]));
}

#[test]
fn a_test_coupon_is_two_small_parts_at_a_free_spot_with_the_joint() {
    let mut doc = job(300.0, 200.0, 12.0);
    let sheet = doc.active_sheet();
    let taken = rect(&mut doc, sheet, 0.0, 0.0, 150.0, 200.0);
    let reply = run(&mut doc, "joints.coupon", json!({ "kind": "finger" }));
    let parts: Vec<u64> = reply["parts"]
        .as_array()
        .expect("parts")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(parts.len(), 2);
    let busy = doc.bbox_of(taken).expect("box");
    for part in &parts {
        let b = doc.bbox_of(*part).expect("box");
        assert!(!b.intersects(busy), "the coupon keeps clear of the drawing");
        assert!(b.max.x - b.min.x <= 120.0);
    }
    assert_eq!(doc.assembly().joint_set.joints.len(), 1);
    // One undo step takes it all back.
    doc.undo();
    assert!(doc.entity(parts[0]).is_err());
    assert!(doc.assembly().joint_set.joints.is_empty());
}

/// A 200 x 100 base on the first sheet and a 200 x 60 shelf on a second,
/// both 12 mm thick.
fn base_and_shelf() -> (Document, u64, u64) {
    let mut doc = job(400.0, 300.0, 12.0);
    let first = doc.active_sheet();
    doc.begin("sheet");
    let second = doc.add_sheet(Some("Shelf".into()));
    doc.commit();
    let base = rect(&mut doc, first, 10.0, 10.0, 210.0, 110.0);
    let shelf = rect(&mut doc, second, 10.0, 150.0, 210.0, 210.0);
    (doc, base, shelf)
}

fn dado_spec(base: u64, shelf: u64, params: Value) -> Value {
    json!({
        "kind": "dado",
        // The shelf's bottom edge stands on the base 40 mm in from its
        // bottom edge, across its whole width.
        "a": edge(shelf, [10.0, 150.0], [210.0, 150.0]),
        "b": {
            "kind": "face",
            "edge": { "vector": base, "from": [10.0, 10.0], "to": [210.0, 10.0] },
            "offset": 40.0,
            "along": 100.0,
        },
        "params": params,
    })
}

/// Makes the joint's toolpaths, puts the parts together and checks every
/// fit face is `clearance` apart with nothing running into anything.
fn assert_cut_fit(doc: &mut Document, id: &Value, clearance: f64) {
    run(doc, "joints.makeToolpaths", json!({ "tool": end_mill() }));
    run(doc, "joints.assemble", json!({ "id": id }));
    let joints = verify(doc, clearance, clearance / 2.0);
    assert!(!joints.is_empty());
    for joint in &joints {
        assert_eq!(joint["source"], "exact", "{joint}");
        assert_eq!(joint["overlapDepth"], 0.0, "{joint}");
        assert_ne!(joint["status"], "interference", "{joint}");
    }
    assert_fit(&joints, clearance);
}

/// Every side of every joint either touches or stands `clearance` off, to
/// 0.01 mm, and at least one stands off (a joint with one wall a side, such
/// as a rabbet, has no wall facing a wall).
fn assert_sides(joints: &[Value], clearance: f64) {
    let sides: Vec<(f64, f64)> = joints
        .iter()
        .flat_map(|j| j["sides"].as_array().cloned().unwrap_or_default())
        .map(|s| (number(&s["min"]), number(&s["max"])))
        .collect();
    for (min, max) in &sides {
        let near = |v: f64| v.abs() < 0.01 || (v - clearance).abs() < 0.01;
        assert!(near(*min) && near(*max), "{min} {max}: {joints:?}");
    }
    assert!(sides.iter().any(|(_, max)| (max - clearance).abs() < 0.01));
}

#[test]
fn a_through_dado_is_a_pocket_across_the_base_the_shelf_fits_in() {
    let (mut doc, base, shelf) = base_and_shelf();
    let before = path(&doc, shelf);
    let reply = run(&mut doc, "joints.add", dado_spec(base, shelf, json!({})));
    // A third of the stock deep; the shelf keeps its shape.
    assert!((number(&reply["outcome"]["resolved"]["depth"]) - 4.0).abs() < 1e-9);
    assert_eq!(path(&doc, shelf), before);
    let joint = doc.assembly().joint_set.joints[0].clone();
    assert_eq!(joint.vectors.len(), 1);
    assert_eq!(joint.vectors[0].role, super::Role::Pocket { depth: 4.0 });
    // 12 + 2 x 0.1 wide, 40 mm in, running out past both ends.
    let pocket = rings(&doc, joint.vectors[0].id);
    assert!(inside_rings(&pocket, [8.0, 50.0 - 0.05]));
    assert!(inside_rings(&pocket, [212.0, 50.0 + 12.05]));
    assert!(!inside_rings(&pocket, [100.0, 50.0 - 0.15]));
    assert!(!inside_rings(&pocket, [100.0, 62.15]));
    assert_cut_fit(&mut doc, &reply["id"], 0.1);
}

#[test]
fn a_stopped_dado_stops_short_and_notches_the_shelf_to_pass_the_stop() {
    let (mut doc, base, shelf) = base_and_shelf();
    let reply = run(
        &mut doc,
        "joints.add",
        dado_spec(base, shelf, json!({ "stop": 30.0 })),
    );
    let joint = doc.assembly().joint_set.joints[0].clone();
    let pocket = rings(&doc, joint.vectors[0].id);
    // The dado ends 30 mm short of the base's right end (x 210).
    assert!(inside_rings(&pocket, [175.0, 56.0]));
    assert!(!inside_rings(&pocket, [185.0, 56.0]));
    // The shelf's edge runs the other way along the base, so its left end
    // meets the stop: it is stepped up by the dado depth.
    let r = rings(&doc, shelf);
    assert!(!inside_rings(&r, [20.0, 152.0]));
    assert!(inside_rings(&r, [70.0, 152.0]));
    assert!(inside_rings(&r, [20.0, 156.0]));
    assert_cut_fit(&mut doc, &reply["id"], 0.1);
}

#[test]
fn a_rabbet_steps_the_edge_and_cuts_the_other_part_back() {
    let (mut doc, a, b) = two_panels();
    let mut spec = finger_spec(a, b);
    spec["kind"] = json!("rabbet");
    let before_a = area(&doc, a);
    let reply = run(&mut doc, "joints.add", spec);
    // Half the stock deep: A is cut back by the other half, 6 mm.
    assert!((number(&reply["outcome"]["resolved"]["depth"]) - 6.0).abs() < 1e-9);
    assert!(((before_a - area(&doc, a)) - 6.0 * 100.0).abs() < 1e-6);
    let joint = doc.assembly().joint_set.joints[0].clone();
    let pocket = rings(&doc, joint.vectors[0].id);
    // A's thickness plus the clearance along B's bottom edge.
    assert!(inside_rings(&pocket, [50.0, 100.0 + 12.05]));
    assert!(!inside_rings(&pocket, [50.0, 100.0 + 12.15]));
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    run(&mut doc, "joints.assemble", json!({ "id": reply["id"] }));
    let joints = verify(&mut doc, 0.1, 0.05);
    assert!(
        joints.iter().all(|j| j["overlapDepth"] == 0.0),
        "{joints:?}"
    );
    assert_sides(&joints, 0.1);
}

#[test]
fn a_dado_needs_a_place_inside_the_second_part() {
    let (mut doc, base, shelf) = base_and_shelf();
    let mut spec = dado_spec(base, shelf, json!({}));
    spec["b"]["offset"] = json!(150.0);
    let error = try_run(&mut doc, "joints.add", spec).expect_err("refused");
    assert!(error.contains("not on the second part"), "{error}");
    let mut edge_spec = dado_spec(base, shelf, json!({}));
    edge_spec["b"] =
        json!({ "kind": "edge", "vector": base, "from": [10.0, 10.0], "to": [210.0, 10.0] });
    let error = try_run(&mut doc, "joints.add", edge_spec).expect_err("refused");
    assert!(error.contains("click inside the second part"), "{error}");
}

fn tab_spec(base: u64, shelf: u64, kind: &str, params: Value) -> Value {
    let mut spec = dado_spec(base, shelf, params);
    spec["kind"] = json!(kind);
    spec
}

#[test]
fn tabs_stand_out_of_the_edge_through_slots_that_fit_all_round() {
    let (mut doc, base, shelf) = base_and_shelf();
    let before = area(&doc, shelf);
    let reply = run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "tabSlot", json!({})),
    );
    let resolved = &reply["outcome"]["resolved"];
    // A 200 mm edge: two tabs 40 mm wide at the quarters, 12 mm long.
    assert_eq!(resolved["count"], 2);
    assert!((number(&resolved["width"]) - 40.0).abs() < 1e-9);
    let gained = area(&doc, shelf) - before;
    assert!(
        gained > 2.0 * 40.0 * 12.0 - 60.0 && gained < 2.0 * 40.0 * 12.0,
        "{gained}"
    );
    let r = rings(&doc, shelf);
    assert!(inside_rings(&r, [60.0, 140.0]), "the first tab's middle");
    assert!(!inside_rings(&r, [110.0, 140.0]), "no tab in the middle");
    let joint = doc.assembly().joint_set.joints[0].clone();
    assert_eq!(joint.vectors.len(), 2);
    assert!(joint.vectors.iter().all(|v| v.role == super::Role::Hole));
    // Slot 1: the first tab plus 0.1 all round, 40 mm in from the base's
    // edge; the shelf's edge runs the other way along the base, so the
    // first tab (30 to 70 along the shelf) lands 140 to 180 along it.
    let slot = rings(&doc, joint.vectors[0].id);
    assert!(inside_rings(&slot, [180.0 + 0.05, 50.0 - 0.05]));
    assert!(!inside_rings(&slot, [160.0, 50.0 - 0.15]));
    assert_cut_fit(&mut doc, &reply["id"], 0.1);
}

#[test]
fn proud_tabs_can_have_round_ends() {
    let (mut doc, base, shelf) = base_and_shelf();
    run(
        &mut doc,
        "joints.add",
        tab_spec(
            base,
            shelf,
            "tabSlot",
            json!({ "proud": 6.0, "ends": "rounded" }),
        ),
    );
    let r = rings(&doc, shelf);
    // The first tab spans x 30 to 70 and reaches 18 mm below the edge;
    // its tip corners are round.
    assert!(inside_rings(&r, [50.0, 150.0 - 17.5]));
    assert!(!inside_rings(&r, [30.5, 150.0 - 17.5]));
    let path = path(&doc, shelf);
    let arcs = path.subpaths[0]
        .segments
        .iter()
        .filter(|s| matches!(s, crate::geom::Segment::Arc { .. }))
        .count();
    assert!(arcs >= 4, "the round tips are arcs: {arcs}");
}

#[test]
fn a_blind_mortise_is_a_pocket_and_the_tenon_stops_short_of_its_floor() {
    let (mut doc, base, shelf) = base_and_shelf();
    let reply = run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "mortiseTenon", json!({ "blind": true })),
    );
    let resolved = &reply["outcome"]["resolved"];
    assert_eq!(resolved["count"], 1);
    assert!((number(&resolved["depth"]) - 8.0).abs() < 1e-9);
    let joint = doc.assembly().joint_set.joints[0].clone();
    assert_eq!(joint.vectors.len(), 1);
    assert_eq!(joint.vectors[0].role, super::Role::Pocket { depth: 8.0 });
    // The tenon is 8 - 0.1 mm long.
    let r = rings(&doc, shelf);
    assert!(inside_rings(&r, [110.0, 150.0 - 7.85]));
    assert!(!inside_rings(&r, [110.0, 150.0 - 7.95]));
    assert_cut_fit(&mut doc, &reply["id"], 0.1);
}

#[test]
fn too_many_tabs_say_so() {
    let (mut doc, base, shelf) = base_and_shelf();
    let error = try_run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "tabSlot", json!({ "count": 6, "width": 40.0 })),
    )
    .expect_err("refused");
    assert!(error.contains("do not fit"), "{error}");
}

/// Two 200 x 80 dividers, 12 mm thick, on two sheets.
fn two_dividers() -> (Document, u64, u64) {
    let mut doc = job(400.0, 300.0, 12.0);
    let first = doc.active_sheet();
    doc.begin("sheet");
    let second = doc.add_sheet(Some("Second".into()));
    doc.commit();
    let a = rect(&mut doc, first, 10.0, 10.0, 210.0, 90.0);
    let b = rect(&mut doc, second, 10.0, 150.0, 210.0, 230.0);
    (doc, a, b)
}

fn cross_spec(a: u64, b: u64, params: Value) -> Value {
    json!({
        "kind": "crossLap",
        // A's top edge and B's bottom edge.
        "a": edge(a, [210.0, 90.0], [10.0, 90.0]),
        "b": { "kind": "edge", "vector": b, "from": [10.0, 150.0], "to": [210.0, 150.0] },
        "params": params,
    })
}

#[test]
fn crossing_parts_get_half_depth_slots_where_they_were_picked() {
    let (mut doc, a, b) = two_dividers();
    let reply = run(
        &mut doc,
        "joints.add",
        cross_spec(a, b, json!({ "atA": 150.0, "atB": 50.0 })),
    );
    assert!((number(&reply["outcome"]["resolved"]["depth"]) - 40.0).abs() < 1e-9);
    // A's slot: 150 mm along its top edge from x 210, so at x 60, 12.2
    // wide and 40 deep.
    let ra = rings(&doc, a);
    assert!(!inside_rings(&ra, [60.0, 55.0]));
    assert!(!inside_rings(&ra, [60.0 + 6.05, 55.0]));
    assert!(inside_rings(&ra, [60.0 + 6.15, 55.0]));
    assert!(inside_rings(&ra, [60.0, 45.0]));
    // B's slot: 50 mm from x 10, from its bottom edge up 40.
    let rb = rings(&doc, b);
    assert!(!inside_rings(&rb, [60.0, 185.0]));
    assert!(inside_rings(&rb, [60.0, 195.0]));
    assert_cut_fit(&mut doc, &reply["id"], 0.1);
}

#[test]
fn a_snap_detent_narrows_the_slot_mouth() {
    let (mut doc, a, b) = two_dividers();
    run(
        &mut doc,
        "joints.add",
        cross_spec(a, b, json!({ "detent": 0.3 })),
    );
    let ra = rings(&doc, a);
    // The slot at x 110 (the middle), walls at 103.9 and 116.1; the bump
    // stands 0.3 into it about 2.5 mm below the edge.
    assert!(!inside_rings(&ra, [104.1, 70.0]));
    assert!(inside_rings(&ra, [104.1, 87.5]));
    assert!(!inside_rings(&ra, [104.3, 87.5]));
}

#[test]
fn a_half_lap_clears_half_of_each_end_the_other_part_wide() {
    let mut doc = job(400.0, 300.0, 12.0);
    let first = doc.active_sheet();
    doc.begin("sheet");
    let second = doc.add_sheet(Some("Second".into()));
    doc.commit();
    // A rail 200 x 40 and a stile 40 x 200: their right and top ends lap.
    let a = rect(&mut doc, first, 10.0, 10.0, 210.0, 50.0);
    let b = rect(&mut doc, second, 250.0, 10.0, 290.0, 210.0);
    let reply = run(
        &mut doc,
        "joints.add",
        json!({
            "kind": "halfLap",
            "a": edge(a, [210.0, 10.0], [210.0, 50.0]),
            "b": { "kind": "edge", "vector": b, "from": [290.0, 210.0], "to": [250.0, 210.0] },
        }),
    );
    let joint = doc.assembly().joint_set.joints[0].clone();
    assert_eq!(joint.vectors.len(), 2);
    assert_eq!(joint.vectors[0].role, super::Role::Pocket { depth: 6.0 });
    assert_eq!(joint.vectors[1].role, super::Role::Pocket { depth: 6.0 });
    // A's lap: the last 40 + 0.1 mm of the rail.
    let lap = rings(&doc, joint.vectors[0].id);
    assert!(inside_rings(&lap, [170.0 - 0.05, 30.0]));
    assert!(!inside_rings(&lap, [170.0 - 0.15, 30.0]));
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    run(&mut doc, "joints.assemble", json!({ "id": reply["id"] }));
    let joints = verify(&mut doc, 0.1, 0.05);
    assert!(
        joints.iter().all(|j| j["overlapDepth"] == 0.0),
        "{joints:?}"
    );
    assert_sides(&joints, 0.1);
}

#[test]
fn a_wedged_tenon_goes_through_with_a_hole_for_the_wedge_and_the_wedge_drawn() {
    let (mut doc, base, shelf) = base_and_shelf();
    let reply = run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "wedgedTenon", json!({})),
    );
    let resolved = &reply["outcome"]["resolved"];
    let proud = number(&resolved["proud"]);
    let width = number(&resolved["width"]);
    assert!(proud > 20.0 && proud < 30.0, "{proud}");
    assert!((width - 200.0 * 2.0 / 3.0).abs() < 1e-6);
    let joint = doc.assembly().joint_set.joints[0].clone();
    let roles: Vec<_> = joint.vectors.iter().map(|v| v.role).collect();
    assert_eq!(
        roles,
        vec![
            super::Role::Hole,
            super::Role::Hole,
            super::Role::Part { thickness: 12.0 }
        ]
    );
    // The tenon reaches 12 + proud below the shelf's edge.
    let r = rings(&doc, shelf);
    assert!(inside_rings(&r, [110.0 - 20.0, 150.0 - 12.0 - proud + 0.1]));
    assert!(!inside_rings(
        &r,
        [110.0 - 20.0, 150.0 - 12.0 - proud - 0.1]
    ));
    // The wedge hole starts 1 mm inside the base's far face.
    let hole = rings(&doc, joint.vectors[1].id);
    assert!(inside_rings(&hole, [110.0, 150.0 - 11.1]));
    assert!(!inside_rings(&hole, [110.0, 150.0 - 10.9]));
    // The wedge is drawn at a free place on the shelf's sheet, clear of
    // the shelf, tapering 1 in 8.
    let wedge = doc.bbox_of(joint.vectors[2].id).expect("wedge");
    assert!(!wedge.intersects(doc.bbox_of(shelf).expect("shelf")));
    assert_eq!(
        doc.entity(joint.vectors[2].id).expect("wedge").sheet,
        doc.entity(shelf).expect("shelf").sheet
    );
    assert!((wedge.max.y - wedge.min.y - 52.0).abs() < 1e-6);
    assert_cut_fit(&mut doc, &reply["id"], 0.1);
}

#[test]
fn dovetail_tails_lock_into_sockets_in_the_same_plane() {
    let (mut doc, a, b) = two_panels();
    let mut spec = finger_spec(a, b);
    spec["kind"] = json!("dovetail");
    let before_a = area(&doc, a);
    let before_b = area(&doc, b);
    let reply = run(&mut doc, "joints.add", spec);
    let resolved = &reply["outcome"]["resolved"];
    assert_eq!(resolved["count"], 2);
    let depth = number(&resolved["depth"]);
    // A gains the tails (less the reliefs at their roots), B loses a
    // little more for the clearance.
    let gained = area(&doc, a) - before_a;
    let lost = before_b - area(&doc, b);
    assert!(
        gained < 50.0 * depth && gained > 50.0 * depth - 60.0,
        "{gained}"
    );
    assert!(lost > gained, "{lost} {gained}");
    // The first tail (A's top edge runs from x 110 to 10): wider at its tip
    // (y 60 + depth) than at its root.
    let ra = rings(&doc, a);
    let mid = 110.0 - 25.0;
    let at_root = 12.5 - depth * 14f64.to_radians().tan() / 2.0;
    assert!(inside_rings(&ra, [mid + at_root - 0.5, 60.5]));
    assert!(!inside_rings(&ra, [mid + at_root + 0.5, 60.5]));
    assert!(inside_rings(&ra, [mid + at_root + 1.5, 60.0 + depth - 0.5]));
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    run(&mut doc, "joints.assemble", json!({ "id": reply["id"] }));
    let joints = verify(&mut doc, 0.1, 0.05);
    // Each tail side and tip stands 0.1 mm off its socket (the slanted
    // sides too); the seam between the tails touches.
    assert!(
        joints.iter().all(|j| j["overlapDepth"] == 0.0),
        "{joints:?}"
    );
    assert_sides(&joints, 0.1);
    let slanted = joints
        .iter()
        .flat_map(|j| j["sides"].as_array().cloned().unwrap_or_default())
        .filter(|s| number(&s["normal"][0]).abs() > 0.9)
        .count();
    // Verify Fit may group a tail's faces into one side or two; there are
    // slanted sides either way.
    assert!(slanted >= 2, "{joints:?}");
}

#[test]
fn dowel_holes_line_up_under_the_part_with_marks_on_its_edge() {
    let (mut doc, base, shelf) = base_and_shelf();
    let before = path(&doc, shelf);
    let reply = run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "dowel", json!({})),
    );
    let resolved = &reply["outcome"]["resolved"];
    // Three 8 mm dowels (6 mm holes are too small for the 6.35 mm tool),
    // 25 mm in from the ends, 8 mm deep.
    assert_eq!(resolved["count"], 3);
    assert!((number(&resolved["diameter"]) - 8.0).abs() < 1e-9);
    assert!((number(&resolved["inset"]) - 25.0).abs() < 1e-9);
    assert!((number(&resolved["depth"]) - 8.0).abs() < 1e-9);
    assert_eq!(path(&doc, shelf), before, "the shelf itself is not cut");
    let joint = doc.assembly().joint_set.joints[0].clone();
    let holes: Vec<_> = joint
        .vectors
        .iter()
        .filter(|v| v.role == super::Role::Pocket { depth: 8.0 })
        .collect();
    let marks = joint
        .vectors
        .iter()
        .filter(|v| v.role == super::Role::Mark)
        .count();
    assert_eq!((holes.len(), marks), (3, 3));
    // Each drill mark says what to drill, and the dowels are as long as
    // both holes are deep.
    assert!((number(&resolved["length"]) - 16.0).abs() < 1e-9);
    let mark = joint
        .vectors
        .iter()
        .find(|v| v.role == super::Role::Mark)
        .expect("a mark");
    let name = doc
        .entity(mark.id)
        .expect("mark")
        .name
        .clone()
        .unwrap_or_default();
    assert!(
        name.contains("drill mark 1: 8 mm, 8 mm deep into the edge"),
        "{name}"
    );
    assert!(reply["outcome"]["notes"][0]
        .as_str()
        .is_some_and(|n| n.contains("drill the first part's edge by hand")));
    // The first hole, under drill mark 1 (25 mm in from the shelf's left
    // end, which lands at the base's right end): 25 mm in from x 210, on
    // the middle of the shelf's thickness 40 + 6 mm in, 8.2 mm across.
    let first = doc.bbox_of(holes[0].id).expect("hole");
    assert!(((first.min.x + first.max.x) / 2.0 - 185.0).abs() < 1e-6);
    assert!(((first.min.y + first.max.y) / 2.0 - 56.0).abs() < 1e-6);
    assert!(((first.max.x - first.min.x) - 8.2).abs() < 1e-3);
    // Pegs go right through.
    let mut pegs = tab_spec(base, shelf, "dowel", json!({ "blind": false }));
    pegs["b"]["offset"] = json!(70.0);
    run(&mut doc, "joints.add", pegs);
    let second = doc.assembly().joint_set.joints[1].clone();
    assert!(second.vectors.iter().any(|v| v.role == super::Role::Hole));
}

#[test]
fn a_captive_nut_joint_has_bolt_holes_and_t_slots_that_fit_the_nut() {
    let (mut doc, base, shelf) = base_and_shelf();
    let reply = run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "captiveNut", json!({})),
    );
    let resolved = &reply["outcome"]["resolved"];
    assert_eq!(resolved["count"], 2);
    assert!((number(&resolved["length"]) - 32.0).abs() < 1e-9);
    let joint = doc.assembly().joint_set.joints[0].clone();
    assert_eq!(joint.vectors.len(), 2);
    assert!(joint.vectors.iter().all(|v| v.role == super::Role::Hole));
    // M8 bolts, the smallest from M6 up whose slots fit the 6.35 mm tool.
    assert!((number(&resolved["diameter"]) - 8.0).abs() < 1e-9);
    // Bolt 1 at 50 mm along the shelf (x 60): its slot runs 20 + 1 mm up
    // from the edge, 8.2 wide; the nut slot crosses it 14.67 mm up, 13.33
    // + 0.2 wide.
    let r = rings(&doc, shelf);
    assert!(!inside_rings(&r, [60.0, 150.0 + 20.5]));
    assert!(inside_rings(&r, [60.0, 150.0 + 23.0]));
    let nut_at = 20.0 - 8.0 * 5.0 / 12.0 - 2.0;
    assert!(!inside_rings(&r, [60.0 + 6.6, 150.0 + nut_at]));
    assert!(inside_rings(&r, [60.0 + 7.5, 150.0 + nut_at]));
    assert!(
        !inside_rings(&r, [60.0 + 4.0, 150.0 + 8.0]),
        "the bolt slot"
    );
    assert!(
        inside_rings(&r, [60.0 + 4.5, 150.0 + 8.0]),
        "only the bolt slot below the nut"
    );
    // A bolt whose slots the tool cannot cut is refused with the size to use.
    let err = try_run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "captiveNut", json!({ "diameter": 6.0 })),
    )
    .expect_err("refused");
    assert!(err.contains("M8"), "{err}");
    assert_cut_fit_sides(&mut doc, &reply["id"]);
}

/// Cuts the joint, puts it together and checks nothing runs into anything.
fn assert_cut_fit_sides(doc: &mut Document, id: &Value) {
    run(doc, "joints.makeToolpaths", json!({ "tool": end_mill() }));
    run(doc, "joints.assemble", json!({ "id": id }));
    let joints = verify(doc, 0.1, 0.05);
    assert!(!joints.is_empty());
    assert!(
        joints.iter().all(|j| j["overlapDepth"] == 0.0),
        "{joints:?}"
    );
}

#[test]
fn butterfly_keys_are_pockets_across_the_seam_and_keys_that_fit_them() {
    let (mut doc, a, b) = two_panels();
    let mut spec = finger_spec(a, b);
    spec["kind"] = json!("butterfly");
    let (pa, pb) = (path(&doc, a), path(&doc, b));
    let reply = run(&mut doc, "joints.add", spec);
    let resolved = &reply["outcome"]["resolved"];
    assert_eq!(resolved["count"], 1);
    assert!((number(&resolved["depth"]) - 6.0).abs() < 1e-9);
    // The panels keep their outlines.
    assert_eq!((path(&doc, a), path(&doc, b)), (pa, pb));
    let joint = doc.assembly().joint_set.joints[0].clone();
    let roles: Vec<_> = joint.vectors.iter().map(|v| v.role).collect();
    assert_eq!(
        roles,
        vec![
            super::Role::Pocket { depth: 6.0 },
            super::Role::Pocket { depth: 6.0 },
            super::Role::Part { thickness: 7.0 }
        ]
    );
    // A's half: 30 mm wide at its far end (y 30), narrower at the seam.
    let half = rings(&doc, joint.vectors[0].id);
    assert!(inside_rings(&half, [60.0 + 14.5, 30.5]));
    assert!(!inside_rings(&half, [60.0 + 15.1, 30.5]));
    assert!(!inside_rings(&half, [60.0 + 10.0, 59.0]));
    let key_box = doc.bbox_of(joint.vectors[2].id).expect("key");
    assert!(
        !key_box.intersects(doc.bbox_of(a).expect("a")),
        "drawn clear of the part"
    );
    // B's half is the other end of the bow tie: in B's layout the bow tie
    // above the seam (y 60) lies 40 mm higher.
    let pocket_b = rings(&doc, joint.vectors[1].id);
    assert!(inside_rings(&pocket_b, [60.0 + 14.5, 129.5]));
    // The key, moved back over the bow tie, is inside it everywhere with
    // at least the clearance to spare.
    let key = rings(&doc, joint.vectors[2].id);
    let dx = 60.0 - (key_box.min.x + key_box.max.x) / 2.0;
    let dy = 60.0 - (key_box.min.y + key_box.max.y) / 2.0;
    for q in key.iter().flatten() {
        let p = [q[0] + dx, q[1] + dy];
        let pocket = |x: f64, y: f64| {
            if y <= 60.0 {
                inside_rings(&half, [x, y])
            } else {
                inside_rings(&pocket_b, [x, y + 40.0])
            }
        };
        assert!(pocket(p[0], p[1]), "{p:?}");
        // 0.09 mm out in every direction is still pocket.
        for (ox, oy) in [(0.09, 0.0), (-0.09, 0.0), (0.0, 0.09), (0.0, -0.09)] {
            let (x, y) = (p[0] + ox, p[1] + oy);
            if (y - 60.0).abs() > 0.2 {
                assert!(pocket(x, y), "{p:?} + {ox} {oy}");
            }
        }
    }
}

#[test]
fn puzzle_knobs_lock_into_round_sockets_with_the_clearance_all_round() {
    let (mut doc, a, b) = two_panels();
    let mut spec = finger_spec(a, b);
    spec["kind"] = json!("puzzle");
    let reply = run(&mut doc, "joints.add", spec);
    let resolved = &reply["outcome"]["resolved"];
    assert_eq!(resolved["count"], 1);
    let across = number(&resolved["width"]);
    assert!((across - 100.0 / 6.0).abs() < 1e-9, "{across}");
    // The knob: a circle 1.6 radii out from A's edge on a narrower neck,
    // drawn as arcs.
    let r = across / 2.0;
    let ra = rings(&doc, a);
    assert!(inside_rings(&ra, [60.0 + r - 0.3, 60.0 + 1.6 * r]));
    assert!(!inside_rings(&ra, [60.0 + 0.55 * r + 0.3, 60.0 + 0.3]));
    let arcs = path(&doc, a).subpaths[0]
        .segments
        .iter()
        .filter(|s| matches!(s, crate::geom::Segment::Arc { .. }))
        .count();
    assert!(arcs >= 2, "{arcs}");
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    run(&mut doc, "joints.assemble", json!({ "id": reply["id"] }));
    let joints = verify(&mut doc, 0.1, 0.05);
    // Nothing runs into anything; the knob stands the clearance off its
    // socket (the reliefs at the neck leave more room there).
    assert!(
        joints.iter().all(|j| j["overlapDepth"] == 0.0),
        "{joints:?}"
    );
    let sides: Vec<(f64, f64)> = joints
        .iter()
        .flat_map(|j| j["sides"].as_array().cloned().unwrap_or_default())
        .map(|s| (number(&s["min"]), number(&s["max"])))
        .collect();
    assert!(sides.iter().all(|(min, _)| *min > -0.001), "{sides:?}");
    assert!(sides
        .iter()
        .any(|(min, max)| (min - 0.1).abs() < 0.01 && (max - 0.1).abs() < 0.01));
}

#[test]
fn puzzle_knobs_sized_by_themselves_fit_the_tool_on_a_short_edge() {
    // Two 60 mm edges: a twelfth of the edge would give necks narrower than
    // the default tool, so the knobs grow to the smallest the tool can cut.
    let mut doc = job(300.0, 200.0, 12.0);
    let sheet = doc.active_sheet();
    let a = rect(&mut doc, sheet, 10.0, 10.0, 70.0, 60.0);
    let b = rect(&mut doc, sheet, 10.0, 100.0, 70.0, 150.0);
    let spec = json!({
        "kind": "puzzle",
        "a": edge(a, [70.0, 60.0], [10.0, 60.0]),
        "b": { "kind": "edge", "vector": b, "from": [10.0, 100.0], "to": [70.0, 100.0] },
        "params": {},
    });
    let reply = run(&mut doc, "joints.add", spec);
    let resolved = &reply["outcome"]["resolved"];
    assert_eq!(resolved["count"], 1);
    assert!(number(&resolved["width"]) >= 12.0, "{resolved}");
}

#[test]
fn a_living_hinge_is_a_band_of_staggered_slots_across_one_part() {
    let mut doc = job(400.0, 300.0, 6.0);
    let sheet = doc.active_sheet();
    let panel = rect(&mut doc, sheet, 10.0, 10.0, 210.0, 110.0);
    let before = path(&doc, panel);
    let bottom = edge(panel, [10.0, 10.0], [210.0, 10.0]);
    let reply = run(
        &mut doc,
        "joints.add",
        json!({
            "kind": "livingHinge",
            "a": bottom,
            "b": { "kind": "edge", "vector": panel, "from": [10.0, 10.0], "to": [210.0, 10.0] },
            "params": { "atA": 100.0 },
        }),
    );
    let resolved = &reply["outcome"]["resolved"];
    let band = number(&resolved["width"]);
    // A quarter turn round 18 mm (three thicknesses) plus half the stock.
    assert!((band - std::f64::consts::FRAC_PI_2 * 21.0).abs() < 1e-9);
    assert_eq!(path(&doc, panel), before, "the outline stays");
    let joint = doc.assembly().joint_set.joints[0].clone();
    let columns = resolved["count"].as_u64().expect("columns") as usize;
    assert!(columns >= 4, "{columns}");
    // Two slots in every other column, one in the rest.
    assert_eq!(joint.vectors.len(), columns + columns.div_ceil(2));
    for v in &joint.vectors {
        assert_eq!(v.role, super::Role::Hole);
        let b = doc.bbox_of(v.id).expect("slot");
        assert!(
            (b.max.x - b.min.x - 6.45).abs() < 1e-6,
            "as narrow as the tool cuts"
        );
        assert!(b.min.x >= 110.0 - band / 2.0 - 1e-6 && b.max.x <= 110.0 + band / 2.0 + 7.0);
        assert!(b.min.y > 10.0 && b.max.y < 110.0);
    }
}

/// Every closed vector the joints touched or made crosses itself nowhere,
/// so the toolpaths never go through the time limited repair.
fn assert_simple(doc: &Document) {
    let tol = crate::geom::Tolerance::new(crate::cam::compute::CUT_TOLERANCE).expect("tol");
    let set = &doc.assembly().joint_set;
    let mut ids: Vec<u64> = set.originals.keys().copied().collect();
    for joint in &set.joints {
        ids.extend(joint.vectors.iter().map(|v| v.id));
    }
    for id in ids {
        for sub in path(doc, id).subpaths.iter().filter(|s| s.closed) {
            let started = std::time::Instant::now();
            assert!(
                !crate::geom::subpath_self_intersects(sub, tol),
                "vector {id} crosses itself"
            );
            assert!(
                started.elapsed().as_secs_f64() < 1.0,
                "vector {id} is slow to check"
            );
        }
    }
}

#[test]
fn every_joint_leaves_vectors_that_cross_themselves_nowhere() {
    let (mut doc, a, b) = two_panels();
    for kind in [
        "finger",
        "dovetail",
        "puzzle",
        "butterfly",
        "rabbet",
        "crossLap",
    ] {
        let mut spec = finger_spec(a, b);
        spec["kind"] = json!(kind);
        let id = run(&mut doc, "joints.add", spec)["id"].clone();
        assert_simple(&doc);
        run(&mut doc, "joints.remove", json!({ "id": id }));
    }
    let (mut doc, base, shelf) = base_and_shelf();
    for kind in [
        "dado",
        "tabSlot",
        "mortiseTenon",
        "wedgedTenon",
        "kerfTenon",
        "dowel",
        "captiveNut",
    ] {
        let id = run(
            &mut doc,
            "joints.add",
            tab_spec(base, shelf, kind, json!({})),
        )["id"]
            .clone();
        assert_simple(&doc);
        run(&mut doc, "joints.remove", json!({ "id": id }));
    }
}

#[test]
fn a_drawn_wedge_stays_where_it_was_moved_and_a_second_goes_elsewhere() {
    let (mut doc, base, shelf) = base_and_shelf();
    run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "wedgedTenon", json!({})),
    );
    let wedge = doc.assembly().joint_set.joints[0].vectors[2].id;
    // Move the wedge; a settings change remakes the joint but keeps it
    // where it now is.
    doc.begin("move");
    doc.transform_entities(
        &[wedge],
        &crate::geom::Mat3::translate(crate::geom::Vec2::new(0.0, 20.0)),
    );
    doc.commit();
    let moved = doc.bbox_of(wedge).expect("wedge");
    let mut settings = doc.assembly().joint_set.settings.clone();
    settings.fit = super::Fit::Slip;
    run(
        &mut doc,
        "joints.setSettings",
        json!({ "settings": settings }),
    );
    let after = doc.bbox_of(wedge).expect("wedge");
    assert!((after.min.x - moved.min.x).abs() < 1e-9 && (after.min.y - moved.min.y).abs() < 1e-9);
    // A second wedged tenon's wedge does not land on the first.
    let mut other = tab_spec(base, shelf, "wedgedTenon", json!({}));
    other["b"]["offset"] = json!(10.0);
    run(&mut doc, "joints.add", other);
    let second = doc.assembly().joint_set.joints[1].vectors[2].id;
    assert!(!doc.bbox_of(second).expect("second").intersects(after));
}

/// The Assembly View's mates as listed.
fn mate_rows(doc: &mut Document) -> Vec<Value> {
    run(doc, "assembly.pieces", json!({}))["mates"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[test]
fn a_joint_keeps_a_mate_that_holds_its_parts_together_until_it_is_removed() {
    let (mut doc, a, b) = two_panels();
    let id = run(&mut doc, "joints.add", finger_spec(a, b))["id"].clone();
    // No toolpaths yet, so no pieces and no mate.
    assert!(doc.assembly().piece_mates.is_empty());
    let made = run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    assert_eq!(made["mates"], 1, "{made}");
    let rows = mate_rows(&mut doc);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["kind"], "joint");
    assert_eq!(rows[0]["joint"], id);
    assert_eq!(rows[0]["state"], "solved", "{rows:?}");
    // Verify Fit finds the joint without Assemble being pressed.
    assert_fit(&verify(&mut doc, 0.1, 0.05), 0.1);

    // A looser fit remakes the joint and its toolpaths: the mate keeps the
    // parts together at the new clearance.
    let mut settings = doc.assembly().joint_set.settings.clone();
    settings.fit = super::Fit::Slip;
    run(
        &mut doc,
        "joints.setSettings",
        json!({ "settings": settings }),
    );
    let again = run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    assert_eq!(again["mates"], 0, "the mate is kept, not made again");
    assert_eq!(doc.assembly().piece_mates.len(), 1);
    assert_eq!(mate_rows(&mut doc)[0]["state"], "solved");
    assert_fit(&verify(&mut doc, 0.2, 0.05), 0.2);

    // Moving the first part takes the second with it.
    let pieces = run(&mut doc, "assembly.pieces", json!({}));
    let first = pieces["pieces"]
        .as_array()
        .and_then(|p| p.first().cloned())
        .expect("a piece");
    let mut position = first["position"].clone();
    position[2] = json!(number(&position[2]) + 50.0);
    run(
        &mut doc,
        "assembly.placePiece",
        json!({
            "sheet": first["sheet"],
            "part": first["part"],
            "position": position,
            "rotation": [0.0, 0.0, 30.0],
        }),
    );
    assert_eq!(mate_rows(&mut doc)[0]["state"], "solved");
    assert_fit(&verify(&mut doc, 0.2, 0.05), 0.2);

    // Remove Joint takes its mate too.
    let removed = run(&mut doc, "joints.remove", json!({ "id": id }));
    assert!(removed["removed"] == id);
    assert!(doc.assembly().piece_mates.is_empty());
}

#[test]
fn a_joint_made_after_the_toolpaths_is_mated_at_once_and_follows_a_toolpath_edit() {
    let (mut doc, a, b) = two_panels();
    cut_out(&mut doc, a, 12.5);
    cut_out(&mut doc, b, 12.5);
    let reply = run(&mut doc, "joints.add", finger_spec(a, b));
    assert_eq!(reply["mates"], 1, "{reply}");
    // The toolpaths cut the parts as they were; making them again changes
    // the parts' boxes and numbers, and the mate follows.
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    let rows = mate_rows(&mut doc);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["state"], "solved", "{rows:?}");
    assert_fit(&verify(&mut doc, 0.1, 0.05), 0.1);
    // A mate the user asks for by hand cannot be a joint mate.
    let err = try_run(
        &mut doc,
        "assembly.editMate",
        json!({ "id": rows[0]["id"], "kind": "against" }),
    )
    .expect_err("refused");
    assert!(err.contains("joint mate"), "{err}");
}

#[test]
fn a_v_joint_notches_each_panel_until_it_clears_the_other_at_the_angle() {
    for (angle, notch) in [(120.0, 12.0 * 0.866_025_403_8), (60.0, 0.0)] {
        let (mut doc, a, b) = two_panels();
        let mut spec = finger_spec(a, b);
        spec["kind"] = json!("vJoint");
        spec["params"] = json!({ "angle": angle });
        let reply = run(&mut doc, "joints.add", spec);
        let resolved = &reply["outcome"]["resolved"];
        assert_eq!(number(&resolved["angle"]), angle);
        let count = resolved["count"].as_u64().expect("count");
        assert!(count >= 3 && count % 2 == 1, "{resolved}");
        if notch > 0.0 {
            // At 120 degrees each notch is the other panel's thickness
            // times the sine of the angle deep.
            let width = number(&resolved["width"]);
            let mid = 110.0 - 1.5 * width;
            let ra = rings(&doc, a);
            assert!(!inside_rings(&ra, [mid, 60.0 - notch + 0.05]));
            assert!(inside_rings(&ra, [mid, 60.0 - notch - 0.05]));
        }
        run(
            &mut doc,
            "joints.makeToolpaths",
            json!({ "tool": end_mill() }),
        );
        let joints = verify(&mut doc, 0.1, 0.05);
        assert!(!joints.is_empty(), "{angle}");
        for joint in &joints {
            assert_eq!(joint["source"], "exact", "{joint}");
            assert_eq!(joint["overlapDepth"], 0.0, "{angle}: {joint}");
            assert_ne!(joint["status"], "interference", "{joint}");
        }
        assert_fit(&joints, 0.1);
        assert_simple(&doc);
    }
}

/// A 200 x 100 base on the first sheet and a 40 x 120 leg on a second, 12
/// mm thick, with a twist lock of the leg's bottom end 40 mm in from the
/// base's bottom edge.
fn base_and_leg() -> (Document, u64, u64, Value) {
    let mut doc = job(400.0, 300.0, 12.0);
    let first = doc.active_sheet();
    doc.begin("sheet");
    let second = doc.add_sheet(Some("Leg".into()));
    doc.commit();
    let base = rect(&mut doc, first, 10.0, 10.0, 210.0, 110.0);
    let leg = rect(&mut doc, second, 10.0, 150.0, 50.0, 270.0);
    let spec = json!({
        "kind": "twistLock",
        "a": edge(leg, [10.0, 150.0], [50.0, 150.0]),
        "b": {
            "kind": "face",
            "edge": { "vector": base, "from": [10.0, 10.0], "to": [210.0, 10.0] },
            "offset": 40.0,
            "along": 100.0,
        },
        "params": {},
    });
    (doc, base, leg, spec)
}

#[test]
fn a_twist_lock_goes_through_across_the_line_and_locks_a_quarter_turn_on() {
    let (mut doc, _, leg, spec) = base_and_leg();
    let reply = run(&mut doc, "joints.add", spec);
    let resolved = &reply["outcome"]["resolved"];
    assert!((number(&resolved["width"]) - 12.0).abs() < 1e-9);
    assert!((number(&resolved["proud"]) - 12.0).abs() < 1e-9);
    let joint = doc.assembly().joint_set.joints[0].clone();
    assert_eq!(joint.vectors.len(), 1);
    assert_eq!(joint.vectors[0].role, super::Role::Hole);
    // The hole: the leg's section long across the line (40 + 0.2 plus the
    // reliefs), its round middle the neck's diagonal plus the clearance.
    let hole = doc.bbox_of(joint.vectors[0].id).expect("hole");
    let round = (2.0f64 * 144.0).sqrt() + 0.2;
    assert!(((hole.max.x - hole.min.x) - round).abs() < 1e-3, "{hole:?}");
    assert!(hole.max.y - hole.min.y >= 40.2 - 1e-6);
    assert!(((hole.min.y + hole.max.y) / 2.0 - 56.0).abs() < 1e-3);
    // The leg: a 12 mm neck in the middle of its end, 12.2 long, 11.9 up
    // from the end.
    let r = rings(&doc, leg);
    assert!(inside_rings(&r, [30.0, 150.0 + 5.0]), "the head");
    assert!(
        inside_rings(&r, [12.0, 150.0 + 5.0]),
        "the head is full width"
    );
    assert!(
        !inside_rings(&r, [12.0, 150.0 + 18.0]),
        "notched beside the neck"
    );
    assert!(inside_rings(&r, [30.0, 150.0 + 18.0]), "the neck");
    assert!(inside_rings(&r, [12.0, 150.0 + 25.0]), "full width past B");
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    let joints = verify(&mut doc, 0.1, 0.05);
    assert!(!joints.is_empty());
    for joint in &joints {
        assert_eq!(joint["source"], "exact", "{joint}");
        assert_eq!(joint["overlapDepth"], 0.0, "{joint}");
    }
    // The head and the shoulder stand the clearance off B's faces; the
    // neck's sides have the round middle's room to turn in.
    let turn_room = round / 2.0 - 6.0;
    let gaps: Vec<(f64, f64)> = joints
        .iter()
        .flat_map(|j| j["sides"].as_array().cloned().unwrap_or_default())
        .map(|s| (number(&s["min"]), number(&s["max"])))
        .collect();
    let near = |v: f64, to: f64| (v - to).abs() < 0.01;
    assert!(
        gaps.iter()
            .filter(|(min, max)| near(*min, 0.1) && near(*max, 0.1))
            .count()
            >= 2,
        "{gaps:?}"
    );
    for (min, max) in gaps {
        assert!(
            (near(min, 0.1) && near(max, 0.1)) || (min >= 0.09 && max <= turn_room + 0.01),
            "{min} {max}"
        );
    }
    assert_simple(&doc);
}

#[test]
fn a_twist_lock_says_when_the_part_is_too_narrow_to_lock() {
    let (mut doc, _, _, mut spec) = base_and_leg();
    spec["params"] = json!({ "width": 36.0 });
    let err = try_run(&mut doc, "joints.add", spec).expect_err("refused");
    assert!(err.contains("at least"), "{err}");
}

/// The least distance between the rings of two vectors, from each one's
/// corners to the other's sides.
fn ring_gap(doc: &Document, a: u64, b: u64) -> f64 {
    let (ra, rb) = (rings(doc, a), rings(doc, b));
    let side = |p: P2, ring: &[P2]| -> f64 {
        (0..ring.len())
            .map(|i| {
                let (q, r) = (ring[i], ring[(i + 1) % ring.len()]);
                let d = [r[0] - q[0], r[1] - q[1]];
                let len2 = (d[0] * d[0] + d[1] * d[1]).max(1e-12);
                let t = (((p[0] - q[0]) * d[0] + (p[1] - q[1]) * d[1]) / len2).clamp(0.0, 1.0);
                ((p[0] - q[0] - d[0] * t).powi(2) + (p[1] - q[1] - d[1] * t).powi(2)).sqrt()
            })
            .fold(f64::INFINITY, f64::min)
    };
    let one = |x: &[Vec<P2>], y: &[Vec<P2>]| {
        x.iter()
            .flatten()
            .map(|p| {
                y.iter()
                    .map(|ring| side(*p, ring))
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(f64::INFINITY, f64::min)
    };
    one(&ra, &rb).min(one(&rb, &ra))
}

#[test]
fn sinuous_and_spiral_hinges_bend_round_the_radius_asked_with_webs_kept() {
    for pattern in ["sinuous", "spiral"] {
        let mut doc = job(500.0, 400.0, 6.0);
        let sheet = doc.active_sheet();
        let panel = rect(&mut doc, sheet, 10.0, 10.0, 310.0, 210.0);
        if pattern == "spiral" {
            let mut settings = doc.assembly().joint_set.settings.clone();
            settings.tool_diameter = 3.0;
            run(
                &mut doc,
                "joints.setSettings",
                json!({ "settings": settings }),
            );
        }
        let bottom = edge(panel, [10.0, 10.0], [310.0, 10.0]);
        let reply = run(
            &mut doc,
            "joints.add",
            json!({
                "kind": "livingHinge",
                "a": bottom,
                "b": { "kind": "edge", "vector": panel, "from": [10.0, 10.0], "to": [310.0, 10.0] },
                "params": { "atA": 150.0, "radius": 30.0, "pattern": pattern },
            }),
        );
        let resolved = &reply["outcome"]["resolved"];
        assert_eq!(resolved["pattern"], pattern);
        let band = number(&resolved["width"]);
        // A quarter turn round 30 mm plus half the stock.
        assert!(
            (band - std::f64::consts::FRAC_PI_2 * 33.0).abs() < 1e-9,
            "{band}"
        );
        let joint = doc.assembly().joint_set.joints[0].clone();
        assert!(
            joint.vectors.len() >= 2,
            "{pattern}: {}",
            joint.vectors.len()
        );
        let web = number(&resolved["depth"]);
        for v in &joint.vectors {
            assert_eq!(v.role, super::Role::Hole);
            let b = doc.bbox_of(v.id).expect("slot");
            assert!(b.min.x >= 160.0 - band / 2.0 - 1e-6 && b.max.x <= 160.0 + band / 2.0 + 1e-6);
            assert!(b.min.y > 10.0 && b.max.y < 210.0);
        }
        for (i, x) in joint.vectors.iter().enumerate() {
            for y in &joint.vectors[i + 1..] {
                let gap = ring_gap(&doc, x.id, y.id);
                assert!(gap >= web - 0.05, "{pattern}: slots {gap} apart, web {web}");
            }
        }
        assert_simple(&doc);
        let diameter = doc.assembly().joint_set.settings.tool_diameter;
        let tool = serde_json::to_value(Tool {
            name: "end mill".into(),
            tool_type: ToolGeometry::EndMill { diameter },
            ..Tool::default()
        })
        .expect("tool");
        let made = run(&mut doc, "joints.makeToolpaths", json!({ "tool": tool }));
        assert!(!made["ids"].as_array().expect("ids").is_empty());
    }
}

#[test]
fn a_corner_dovetail_locks_with_smooth_sloped_socket_walls_that_fit() {
    let (mut doc, a, b) = two_panels();
    let mut spec = finger_spec(a, b);
    spec["kind"] = json!("cornerDovetail");
    let reply = run(&mut doc, "joints.add", spec);
    let resolved = &reply["outcome"]["resolved"];
    // Two tails on 100 mm of 12 mm stock.
    assert_eq!(resolved["count"], 2);
    let joint = doc.assembly().joint_set.joints[0].clone();
    // Two sloped walls per socket, each a tapered wall at the tail angle
    // right through the second part.
    assert_eq!(joint.vectors.len(), 4);
    for v in &joint.vectors {
        assert_eq!(
            v.role,
            super::Role::Taper {
                depth: 12.0,
                angle: 14.0
            }
        );
    }
    let pitch = 50.0;
    let flare = 12.0 * 14f64.to_radians().tan();
    let side = 0.1 / 14f64.to_radians().cos();
    // The tail is wider at its tip than the socket's foot: it locks.
    let ra = rings(&doc, a);
    let tip = 25.0 + pitch / 4.0 + flare / 2.0 - 0.05;
    assert!(inside_rings(&ra, [110.0 - tip, 59.9]));
    assert!(pitch / 4.0 + flare / 2.0 > pitch / 4.0 - flare / 2.0 + side);
    let made = run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    assert!(!made["ids"].as_array().expect("ids").is_empty());
    let tapered = doc
        .toolpaths()
        .iter()
        .find(|t| t.name.contains("tapered"))
        .cloned()
        .expect("a tapered wall toolpath");
    assert_eq!(tapered.slopes.len(), 4);
    let joints = verify(&mut doc, 0.1, 0.05);
    assert!(!joints.is_empty());
    for joint in &joints {
        assert_eq!(joint["source"], "exact", "{joint}");
        assert_eq!(joint["overlapDepth"], 0.0, "{joint}");
        assert_ne!(joint["status"], "interference", "{joint}");
    }
    let sides: Vec<Value> = joints
        .iter()
        .flat_map(|j| j["sides"].as_array().cloned().unwrap_or_default())
        .collect();
    // Both sloped sides of the tails fit, standing the clearance off
    // across their faces (the reliefs at the back corners stand further).
    let fits: Vec<&Value> = sides.iter().filter(|s| s["fit"] == true).collect();
    assert_eq!(fits.len(), 2, "{sides:?}");
    for side in &fits {
        assert!((number(&side["min"]) - 0.1).abs() < 0.01, "{side}");
        // Leaning at the tail angle, to 0.1 degree, across the corner.
        let normal: Vec<f64> = side["normal"]
            .as_array()
            .expect("normal")
            .iter()
            .map(number)
            .collect();
        let lean = 14f64.to_radians().sin();
        let within = 0.1f64.to_radians().sin();
        assert!(
            normal.iter().any(|v| (v.abs() - lean).abs() < within),
            "{normal:?}"
        );
    }
    // The socket backs stand the clearance off the first part's inside
    // face (the reliefs behind them further), and nothing is loose.
    assert!(
        sides
            .iter()
            .any(|s| s["fit"] == false && (number(&s["min"]) - 0.1).abs() < 0.01),
        "{sides:?}"
    );
    assert!(
        sides.iter().all(|s| number(&s["min"]) > -0.001),
        "{sides:?}"
    );
    // The four sloped walls, each over 40 square mm of the tails' faces,
    // stand 0.1 mm off to 0.01 mm all the way up.
    let area = |tri: &[f64]| {
        tri.chunks_exact(9)
            .map(|t| {
                let u = [t[3] - t[0], t[4] - t[1], t[5] - t[2]];
                let v = [t[6] - t[0], t[7] - t[1], t[8] - t[2]];
                let c = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() / 2.0
            })
            .sum::<f64>()
    };
    let walls: Vec<(f64, f64, f64)> = joints
        .iter()
        .flat_map(|j| j["patches"].as_array().cloned().unwrap_or_default())
        .map(|p| {
            let tri: Vec<f64> = p["a"].as_array().expect("a").iter().map(number).collect();
            (area(&tri), number(&p["min"]), number(&p["max"]))
        })
        .filter(|(a, min, _)| *a > 40.0 && *min > 0.05)
        .collect();
    assert!(walls.len() >= 4, "{walls:?}");
    for (_, min, max) in &walls {
        assert!(
            (min - 0.1).abs() < 0.01 && (max - 0.1).abs() < 0.01,
            "{walls:?}"
        );
    }
    assert_simple(&doc);
}

#[test]
fn a_tenon_listed_first_is_measured_as_a_fit_between_its_walls() {
    // One sheet, the tenon's part first: its faces face out of it, so the
    // wall across from each lies behind it. Verify Fit still pairs them into
    // one joint with its four sides 0.1 mm off, not one resting side each.
    let mut doc = job(300.0, 200.0, 18.0);
    let sheet = doc.active_sheet();
    let a = rect(&mut doc, sheet, 10.0, 10.0, 140.0, 90.0);
    let b = rect(&mut doc, sheet, 160.0, 110.0, 290.0, 190.0);
    let spec = json!({
        "kind": "mortiseTenon",
        "a": edge(a, [140.0, 90.0], [10.0, 90.0]),
        "b": {
            "kind": "face",
            "edge": { "vector": b, "from": [160.0, 110.0], "to": [290.0, 110.0] },
            "offset": 40.0,
            "along": 65.0,
        },
        "params": {},
    });
    let reply = run(&mut doc, "joints.add", spec);
    let small = serde_json::to_value(Tool {
        name: "3 mm".into(),
        tool_type: ToolGeometry::EndMill { diameter: 3.0 },
        ..Tool::default()
    })
    .expect("tool");
    run(&mut doc, "joints.makeToolpaths", json!({ "tool": small }));
    run(&mut doc, "joints.assemble", json!({ "id": reply["id"] }));
    let joints = verify(&mut doc, 0.1, 0.02);
    assert_eq!(joints.len(), 1, "{joints:?}");
    assert_eq!(joints[0]["status"], "ok", "{joints:?}");
    let gaps = fit_gaps(&joints);
    assert_eq!(gaps.len(), 4, "{joints:?}");
    // The walls stand 0.1 off; the relief round the tenon's root is wider.
    for (min, _) in gaps {
        assert!((min - 0.1).abs() < 0.01, "{min}");
    }
}

/// A shipped Tool Database end mill, as the panel picks it.
fn db_end_mill(name: &str) -> Value {
    let db = crate::cam::tooldb::ToolDatabase::shipped_default().expect("database");
    let tool = db
        .tools()
        .find(|t| t.name == name)
        .expect("the end mill")
        .clone();
    serde_json::to_value(tool).expect("tool")
}

fn db_end_mill_3() -> Value {
    db_end_mill("End Mill 3 mm")
}

/// Two 140 x 80 panels of 18 mm stock on one 300 x 300 sheet, 40 mm
/// apart, the joints laid out for a 3 mm end mill: A's top edge and B's
/// bottom edge.
fn panels_for_3_mm(kind: &str) -> (Document, Value) {
    panels_for(kind, 3.0)
}

fn panels_for(kind: &str, tool: f64) -> (Document, Value) {
    let mut doc = job(300.0, 300.0, 18.0);
    let sheet = doc.active_sheet();
    let mut settings = doc.assembly().joint_set.settings.clone();
    settings.tool_diameter = tool;
    run(
        &mut doc,
        "joints.setSettings",
        json!({ "settings": settings }),
    );
    let a = rect(&mut doc, sheet, 10.0, 10.0, 150.0, 90.0);
    let b = rect(&mut doc, sheet, 10.0, 130.0, 150.0, 210.0);
    let spec = json!({
        "kind": kind,
        "a": edge(a, [150.0, 90.0], [10.0, 90.0]),
        "b": { "kind": "edge", "vector": b, "from": [10.0, 130.0], "to": [150.0, 130.0] },
        "params": {},
    });
    (doc, spec)
}

#[test]
fn a_flat_dovetail_reads_as_its_seam_with_both_slanted_sides_measured() {
    // As testD cut it: 18 mm stock, Press, Verify Fit at 0.1 +/- 0.02. The
    // sides of each tail lean 28 degrees apart; they are the two walls of
    // one fit, not one sided contacts of joints of their own, also where a
    // big tool's reliefs leave only a short stretch of each side.
    for (diameter, name) in [(3.0, "End Mill 3 mm"), (6.35, "End Mill 1/4 inch")] {
        let (mut doc, spec) = panels_for("dovetail", diameter);
        run(&mut doc, "joints.add", spec);
        run(
            &mut doc,
            "joints.makeToolpaths",
            json!({ "tool": db_end_mill(name) }),
        );
        let joints = verify(&mut doc, 0.1, 0.02);
        assert_eq!(joints.len(), 1, "{name}: {joints:?}");
        let joint = &joints[0];
        assert_eq!(joint["status"], "ok", "{joint}");
        let slanted: Vec<Value> = joint["sides"]
            .as_array()
            .expect("sides")
            .iter()
            .filter(|s| number(&s["normal"][0]).abs() > 0.9)
            .cloned()
            .collect();
        assert_eq!(slanted.len(), 2, "{joint}");
        for side in &slanted {
            assert_eq!(side["fit"], true, "{side}");
            assert!((number(&side["min"]) - 0.1).abs() < 0.01, "{side}");
            assert!((number(&side["max"]) - 0.1).abs() < 0.01, "{side}");
        }
    }
}

#[test]
fn two_tabs_on_one_shoulder_are_a_joint_each_with_the_tab_part_first() {
    // The tab part is listed first and its shoulder is one face across
    // both tabs; each tab in its slot is still a joint of its own.
    let mut doc = job(400.0, 300.0, 18.0);
    let sheet = doc.active_sheet();
    let mut settings = doc.assembly().joint_set.settings.clone();
    settings.tool_diameter = 3.0;
    run(
        &mut doc,
        "joints.setSettings",
        json!({ "settings": settings }),
    );
    let a = rect(&mut doc, sheet, 10.0, 10.0, 140.0, 90.0);
    let b = rect(&mut doc, sheet, 10.0, 130.0, 160.0, 210.0);
    let spec = json!({
        "kind": "tabSlot",
        "a": edge(a, [140.0, 90.0], [10.0, 90.0]),
        "b": {
            "kind": "face",
            "edge": { "vector": b, "from": [10.0, 130.0], "to": [160.0, 130.0] },
            "offset": 40.0,
            "along": 75.0,
        },
        "params": {},
    });
    let reply = run(&mut doc, "joints.add", spec);
    assert_eq!(reply["outcome"]["resolved"]["count"], 2);
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": db_end_mill_3() }),
    );
    let joints = verify(&mut doc, 0.1, 0.02);
    assert_eq!(joints.len(), 2, "{joints:?}");
    for joint in &joints {
        assert_eq!(joint["status"], "ok", "{joint}");
        let fits = fit_gaps(std::slice::from_ref(joint));
        assert_eq!(fits.len(), 4, "four walls round each tab: {joint}");
        for (min, max) in fits {
            assert!(
                (min - 0.1).abs() < 0.01 && (max - 0.1).abs() < 0.01,
                "{min} {max}"
            );
        }
    }
}

#[test]
fn a_butterfly_key_is_seated_in_its_recess_with_the_clearance_all_round() {
    let (mut doc, spec) = panels_for_3_mm("butterfly");
    let id = run(&mut doc, "joints.add", spec)["id"].clone();
    let made = run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": db_end_mill_3() }),
    );
    // The second panel against the first, and the key in its recess.
    assert_eq!(made["mates"], 2, "{made}");
    let rows = mate_rows(&mut doc);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows
        .iter()
        .all(|r| r["joint"] == id && r["state"] == "solved"));
    let joints = verify(&mut doc, 0.1, 0.02);
    // The key against each panel's half of the recess, and the seam.
    assert_eq!(joints.len(), 3, "{joints:?}");
    for joint in &joints {
        assert_eq!(joint["source"], "exact", "{joint}");
        assert_eq!(joint["overlapDepth"], 0.0, "{joint}");
        assert_eq!(joint["status"], "ok", "{joint}");
    }
    // Round the key: its walls stand the clearance off the recess's.
    let gaps = fit_gaps(&joints);
    assert!(gaps.len() >= 4, "{joints:?}");
    for (min, _) in gaps {
        assert!((min - 0.1).abs() < 0.01, "{min}");
    }
    // Moved by hand, the key is seated from where it is drawn now.
    let key = doc.assembly().joint_set.joints[0].vectors[2].id;
    doc.begin("move");
    doc.transform_entities(
        &[key],
        &crate::geom::Mat3::translate(crate::geom::Vec2::new(0.0, 30.0)),
    );
    doc.commit();
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": db_end_mill_3() }),
    );
    let again = verify(&mut doc, 0.1, 0.02);
    assert!(again.iter().all(|j| j["status"] == "ok"), "{again:?}");
    // Remove Joint takes both mates.
    run(&mut doc, "joints.remove", json!({ "id": id }));
    assert!(doc.assembly().piece_mates.is_empty());
}

#[test]
fn a_half_lap_on_long_edges_is_refused_by_the_preview_in_plain_words() {
    let (mut doc, spec) = panels_for_3_mm("halfLap");
    // The panel previews before Create: the preview refuses already.
    let error = try_run(&mut doc, "joints.preview", spec).expect_err("refused");
    assert!(error.contains("joins the ends of two parts"), "{error}");
}

#[test]
fn a_part_past_the_material_edge_is_named_in_the_notes() {
    // The second panel runs 10 mm past the top of a 300 x 200 sheet: no
    // toolpath can cut it free, so the Assembly View has no piece for it.
    let mut doc = job(300.0, 200.0, 18.0);
    let sheet = doc.active_sheet();
    let a = rect(&mut doc, sheet, 10.0, 10.0, 150.0, 90.0);
    let b = rect(&mut doc, sheet, 10.0, 130.0, 150.0, 210.0);
    let reply = run(
        &mut doc,
        "joints.add",
        json!({
            "kind": "finger",
            "a": edge(a, [150.0, 90.0], [10.0, 90.0]),
            "b": { "kind": "edge", "vector": b, "from": [10.0, 130.0], "to": [150.0, 130.0] },
        }),
    );
    let notes = reply["outcome"]["notes"].as_array().expect("notes");
    assert!(
        notes.iter().any(|n| n
            .as_str()
            .is_some_and(|n| n.starts_with("the second part runs past the edge of the material"))),
        "{notes:?}"
    );
    assert!(!notes
        .iter()
        .any(|n| n.as_str().is_some_and(|n| n.starts_with("the first part"))));
}

#[test]
fn a_kerf_tenon_is_split_by_a_kerf_and_its_wedge_sits_where_it_binds() {
    let (mut doc, base, shelf) = base_and_shelf();
    let reply = run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "kerfTenon", json!({})),
    );
    let resolved = &reply["outcome"]["resolved"];
    let width = number(&resolved["width"]);
    let depth = number(&resolved["depth"]);
    assert!((depth - 9.0).abs() < 1e-9, "three quarters of 12: {depth}");
    assert!(reply["outcome"]["notes"][0]
        .as_str()
        .is_some_and(|n| n.contains("drive it")));
    // The tenon (the shelf's middle, x 110) stands 12 mm below its edge
    // (y 150), flush with the base's far face; the kerf runs 9 mm up
    // from its end, 6.35 + 0.2 wide.
    let r = rings(&doc, shelf);
    assert!(inside_rings(&r, [110.0 - width / 4.0, 150.0 - 11.5]));
    assert!(!inside_rings(&r, [110.0, 150.0 - 11.5]), "the kerf");
    assert!(
        !inside_rings(&r, [110.0 + 3.2, 150.0 - 6.0]),
        "the kerf's side"
    );
    assert!(inside_rings(&r, [110.0 + 3.35, 150.0 - 6.0]));
    assert!(inside_rings(&r, [110.0, 150.0 - 2.0]), "above the kerf");
    let joint = doc.assembly().joint_set.joints[0].clone();
    let wedge = joint
        .vectors
        .iter()
        .find(|v| matches!(v.role, super::Role::Part { .. }))
        .expect("the wedge");
    assert!(wedge.seat.is_some());
    let wb = doc.bbox_of(wedge.id).expect("wedge");
    assert!(!wb.intersects(doc.bbox_of(shelf).expect("shelf")));
    assert!((wb.max.y - wb.min.y - depth).abs() < 1e-6);
    // Cut, assembled: the tenon fits its mortise and the wedge rests in the
    // kerf's mouth without running into anything.
    let made = run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    assert_eq!(made["mates"], 2, "{made}");
    let joints = verify(&mut doc, 0.1, 0.05);
    for joint in &joints {
        assert_eq!(joint["source"], "exact", "{joint}");
        assert_eq!(joint["overlapDepth"], 0.0, "{joint}");
        assert_ne!(joint["status"], "interference", "{joint}");
    }
    let tenon = joints
        .iter()
        .find(|j| fit_gaps(std::slice::from_ref(j)).len() >= 4)
        .expect("the tenon in its mortise");
    for (min, _) in fit_gaps(std::slice::from_ref(tenon)) {
        assert!((min - 0.1).abs() < 0.01, "{min}");
    }
    assert!(joints.len() >= 2, "the wedge touches the tenon: {joints:?}");
}

#[test]
fn a_butterfly_key_shows_in_the_assembly_at_its_own_thickness() {
    let (mut doc, spec) = panels_for_3_mm("butterfly");
    let reply = run(&mut doc, "joints.add", spec);
    let depth = number(&reply["outcome"]["resolved"]["depth"]);
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": db_end_mill_3() }),
    );
    // The key is cut out with the panel's sheet but is as thick as the
    // stock it is made from (the recess and 1 mm to sand), not the sheet.
    let listed = run(&mut doc, "assembly.pieces", json!({}));
    let rows = listed["pieces"].as_array().cloned().unwrap_or_default();
    let thickness = |row: &Value| number(&row["size"][2]);
    let key = rows
        .iter()
        .find(|r| r["vectorName"].as_str().is_some_and(|n| n.contains("key")))
        .unwrap_or_else(|| panic!("the key piece: {rows:?}"));
    assert!((thickness(key) - (depth + 1.0)).abs() < 1e-6, "{key}");
    let sheet = doc.job_setup().thickness;
    assert!(rows
        .iter()
        .filter(|r| r != &key)
        .all(|r| (thickness(r) - sheet).abs() < 1e-6));
    // Seated on the recess floors it stands the 1 mm proud and fits.
    let joints = verify(&mut doc, 0.1, 0.02);
    assert!(joints.iter().all(|j| j["status"] == "ok"), "{joints:?}");
}

#[test]
fn a_wedged_tenons_wedge_is_seated_in_its_hole() {
    let (mut doc, base, shelf) = base_and_shelf();
    run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "wedgedTenon", json!({})),
    );
    let joint = doc.assembly().joint_set.joints[0].clone();
    assert!(joint.vectors[2].seat.is_some(), "the wedge has a seat");
    // Cut, assembled: the shelf against the base and the wedge down
    // through the hole in the tenon, bearing on the base without running
    // into anything.
    let made = run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    assert_eq!(made["mates"], 2, "{made}");
    let joints = verify(&mut doc, 0.1, 0.05);
    assert!(joints.len() >= 2, "{joints:?}");
    for joint in &joints {
        assert_eq!(joint["source"], "exact", "{joint}");
        assert_eq!(joint["overlapDepth"], 0.0, "{joint}");
        assert_ne!(joint["status"], "interference", "{joint}");
    }
}

/// A knock-down table drawn flat, 18 mm: a 360 x 240 top, two 300 x 240
/// legs drawn alike, a stretcher and a shelf. Each leg's tabs stand in
/// slots 30 mm in from the top's end; the stretcher's wedged tenons go
/// through both legs 100 mm down, in the middle of their width; the shelf sits in a dado in each leg 60
/// mm up from its foot. The legs' inside faces are 264 mm apart by the
/// top's slots, so the stretcher is 264 long and the shelf 276 (two dados
/// 6 deep).
fn knock_down_table() -> Document {
    let mut doc = job(1200.0, 1000.0, 18.0);
    let sheet = doc.active_sheet();
    let top = rect(&mut doc, sheet, 10.0, 10.0, 370.0, 250.0);
    let leg1 = rect(&mut doc, sheet, 440.0, 10.0, 740.0, 250.0);
    let leg2 = rect(&mut doc, sheet, 440.0, 330.0, 740.0, 570.0);
    let stretcher = rect(&mut doc, sheet, 850.0, 60.0, 930.0, 324.0);
    let shelf = rect(&mut doc, sheet, 10.0, 330.0, 160.0, 606.0);
    let face = |vector: u64, from: P2, to: P2, offset: f64, along: f64| json!({ "kind": "face", "edge": edge(vector, from, to), "offset": offset, "along": along });
    let specs = [
        (
            "tabSlot",
            edge(leg1, [440.0, 250.0], [440.0, 10.0]),
            face(top, [10.0, 250.0], [10.0, 10.0], 30.0, 120.0),
        ),
        (
            "tabSlot",
            edge(leg2, [440.0, 570.0], [440.0, 330.0]),
            face(top, [370.0, 10.0], [370.0, 250.0], 30.0, 120.0),
        ),
        (
            "wedgedTenon",
            edge(stretcher, [850.0, 60.0], [930.0, 60.0]),
            face(leg1, [440.0, 10.0], [740.0, 10.0], 111.0, 100.0),
        ),
        // Picked from leg 2's lower edge as drawn, as leg 1 was: the legs
        // face each other, so from this edge the stretcher is turned round.
        (
            "wedgedTenon",
            edge(stretcher, [930.0, 324.0], [850.0, 324.0]),
            face(leg2, [440.0, 330.0], [740.0, 330.0], 111.0, 100.0),
        ),
        (
            "dado",
            edge(shelf, [10.0, 330.0], [160.0, 330.0]),
            face(leg1, [740.0, 10.0], [740.0, 250.0], 60.0, 120.0),
        ),
        (
            "dado",
            edge(shelf, [160.0, 606.0], [10.0, 606.0]),
            face(leg2, [740.0, 330.0], [740.0, 570.0], 60.0, 120.0),
        ),
    ];
    for (kind, a, b) in specs {
        run(
            &mut doc,
            "joints.add",
            json!({ "kind": kind, "a": a, "b": b, "params": {} }),
        );
    }
    doc
}

/// The table goes together: the legs face each other, so the stretcher
/// and the shelf, which stand on a leg's top face, are between them; the
/// stretcher's second joint turns it the way the first one holds it; and
/// every mate holds with every joint at its clearance.
#[test]
fn a_knock_down_table_goes_together_with_its_legs_facing_in() {
    let mut doc = knock_down_table();
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    let rows = mate_rows(&mut doc);
    assert!(rows.len() >= 6, "{rows:?}");
    for row in &rows {
        assert_eq!(row["state"], "solved", "{rows:#?}");
    }
    // Press fit, 0.1 a side, read on every joint.
    let joints = verify(&mut doc, 0.1, 0.05);
    assert!(joints.len() >= 6, "{joints:?}");
    for joint in &joints {
        assert_eq!(joint["status"], "ok", "{joint}");
    }
    assert_fit(&joints, 0.1);
}

/// A tenon grown into a part laid out next to its own part says so; the
/// same joint on parts laid out apart says nothing of the kind.
#[test]
fn a_tenon_grown_into_the_next_part_says_to_move_them_apart() {
    let notes = |doc: &mut Document, base: u64, shelf: u64| -> Vec<String> {
        let reply = run(
            doc,
            "joints.add",
            tab_spec(base, shelf, "wedgedTenon", json!({})),
        );
        reply["outcome"]["notes"]
            .as_array()
            .map(|n| {
                n.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    let runs_into = |notes: &[String]| notes.iter().any(|n| n.contains("runs into another part"));
    // Base and shelf on one sheet, a third part just under the shelf's
    // bottom edge, where its tenon grows.
    let mut doc = job(400.0, 300.0, 12.0);
    let sheet = doc.active_sheet();
    let base = rect(&mut doc, sheet, 10.0, 10.0, 210.0, 110.0);
    let shelf = rect(&mut doc, sheet, 10.0, 150.0, 210.0, 210.0);
    rect(&mut doc, sheet, 250.0, 10.0, 390.0, 140.0);
    rect(&mut doc, sheet, 60.0, 125.0, 160.0, 140.0);
    let found = notes(&mut doc, base, shelf);
    assert!(runs_into(&found), "{found:?}");
    // Laid out apart (the shelf on a sheet of its own): no such note.
    let (mut doc, base, shelf) = base_and_shelf();
    let found = notes(&mut doc, base, shelf);
    assert!(!runs_into(&found), "{found:?}");
}

/// A leg laid out 10 mm from the top, its tabs pointing at it: the waste
/// left between the two tabs, the top's cut and the leg's cut (closer than
/// two tools) is not a piece, nor are the slugs of the slots.
#[test]
fn the_waste_between_parts_laid_out_close_is_not_a_piece() {
    let mut doc = job(800.0, 400.0, 18.0);
    let sheet = doc.active_sheet();
    let top = rect(&mut doc, sheet, 10.0, 10.0, 370.0, 250.0);
    let leg = rect(&mut doc, sheet, 398.0, 10.0, 698.0, 250.0);
    run(
        &mut doc,
        "joints.add",
        json!({
            "kind": "tabSlot",
            "a": edge(leg, [398.0, 250.0], [398.0, 10.0]),
            "b": {
                "kind": "face",
                "edge": edge(top, [10.0, 250.0], [10.0, 10.0]),
                "offset": 30.0,
                "along": 120.0,
            },
            "params": {},
        }),
    );
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    let pieces = run(&mut doc, "assembly.pieces", json!({}));
    assert_eq!(
        pieces["pieces"].as_array().map(Vec::len),
        Some(2),
        "{pieces}"
    );
}

/// In an inch job the notes and refusals give their sizes in inches.
#[test]
fn an_inch_job_reads_its_joint_notes_in_inches() {
    let (mut doc, base, shelf) = base_and_shelf();
    let mut setup = *doc.job();
    setup.units = crate::doc::Units::Inch;
    doc.begin("units");
    doc.set_job(setup).expect("job");
    doc.commit();
    let reply = run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "tabSlot", json!({})),
    );
    let notes = reply["outcome"]["notes"].to_string();
    assert!(notes.contains("2 tabs 1.5748 inch wide"), "{notes}");
    assert!(!notes.contains(" mm"), "{notes}");
    let refused = try_run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "tabSlot", json!({ "count": 9, "width": 40.0 })),
    )
    .expect_err("too many tabs");
    assert!(refused.contains("1.5748 inch"), "{refused}");
    assert!(super::in_job_units("M6 bolts 30 mm long", true) == "M6 bolts 1.1811 inch long");
    assert!(super::in_job_units("a 5 mmHg gap", true) == "a 5 mmHg gap");
}

// Review 033 row 6: a coupon's parts stand apart by more than the tool
// whatever the joint reaches, so they cut as two pieces.
#[test]
fn a_test_coupon_leaves_more_than_the_tool_between_its_parts() {
    for kind in [
        "dovetail",
        "puzzle",
        "wedgedTenon",
        "tabSlot",
        "mortiseTenon",
        "finger",
    ] {
        let mut doc = job(400.0, 400.0, 12.0);
        let reply = run(&mut doc, "joints.coupon", json!({ "kind": kind }));
        let parts: Vec<u64> = reply["parts"]
            .as_array()
            .expect("parts")
            .iter()
            .filter_map(Value::as_u64)
            .collect();
        let (a, b) = (
            doc.bbox_of(parts[0]).expect("a"),
            doc.bbox_of(parts[1]).expect("b"),
        );
        let apart = (b.min.y - a.max.y).max(a.min.y - b.max.y);
        assert!(apart >= 6.35 + 3.0 - 1e-9, "{kind}: {apart}");
        // One step: undo takes the coupon away.
        doc.undo();
        assert!(doc.entity(parts[0]).is_err(), "{kind}");
    }
}

// Review 033 row 13: parts drawn at different angles read the same fit
// gaps as axis aligned ones.
#[test]
fn a_finger_joint_between_parts_drawn_at_angles_reads_its_true_gaps() {
    let mut doc = job(400.0, 400.0, 12.0);
    let first = doc.active_sheet();
    doc.begin("sheet");
    let second = doc.add_sheet(Some("Second".into()));
    doc.commit();
    let turn = 30f64.to_radians();
    let (c, s) = (turn.cos(), turn.sin());
    let at = |x: f64, y: f64| [150.0 + x * c - y * s, 150.0 + x * s + y * c];
    let a = polygon(
        &mut doc,
        first,
        &[at(0.0, 0.0), at(100.0, 0.0), at(100.0, 50.0), at(0.0, 50.0)],
    );
    let b = rect(&mut doc, second, 10.0, 100.0, 110.0, 150.0);
    let spec = json!({
        "kind": "finger",
        "a": edge(a, at(100.0, 50.0), at(0.0, 50.0)),
        "b": { "kind": "edge", "vector": b, "from": [10.0, 100.0], "to": [110.0, 100.0] },
        "params": {},
    });
    let id = run(&mut doc, "joints.add", spec)["id"].clone();
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    run(&mut doc, "joints.assemble", json!({ "id": id }));
    let joints = verify(&mut doc, 0.1, 0.05);
    assert_fit(&joints, 0.1);
}

// Review 033 row 14: toolpaths made with a bigger tool than the reliefs
// were sized for remake the reliefs for it, so the corners clear.
#[test]
fn joint_toolpaths_with_another_tool_remake_the_reliefs_for_it() {
    let (mut doc, spec) = panels_for("finger", 3.0);
    let id = run(&mut doc, "joints.add", spec)["id"].clone();
    run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    assert_eq!(doc.assembly().joint_set.settings.tool_diameter, 6.35);
    run(&mut doc, "joints.assemble", json!({ "id": id }));
    let joints = verify(&mut doc, 0.1, 0.05);
    assert!(!joints.is_empty());
    for joint in &joints {
        assert_eq!(joint["overlapDepth"], 0.0, "{joint}");
        assert_ne!(joint["status"], "interference", "{joint}");
    }
    // One step: undo puts the 3 mm reliefs back with no toolpaths.
    assert_eq!(doc.journal().undo_label(), Some("Joint Toolpaths"));
    doc.undo();
    assert_eq!(doc.assembly().joint_set.settings.tool_diameter, 3.0);
    assert!(doc.toolpaths().is_empty());
}

#[test]
fn cam_depths_follow_the_fittings_tables() {
    assert!((super::kinds::cam_depth(16.0) - 12.5).abs() < 1e-12);
    assert!((super::kinds::cam_depth(18.0) - 13.5).abs() < 1e-12);
    assert!((super::kinds::cam_depth(19.0) - 14.5).abs() < 1e-12);
    // Never closer than 3 mm to the far face.
    assert!((super::kinds::cam_depth(12.0) - 9.0).abs() < 1e-12);
}

#[test]
fn cam_locks_bore_the_first_part_and_drill_the_second_for_the_bolts() {
    let (mut doc, base, shelf) = base_and_shelf();
    let shelf_before = path(&doc, shelf);
    let base_before = path(&doc, base);
    let reply = run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "camLock", json!({})),
    );
    let resolved = &reply["outcome"]["resolved"];
    // A 200 mm edge takes two fittings 37 mm in from its ends; in 12 mm
    // stock the cam stops 3 mm short of the far face; 34 mm in from the
    // edge.
    assert_eq!(resolved["count"], 2);
    assert!((number(&resolved["depth"]) - 9.0).abs() < 1e-9);
    assert!((number(&resolved["length"]) - 34.0).abs() < 1e-9);
    assert!((number(&resolved["inset"]) - 37.0).abs() < 1e-9);
    assert_eq!(path(&doc, shelf), shelf_before, "the outlines stay");
    assert_eq!(path(&doc, base), base_before);
    let joint = doc.assembly().joint_set.joints[0].clone();
    let shelf_sheet = doc.entity(shelf).expect("shelf").sheet;
    let mut cams = Vec::new();
    let mut bolts = Vec::new();
    let mut marks = 0;
    for v in &joint.vectors {
        let b = doc.bbox_of(v.id).expect("vector");
        let centre = [(b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0];
        match v.role {
            super::Role::Pocket { depth } if doc.entity(v.id).expect("v").sheet == shelf_sheet => {
                cams.push((centre, b.max.x - b.min.x, depth));
            }
            super::Role::Pocket { depth } => bolts.push((centre, b.max.x - b.min.x, depth)),
            super::Role::Mark => marks += 1,
            other => panic!("{other:?}"),
        }
    }
    // Cams in the shelf's face 34 mm up from its bottom edge (y 150), 15
    // mm plus the clearance each side across, 9 mm deep.
    assert_eq!(cams.len(), 2);
    for (k, x) in [47.0, 173.0].into_iter().enumerate() {
        assert!(
            (cams[k].0[0] - x).abs() < 1e-6 && (cams[k].0[1] - 184.0).abs() < 1e-6,
            "{cams:?}"
        );
        assert!((cams[k].1 - 15.2).abs() < 1e-3, "{cams:?}");
        assert!((cams[k].2 - 9.0).abs() < 1e-12);
    }
    // Bolt holes in the base on the shelf's middle (40 + 6 mm in), under
    // each cam (the shelf's edge runs the other way along the base's):
    // 8 mm for an expanding dowel, the 6.35 mm tool being too big for a 5
    // mm pilot; 3 mm short of the base's far face.
    assert_eq!(bolts.len(), 2);
    for (k, x) in [173.0, 47.0].into_iter().enumerate() {
        assert!(
            (bolts[k].0[0] - x).abs() < 1e-6 && (bolts[k].0[1] - 56.0).abs() < 1e-6,
            "{bolts:?}"
        );
        assert!((bolts[k].1 - 8.0).abs() < 1e-3, "{bolts:?}");
        assert!((bolts[k].2 - 9.0).abs() < 1e-12);
    }
    assert_eq!(marks, 2, "one hand drilling mark per edge bore");
    assert!(reply["outcome"]["notes"][0]
        .as_str()
        .is_some_and(|n| n.contains("drill the 8 mm bores into the first part's edge by hand")));
}

#[test]
fn screws_mark_the_clearance_holes_and_the_pilots() {
    let (mut doc, base, shelf) = base_and_shelf();
    let base_before = path(&doc, base);
    let reply = run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "screws", json!({})),
    );
    let resolved = &reply["outcome"]["resolved"];
    // 200 mm: two screws 50 mm in from the ends.
    assert_eq!(resolved["count"], 2);
    assert!((number(&resolved["inset"]) - 50.0).abs() < 1e-9);
    assert_eq!(path(&doc, base), base_before);
    let joint = doc.assembly().joint_set.joints[0].clone();
    assert!(joint.vectors.iter().all(|v| v.role == super::Role::Mark));
    assert_eq!(joint.vectors.len(), 4);
    let first = doc.bbox_of(joint.vectors[0].id).expect("mark");
    assert!(((first.min.x + first.max.x) / 2.0 - 160.0).abs() < 1e-6);
    assert!(((first.min.y + first.max.y) / 2.0 - 56.0).abs() < 1e-6);
    assert!(((first.max.x - first.min.x) - 4.5).abs() < 1e-3);
    // With five screws asked for.
    let id = reply["id"].clone();
    let mut spec = tab_spec(base, shelf, "screws", json!({ "count": 5 }));
    spec["id"] = id;
    run(&mut doc, "joints.update", spec);
    let joint = doc.assembly().joint_set.joints[0].clone();
    assert_eq!(joint.vectors.len(), 10);
}

#[test]
fn a_groove_is_a_dado_by_another_name() {
    let (mut doc, base, shelf) = base_and_shelf();
    let reply = run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "groove", json!({})),
    );
    // A third of the base's 12 mm.
    assert!((number(&reply["outcome"]["resolved"]["depth"]) - 4.0).abs() < 1e-9);
    let joint = doc.assembly().joint_set.joints[0].clone();
    assert_eq!(joint.vectors.len(), 1);
    assert_eq!(joint.vectors[0].role, super::Role::Pocket { depth: 4.0 });
    let name = doc
        .entity(joint.vectors[0].id)
        .expect("v")
        .name
        .clone()
        .unwrap_or_default();
    assert!(name.starts_with("Groove"), "{name}");
}

#[test]
fn joint_toolpaths_cut_each_sheet_through_its_own_material() {
    // The sheet on show is 20 mm thick and zeroed on the bed; the other
    // sheet is 12 mm, zeroed on top. Each outline goes through its own
    // sheet: to the bed (Z 0) on the first, to -12 on the second, never
    // more than a millimetre of through cut past it.
    let (mut doc, a, b) = two_panels();
    let first = doc.active_sheet();
    let second = doc.entity(b).expect("b").sheet;
    let mut thick = doc.sheet_material(first).expect("material");
    thick.thickness = 20.0;
    thick.z_zero = crate::doc::ZZero::Bottom;
    doc.begin("material");
    doc.set_sheet_material(first, thick).expect("set");
    doc.commit();
    run(&mut doc, "joints.add", finger_spec(a, b));
    let made = run(
        &mut doc,
        "joints.makeToolpaths",
        json!({ "tool": end_mill() }),
    );
    assert_eq!(doc.active_sheet(), first, "the sheet on show is put back");
    let deepest = |sheet: u64| {
        made["ids"]
            .as_array()
            .expect("ids")
            .iter()
            .filter_map(Value::as_u64)
            .filter_map(|id| doc.toolpath(id).ok())
            .filter(|t| t.sheet == sheet)
            .flat_map(|t| t.moves.iter().map(|m| m.to.z))
            .fold(f64::INFINITY, f64::min)
    };
    let (one, two) = (deepest(first), deepest(second));
    assert!(one <= 1e-6 && one > -1.0, "first {one}");
    assert!(two <= -12.0 + 1e-6 && two > -13.0, "second {two}");
}

#[test]
fn dowels_edge_to_edge_are_drill_marks_lined_up_on_both_parts() {
    // Two 100 mm edges of 12 mm stock: two 6 mm dowels (the usual size at
    // most half the stock) 12.5 mm in from the ends, 18 mm into each edge.
    let (mut doc, a, b) = two_panels();
    let (area_a, area_b) = (area(&doc, a), area(&doc, b));
    let mut spec = finger_spec(a, b);
    spec["kind"] = json!("dowel");
    let reply = run(&mut doc, "joints.add", spec);
    let resolved = &reply["outcome"]["resolved"];
    assert_eq!(resolved["count"], 2);
    assert!((number(&resolved["diameter"]) - 6.0).abs() < 1e-9);
    assert!((number(&resolved["inset"]) - 12.5).abs() < 1e-9);
    assert!((number(&resolved["length"]) - 36.0).abs() < 1e-9);
    // Nothing is cut: both outlines are as they were.
    assert!((area(&doc, a) - area_a).abs() < 1e-6);
    assert!((area(&doc, b) - area_b).abs() < 1e-6);
    let joint = doc.assembly().joint_set.joints[0].clone();
    assert_eq!(joint.vectors.len(), 4);
    assert!(joint.vectors.iter().all(|v| v.role == super::Role::Mark));
    let sheet_of = |id: u64| doc.entity(id).expect("mark").sheet;
    let first_sheet = doc.entity(a).expect("a").sheet;
    let boxes: Vec<(u64, crate::geom::BBox)> = joint
        .vectors
        .iter()
        .map(|v| (sheet_of(v.id), doc.bbox_of(v.id).expect("mark")))
        .collect();
    // A's edge runs from x 110 to 10 at y 60: the first dowel 12.5 mm in,
    // at x 97.5, its mark 6 mm wide and 18 mm down into A; B's matching
    // mark is at x 97.5 too, 18 mm up into B from its edge at y 100.
    let on_a: Vec<_> = boxes.iter().filter(|(s, _)| *s == first_sheet).collect();
    let on_b: Vec<_> = boxes.iter().filter(|(s, _)| *s != first_sheet).collect();
    assert_eq!((on_a.len(), on_b.len()), (2, 2));
    let near = |got: f64, want: f64| (got - want).abs() < 1e-6;
    let (_, ma) = on_a[0];
    assert!(near(ma.min.x, 94.5) && near(ma.max.x, 100.5), "{ma:?}");
    assert!(near(ma.min.y, 42.0) && near(ma.max.y, 60.0), "{ma:?}");
    let (_, mb) = on_b[0];
    assert!(near(mb.min.x, 94.5) && near(mb.max.x, 100.5), "{mb:?}");
    assert!(near(mb.min.y, 100.0) && near(mb.max.y, 118.0), "{mb:?}");
    let name = doc.entity(joint.vectors[0].id).expect("mark").name.clone();
    assert!(name.is_some_and(|n| n.contains("drill mark 1: 6 mm, 18 mm deep into the edge")));
}

#[test]
fn a_groove_for_stock_thinner_than_the_tool_is_cut_a_little_wider_than_the_tool() {
    // A 6 mm panel and a 6.35 mm end mill: a dado (6.2 mm with the
    // clearance) is refused, a groove is cut 6.45 mm wide (the tool plus
    // 0.05 a side), centred where the panel stands.
    let (mut doc, base, shelf) = base_and_shelf();
    let sheet = doc.entity(shelf).expect("shelf").sheet;
    let mut thin = doc.sheet_material(sheet).expect("material");
    thin.thickness = 6.0;
    doc.begin("material");
    doc.set_sheet_material(sheet, thin).expect("set");
    doc.commit();
    let refused = try_run(&mut doc, "joints.add", dado_spec(base, shelf, json!({})));
    assert!(
        refused.is_err_and(|e| e.to_string().contains("narrower than the tool")),
        "a dado"
    );
    run(
        &mut doc,
        "joints.add",
        tab_spec(base, shelf, "groove", json!({})),
    );
    let joint = doc.assembly().joint_set.joints[0].clone();
    let pocket = joint
        .vectors
        .iter()
        .find(|v| matches!(v.role, super::Role::Pocket { .. }))
        .expect("pocket");
    let b = doc.bbox_of(pocket.id).expect("groove");
    assert!((b.height() - 6.45).abs() < 1e-6, "{b:?}");
    assert!(((b.min.y + b.max.y) / 2.0 - 53.0).abs() < 1e-6, "{b:?}");
}

#[test]
fn dowels_on_a_short_edge_go_a_quarter_of_it_in_from_the_ends() {
    // A 30 mm edge of 12 mm stock: two 8 mm dowels; twice their size in
    // from each end would leave no room, so 7.5 mm (a quarter) in.
    let (mut doc, base, _) = base_and_shelf();
    let sheet = doc.sheets()[1].id;
    let short = rect(&mut doc, sheet, 10.0, 150.0, 40.0, 210.0);
    let spec = json!({
        "kind": "dowel",
        "a": edge(short, [10.0, 150.0], [40.0, 150.0]),
        "b": {
            "kind": "face",
            "edge": { "vector": base, "from": [10.0, 10.0], "to": [210.0, 10.0] },
            "offset": 40.0,
            "along": 100.0,
        },
        "params": {},
    });
    let reply = run(&mut doc, "joints.add", spec);
    let resolved = &reply["outcome"]["resolved"];
    assert_eq!(resolved["count"], 2);
    assert!((number(&resolved["diameter"]) - 8.0).abs() < 1e-9);
    assert!((number(&resolved["inset"]) - 7.5).abs() < 1e-9);
}
