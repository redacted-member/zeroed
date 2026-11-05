//! Furniture Designer tests through the commands: every built-in template
//! builds at its defaults and at its limits; every connection type with
//! every joint valid for it keeps the piece's outside box (measured on the
//! exact solids the toolpaths cut) and puts B where the boxes say; a
//! regenerate keeps ids; a joint switch is one undo step; files without
//! designs load.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::assembly::exact::sheet_exact;
use crate::assembly::pose::Pose;
use crate::doc::{Document, JobSetup};
use crate::kernel::KernelReply;

use super::connect::FJoint;
use super::design::FurnitureDesign;
use super::eval::TOL;
use super::generate::{generate, lay_out, Placement};
use super::layout::{bounds, pose_error};
use super::template::{builtin, Template};

pub(crate) fn job() -> Document {
    let mut doc = Document::new();
    doc.begin("job");
    doc.set_job(JobSetup {
        width: 2440.0,
        height: 1220.0,
        thickness: 18.0,
        ..JobSetup::default()
    })
    .expect("job");
    doc.commit();
    doc
}

pub(crate) fn try_run(doc: &mut Document, name: &str, args: Value) -> Result<Value, String> {
    let mut library = super::library::Library::default();
    match super::dispatch(doc, &mut library, name, &args.to_string()).map_err(|e| e.to_string())? {
        KernelReply::Json(text) => {
            Ok(serde_json::from_str::<Value>(&text).expect("json")["result"].clone())
        }
        KernelReply::Bytes(_) => Err("bytes".into()),
    }
}

pub(crate) fn run(doc: &mut Document, name: &str, args: Value) -> Value {
    try_run(doc, name, args).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn design(doc: &Document, id: u64) -> FurnitureDesign {
    doc.assembly()
        .designs
        .iter()
        .find(|d| d.id == id)
        .cloned()
        .expect("design")
}

/// The design worked out as its last regenerate did (the joints the Joint
/// Creator refused left as butt joints).
fn made_layout(doc: &Document, d: &FurnitureDesign) -> super::generate::Generated {
    generate(&d.inputs_in(doc), &doc.assembly().joint_set.settings)
        .expect("generates")
        .0
}

fn pose_of(m: &[f64; 16]) -> Pose {
    Pose {
        r: [[m[0], m[4], m[8]], [m[1], m[5], m[9]], [m[2], m[6], m[10]]],
        t: [m[12], m[13], m[14]],
    }
}

fn place_of(m: [f64; 3]) -> Placement {
    Placement {
        turn: m[0],
        shift: [m[1], m[2]],
    }
}

/// The box round the design's panels as the toolpaths cut them, placed by
/// their frames: each panel's exact material on its sheet, through the
/// panel's part space to furniture space.
fn assembled_box(doc: &Document, id: u64) -> ([f64; 3], [f64; 3]) {
    let d = design(doc, id);
    let gen = made_layout(doc, &d);
    let mut exact = BTreeMap::new();
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for (i, panel) in gen.eval.panels.iter().enumerate() {
        let made = &d.made[&panel.key];
        let sheet = exact
            .entry(made.sheet)
            .or_insert_with(|| sheet_exact(doc, made.sheet).expect("exact"));
        let b = doc.bbox_of(made.vector).expect("vector");
        // The shape of the sheet's material that is this panel: the one
        // whose box matches the vector's best.
        // Where a cut that cannot be made exact runs over the panel (a
        // tapered wall), its outline vector is what the outline profile
        // leaves.
        let blocked = sheet
            .blocked
            .iter()
            .any(|k| k[0] < b.max.x && b.min.x < k[2] && k[1] < b.max.y && b.min.y < k[3]);
        let own;
        let shape = if blocked {
            let crate::doc::EntityKind::Path { path } = &doc.entity(made.vector).expect("v").kind
            else {
                panic!("not a path");
            };
            own = crate::joints::geometry::path_rings(path);
            &own
        } else {
            sheet
                .material
                .iter()
                .min_by(|x, y| {
                    let fit = |s: &Vec<Vec<[f64; 2]>>| {
                        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
                        for p in &s[0] {
                            for k in 0..2 {
                                lo[k] = lo[k].min(p[k]);
                                hi[k] = hi[k].max(p[k]);
                            }
                        }
                        (lo[0] - b.min.x).abs()
                            + (lo[1] - b.min.y).abs()
                            + (hi[0] - b.max.x).abs()
                            + (hi[1] - b.max.y).abs()
                    };
                    fit(x).total_cmp(&fit(y))
                })
                .expect("a shape")
        };
        let place = place_of(made.place);
        let frame = gen.frames[i].pose();
        for p in &shape[0] {
            let flat = place.back(*p);
            for z in [0.0, panel.thickness] {
                let q = frame.apply([flat[0], flat[1], z]);
                for k in 0..3 {
                    min[k] = min[k].min(q[k]);
                    max[k] = max[k].max(q[k]);
                }
            }
        }
    }
    (min, max)
}

/// Every made joint's own B to A transform is where the boxes put B (for
/// the joints that are assembled as their boxes say).
fn check_joint_poses(doc: &Document, id: u64) -> usize {
    joint_pose_errors(doc, id).unwrap_or_else(|e| panic!("{e}"))
}

/// [`check_joint_poses`], answering what is off instead of failing.
fn joint_pose_errors(doc: &Document, id: u64) -> Result<usize, String> {
    let d = design(doc, id);
    let gen = made_layout(doc, &d);
    let index_of = |vector: u64| {
        gen.eval
            .panels
            .iter()
            .position(|p| d.made[&p.key].vector == vector)
            .expect("panel")
    };
    let mut checked = 0;
    for joint in &doc.assembly().joint_set.joints {
        if !d.joints.values().any(|j| *j == joint.id) {
            continue;
        }
        let Some(m) = joint.b_to_a else {
            continue;
        };
        let (a, b) = (index_of(joint.a.vector), index_of(joint.b.vector()));
        let pa = place_of(d.made[&gen.eval.panels[a].key].place);
        let pb = place_of(d.made[&gen.eval.panels[b].key].place);
        let expected = pa
            .pose()
            .then_local(&gen.frames[a].pose().inverse())
            .then_local(&gen.frames[b].pose())
            .then_local(&pb.pose().inverse());
        let err = pose_error(&pose_of(&m), &expected);
        if err >= 1e-6 {
            return Err(format!(
                "{} {} between {} and {}: B is {err} off where the boxes put it",
                joint.kind.title(),
                joint.id,
                gen.eval.panels[a].key,
                gen.eval.panels[b].key
            ));
        }
        checked += 1;
    }
    Ok(checked)
}

fn assert_box(doc: &Document, id: u64, what: &str) {
    let d = design(doc, id);
    let gen = made_layout(doc, &d);
    let (want_min, want_max) = bounds(&gen.eval.panels);
    let (min, max) = assembled_box(doc, id);
    for k in 0..3 {
        assert!(
            (min[k] - want_min[k]).abs() <= TOL && (max[k] - want_max[k]).abs() <= TOL,
            "{what}: the piece is {min:?} to {max:?}, its boxes {want_min:?} to {want_max:?}"
        );
    }
}

#[test]
fn the_bookcase_builds_into_the_job() {
    let mut doc = job();
    let before_sheets = doc.sheets().len();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 2 } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let summary = &reply["summary"];
    assert_eq!(summary["name"], "Bookcase 1");
    assert!(
        summary["issues"].as_array().is_some_and(Vec::is_empty),
        "{summary}"
    );
    // left, right, top, bottom, 2 shelves, kick, back.
    assert_eq!(summary["panels"], 8);
    let d = design(&doc, id);
    // One carcass sheet and one back sheet, named after the design.
    let names: Vec<String> = doc
        .sheets()
        .iter()
        .skip(before_sheets)
        .map(|s| s.name.clone())
        .collect();
    // The panels do not all fit on one 2440 x 1220 sheet with the grain
    // along it.
    assert_eq!(
        names,
        vec![
            "Bookcase 1 Back 1",
            "Bookcase 1 Carcass 1",
            "Bookcase 1 Carcass 2"
        ]
    );
    let back_sheet = d
        .sheets
        .iter()
        .find(|s| s.material == "back")
        .expect("back");
    assert_eq!(doc.sheet_material(back_sheet.id).expect("m").thickness, 9.0);
    // The panel vectors are on the design's layer.
    let layer = doc.layer(d.layer).expect("layer");
    assert_eq!(layer.name, "Bookcase 1");
    for made in d.made.values() {
        assert_eq!(doc.entity(made.vector).expect("vector").layer, d.layer);
    }
    // Toolpaths: outlines on both sheets, dado and rabbet pockets.
    let names: Vec<&str> = doc.toolpaths().iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"Bookcase 1 outlines"), "{names:?}");
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("Bookcase 1 pockets 6 mm")),
        "{names:?}"
    );
    // A piece per panel.
    assert_eq!(doc.assembly().pieces.len(), 8);
    assert!(check_joint_poses(&doc, id) > 0);
    assert_box(&doc, id, "bookcase");
}

