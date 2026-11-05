//! Works a design out: the panels, their connections and joints, each
//! panel's frame and growth, and the joints laid out; then the joints made
//! by the Joint Creator on the parts laid out on a sheet of their own (the
//! preview's scratch document), so the panels' real outlines are known
//! before anything is nested or added to the job.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::assembly::pose::Pose;
use crate::doc::{Document, Entity, EntityKind, JobSetup};
use crate::error::CoreError;
use crate::geom::{path_bbox, transform_path, Mat3, Path, Point, SubPath, Vec2};
use crate::joints::geometry::{path_rings, P2};
use crate::joints::{EdgeLine, Joint, JointParams, JointSet, JointSettings, Outcome, Role, Target};

use super::connect::{self, Connection, FJoint, JointChoice, Source, Style};
use super::eval::{evaluate_template, Evaluated, Issue, Panel};
use super::features::FeatureVector;
use super::layout::{self, flat_rect, FlatTarget, Growth, PanelFrame, Spec};
use super::template::Template;

/// What a design asks for.
#[derive(Debug, Clone)]
pub struct Inputs<'a> {
    pub template: &'a Template,
    pub values: &'a BTreeMap<String, Value>,
    pub style: Style,
    pub classes: &'a BTreeMap<String, JointChoice>,
    pub picked: &'a BTreeMap<String, JointChoice>,
    /// The face each panel's models are carved into (+1 or -1 along its
    /// normal), by panel key.
    pub faces: BTreeMap<String, f64>,
    /// Whether the job is cut from both sides: then a panel's joints
    /// choose its top and its models on the other face are carved from
    /// below; else its models choose.
    pub double_sided: bool,
}

/// A design worked out, before anything is made.
#[derive(Debug, Clone)]
pub struct Generated {
    pub eval: Evaluated,
    pub conns: Vec<Connection>,
    /// The joint each connection asks for and where it comes from.
    pub resolved: Vec<(JointChoice, Source)>,
    /// The joint each connection gets (a butt joint where the one asked for
    /// cannot be made).
    pub used: Vec<JointChoice>,
    pub up: Vec<f64>,
    pub frames: Vec<PanelFrame>,
    pub growth: Vec<Growth>,
    /// The joints laid out (butt joints have none).
    pub specs: Vec<Spec>,
    /// Each panel's outline before the joints, flat coordinates.
    pub outlines: Vec<Path>,
    /// Each panel's feature vectors, flat coordinates.
    pub features: Vec<Vec<FeatureVector>>,
    pub issues: Vec<Issue>,
    /// Connections whose joint sinks pockets into the face of B turned
    /// away from A: in a double sided job those pockets are cut from the
    /// job's other side.
    pub back: BTreeSet<String>,
}

/// A closed path through `ring`.
pub fn ring_path(ring: &[P2]) -> Path {
    let mut sub = SubPath::open(Point::new(ring[0][0], ring[0][1]));
    for p in &ring[1..] {
        sub.line_to(Point::new(p[0], p[1]));
    }
    sub.line_to(Point::new(ring[0][0], ring[0][1]));
    sub.closed = true;
    Path {
        subpaths: vec![sub],
    }
}

/// A panel's outline before joints: its grown box, or its template
/// outline with the growth added along the sides it touches.
fn outline_of(panel: &Panel, frame: &PanelFrame, g: &Growth) -> Path {
    let rect = flat_rect(panel, frame, g);
    let Some(points) = panel.outline.as_ref().filter(|p| p.len() >= 3) else {
        return ring_path(&rect);
    };
    let flip = if frame.mirror { -1.0 } else { 1.0 };
    let at = |p: &[f64; 3]| {
        let q = frame.flat_uv(panel, p[0], p[1]);
        Point::new(q[0], q[1])
    };
    let mut sub = SubPath::open(at(&points[0]));
    for k in 0..points.len() {
        let next = &points[(k + 1) % points.len()];
        let bulge = points[k][2] * flip;
        if bulge.abs() > 1e-9 {
            sub.arc_to(at(next), bulge);
        } else {
            sub.line_to(at(next));
        }
    }
    sub.closed = true;
    let path = Path {
        subpaths: vec![sub],
    };
    if g.iter().all(|s| s[0] == 0.0 && s[1] == 0.0) {
        return path;
    }
    // Growth: the strip between the box side and the grown side, over
    // the stretch of the side the outline runs along.
    let mut strips = vec![path.clone()];
    let (u, v) = (frame.u, frame.v);
    let size = [panel.size(u), panel.size(v)];
    for (axis, other, extent) in [(0usize, 1usize, size[0]), (1, 0, size[1])] {
        let k = if axis == 0 { u } else { v };
        for (side, by) in g[k].iter().copied().enumerate() {
            if by <= 0.0 {
                continue;
            }
            let line = if side == 0 { 0.0 } else { extent };
            let along: Vec<f64> = points
                .iter()
                .filter(|p| (p[axis] - line).abs() <= super::eval::TOL)
                .map(|p| p[other])
                .collect();
            let (Some(lo), Some(hi)) = (
                along.iter().copied().reduce(f64::min),
                along.iter().copied().reduce(f64::max),
            ) else {
                continue;
            };
            let out = if side == 0 { -by } else { extent + by };
            let corner = |a: f64, b: f64| {
                let (cu, cv) = if axis == 0 { (a, b) } else { (b, a) };
                frame.flat_uv(panel, cu, cv)
            };
            strips.push(ring_path(&crate::joints::geometry::ccw(vec![
                corner(line.min(out), lo),
                corner(line.max(out), lo),
                corner(line.max(out), hi),
                corner(line.min(out), hi),
            ])));
        }
    }
    match crate::geom::Tolerance::new(0.001).and_then(|tol| crate::geom::weld(&strips, tol)) {
        Ok(mut joined) if !joined.is_empty() => joined.swap_remove(0),
        _ => path,
    }
}