#[test]
fn a_dado_shelf_grows_by_both_dado_depths_and_a_rabbeted_top_by_the_rabbet_depth() {
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 1, "back": "none", "kick": 0 } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let d = design(&doc, id);
    let width = |key: &str| {
        let b = doc.bbox_of(d.made[key].vector).expect("vector");
        b.width().max(b.height())
    };
    // 800 wide, 18 sides: the shelf's box is 764; dados 6 deep each end.
    assert!(
        (width("shelf#0") - (764.0 + 12.0)).abs() < 1e-6,
        "{}",
        width("shelf#0")
    );
    // The top in rabbets 9 deep: 764 + 2 * 9.
    assert!(
        (width("top") - (764.0 + 18.0)).abs() < 1e-6,
        "{}",
        width("top")
    );
    // A butt joint keeps the box.
    run(
        &mut doc,
        "furniture.setJoint",
        json!({ "design": id, "scope": "class", "key": "shelf", "joint": "none" }),
    );
    let d = design(&doc, id);
    let b = doc.bbox_of(d.made["shelf#0"].vector).expect("vector");
    assert!((b.width().max(b.height()) - 764.0).abs() < 1e-6);
    assert_box(&doc, id, "butt shelf");
}

/// A test template: `panels` in a JSON array, materials main (18) and thin
/// (9), every connection in class `c`.
fn template(id: &str, panels: Value) -> Value {
    json!({
        "format": 1,
        "id": id,
        "name": "Test",
        "params": [],
        "materials": {
            "main": { "label": "Main", "thickness": "18" },
            "thin": { "label": "Thin", "thickness": "9" }
        },
        "panels": panels,
        "connections": [ { "a": "*", "b": "*", "class": "c" } ],
        "jointClasses": { "c": { "label": "All", "default": "none" } }
    })
}

fn panel(id: &str, material: &str, b: [f64; 6]) -> Value {
    json!({
        "id": id, "name": id, "material": material, "grain": "none",
        "box": {
            "x": b[0].to_string(), "y": b[1].to_string(), "z": b[2].to_string(),
            "w": b[3].to_string(), "d": b[4].to_string(), "h": b[5].to_string()
        }
    })
}

/// Builds `tpl` with `joint` on connection `key` (every other connection a
/// butt joint), checks the joint was made and the box kept; answers the
/// design id.
fn build_with(tpl: &Value, key: &str, joint: FJoint) -> (Document, u64) {
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({
            "template": tpl,
            "joints": { "connections": { key: { "joint": joint } } }
        }),
    );
    let id = reply["design"].as_u64().expect("id");
    let issues = &reply["summary"]["issues"];
    assert!(
        issues.as_array().is_some_and(Vec::is_empty),
        "{joint:?} on {key}: {issues}"
    );
    let d = design(&doc, id);
    if joint != FJoint::None {
        assert!(d.joints.contains_key(key), "{joint:?}: no joint on {key}");
    }
    (doc, id)
}

fn every_joint_of(tpl: Value, key: &str, kind: &str) {
    let t: Template = serde_json::from_value(tpl.clone()).expect("template");
    let gen = lay_out(
        &super::generate::Inputs {
            template: &t,
            values: &BTreeMap::new(),
            style: Default::default(),
            classes: &BTreeMap::new(),
            picked: &BTreeMap::new(),
            faces: BTreeMap::new(),
            double_sided: false,
        },
        &BTreeMap::new(),
        6.35,
    )
    .expect("lays out");
    let c = gen.conns.iter().find(|c| c.key == key).expect("connection");
    assert_eq!(serde_json::to_value(c.kind).expect("json"), json!(kind));
    assert!(c.choices.len() > 1 || kind == "cross", "{:?}", c.choices);
    for joint in c.choices.clone() {
        let (doc, id) = build_with(&tpl, key, joint);
        assert_box(&doc, id, &format!("{joint:?} on a {kind}"));
        check_joint_poses(&doc, id);
    }
}

#[test]
fn every_tee_joint_keeps_the_outside_box() {
    // A shelf between a side and a middle divider; a joint standing proud
    // of the divider runs into the empty bay beyond it.
    let tpl = template(
        "tee",
        json!([
            panel("side", "main", [0.0, 0.0, 0.0, 18.0, 300.0, 600.0]),
            panel("divider", "main", [291.0, 0.0, 0.0, 18.0, 300.0, 600.0]),
            panel("shelf", "main", [18.0, 0.0, 300.0, 273.0, 300.0, 18.0]),
            panel("end", "main", [582.0, 0.0, 0.0, 18.0, 300.0, 600.0]),
            panel("base", "main", [0.0, 0.0, -18.0, 600.0, 300.0, 18.0])
        ]),
    );
    every_joint_of(tpl, "divider~shelf", "tee");
}

#[test]
fn every_corner_joint_keeps_the_outside_box() {
    let tpl = template(
        "corner",
        json!([
            panel("side", "main", [0.0, 0.0, 0.0, 18.0, 300.0, 600.0]),
            panel("top", "main", [18.0, 0.0, 582.0, 400.0, 300.0, 18.0])
        ]),
    );
    every_joint_of(tpl, "side~top", "corner");
}

#[test]
fn every_edge_joint_keeps_the_outside_box() {
    let tpl = template(
        "edge",
        json!([
            panel("left", "main", [0.0, 0.0, 0.0, 300.0, 400.0, 18.0]),
            panel("right", "main", [300.0, 0.0, 0.0, 300.0, 400.0, 18.0])
        ]),
    );
    every_joint_of(tpl, "left~right", "edge");
}

#[test]
fn every_face_on_edges_joint_keeps_the_outside_box() {
    let tpl = template(
        "face",
        json!([
            panel("side", "main", [0.0, 0.0, 0.0, 18.0, 291.0, 600.0]),
            panel("back", "thin", [0.0, 291.0, 0.0, 400.0, 9.0, 600.0])
        ]),
    );
    every_joint_of(tpl, "back~side", "faceOnEdges");
}

#[test]
fn crossings_keep_the_outside_box() {
    // An egg crate: both run the full height.
    let tpl = template(
        "cross",
        json!([
            panel("a", "main", [0.0, 200.0, 0.0, 400.0, 18.0, 300.0]),
            panel("b", "main", [200.0, 0.0, 0.0, 18.0, 400.0, 300.0])
        ]),
    );
    every_joint_of(tpl, "a~b", "cross");
    // A stretcher through a leg: the leg's slot comes from its nearer end.
    let tpl = template(
        "stretcher",
        json!([
            panel("leg", "main", [100.0, 0.0, 0.0, 18.0, 300.0, 700.0]),
            panel("rail", "main", [0.0, 140.0, 120.0, 400.0, 18.0, 90.0])
        ]),
    );
    every_joint_of(tpl, "leg~rail", "cross");
}

#[test]
fn every_template_builds_clean_at_its_defaults_and_its_limits() {
    // Clean: every joint asked for is made, nothing is left out, said or
    // drawn beside the sheet.
    let mut failures = Vec::new();
    for t in builtin() {
        let mut sets: Vec<BTreeMap<String, Value>> = vec![BTreeMap::new()];
        let pick = |lowest: bool| {
            t.params
                .iter()
                .filter_map(|p| {
                    let v = if lowest { p.min } else { p.max }?;
                    Some((p.key.clone(), json!(v)))
                })
                .collect::<BTreeMap<String, Value>>()
        };
        sets.push(pick(true));
        sets.push(pick(false));
        for values in sets {
            let mut doc = job();
            let reply = match try_run(
                &mut doc,
                "furniture.create",
                json!({ "template": t.id, "values": values }),
            ) {
                Ok(reply) => reply,
                Err(e) => {
                    failures.push(format!("{} at {values:?}: {e}", t.id));
                    continue;
                }
            };
            let id = reply["design"].as_u64().expect("id");
            let d = design(&doc, id);
            if d.made.is_empty() {
                failures.push(format!("{} at {values:?}: no panels", t.id));
            }
            for issue in reply["summary"]["issues"].as_array().into_iter().flatten() {
                failures.push(format!("{} at {values:?}: {}", t.id, issue["message"]));
            }
            if let Err(e) = joint_pose_errors(&doc, id) {
                failures.push(format!("{} at {values:?}: {e}", t.id));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_regenerate_keeps_the_ids_of_what_still_exists() {
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 3 } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let before = design(&doc, id);
    run(
        &mut doc,
        "furniture.update",
        json!({ "design": id, "values": { "width": 900, "shelves": 2 } }),
    );
    let after = design(&doc, id);
    for (key, made) in &after.made {
        assert_eq!(made.vector, before.made[key].vector, "{key}");
    }
    assert!(!after.made.contains_key("shelf#2"));
    assert!(doc.entity(before.made["shelf#2"].vector).is_err());
    for (key, joint) in &after.joints {
        assert_eq!(*joint, before.joints[key], "{key}");
    }
    for (key, toolpath) in &after.toolpaths {
        if let Some(old) = before.toolpaths.get(key) {
            assert_eq!(toolpath, old, "{key}");
        }
    }
    let sheets = |d: &FurnitureDesign| d.sheets.iter().map(|s| s.id).collect::<Vec<_>>();
    assert_eq!(sheets(&after), sheets(&before));
    // The width reached the panels: the top is 100 longer.
    let len = |d: &FurnitureDesign, key: &str| {
        let b = doc.bbox_of(d.made[key].vector).expect("vector");
        b.width().max(b.height())
    };
    assert!(
        (len(&after, "top") - 900.0 + 36.0 - 18.0).abs() < 1e-6,
        "{}",
        len(&after, "top")
    );
    assert_box(&doc, id, "wider bookcase");
}

#[test]
fn a_joint_switch_is_one_undo_step() {
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase" }),
    );
    let id = reply["design"].as_u64().expect("id");
    let assembly = doc.assembly().clone();
    let entities = doc.entities().clone();
    let toolpaths: Vec<String> = doc.toolpaths().iter().map(|t| t.name.clone()).collect();
    run(
        &mut doc,
        "furniture.setJoint",
        json!({ "design": id, "scope": "class", "key": "shelf", "joint": "tabSlot" }),
    );
    assert_ne!(doc.assembly(), &assembly);
    let d = design(&doc, id);
    assert_eq!(d.class_joints["shelf"].joint, FJoint::TabSlot);
    doc.undo();
    assert_eq!(doc.assembly(), &assembly);
    assert_eq!(doc.entities(), &entities);
    let now: Vec<String> = doc.toolpaths().iter().map(|t| t.name.clone()).collect();
    assert_eq!(now, toolpaths);
    // Dropping the override goes back to the class default.
    doc.redo();
    run(
        &mut doc,
        "furniture.setJoint",
        json!({ "design": id, "scope": "class", "key": "shelf", "joint": null }),
    );
    assert!(design(&doc, id).class_joints.is_empty());
}

#[test]
fn removing_a_design_takes_everything_it_made() {
    let mut doc = job();
    let sheets = doc.sheets().len();
    let entities = doc.entities().len();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase" }),
    );
    let id = reply["design"].as_u64().expect("id");
    run(&mut doc, "furniture.remove", json!({ "design": id }));
    assert!(doc.assembly().designs.is_empty());
    assert!(doc.assembly().joint_set.joints.is_empty());
    assert!(doc.assembly().pieces.is_empty());
    assert!(doc.toolpaths().is_empty());
    assert_eq!(doc.sheets().len(), sheets);
    assert_eq!(doc.entities().len(), entities);
}

#[test]
fn files_without_designs_load_and_designs_save() {
    let old: crate::assembly::Assembly =
        serde_json::from_value(json!({ "parts": [], "nextId": 3 })).expect("old assembly");
    assert!(old.designs.is_empty());
    let mut doc = job();
    run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase" }),
    );
    let text = serde_json::to_string(doc.assembly()).expect("saves");
    let back: crate::assembly::Assembly = serde_json::from_str(&text).expect("loads");
    assert_eq!(&back, doc.assembly());
    // An empty assembly writes no designs field.
    let empty = serde_json::to_value(crate::assembly::Assembly::default()).expect("json");
    assert!(empty.get("designs").is_none());
}

#[test]
fn the_preview_answers_meshes_without_touching_the_document() {
    let mut doc = job();
    let revision = doc.revision();
    let reply = run(
        &mut doc,
        "furniture.preview",
        json!({ "template": "bookcase", "values": { "shelves": 1, "back": "none", "kick": 0 } }),
    );
    assert_eq!(doc.revision(), revision);
    let panels = reply["panels"].as_array().expect("panels");
    // Two sides, top, bottom and the shelf.
    assert_eq!(panels.len(), 5);
    // The meshes fill the outside box exactly.
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in panels {
        for k in 0..3 {
            min[k] = min[k].min(p["box"]["min"][k].as_f64().expect("min"));
            max[k] = max[k].max(p["box"]["max"][k].as_f64().expect("max"));
        }
    }
    let want = [800.0, 300.0, 1800.0];
    for k in 0..3 {
        assert!(
            min[k].abs() < 1e-3 && (max[k] - want[k]).abs() < 1e-3,
            "{min:?} {max:?}"
        );
    }
    let c = reply["connections"]
        .as_array()
        .expect("connections")
        .iter()
        .find(|c| c["key"] == "left~shelf#0")
        .expect("shelf");
    assert_eq!(c["joint"], "dado");
    assert_eq!(c["source"], "style");
    assert_eq!(c["made"], true);
    assert_eq!(c["classLabel"], "Shelves to sides");
    assert!(c["choices"]
        .as_array()
        .is_some_and(|a| a.contains(&json!("tabSlot"))));
}

#[test]
fn a_pick_ray_finds_the_nearest_seam_within_reach() {
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 1 } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let d = design(&doc, id);
    let gen = generate(&d.inputs(), &Default::default())
        .expect("generates")
        .0;
    let c = gen
        .conns
        .iter()
        .find(|c| c.key == "left~shelf#0")
        .expect("shelf");
    let mid = [0, 1, 2].map(|k| (c.seam[0][k] + c.seam[1][k]) / 2.0 + d.origin[k]);
    // Level, towards the front from beside the seam's middle.
    let hit = run(
        &mut doc,
        "furniture.at",
        json!({ "design": id, "point": [mid[0] + 7.9, mid[1], mid[2]], "direction": [0, -1, 0] }),
    );
    assert_eq!(hit["connection"]["key"], "left~shelf#0");
    let miss = run(
        &mut doc,
        "furniture.at",
        json!({ "design": id, "point": [mid[0] + 8.1, mid[1], mid[2]], "direction": [0, -1, 0] }),
    );
    assert!(miss["connection"].is_null(), "{miss}");
}

/// A job whose sheet on show is thicker than the panels and zeroed on the
/// bed: each design sheet's toolpaths still cut its own panels free.
fn thick_bed_job() -> Document {
    let mut doc = Document::new();
    doc.begin("job");
    doc.set_job(JobSetup {
        width: 2440.0,
        height: 1220.0,
        thickness: 25.0,
        z_zero: crate::doc::ZZero::Bottom,
        ..JobSetup::default()
    })
    .expect("job");
    doc.commit();
    doc
}

#[test]
fn every_panel_is_a_piece_standing_where_its_box_puts_it() {
    use crate::assembly::pieces::{pieces, PartsCache};
    let mut doc = thick_bed_job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 2 } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let d = design(&doc, id);
    let gen = made_layout(&doc, &d);
    let mut cache = PartsCache::default();
    let cells = crate::cam::sim::Quality::default().cells();
    let infos = pieces(&doc, &mut cache, cells).expect("pieces");
    // One piece per panel on each of the design's sheets.
    for s in &d.sheets {
        let on = infos.iter().filter(|p| p.piece.sheet == s.id).count();
        let panels = d.made.values().filter(|m| m.sheet == s.id).count();
        assert_eq!(on, panels, "sheet {}", s.id);
    }
    // The grid's cell, which the pieces' boxes are measured on.
    let cell = (2440.0_f64 * 1220.0 / cells).sqrt();
    for (i, panel) in gen.eval.panels.iter().enumerate() {
        let made = &d.made[&panel.key];
        let stored = doc
            .assembly()
            .pieces
            .iter()
            .position(|q| q.sheet == made.sheet && q.part == made.piece)
            .expect("stored piece");
        let info = infos
            .iter()
            .find(|p| p.stored == Some(stored))
            .unwrap_or_else(|| panic!("{}: its placement fits no part", panel.key));
        let pose = info.piece.pose(info.size);
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for c in 0..8 {
            let p = [
                if c & 1 == 0 { 0.0 } else { info.size[0] },
                if c & 2 == 0 { 0.0 } else { info.size[1] },
                if c & 4 == 0 { 0.0 } else { info.size[2] },
            ];
            let q = pose.apply(p);
            for k in 0..3 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
        }
        // Where the panel's outline goes: through its frame, plus the
        // design's origin.
        let place = place_of(made.place);
        let b = doc.bbox_of(made.vector).expect("vector");
        let frame = gen.frames[i].pose();
        let mut want_lo = [f64::INFINITY; 3];
        let mut want_hi = [f64::NEG_INFINITY; 3];
        for (x, y) in [(b.min.x, b.min.y), (b.max.x, b.max.y)] {
            let flat = place.back([x, y]);
            for z in [0.0, panel.thickness] {
                let q = frame.apply([flat[0], flat[1], z]);
                for k in 0..3 {
                    want_lo[k] = want_lo[k].min(q[k] + d.origin[k]);
                    want_hi[k] = want_hi[k].max(q[k] + d.origin[k]);
                }
            }
        }
        // The thickness is exact; across the sheet the box is the grid's,
        // within a cell of the outline's.
        for k in 0..3 {
            let reach = if k == panel.normal { 1e-6 } else { cell };
            assert!(
                (lo[k] - want_lo[k]).abs() <= reach && (hi[k] - want_hi[k]).abs() <= reach,
                "{}: the piece is {lo:?} to {hi:?}, the panel {want_lo:?} to {want_hi:?}",
                panel.key
            );
        }
    }
}