/// Works out `inputs`, leaving the connections in `failed` as butt joints
/// (their issues already said); `tool` is the cutter diameter (a feature
/// hole smaller than it is drilled).
///
/// # Errors
/// [`CoreError::Validation`] when the template does not work out for the
/// values.
pub fn lay_out(
    inputs: &Inputs,
    failed: &BTreeMap<usize, Issue>,
    tool: f64,
) -> Result<Generated, CoreError> {
    let eval = evaluate_template(inputs.template, inputs.values)?;
    let mut issues = eval.issues.clone();
    let (conns, found) = connect::connections(inputs.template, &eval.panels);
    issues.extend(found);
    let resolved: Vec<(JointChoice, Source)> = conns
        .iter()
        .map(|c| {
            connect::resolve(
                inputs.template,
                c,
                inputs.style,
                inputs.classes,
                inputs.picked,
            )
        })
        .collect();
    let butt = JointChoice {
        joint: FJoint::None,
        params: JointParams::default(),
    };
    let mut used: Vec<JointChoice> = resolved
        .iter()
        .enumerate()
        .map(|(i, (j, _))| {
            if failed.contains_key(&i) {
                butt.clone()
            } else {
                j.clone()
            }
        })
        .collect();
    issues.extend(failed.values().cloned());
    let faces: Vec<Option<f64>> = eval
        .panels
        .iter()
        .map(|p| inputs.faces.get(&p.key).copied())
        .collect();
    let up = layout::tops(&eval.panels, &conns, &used, &faces, inputs.double_sided);
    let mut back = BTreeSet::new();
    for (i, c) in conns.iter().enumerate() {
        if let Some(issue) = layout::wrong_face(&eval.panels, c, &used[i], &up) {
            if inputs.double_sided && layout::pockets_into_b(c, &used[i]) {
                back.insert(c.key.clone());
            } else {
                issues.push(issue);
                used[i] = butt.clone();
            }
        }
    }
    let frames: Vec<PanelFrame> = eval
        .panels
        .iter()
        .zip(&up)
        .map(|(p, u)| PanelFrame::of(p, *u))
        .collect();
    let growth = layout::growth(&eval.panels, &conns, &used);
    let ctx = layout::Ctx {
        panels: &eval.panels,
        frames: &frames,
        growth: &growth,
    };
    let mut specs = Vec::new();
    for (i, c) in conns.iter().enumerate() {
        match layout::spec(&ctx, i, c, &used[i]) {
            Ok(Some(s)) => specs.push(s),
            Ok(None) => {}
            Err(issue) => issues.push(issue),
        }
    }
    // Features: outline cuts and vectors. One sunk into the face that is
    // not cut from the top is left out and said.
    let middle = layout::middle_of(&eval.panels);
    let mut outlines = Vec::with_capacity(eval.panels.len());
    let mut features = Vec::with_capacity(eval.panels.len());
    for (i, panel) in eval.panels.iter().enumerate() {
        let mut usable = panel.clone();
        usable.features.retain(|f| match super::features::feature_face(panel, f, middle) {
            Some(face) if face != up[i] => {
                let words = feature_words(&f.kind);
                let (is, it) = if words.ends_with('s') {
                    ("are", "they are")
                } else {
                    ("is", "it is")
                };
                issues.push(Issue::on_panel(
                    format!(
                        "the {words} on {} {is} on the face cut from below; {it} left out until that panel is cut from both sides",
                        panel.name
                    ),
                    &panel.key,
                ));
                false
            }
            _ => true,
        });
        let free = layout::free_sides(&eval.panels, i, &frames[i]);
        let applied = super::features::apply(&usable, &frames[i], free, tool);
        issues.extend(applied.issues);
        let outline = outline_of(panel, &frames[i], &growth[i]);
        outlines.push(super::features::cut_outline(&outline, &applied.cuts));
        features.push(applied.vectors);
    }
    Ok(Generated {
        back,
        eval,
        conns,
        resolved,
        used,
        up,
        frames,
        growth,
        specs,
        outlines,
        features,
        issues,
    })
}

/// A 2D rigid transform from flat coordinates to a sheet's job
/// coordinates: a turn (radians, counter-clockwise) then a shift.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub turn: f64,
    pub shift: P2,
}

impl Placement {
    pub fn matrix(&self) -> Mat3 {
        Mat3::rotate(self.turn).then(&Mat3::translate(Vec2::new(self.shift[0], self.shift[1])))
    }

    pub fn apply(&self, p: P2) -> P2 {
        let (s, c) = self.turn.sin_cos();
        [
            c * p[0] - s * p[1] + self.shift[0],
            s * p[0] + c * p[1] + self.shift[1],
        ]
    }

    /// Job coordinates back to flat.
    pub fn back(&self, p: P2) -> P2 {
        let (s, c) = self.turn.sin_cos();
        let (x, y) = (p[0] - self.shift[0], p[1] - self.shift[1]);
        [c * x + s * y, -s * x + c * y]
    }

    /// As a 3D pose (turn about Z).
    pub fn pose(&self) -> Pose {
        let (s, c) = self.turn.sin_cos();
        Pose {
            r: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
            t: [self.shift[0], self.shift[1], 0.0],
        }
    }
}

/// The joint of `spec` on the vectors of its parts, placed by `places`.
pub fn joint_of(spec: &Spec, vectors: &[u64], places: &[Placement], id: u64) -> Joint {
    let pa = places[spec.a];
    let pb = places[spec.b];
    let a = EdgeLine {
        vector: vectors[spec.a],
        from: pa.apply(spec.a_edge.0),
        to: pa.apply(spec.a_edge.1),
    };
    let b = match spec.b_target {
        FlatTarget::Edge(from, to) => Target::Edge(EdgeLine {
            vector: vectors[spec.b],
            from: pb.apply(from),
            to: pb.apply(to),
        }),
        FlatTarget::Face {
            from,
            to,
            offset,
            along,
        } => Target::Face {
            edge: EdgeLine {
                vector: vectors[spec.b],
                from: pb.apply(from),
                to: pb.apply(to),
            },
            offset,
            along,
        },
    };
    Joint {
        id,
        kind: spec.kind,
        a,
        b,
        params: spec.params.clone(),
        vectors: Vec::new(),
        resolved: JointParams::default(),
        b_to_a: None,
        flip: spec.flip,
    }
}