#[test]
fn the_cut_list_groups_panels_and_bands_the_front_edges() {
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 2, "back": "none", "kick": 0 } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let list = run(&mut doc, "furniture.cutList", json!({ "design": id }));
    let rows = list["rows"].as_array().expect("rows");
    // Sides, top and bottom (rabbeted, 764 + 18), two shelves (dadoed,
    // 764 + 12).
    let shelves = rows
        .iter()
        .find(|r| {
            r["panels"]
                .as_array()
                .is_some_and(|p| p.contains(&json!("shelf#0")))
        })
        .expect("shelves");
    assert_eq!(shelves["quantity"], 2);
    assert_eq!(shelves["length"], 776.0);
    assert_eq!(shelves["width"], 300.0);
    assert_eq!(shelves["thickness"], 18.0);
    // A shelf's front edge is banded, the whole length.
    assert_eq!(shelves["banding"], json!(["front"]));
    assert_eq!(shelves["bandingLength"], 776.0);
    let sides = rows
        .iter()
        .find(|r| {
            r["panels"]
                .as_array()
                .is_some_and(|p| p.contains(&json!("left")))
        })
        .expect("sides");
    assert_eq!(sides["quantity"], 2);
    assert_eq!(
        (sides["length"].clone(), sides["width"].clone()),
        (json!(1800.0), json!(300.0))
    );
    assert_eq!(sides["bandingLength"], 1800.0);
    let total: u64 = rows.iter().filter_map(|r| r["quantity"].as_u64()).sum();
    assert_eq!(total, list["panels"].as_u64().expect("panels"));
}

#[test]
fn hardware_adds_up_the_joints_and_the_template() {
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({
            "template": "bookcase",
            "values": { "shelves": 2, "back": "none", "kick": 0 },
            "joints": { "classes": { "shelf": { "joint": "camLock" } } }
        }),
    );
    let id = reply["design"].as_u64().expect("id");
    let hw = run(&mut doc, "furniture.hardware", json!({ "design": id }));
    let rows = hw["rows"].as_array().expect("rows");
    let row = |name: &str| {
        rows.iter()
            .find(|r| r["name"].as_str().is_some_and(|n| n.starts_with(name)))
    };
    // Two shelves, two ends each, two fittings a 300 mm edge.
    assert_eq!(row("Cam lock fitting 15 mm").expect("cams")["count"], 8.0);
    assert_eq!(row("Wall anchor").expect("anchor")["count"], 1.0);
}

#[test]
fn the_estimate_counts_sheets_material_and_time() {
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase" }),
    );
    let id = reply["design"].as_u64().expect("id");
    let e = run(&mut doc, "furniture.estimate", json!({ "design": id }));
    let sheets = e["sheets"].as_array().expect("sheets");
    let main = sheets
        .iter()
        .find(|s| s["material"] == "main")
        .expect("main");
    assert_eq!(main["count"], 2);
    let area = main["area"].as_f64().expect("area");
    assert!((area - 2.0 * 2440.0 * 1220.0).abs() < 0.1);
    let used = main["used"].as_f64().expect("used");
    // Two 1800 x 300 sides at least, less than the two sheets.
    assert!(used > 2.0 * 1800.0 * 300.0 && used < area, "{used}");
    let waste = main["waste"].as_f64().expect("waste");
    assert!((waste - (1.0 - used / area)).abs() < 1e-3);
    assert!(e["seconds"].as_f64().expect("seconds") > 0.0);
}

#[test]
fn saved_templates_take_the_design_values_as_defaults() {
    let dir = crate::test_support::TempDir::new("furniture-save");
    let mut library = super::library::Library {
        folder: Some(dir.path().to_path_buf()),
    };
    let mut doc = job();
    let call =
        |doc: &mut Document, library: &mut super::library::Library, name: &str, args: Value| {
            match super::dispatch(doc, library, name, &args.to_string()).expect(name) {
                KernelReply::Json(text) => {
                    serde_json::from_str::<Value>(&text).expect("json")["result"].clone()
                }
                KernelReply::Bytes(_) => Value::Null,
            }
        };
    let reply = call(
        &mut doc,
        &mut library,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "width": 1000 } }),
    );
    let id = reply["design"].as_u64().expect("id");
    call(
        &mut doc,
        &mut library,
        "furniture.saveTemplate",
        json!({ "design": id, "id": "wide-bookcase", "name": "Wide Bookcase" }),
    );
    let list = call(&mut doc, &mut library, "furniture.templates", json!({}));
    let mine = list["templates"]
        .as_array()
        .expect("templates")
        .iter()
        .find(|t| t["id"] == "wide-bookcase")
        .expect("saved")
        .clone();
    assert_eq!(mine["builtIn"], false);
    let width = mine["params"]
        .as_array()
        .expect("params")
        .iter()
        .find(|p| p["key"] == "width")
        .expect("width")
        .clone();
    assert_eq!(width["default"], 1000.0);
    // A new design from it starts 1000 wide.
    let reply = call(
        &mut doc,
        &mut library,
        "furniture.create",
        json!({ "template": "wide-bookcase" }),
    );
    assert_eq!(reply["summary"]["size"][0], 1000.0);
    // Import refuses a built-in id; remove takes the maker's only.
    let bookcase = call(
        &mut doc,
        &mut library,
        "furniture.template",
        json!({ "id": "bookcase" }),
    );
    assert!(super::dispatch(
        &mut doc,
        &mut library,
        "furniture.importTemplate",
        &json!({ "json": bookcase }).to_string()
    )
    .is_err());
    call(
        &mut doc,
        &mut library,
        "furniture.removeTemplate",
        json!({ "id": "wide-bookcase" }),
    );
    assert!(super::dispatch(
        &mut doc,
        &mut library,
        "furniture.template",
        &json!({ "id": "wide-bookcase" }).to_string()
    )
    .is_err());
}

#[test]
fn materials_list_real_thicknesses_and_sheet_sizes() {
    let mut doc = job();
    let m = run(&mut doc, "furniture.materials", json!({}));
    let thick = m["thicknesses"].as_array().expect("thicknesses");
    assert!(
        thick
            .iter()
            .any(|t| t["id"] == "birch-18" && t["actual"] == 18.0),
        "{m}"
    );
    let sizes = m["sizes"].as_array().expect("sizes");
    assert!(sizes
        .iter()
        .any(|s| s["width"] == 2440.0 && s["height"] == 1220.0));
}

#[test]
fn the_list_says_what_each_design_made() {
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "joints": { "classes": { "shelf": { "joint": "dowel" } } } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let list = run(&mut doc, "furniture.list", json!({}));
    let d = &list["designs"][0];
    assert_eq!(d["id"], id);
    assert_eq!(d["classJoints"]["shelf"]["joint"], "dowel");
    let left = &d["made"]["left"];
    let made = &design(&doc, id).made["left"];
    assert_eq!(left["vector"], made.vector);
    assert_eq!(left["sheet"], made.sheet);
    assert_eq!(left["piece"], made.piece);
}

/// Run with `cargo test --release -p carve-core --lib furniture::tests::preview_speed
/// -- --ignored --nocapture`: the preview of a design of 40 panels or more
/// must answer within 150 ms in a release build.
#[test]
#[ignore]
fn preview_speed() {
    let t = builtin()
        .into_iter()
        .find(|t| t.id == "drawer-unit")
        .expect("drawer unit");
    let values: BTreeMap<String, Value> = t
        .params
        .iter()
        .filter_map(|p| Some((p.key.clone(), json!(p.max?))))
        .collect();
    let mut doc = job();
    let first = run(
        &mut doc,
        "furniture.preview",
        json!({ "template": t.id, "values": values }),
    );
    let panels = first["panels"].as_array().map_or(0, Vec::len);
    assert!(panels >= 40, "{panels} panels");
    let runs = 5;
    let started = std::time::Instant::now();
    for _ in 0..runs {
        run(
            &mut doc,
            "furniture.preview",
            json!({ "template": t.id, "values": values }),
        );
    }
    let each = started.elapsed().as_secs_f64() * 1000.0 / f64::from(runs);
    println!("furniture.preview of {panels} panels: {each:.1} ms");
    assert!(each < 150.0, "{each:.1} ms");
}

#[test]
fn pockets_on_both_faces_of_a_panel_are_said_with_a_through_joint_to_use() {
    // A divider with a dadoed shelf each side at the same height.
    let tpl = template(
        "both",
        json!([
            panel("left", "main", [0.0, 0.0, 0.0, 18.0, 300.0, 600.0]),
            panel("divider", "main", [300.0, 0.0, 0.0, 18.0, 300.0, 600.0]),
            panel("right", "main", [600.0, 0.0, 0.0, 18.0, 300.0, 600.0]),
            panel("shelf_l", "main", [18.0, 0.0, 300.0, 282.0, 300.0, 18.0]),
            panel("shelf_r", "main", [318.0, 0.0, 300.0, 282.0, 300.0, 18.0])
        ]),
    );
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": tpl, "joints": { "classes": { "c": { "joint": "dado" } } } }),
    );
    let issues = reply["summary"]["issues"]
        .as_array()
        .expect("issues")
        .clone();
    let both: Vec<&Value> = issues
        .iter()
        .filter(|i| {
            i["message"]
                .as_str()
                .is_some_and(|m| m.contains("pockets on both faces"))
        })
        .collect();
    assert_eq!(both.len(), 1, "{issues:?}");
    assert_eq!(both[0]["suggest"], "tabSlot");
    assert!(both[0]["message"]
        .as_str()
        .is_some_and(|m| m.starts_with("divider needs pockets")));
    // That connection is a butt joint until it is changed; the through
    // joint it suggests is made.
    let key = both[0]["connection"].as_str().expect("key").to_string();
    let id = reply["design"].as_u64().expect("id");
    assert!(!design(&doc, id).joints.contains_key(&key));
    let reply = run(
        &mut doc,
        "furniture.setJoint",
        json!({ "design": id, "scope": "connection", "key": key, "joint": "tabSlot" }),
    );
    let issues = reply["summary"]["issues"].as_array().expect("issues");
    assert!(issues.is_empty(), "{issues:?}");
    assert!(design(&doc, id).joints.contains_key(&key));
    assert_box(&doc, id, "divider with a tab and slot shelf");
}

/// A 40 x 40 mm pyramid 10 mm tall as an ASCII STL in `dir`.
fn pyramid(dir: &std::path::Path) -> std::path::PathBuf {
    pyramid_of_height(dir, 10.0)
}

/// A square based pyramid 40 by 40, `height` tall, as an STL file in `dir`.
fn pyramid_of_height(dir: &std::path::Path, height: f64) -> std::path::PathBuf {
    let path = dir.join("pyramid.stl");
    let apex = format!("20 20 {height}");
    let base = ["0 0 0", "40 0 0", "40 40 0", "0 40 0"];
    let mut text = String::from("solid p\n");
    for k in 0..4 {
        let (a, b) = (base[k], base[(k + 1) % 4]);
        text += &format!(
            "facet normal 0 0 1\nouter loop\nvertex {a}\nvertex {b}\nvertex {apex}\nendloop\nendfacet\n"
        );
    }
    for (a, b, c) in [(base[0], base[2], base[1]), (base[0], base[3], base[2])] {
        text += &format!(
            "facet normal 0 0 -1\nouter loop\nvertex {a}\nvertex {b}\nvertex {c}\nendloop\nendfacet\n"
        );
    }
    text += "endsolid p\n";
    std::fs::write(&path, text).expect("writes the model");
    path
}

/// The middle of a component's heights, job millimetres.
fn component_middle(doc: &Document, id: u64) -> [f64; 2] {
    let b = super::attach::component_box(doc, id).expect("component");
    [(b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0]
}

#[test]
fn a_model_added_to_a_face_stays_on_its_panel_when_the_design_changes() {
    let dir = crate::test_support::TempDir::new("furniture-model");
    let stl = pyramid(dir.path());
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 2, "kick": 0, "back": "none" } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let d = design(&doc, id);
    let left = d.made["left"].clone();
    let up_before = made_layout(&doc, &d).frames[0].up;
    let reply = run(
        &mut doc,
        "furniture.addModel",
        json!({ "design": id, "panel": "left", "face": "top", "source": { "file": stl }, "point": [100, 150] }),
    );
    let ids: Vec<u64> = reply["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(ids.len(), 1);
    let comp = ids[0];
    // On the panel's sheet, sized to the panel less 10% of its smaller
    // side all round (300 wide: 240 room), its middle where asked.
    let c = doc.component(comp).expect("component");
    assert_eq!(doc.sheet_of_component(c), left.sheet);
    let outline = doc.bbox_of(left.vector).expect("left");
    let b = super::attach::component_box(&doc, comp).expect("box");
    assert!(
        b.min.x >= outline.min.x && b.max.x <= outline.max.x,
        "{b:?} {outline:?}"
    );
    assert!(
        b.min.y >= outline.min.y && b.max.y <= outline.max.y,
        "{b:?} {outline:?}"
    );
    // About the room left (the pyramid's edge cells blur by a cell of its
    // own scaled up).
    let short = outline.width().min(outline.height());
    let side = b.width().max(b.height());
    assert!(side > 0.75 * short && side < 0.9 * short, "{b:?}");
    let attached = design(&doc, id).attachments.clone();
    assert_eq!(attached.len(), 1);
    assert_eq!(attached[0].face, up_before, "the top as laid");
    let share = attached[0].at;
    // Wider and taller: the model keeps its share of the panel and its size.
    run(
        &mut doc,
        "furniture.update",
        json!({ "design": id, "values": { "width": 1000, "height": 2000 } }),
    );
    let d = design(&doc, id);
    assert_eq!(d.attachments.len(), 1);
    assert_eq!(d.attachments[0].at, share);
    let gen = made_layout(&doc, &d);
    let i = gen
        .eval
        .panels
        .iter()
        .position(|p| p.key == "left")
        .expect("left");
    let (panel, frame) = (&gen.eval.panels[i], &gen.frames[i]);
    assert_eq!(frame.up, up_before, "the model's face is still up");
    let place = place_of(d.made["left"].place);
    let want = place.apply(frame.flat_uv(
        panel,
        share[0] * panel.size(frame.u),
        share[1] * panel.size(frame.v),
    ));
    let now = component_middle(&doc, comp);
    let cell = doc.component(comp).expect("c").field.cell;
    assert!(
        (now[0] - want[0]).abs() <= cell && (now[1] - want[1]).abs() <= cell,
        "{now:?} {want:?}"
    );
    let nb = super::attach::component_box(&doc, comp).expect("box");
    assert!((nb.width() - b.width()).abs() <= cell && (nb.height() - b.height()).abs() <= cell);
    assert_eq!(
        doc.sheet_of_component(doc.component(comp).expect("c")),
        d.made["left"].sheet
    );
    // The preview shows it on the panel's face.
    let preview = run(&mut doc, "furniture.preview", json!({ "design": id }));
    let p = preview["panels"]
        .as_array()
        .expect("panels")
        .iter()
        .find(|p| p["key"] == "left")
        .expect("left")
        .clone();
    let max_out: f64 = p["mesh"]["positions"]
        .as_array()
        .expect("positions")
        .chunks(3)
        .map(|q| q[0].as_f64().unwrap_or(0.0))
        .fold(f64::NEG_INFINITY, f64::max);
    // The left side's top faces the inside (+X): carving only reaches the
    // panel's own stock, so the relief never stands past its thickness (18).
    assert!(max_out <= 18.0 + 1e-6, "{max_out}");
    assert!(max_out > 17.0, "{max_out}");
    // Attachments and the panel under a point.
    let list = run(&mut doc, "furniture.attachments", json!({ "design": id }));
    assert_eq!(list["attachments"][0]["panel"], "left");
    assert_eq!(list["attachments"][0]["face"], "top");
    assert_eq!(list["attachments"][0]["faceWord"], "right");
    let o = doc.bbox_of(d.made["left"].vector).expect("left");
    let under = run(
        &mut doc,
        "furniture.panelOf",
        json!({ "sheet": d.made["left"].sheet, "point": [(o.min.x + o.max.x) / 2.0, (o.min.y + o.max.y) / 2.0] }),
    );
    assert_eq!(under["panel"], "left");
    assert_eq!(under["top"], "right");
    assert_eq!(under["bottom"], "left");
    // Carving toolpaths, made again in place.
    let first = run(&mut doc, "furniture.makeCarving", json!({ "design": id }));
    let again = run(&mut doc, "furniture.makeCarving", json!({ "design": id }));
    assert_eq!(first["ids"], again["ids"]);
    assert_eq!(first["ids"].as_array().map(Vec::len), Some(2));
}

#[test]
fn a_model_on_the_outer_face_turns_the_panel_over_and_says_which_joints_to_switch() {
    let dir = crate::test_support::TempDir::new("furniture-model-out");
    let stl = pyramid(dir.path());
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 1, "kick": 0, "back": "none" } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let up = made_layout(&doc, &design(&doc, id)).frames[0].up;
    let reply = run(
        &mut doc,
        "furniture.addModel",
        json!({ "design": id, "panel": "left", "face": "bottom", "source": { "file": stl } }),
    );
    let d = design(&doc, id);
    let gen = made_layout(&doc, &d);
    // One sided: the model's face goes up, and the dados the side needs on
    // its inside face are said, with the through joint to use.
    assert_eq!(gen.frames[0].up, -up);
    let issues = reply["summary"]["issues"].as_array().expect("issues");
    assert!(
        issues.iter().any(|i| i["suggest"] == "tabSlot"
            && i["message"]
                .as_str()
                .is_some_and(|m| m.starts_with("Left side needs pockets"))),
        "{issues:?}"
    );
}