/// A vector a joint made on the parts, flat coordinates of `panel`.
#[derive(Debug, Clone)]
pub struct MadeVector {
    pub panel: usize,
    pub path: Path,
    pub role: Role,
}

/// The joints made on a scratch sheet.
#[derive(Debug, Clone)]
pub struct Joined {
    /// Each panel's outline after its joints, flat coordinates.
    pub outlines: Vec<Path>,
    /// The holes, pockets and marks the joints made, by panel.
    pub made: Vec<MadeVector>,
    /// Each joint's outcome, by connection.
    pub outcomes: BTreeMap<usize, Outcome>,
    /// Where each panel lay on the scratch sheet.
    pub places: Vec<Placement>,
}

/// Room left between parts on the scratch sheet, millimetres.
const SCRATCH_GAP: f64 = 300.0;

/// Makes the joints of `gen` on the parts laid out in a row on a scratch
/// document, with `settings`.
///
/// # Errors
/// What adding vectors or sheets reports (not a joint that cannot be made:
/// its outcome says so).
pub fn join_scratch(gen: &Generated, settings: &JointSettings) -> Result<Joined, CoreError> {
    let panels = &gen.eval.panels;
    let mut places = Vec::with_capacity(panels.len());
    let mut x = SCRATCH_GAP;
    let mut height: f64 = 0.0;
    for path in &gen.outlines {
        let b = path_bbox(path).unwrap_or(crate::geom::BBox {
            min: Point::new(0.0, 0.0),
            max: Point::new(1.0, 1.0),
        });
        places.push(Placement {
            turn: 0.0,
            shift: [x - b.min.x, SCRATCH_GAP - b.min.y],
        });
        x += b.width() + SCRATCH_GAP;
        height = height.max(b.height());
    }
    let mut doc = Document::new();
    doc.begin("Furniture preview");
    let first = gen
        .eval
        .materials
        .values()
        .next()
        .map_or(18.0, |m| m.thickness);
    doc.set_job(JobSetup {
        width: x + SCRATCH_GAP,
        height: height + 2.0 * SCRATCH_GAP,
        thickness: first,
        ..*doc.job_setup()
    })?;
    let mut sheets: BTreeMap<String, u64> = BTreeMap::new();
    for (key, m) in &gen.eval.materials {
        let id = if sheets.is_empty() {
            doc.active_sheet()
        } else {
            doc.add_sheet(None)
        };
        let mut material = doc.sheet_material(doc.active_sheet())?;
        material.thickness = m.thickness;
        doc.set_sheet_material(id, material)?;
        sheets.insert(key.clone(), id);
    }
    let layer = doc.active_layer();
    let vectors: Vec<u64> = panels
        .iter()
        .zip(&gen.outlines)
        .zip(&places)
        .map(|((p, path), place)| {
            doc.insert_entity(Entity {
                id: 0,
                layer,
                sheet: sheets.get(&p.material).copied().unwrap_or(0),
                revision: 0,
                name: Some(p.name.clone()),
                parent: None,
                rotation: 0.0,
                kind: EntityKind::Path {
                    path: transform_path(path, &place.matrix()),
                },
            })
        })
        .collect();
    let mut set = JointSet {
        settings: settings.clone(),
        ..JointSet::default()
    };
    let mut by_joint = BTreeMap::new();
    for spec in &gen.specs {
        let id = set.take_id();
        by_joint.insert(id, spec.conn);
        set.joints.push(joint_of(spec, &vectors, &places, id));
    }
    let outcomes = crate::joints::rebuild_skipping(&mut doc, &mut set)?;
    let mut outlines = Vec::with_capacity(panels.len());
    for (i, v) in vectors.iter().enumerate() {
        let path = match &doc.entity(*v)?.kind {
            EntityKind::Path { path } => path.clone(),
            _ => gen.outlines[i].clone(),
        };
        let back = places[i].matrix().invert()?;
        outlines.push(transform_path(&path, &back));
    }
    let mut made = Vec::new();
    for joint in &set.joints {
        let Some(conn) = by_joint.get(&joint.id) else {
            continue;
        };
        let c = &gen.conns[*conn];
        for jv in &joint.vectors {
            let EntityKind::Path { path } = &doc.entity(jv.id)?.kind else {
                continue;
            };
            // On A or on B: the one whose outline holds it.
            let first = path_rings(path)
                .first()
                .and_then(|r| r.first().copied())
                .unwrap_or([0.0, 0.0]);
            let panel = [c.a, c.b]
                .into_iter()
                .find(|i| {
                    let back = places[*i].back(first);
                    crate::joints::geometry::inside_rings(&path_rings(&outlines[*i]), back)
                })
                .unwrap_or(c.b);
            let back = places[panel].matrix().invert()?;
            made.push(MadeVector {
                panel,
                path: transform_path(path, &back),
                role: jv.role,
            });
        }
    }
    let outcomes = outcomes
        .into_iter()
        .filter_map(|o| by_joint.get(&o.id).map(|c| (*c, o)))
        .collect();
    Ok(Joined {
        outlines,
        made,
        outcomes,
        places,
    })
}