#[test]
fn a_model_whose_face_has_dados_on_the_other_side_is_carved_from_the_back_in_a_double_sided_job() {
    let dir = crate::test_support::TempDir::new("furniture-model-back");
    let stl = pyramid(dir.path());
    let mut doc = Document::new();
    doc.begin("job");
    doc.set_job(JobSetup {
        width: 2440.0,
        height: 1220.0,
        thickness: 18.0,
        kind: crate::doc::JobKind::DoubleSided,
        ..JobSetup::default()
    })
    .expect("job");
    doc.commit();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 1, "kick": 0, "back": "none" } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let up = made_layout(&doc, &design(&doc, id)).frames[0].up;
    let reply = run(
        &mut doc,
        "furniture.addModel",
        json!({ "design": id, "panel": "left", "face": "bottom", "source": { "file": stl } }),
    );
    let issues = reply["summary"]["issues"]
        .as_array()
        .expect("issues")
        .clone();
    assert!(issues.is_empty(), "{issues:?}");
    let d = design(&doc, id);
    // The dados keep the inside up; the model went to the back side.
    assert_eq!(made_layout(&doc, &d).frames[0].up, up);
    let a = &d.attachments[0];
    assert!(a.other_side);
    assert!(doc.component(a.id).is_err(), "not on this side");
    doc.begin("look");
    doc.set_active_side(crate::doc::Side::Bottom).expect("side");
    let c = doc.component(a.id).expect("on the back side").clone();
    // Under the left side: the flip of where the panel lies on the top side.
    let o = d.made["left"].vector;
    let top_side_box = {
        doc.set_active_side(crate::doc::Side::Top).expect("side");
        let b = doc.bbox_of(o).expect("left");
        doc.set_active_side(crate::doc::Side::Bottom).expect("side");
        b
    };
    let m = component_middle(&doc, c.id);
    let (fx, _) = doc.job().flip_xy(m[0], m[1]);
    assert!(
        fx >= top_side_box.min.x && fx <= top_side_box.max.x,
        "{fx} {top_side_box:?}"
    );
    doc.rollback();
}

#[test]
fn a_model_set_to_stretch_grows_with_its_panel_from_its_first_size_each_time() {
    let dir = crate::test_support::TempDir::new("furniture-stretch");
    let stl = pyramid(dir.path());
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "height": 1200, "shelves": 0, "kick": 0, "back": "none" } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let reply = run(
        &mut doc,
        "furniture.addModel",
        json!({ "design": id, "panel": "left", "face": "top", "source": { "file": stl } }),
    );
    let comp = reply["ids"][0].as_u64().expect("component");
    let sides = |doc: &Document| {
        let b = super::attach::component_box(doc, comp).expect("box");
        let (w, h) = (b.width(), b.height());
        (w.min(h), w.max(h))
    };
    let (short, long) = sides(&doc);
    let cell = doc.component(comp).expect("c").field.cell;
    assert!(
        (short - long).abs() <= cell,
        "a square model: {short} {long}"
    );
    let stretched = run(
        &mut doc,
        "furniture.setStretch",
        json!({ "design": id, "kind": "component", "id": comp, "stretch": true }),
    );
    assert_eq!(stretched["design"], id);
    assert_eq!(doc.journal().undo_label(), Some("Stretch to Fit"));
    let list = run(&mut doc, "furniture.attachments", json!({ "design": id }));
    assert_eq!(list["attachments"][0]["stretch"], true);
    // 1200 to 1800 to 2400 tall: along the height the model grows by 1.5
    // and then by 2 of its first size; across the depth it keeps its size.
    for (height, factor) in [(1800.0, 1.5), (2400.0, 2.0)] {
        run(
            &mut doc,
            "furniture.update",
            json!({ "design": id, "values": { "height": height } }),
        );
        let (s, l) = sides(&doc);
        assert!(
            (s - short).abs() <= 2.0 * cell,
            "{height}: across {s} vs {short}"
        );
        assert!(
            (l - long * factor).abs() <= 2.0 * cell * factor,
            "{height}: along {l} vs {}",
            long * factor
        );
    }
    // Keep Size again: the next change leaves its size.
    run(
        &mut doc,
        "furniture.setStretch",
        json!({ "design": id, "kind": "component", "id": comp, "stretch": false }),
    );
    let (_, before) = sides(&doc);
    run(
        &mut doc,
        "furniture.update",
        json!({ "design": id, "values": { "height": 2000 } }),
    );
    let (_, after) = sides(&doc);
    assert!((after - before).abs() <= 2.0 * cell, "{before} {after}");
    // A vector or object not on the design is refused in plain words.
    let refused = try_run(
        &mut doc,
        "furniture.setStretch",
        json!({ "design": id, "kind": "component", "id": comp + 999, "stretch": true }),
    );
    assert!(refused.is_err());
}

#[test]
fn a_panel_moved_or_turned_by_hand_keeps_its_place_when_the_design_is_made_again() {
    use crate::geom::{Mat3, Vec2};
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "height": 1000, "shelves": 2, "kick": 0, "back": "none" } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let bounds = doc.job_setup().material_bounds();
    let left = design(&doc, id).made["left"].clone();
    assert!(!left.pinned);
    let b0 = doc.bbox_of(left.vector).expect("left");
    // Moved by hand into the top left corner of its sheet.
    let (dx, dy) = (
        bounds.min.x + 10.0 - b0.min.x,
        bounds.max.y - 10.0 - b0.max.y,
    );
    doc.begin("Move");
    doc.transform_entities(&[left.vector], &Mat3::translate(Vec2::new(dx, dy)));
    doc.commit();
    let moved = doc.bbox_of(left.vector).expect("moved");
    let clear_of_others = |doc: &Document, d: &FurnitureDesign, key: &str| {
        let me = &d.made[key];
        let b = doc.bbox_of(me.vector).expect("me");
        for (k, m) in &d.made {
            if k == key || m.sheet != me.sheet {
                continue;
            }
            let o = doc.bbox_of(m.vector).expect("other");
            let apart = b.max.x <= o.min.x
                || o.max.x <= b.min.x
                || b.max.y <= o.min.y
                || o.max.y <= b.min.y;
            assert!(apart, "{k} {o:?} lies over {key} {b:?}");
        }
    };
    // Wider: the side is the same size and stays where it was put.
    run(
        &mut doc,
        "furniture.update",
        json!({ "design": id, "values": { "width": 900 } }),
    );
    let d = design(&doc, id);
    let now = &d.made["left"];
    assert!(now.pinned);
    assert!((now.place[0] - left.place[0]).abs() < 1e-9);
    assert!((now.place[1] - (left.place[1] + dx)).abs() < 1e-6);
    assert!((now.place[2] - (left.place[2] + dy)).abs() < 1e-6);
    let b = doc.bbox_of(now.vector).expect("left");
    for (got, want) in [
        (b.min.x, moved.min.x),
        (b.min.y, moved.min.y),
        (b.max.x, moved.max.x),
        (b.max.y, moved.max.y),
    ] {
        assert!((got - want).abs() < 1e-6, "{b:?} {moved:?}");
    }
    clear_of_others(&doc, &d, "left");
    // Turned a quarter by hand and put near the bottom left corner, then
    // made again: it keeps the turn and the place.
    let c = [(b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0];
    let (w, h) = (b.width(), b.height());
    let quarter = Mat3::translate(Vec2::new(-c[0], -c[1]))
        .then(&Mat3::rotate(std::f64::consts::FRAC_PI_2))
        .then(&Mat3::translate(Vec2::new(
            bounds.min.x + 10.0 + h / 2.0,
            bounds.min.y + 110.0 + w / 2.0,
        )));
    doc.begin("Rotate");
    doc.transform_entities(&[now.vector], &quarter);
    doc.commit();
    let put = doc.bbox_of(now.vector).expect("put");
    let r = run(
        &mut doc,
        "furniture.update",
        json!({ "design": id, "values": { "width": 1000 } }),
    );
    // Panels in its way move to a sheet of their own, and say so.
    let issues = r["summary"]["issues"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(issues.iter().all(|i| i["panel"] != "left"), "{r}");
    assert!(issues.iter().all(|i| i["message"]
        .as_str()
        .is_some_and(|m| m.ends_with("to clear the panels moved by hand"))));
    let d = design(&doc, id);
    let turned = &d.made["left"];
    assert!(
        (turned.place[0] - (left.place[0] + std::f64::consts::FRAC_PI_2)).abs() < 1e-9,
        "{:?}",
        turned.place
    );
    let t = doc.bbox_of(turned.vector).expect("turned");
    for (got, want) in [
        (t.min.x, put.min.x),
        (t.min.y, put.min.y),
        (t.max.x, put.max.x),
        (t.max.y, put.max.y),
    ] {
        assert!((got - want).abs() < 1e-6, "{t:?} {put:?}");
    }
    assert!((t.width() - h).abs() < 1e-6 && (t.height() - w).abs() < 1e-6);
    clear_of_others(&doc, &d, "left");
    // Moved off its sheet: said, and nested again.
    doc.begin("Move");
    doc.transform_entities(&[turned.vector], &Mat3::translate(Vec2::new(0.0, 2000.0)));
    doc.commit();
    let r = run(
        &mut doc,
        "furniture.update",
        json!({ "design": id, "values": { "width": 900 } }),
    );
    assert!(
        r["summary"]["issues"].as_array().is_some_and(|a| a.iter().any(|i| i["message"]
            == "Left side no longer fits on its sheet where it was moved by hand; it is nested again")),
        "{r}"
    );
    let d = design(&doc, id);
    assert!(!d.made["left"].pinned);
    clear_of_others(&doc, &d, "left");
    // A design not touched by hand keeps nesting: the right side is not pinned.
    assert!(!d.made["right"].pinned);
}

#[test]
fn dowels_into_the_edges_a_panel_lies_on_are_drill_marks_and_hardware() {
    // A 9 mm back lying on an 18 mm side's back edge, 600 long: 7 dowels
    // of 8 mm (the usual size at most half the side), 75 mm in from the
    // ends; circles on the back's face, marks across the side's edge.
    let tpl = template(
        "face",
        json!([
            panel("side", "main", [0.0, 0.0, 0.0, 18.0, 291.0, 600.0]),
            panel("back", "thin", [0.0, 291.0, 0.0, 400.0, 9.0, 600.0])
        ]),
    );
    let (doc, id) = build_with(&tpl, "back~side", FJoint::Dowel);
    let d = design(&doc, id);
    let joint = doc
        .assembly()
        .joint_set
        .joints
        .iter()
        .find(|j| j.id == d.joints["back~side"])
        .cloned()
        .expect("joint");
    assert_eq!(joint.resolved.count, Some(7));
    assert_eq!(joint.resolved.diameter, Some(8.0));
    assert_eq!(joint.resolved.inset, Some(75.0));
    assert_eq!(joint.vectors.len(), 14);
    assert!(joint
        .vectors
        .iter()
        .all(|v| v.role == crate::joints::Role::Mark));
    // On the back: 8 mm circles. On the side: marks 8 wide, 24 deep.
    let sizes: Vec<(f64, f64)> = joint
        .vectors
        .iter()
        .map(|v| doc.bbox_of(v.id).expect("mark"))
        .map(|b| (b.width().min(b.height()), b.width().max(b.height())))
        .collect();
    let circles = sizes
        .iter()
        .filter(|(s, l)| (s - 8.0).abs() < 1e-6 && (l - 8.0).abs() < 1e-6)
        .count();
    let across = sizes
        .iter()
        .filter(|(s, l)| (s - 8.0).abs() < 1e-6 && (l - 24.0).abs() < 1e-6)
        .count();
    assert_eq!((circles, across), (7, 7), "{sizes:?}");
    let mut doc = doc;
    let hardware = run(&mut doc, "furniture.hardware", json!({ "design": id }));
    let row = hardware["rows"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|r| r["name"].as_str().is_some_and(|n| n.starts_with("Dowel")))
        })
        .cloned()
        .expect("a dowel row");
    assert_eq!(row["count"], 7.0);
    assert_eq!(row["name"], "Dowel 8 x 30 mm");
}

#[test]
fn a_dado_in_the_face_turned_away_is_cut_from_the_other_side_of_a_double_sided_job() {
    // A divider with a shelf on each side: its two dados are in opposite
    // faces. One sided, one of them is an issue; double sided, it is cut
    // from the job's other side, flipped to where it lies seen from there.
    let mut tpl = template(
        "divider",
        json!([
            panel("divider", "main", [291.0, 0.0, 0.0, 18.0, 300.0, 600.0]),
            panel("lower", "main", [18.0, 0.0, 200.0, 273.0, 300.0, 18.0]),
            panel("upper", "main", [309.0, 0.0, 400.0, 273.0, 300.0, 18.0])
        ]),
    );
    tpl["jointClasses"]["c"]["default"] = json!("dado");
    let mut one = job();
    let reply = run(&mut one, "furniture.create", json!({ "template": tpl }));
    let issues = reply["summary"]["issues"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(issues.len(), 1, "{issues:?}");

    let mut doc = Document::new();
    doc.begin("job");
    doc.set_job(JobSetup {
        width: 2440.0,
        height: 1220.0,
        thickness: 18.0,
        kind: crate::doc::JobKind::DoubleSided,
        ..JobSetup::default()
    })
    .expect("job");
    doc.commit();
    let reply = run(&mut doc, "furniture.create", json!({ "template": tpl }));
    let id = reply["design"].as_u64().expect("id");
    let issues = reply["summary"]["issues"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(issues.is_empty(), "{issues:?}");
    let d = design(&doc, id);
    assert_eq!(d.joints.len(), 2);
    assert_eq!((d.back.vectors.len(), d.back.toolpaths.len()), (1, 1));
    // The divider's two dado pockets: one cut here, one only drawn here.
    let divider = d.made["divider"].vector;
    let dbox = doc.bbox_of(divider).expect("divider");
    let pockets: Vec<u64> = doc
        .assembly()
        .joint_set
        .joints
        .iter()
        .flat_map(|j| j.vectors.iter())
        .filter(|v| matches!(v.role, crate::joints::Role::Pocket { .. }))
        .map(|v| v.id)
        .filter(|id| {
            doc.bbox_of(*id)
                .is_some_and(|b| b.min.x >= dbox.min.x - 7.0 && b.max.x <= dbox.max.x + 7.0)
        })
        .collect();
    assert_eq!(pockets.len(), 2);
    let cut_here: Vec<u64> = doc
        .toolpaths()
        .iter()
        .flat_map(|t| t.sources.iter().map(|s| s.id))
        .collect();
    let away: Vec<&u64> = pockets.iter().filter(|p| !cut_here.contains(p)).collect();
    assert_eq!(away.len(), 1, "one dado is not cut from this side");
    // Each pocket is where its shelf meets the divider: the middle of one
    // at the lower shelf's height (209), the other at the upper's (409).
    let gen = made_layout(&doc, &d);
    let k = gen
        .eval
        .panels
        .iter()
        .position(|p| p.key == "divider")
        .expect("divider");
    let place = place_of(d.made["divider"].place);
    let mut heights: Vec<f64> = pockets
        .iter()
        .map(|id| {
            let b = doc.bbox_of(*id).expect("pocket");
            let flat = place.back([(b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0]);
            gen.frames[k].pose().apply([flat[0], flat[1], 0.0])[2]
        })
        .collect();
    heights.sort_by(f64::total_cmp);
    assert!(
        (heights[0] - 209.0).abs() < 1e-6 && (heights[1] - 409.0).abs() < 1e-6,
        "{heights:?}"
    );
    let drawn = doc.bbox_of(*away[0]).expect("dado");
    let job_setup = *doc.job();
    let (x0, y0) = job_setup.flip_xy(drawn.min.x, drawn.min.y);
    let (x1, y1) = job_setup.flip_xy(drawn.max.x, drawn.max.y);
    let want = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
    let here = doc.active_side();
    doc.set_active_side(here.other()).expect("other side");
    let b = doc.bbox_of(d.back.vectors[0]).expect("back dado");
    for (got, w) in [
        (b.min.x, want.0),
        (b.min.y, want.1),
        (b.max.x, want.2),
        (b.max.y, want.3),
    ] {
        assert!((got - w).abs() < 1e-6, "{b:?} {want:?}");
    }
    // A third of the stock deep, from the other face.
    let t = doc.toolpath(d.back.toolpaths[0]).expect("toolpath");
    let deepest = t.moves.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    assert!((deepest + 6.0).abs() < 1e-6, "{deepest}");
    doc.set_active_side(here).expect("back");
    // Made again, the other side's cut is replaced, not added to; removed,
    // it goes.
    run(
        &mut doc,
        "furniture.update",
        json!({ "design": id, "values": {} }),
    );
    let d = design(&doc, id);
    assert_eq!((d.back.vectors.len(), d.back.toolpaths.len()), (1, 1));
    run(&mut doc, "furniture.remove", json!({ "design": id }));
    doc.set_active_side(here.other()).expect("other side");
    assert!(doc.entity(d.back.vectors[0]).is_err());
    assert!(doc.toolpath(d.back.toolpaths[0]).is_err());
    assert!(doc.toolpaths().is_empty());
}

#[test]
fn a_vector_drawn_on_a_panel_keeps_its_share_when_the_same_change_resizes_the_panel() {
    // A rectangle drawn in the middle of the top, then the width changed:
    // it is attached against the design as it was made, so it stays in the
    // middle of the wider top.
    use crate::doc::{Entity, EntityKind};
    use crate::geom::{rect, Point};
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 1, "kick": 0, "back": "none" } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let d = design(&doc, id);
    let middle_of = |doc: &Document, d: &FurnitureDesign| {
        let gen = made_layout(doc, d);
        let i = gen
            .eval
            .panels
            .iter()
            .position(|p| p.key == "top")
            .expect("top");
        let (panel, frame) = (&gen.eval.panels[i], &gen.frames[i]);
        place_of(d.made["top"].place).apply(frame.flat_uv(
            panel,
            panel.size(frame.u) / 2.0,
            panel.size(frame.v) / 2.0,
        ))
    };
    let m = middle_of(&doc, &d);
    doc.begin("Draw");
    let drawn = doc.insert_entity(Entity {
        id: 0,
        layer: doc.active_layer(),
        sheet: d.made["top"].sheet,
        revision: 0,
        name: None,
        parent: None,
        rotation: 0.0,
        kind: EntityKind::Path {
            path: rect(
                Point::new(m[0] - 50.0, m[1] - 25.0),
                Point::new(m[0] + 50.0, m[1] + 25.0),
            )
            .expect("rect"),
        },
    });
    doc.commit();
    run(
        &mut doc,
        "furniture.update",
        json!({ "design": id, "values": { "width": 1000 } }),
    );
    let d = design(&doc, id);
    let a = d
        .attachments
        .iter()
        .find(|a| a.id == drawn)
        .expect("attached");
    assert!(
        (a.at[0] - 0.5).abs() < 1e-9 && (a.at[1] - 0.5).abs() < 1e-9,
        "{:?}",
        a.at
    );
    let now = middle_of(&doc, &d);
    let b = doc.bbox_of(drawn).expect("drawn");
    let c = [(b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0];
    assert!(
        (c[0] - now[0]).abs() < 1e-6 && (c[1] - now[1]).abs() < 1e-6,
        "{c:?} {now:?}"
    );
}

#[test]
fn a_model_on_a_door_s_outer_face_is_raised_in_the_middle_of_the_door_in_the_preview() {
    let dir = crate::test_support::TempDir::new("furniture-door-model");
    let stl = pyramid(dir.path());
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "base-cabinet" }),
    );
    let id = reply["design"].as_u64().expect("id");
    let d = design(&doc, id);
    let door = d.made["door#0"].clone();
    let b = doc.bbox_of(door.vector).expect("door");
    let under = run(
        &mut doc,
        "furniture.panelOf",
        json!({ "sheet": door.sheet, "point": [(b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0] }),
    );
    let outside = if under["top"] == "front" {
        "top"
    } else {
        "bottom"
    };
    run(
        &mut doc,
        "furniture.addModel",
        json!({ "design": id, "panel": "door#0", "face": outside, "source": { "file": stl } }),
    );
    let preview = run(&mut doc, "furniture.preview", json!({ "design": id }));
    let p = preview["panels"]
        .as_array()
        .expect("panels")
        .iter()
        .find(|p| p["key"] == "door#0")
        .expect("door")
        .clone();
    let min: Vec<f64> = p["box"]["min"]
        .as_array()
        .expect("min")
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    let max: Vec<f64> = p["box"]["max"]
        .as_array()
        .expect("max")
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    // The door stands across Y at the front, 18 thick, its back at the
    // box's back: carving only reaches within that stock, so the model's
    // relief dips back from the flush front face (the box's front, no
    // longer standing past it); their middle across X and Z is the door's
    // middle.
    let front = max[1];
    let raised: Vec<[f64; 3]> = p["mesh"]["positions"]
        .as_array()
        .expect("positions")
        .chunks(3)
        .map(|q| {
            [
                q[0].as_f64().unwrap_or(0.0),
                q[1].as_f64().unwrap_or(0.0),
                q[2].as_f64().unwrap_or(0.0),
            ]
        })
        .filter(|q| q[1] < front - 0.5)
        .collect();
    assert!(!raised.is_empty(), "the model shows in front of the door");
    let (lo_x, hi_x) = raised
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), q| {
            (a.min(q[0]), b.max(q[0]))
        });
    let (lo_z, hi_z) = raised
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), q| {
            (a.min(q[2]), b.max(q[2]))
        });
    let mid = [(min[0] + max[0]) / 2.0, (min[2] + max[2]) / 2.0];
    let step = 3.0;
    assert!(
        ((lo_x + hi_x) / 2.0 - mid[0]).abs() < step,
        "x {lo_x}..{hi_x} vs {}",
        mid[0]
    );
    assert!(
        ((lo_z + hi_z) / 2.0 - mid[1]).abs() < step,
        "z {lo_z}..{hi_z} vs {}",
        mid[1]
    );
    // The raised cells face out of the door's front: wound counter-clockwise
    // seen from the front, as their normals say.
    let pos: Vec<f64> = p["mesh"]["positions"]
        .as_array()
        .expect("positions")
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    let normals: Vec<f64> = p["mesh"]["normals"]
        .as_array()
        .expect("normals")
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    let at = |k: usize| [pos[3 * k], pos[3 * k + 1], pos[3 * k + 2]];
    let mut checked = 0;
    for t in p["mesh"]["indices"].as_array().expect("indices").chunks(3) {
        let k: Vec<usize> = t
            .iter()
            .filter_map(Value::as_u64)
            .map(|k| k as usize)
            .collect();
        let (a, b, c) = (at(k[0]), at(k[1]), at(k[2]));
        if a[1] >= front - 0.5 {
            continue;
        }
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        assert!(n[1] < 0.0, "a raised cell faces into the door: {n:?}");
        assert!(
            normals[3 * k[0] + 1] <= 1e-9,
            "its normal points out of the front"
        );
        checked += 1;
    }
    assert!(checked > 0);
}