/// Works out `inputs` and makes its joints on a scratch sheet. A joint
/// the Joint Creator cannot make is reported and left as a butt joint,
/// and the design worked out again without it.
///
/// # Errors
/// As [`lay_out`] and [`join_scratch`].
pub fn generate(
    inputs: &Inputs,
    settings: &JointSettings,
) -> Result<(Generated, Joined), CoreError> {
    let mut failed: BTreeMap<usize, Issue> = BTreeMap::new();
    let tool = settings.tool_diameter;
    for _ in 0..4 {
        let gen = lay_out(inputs, &failed, tool)?;
        let joined = join_scratch(&gen, settings)?;
        let broken: Vec<(usize, String)> = joined
            .outcomes
            .iter()
            .filter_map(|(c, o)| o.broken.clone().map(|m| (*c, m)))
            .collect();
        if broken.is_empty() {
            return Ok((gen, joined));
        }
        for (c, message) in broken {
            let conn = &gen.conns[c];
            let (pa, pb) = (&gen.eval.panels[conn.a], &gen.eval.panels[conn.b]);
            failed.insert(
                c,
                Issue::on_connection(
                    format!(
                        "the {} between {} and {} cannot be made ({message}), so it is a butt joint for now",
                        gen.used[c].joint.title().to_lowercase(),
                        pa.name,
                        pb.name
                    ),
                    &conn.key,
                ),
            );
        }
    }
    let gen = lay_out(inputs, &failed, tool)?;
    let joined = join_scratch(&gen, settings)?;
    Ok((gen, joined))
}

/// A feature kind in plain words: "hingeCups" reads "hinge cups".
fn feature_words(kind: &str) -> String {
    let mut out = String::with_capacity(kind.len() + 4);
    for c in kind.chars() {
        if c.is_ascii_uppercase() {
            out.push(' ');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod words_tests {
    #[test]
    fn feature_kinds_read_as_plain_words() {
        assert_eq!(super::feature_words("hingeCups"), "hinge cups");
        assert_eq!(super::feature_words("shelfPins"), "shelf pins");
        assert_eq!(super::feature_words("pocket"), "pocket");
    }
}