/// A flat component `height` high over a small square at `mid` (job
/// millimetres), on `sheet`: bypasses `furniture.addModel`'s fit so a
/// model taller than its panel can be put on it directly, as a model made
/// in the Modeling tab and drawn over the panel by hand would be.
fn flat_model_on_sheet(doc: &mut Document, sheet: u64, mid: [f64; 2], height: f64) -> u64 {
    use crate::model::field::Field;
    use crate::model::{Combine, Component};
    let cell = 10.0;
    let (width, rows) = (4usize, 4usize);
    let origin = crate::geom::Point::new(
        mid[0] - (width as f64 - 1.0) * cell / 2.0,
        mid[1] - (rows as f64 - 1.0) * cell / 2.0,
    );
    let mut field = Field::empty(width, rows, cell, origin);
    field.z.fill(height as f32);
    doc.begin("Create Shape");
    let id = doc.add_component(Component {
        id: 0,
        name: "Too Tall".to_string(),
        visible: true,
        combine: Combine::Add,
        height_scale: 1.0,
        level: 0,
        extra: crate::model::tree::ComponentExtra {
            sheet,
            ..Default::default()
        },
        field,
    });
    doc.commit();
    id
}

#[test]
fn face_relief_mesh_height_never_exceeds_the_panel_thickness() {
    // A model far taller than the door it sits on: carving can only reach
    // within the panel's own stock, so the preview mesh must never stand
    // higher than the panel's thickness, however tall the model is.
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 1, "kick": 0, "back": "none" } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let d = design(&doc, id);
    let left = d.made["left"].clone();
    let outline = doc.bbox_of(left.vector).expect("left");
    let mid = [
        (outline.min.x + outline.max.x) / 2.0,
        (outline.min.y + outline.max.y) / 2.0,
    ];
    flat_model_on_sheet(&mut doc, left.sheet, mid, 197.0);
    run(
        &mut doc,
        "furniture.update",
        json!({ "design": id, "values": {} }),
    );
    let preview = run(&mut doc, "furniture.preview", json!({ "design": id }));
    let p = preview["panels"]
        .as_array()
        .expect("panels")
        .iter()
        .find(|p| p["key"] == "left")
        .expect("left")
        .clone();
    let thickness = p["thickness"].as_f64().expect("thickness");
    assert!((thickness - 18.0).abs() < 1e-6, "{thickness}");
    // The left panel's normal is X: nothing in its mesh, including the
    // model's relief, stands past its own thickness along it.
    let min_x: f64 = p["mesh"]["positions"]
        .as_array()
        .expect("positions")
        .chunks(3)
        .map(|q| q[0].as_f64().unwrap_or(0.0))
        .fold(f64::INFINITY, f64::min);
    let max_x: f64 = p["mesh"]["positions"]
        .as_array()
        .expect("positions")
        .chunks(3)
        .map(|q| q[0].as_f64().unwrap_or(0.0))
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        max_x - min_x <= thickness + 1e-6,
        "{min_x} .. {max_x} vs {thickness}"
    );
}

#[test]
fn a_too_tall_face_model_raises_an_issue_naming_the_panel_and_both_heights() {
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 1, "kick": 0, "back": "none" } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let d = design(&doc, id);
    let left = d.made["left"].clone();
    let outline = doc.bbox_of(left.vector).expect("left");
    let mid = [
        (outline.min.x + outline.max.x) / 2.0,
        (outline.min.y + outline.max.y) / 2.0,
    ];
    flat_model_on_sheet(&mut doc, left.sheet, mid, 197.0);
    let reply = run(
        &mut doc,
        "furniture.update",
        json!({ "design": id, "values": {} }),
    );
    let issues = reply["summary"]["issues"].as_array().expect("issues");
    let found = issues
        .iter()
        .find(|i| i["panel"] == "left" && i["message"].as_str().is_some_and(|m| m.contains("197")))
        .unwrap_or_else(|| panic!("no too tall issue: {issues:?}"));
    let message = found["message"].as_str().expect("message");
    assert!(message.contains("18"), "{message}");
    assert!(message.contains("cut flat"), "{message}");
}

#[test]
fn add_model_s_default_height_fits_the_panel_stock_less_the_skin() {
    // A model much taller than the door it is added to: the default
    // height scaling keeps at least the skin (3 mm, no Material Position
    // in Material gap set) under its deepest point.
    let dir = crate::test_support::TempDir::new("furniture-tall-model");
    let stl = pyramid_of_height(dir.path(), 500.0);
    let mut doc = job();
    let reply = run(
        &mut doc,
        "furniture.create",
        json!({ "template": "bookcase", "values": { "shelves": 1, "kick": 0, "back": "none" } }),
    );
    let id = reply["design"].as_u64().expect("id");
    let reply = run(
        &mut doc,
        "furniture.addModel",
        json!({ "design": id, "panel": "left", "face": "top", "source": { "file": stl } }),
    );
    let ids: Vec<u64> = reply["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(ids.len(), 1);
    let c = doc.component(ids[0]).expect("component");
    let top = crate::model::component_cmd::top_of(c).expect("a model");
    // The left panel is 18 mm thick: fits inside it less the 3 mm default
    // skin, never past it.
    assert!(top <= 15.0 + 1e-6, "{top}");
    assert!(top > 0.0, "{top}");
}
