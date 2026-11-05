//! Connections between panels, found from their boxes (tolerance 0.01 mm),
//! their classes from the template's rules, the joints valid for each and
//! the joint each one gets: the maker's choice for the connection, else
//! for its class, else the design's joinery style.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::joints::{JointKind, JointParams};

use super::eval::{Issue, Panel, TOL};
use super::template::Template;

/// How two panels meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnType {
    /// A's edge face on B's broad face, away from B's edges.
    Tee,
    /// A's edge face on B's broad face at B's end (or the boxes meet along
    /// a line at a corner).
    Corner,
    /// The boxes pass through each other.
    Cross,
    /// Coplanar panels sharing an edge.
    Edge,
    /// A thin panel (A) lying across B's edge.
    FaceOnEdges,
}

impl ConnType {
    /// The words used in messages.
    pub fn words(self) -> &'static str {
        match self {
            ConnType::Tee => "T",
            ConnType::Corner => "corner",
            ConnType::Cross => "cross",
            ConnType::Edge => "edge to edge",
            ConnType::FaceOnEdges => "face on edges",
        }
    }
}

/// The joints a maker can pick for a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FJoint {
    /// Butt: screws or glue, no cut.
    None,
    Dado,
    /// A dado stopped short of B's front edge.
    StoppedDado,
    TabSlot,
    MortiseTenon,
    WedgedTenon,
    KerfTenon,
    Dowel,
    CaptiveNut,
    TwistLock,
    Rabbet,
    Finger,
    CornerDovetail,
    CrossLap,
    Dovetail,
    Butterfly,
    Puzzle,
    /// Knock-down cam fittings.
    CamLock,
    /// A dado for thin stock.
    Groove,
    /// A butt joint held by screws (marks to drill).
    Screws,
}

impl FJoint {
    /// The Joint Creator kind that makes it, `None` for a butt joint.
    pub fn kind(self) -> Option<JointKind> {
        Some(match self {
            FJoint::None => return None,
            FJoint::Dado | FJoint::StoppedDado => JointKind::Dado,
            FJoint::TabSlot => JointKind::TabSlot,
            FJoint::MortiseTenon => JointKind::MortiseTenon,
            FJoint::WedgedTenon => JointKind::WedgedTenon,
            FJoint::KerfTenon => JointKind::KerfTenon,
            FJoint::Dowel => JointKind::Dowel,
            FJoint::CaptiveNut => JointKind::CaptiveNut,
            FJoint::TwistLock => JointKind::TwistLock,
            FJoint::Rabbet => JointKind::Rabbet,
            FJoint::Finger => JointKind::Finger,
            FJoint::CornerDovetail => JointKind::CornerDovetail,
            FJoint::CrossLap => JointKind::CrossLap,
            FJoint::Dovetail => JointKind::Dovetail,
            FJoint::Butterfly => JointKind::Butterfly,
            FJoint::Puzzle => JointKind::Puzzle,
            FJoint::CamLock => JointKind::CamLock,
            FJoint::Groove => JointKind::Groove,
            FJoint::Screws => JointKind::Screws,
        })
    }

    /// The name in messages.
    pub fn title(self) -> &'static str {
        match self {
            FJoint::None => "Butt joint",
            FJoint::StoppedDado => "Stopped dado",
            other => other.kind().map_or("Butt joint", JointKind::title),
        }
    }

    /// Whether the joint sinks pockets into B's face from the top, so B's
    /// top must face A.
    pub fn pockets_b(self) -> bool {
        matches!(
            self,
            FJoint::Dado
                | FJoint::StoppedDado
                | FJoint::Groove
                | FJoint::Rabbet
                | FJoint::Dowel
                | FJoint::CamLock
        )
    }

    /// Whether the joint's cut in B is made from B's outside face (the
    /// sockets of a corner dovetail).
    pub fn from_outside(self) -> bool {
        self == FJoint::CornerDovetail
    }
}

/// The joinery style of a design: one joint per connection type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Style {
    /// The template's own defaults, class by class.
    #[default]
    Template,
    /// Dado, rabbet and dovetail.
    Traditional,
    /// Knock down: tabs and slots, captive nuts, no glue.
    FlatPack,
    /// Dowels everywhere.
    QuickBuild,
    /// Fingers and tabs for a laser or thin stock.
    Laser,
    /// No cuts: glue and screws.
    GlueScrews,
}

impl Style {
    /// Every style, in menu order.
    pub const ALL: [Style; 6] = [
        Style::Template,
        Style::Traditional,
        Style::FlatPack,
        Style::QuickBuild,
        Style::Laser,
        Style::GlueScrews,
    ];

    /// The joint the style gives a connection type (before checking it is
    /// valid there); `None` for the template style.
    pub fn joint(self, kind: ConnType) -> Option<FJoint> {
        use ConnType as C;
        use FJoint as J;
        Some(match (self, kind) {
            (Style::Template, _) => return None,
            (_, C::Cross) => J::CrossLap,
            (Style::Traditional, C::Tee) => J::Dado,
            (Style::Traditional, C::Corner) => J::Rabbet,
            (Style::Traditional, C::Edge) => J::None,
            (Style::Traditional, C::FaceOnEdges) => J::Rabbet,
            (Style::FlatPack, C::Tee) => J::TabSlot,
            (Style::FlatPack, C::Corner) => J::CamLock,
            (Style::FlatPack, C::Edge) => J::Puzzle,
            (Style::FlatPack, C::FaceOnEdges) => J::None,
            (Style::QuickBuild, C::Tee | C::Corner) => J::Dowel,
            (Style::QuickBuild, C::Edge | C::FaceOnEdges) => J::None,
            (Style::Laser, C::Tee) => J::TabSlot,
            (Style::Laser, C::Corner) => J::Finger,
            (Style::Laser, C::Edge) => J::Puzzle,
            (Style::Laser, C::FaceOnEdges) => J::None,
            (Style::GlueScrews, C::Tee | C::Corner) => J::Screws,
            (Style::GlueScrews, _) => J::None,
        })
    }
}

/// A joint the maker picked, with its own sizes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JointChoice {
    pub joint: FJoint,
    #[serde(default, skip_serializing_if = "is_default_params")]
    pub params: JointParams,
}

fn is_default_params(p: &JointParams) -> bool {
    *p == JointParams::default()
}

/// Where a connection's joint comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Style,
    Class,
    Connection,
}

/// Two panels that meet, found from their boxes.
#[derive(Debug, Clone, PartialEq)]
pub struct Connection {
    /// `<panel key>~<panel key>`, keys sorted.
    pub key: String,
    /// Index of part A (the Joint Creator's first part) in the panels.
    pub a: usize,
    /// Index of part B.
    pub b: usize,
    pub kind: ConnType,
    /// The axis the faces touch across (the contact plane's normal), or for
    /// a crossing the axis both panels run along.
    pub axis: usize,
    /// The axis the seam runs along.
    pub line: usize,
    /// The seam: where the panels meet, furniture space.
    pub seam: [[f64; 3]; 2],
    /// The class from the template's rules, or "other".
    pub class: String,
    /// The joints valid here.
    pub choices: Vec<FJoint>,
    /// For a corner where the boxes only meet along a line: B grows by A's
    /// thickness to reach A's outer face first.
    pub line_touch: bool,
}

/// Whether panel `id` matches `pattern` (`|` between ids, `*` any).
pub fn matches(pattern: &str, id: &str) -> bool {
    pattern.split('|').any(|p| {
        let p = p.trim();
        p == "*" || p == id
    })
}

/// The connection key of two panel keys.
pub fn key_of(p: &str, q: &str) -> String {
    if p <= q {
        format!("{p}~{q}")
    } else {
        format!("{q}~{p}")
    }
}

fn overlap(p: &Panel, q: &Panel, k: usize) -> f64 {
    p.max[k].min(q.max[k]) - p.min[k].max(q.min[k])
}

/// The connection of panels `pi` and `qi`, if they meet; an issue when
/// they meet in a way no joint makes.
fn connect_pair(panels: &[Panel], pi: usize, qi: usize) -> Result<Option<Connection>, Issue> {
    let (p, q) = (&panels[pi], &panels[qi]);
    let ov = [0, 1, 2].map(|k| overlap(p, q, k));
    if ov.iter().any(|o| *o < -TOL) {
        return Ok(None);
    }
    let touch: Vec<usize> = (0..3).filter(|k| ov[*k].abs() <= TOL).collect();
    let key = key_of(&p.key, &q.key);
    let blank = |a: usize, b: usize, kind: ConnType, axis: usize, line: usize| Connection {
        key: key.clone(),
        a,
        b,
        kind,
        axis,
        line,
        seam: [[0.0; 3]; 2],
        class: String::new(),
        choices: Vec::new(),
        line_touch: false,
    };
    match touch.len() {
        0 => {
            if p.normal == q.normal {
                return Err(Issue::on_connection(
                    format!(
                        "{} and {} overlap; change the sizes so they only touch",
                        p.name, q.name
                    ),
                    &key,
                ));
            }
            let common = 3 - p.normal - q.normal;
            let (a, b) = if p.size(common) <= q.size(common) {
                (pi, qi)
            } else {
                (qi, pi)
            };
            Ok(Some(blank(a, b, ConnType::Cross, common, common)))
        }
        1 => {
            let k = touch[0];
            match (p.normal == k, q.normal == k) {
                (true, true) => Ok(None),
                (false, true) => Ok(Some(face_contact(panels, pi, qi, k, &key))),
                (true, false) => Ok(Some(face_contact(panels, qi, pi, k, &key))),
                (false, false) => {
                    let n = p.normal;
                    let coplanar = q.normal == n
                        && (p.min[n] - q.min[n]).abs() <= TOL
                        && (p.max[n] - q.max[n]).abs() <= TOL;
                    if !coplanar {
                        return Ok(None);
                    }
                    let line = 3 - n - k;
                    Ok(Some(blank(pi, qi, ConnType::Edge, k, line)))
                }
            }
        }
        2 => {
            let (k1, k2) = (touch[0], touch[1]);
            let pair = (p.normal == k1 && q.normal == k2) || (p.normal == k2 && q.normal == k1);
            if !pair {
                return Ok(None);
            }
            // A's edge face lies in B's broad face's plane, B grows to
            // A's outer face.
            let line = 3 - k1 - k2;
            let mut c = blank(pi, qi, ConnType::Corner, q.normal, line);
            c.line_touch = true;
            Ok(Some(c))
        }
        _ => Ok(None),
    }
}

/// A's edge face (panel `e`) on B's broad face (panel `f`) across axis `k`.
fn face_contact(panels: &[Panel], e: usize, f: usize, k: usize, key: &str) -> Connection {
    let (pe, pf) = (&panels[e], &panels[f]);
    let ne = pe.normal;
    let line = 3 - k - ne;
    let off_lo = pe.min[ne] - pf.min[ne];
    let off_hi = pf.max[ne] - pe.max[ne];
    let flush = off_lo.abs() <= TOL || off_hi.abs() <= TOL;
    let (a, b, kind) = if pf.thickness < pe.thickness - TOL {
        (f, e, ConnType::FaceOnEdges)
    } else if flush {
        (e, f, ConnType::Corner)
    } else {
        (e, f, ConnType::Tee)
    };
    Connection {
        key: key.to_string(),
        a,
        b,
        kind,
        axis: k,
        line,
        seam: [[0.0; 3]; 2],
        class: String::new(),
        choices: Vec::new(),
        line_touch: false,
    }
}

/// The span the two panels share along `axis`.
pub fn shared(p: &Panel, q: &Panel, axis: usize) -> (f64, f64) {
    (p.min[axis].max(q.min[axis]), p.max[axis].min(q.max[axis]))
}

/// Where the panels of `c` meet, furniture space.
fn seam(panels: &[Panel], c: &Connection) -> [[f64; 3]; 2] {
    let (pa, pb) = (&panels[c.a], &panels[c.b]);
    let mid = [0, 1, 2].map(|k| {
        let (lo, hi) = shared(pa, pb, k);
        (lo + hi) / 2.0
    });
    let (lo, hi) = shared(pa, pb, c.line);
    let mut from = mid;
    let mut to = mid;
    from[c.line] = lo;
    to[c.line] = hi;
    [from, to]
}

/// Whether the panels of `c` share their whole span along the seam: the
/// edges a corner or edge joint pairs are the same length.
fn same_span(panels: &[Panel], c: &Connection) -> bool {
    let (pa, pb) = (&panels[c.a], &panels[c.b]);
    (pa.min[c.line] - pb.min[c.line]).abs() <= TOL && (pa.max[c.line] - pb.max[c.line]).abs() <= TOL
}

/// Whether B of a face on edges connection ends flush with A's border, so
/// A can sit in a rabbet along B's edge.
fn at_border(panels: &[Panel], c: &Connection) -> bool {
    let (pa, pb) = (&panels[c.a], &panels[c.b]);
    let nb = pb.normal;
    (pb.min[nb] - pa.min[nb]).abs() <= TOL || (pb.max[nb] - pa.max[nb]).abs() <= TOL
}

/// Whether A's edge stays within B along B's thickness axis (no part of
/// A's edge face hangs past B's edge).
fn within(panels: &[Panel], c: &Connection) -> bool {
    let (pa, pb) = (&panels[c.a], &panels[c.b]);
    let na = pa.normal;
    pa.min[na] >= pb.min[na] - TOL && pa.max[na] <= pb.max[na] + TOL
}

/// The joints valid for `c`.
fn choices(panels: &[Panel], c: &Connection) -> Vec<FJoint> {
    use FJoint as J;
    match c.kind {
        ConnType::Tee if !within(panels, c) => vec![J::None],
        ConnType::Tee => vec![
            J::None,
            J::Dado,
            J::StoppedDado,
            J::TabSlot,
            J::MortiseTenon,
            J::WedgedTenon,
            J::KerfTenon,
            J::Dowel,
            J::CaptiveNut,
            J::CamLock,
            J::TwistLock,
            J::Groove,
            J::Screws,
        ],
        ConnType::Corner => {
            let mut out = vec![J::None, J::Rabbet];
            if same_span(panels, c) {
                out.extend([J::Finger, J::CornerDovetail]);
            }
            out.extend([J::Dowel, J::CaptiveNut, J::CamLock, J::Screws]);
            out
        }
        ConnType::Cross => vec![J::CrossLap],
        ConnType::Edge => {
            let mut out = vec![J::None];
            if same_span(panels, c) {
                out.extend([J::Dovetail, J::Butterfly, J::Puzzle]);
            }
            out.push(J::Dowel);
            out
        }
        ConnType::FaceOnEdges => {
            if at_border(panels, c) {
                vec![J::None, J::Rabbet, J::Dowel]
            } else {
                vec![J::None]
            }
        }
    }
}

/// Every connection between `panels`, sorted by key, with its class and
/// valid joints; issues for panels that meet in a way no joint makes.
pub fn connections(template: &Template, panels: &[Panel]) -> (Vec<Connection>, Vec<Issue>) {
    let mut out = Vec::new();
    let mut issues = Vec::new();
    for pi in 0..panels.len() {
        for qi in pi + 1..panels.len() {
            match connect_pair(panels, pi, qi) {
                Ok(Some(mut c)) => {
                    c.seam = seam(panels, &c);
                    c.choices = choices(panels, &c);
                    let (ida, idb) = (&panels[c.a].id, &panels[c.b].id);
                    c.class = template
                        .connections
                        .iter()
                        .find(|r| {
                            (matches(&r.a, ida) && matches(&r.b, idb))
                                || (matches(&r.a, idb) && matches(&r.b, ida))
                        })
                        .map_or_else(|| "other".to_string(), |r| r.class.clone());
                    out.push(c);
                }
                Ok(None) => {}
                Err(issue) => issues.push(issue),
            }
        }
    }
    out.sort_by(|x, y| x.key.cmp(&y.key));
    (out, issues)
}

/// The joint a connection gets and where it comes from: the maker's choice
/// for the connection, else for its class, else the style (the template's
/// class default for the template style, the first valid joint when the
/// style's joint is not valid here).
pub fn resolve(
    template: &Template,
    c: &Connection,
    style: Style,
    classes: &BTreeMap<String, JointChoice>,
    picked: &BTreeMap<String, JointChoice>,
) -> (JointChoice, Source) {
    if let Some(choice) = picked.get(&c.key).filter(|j| c.choices.contains(&j.joint)) {
        return (choice.clone(), Source::Connection);
    }
    if let Some(choice) = classes
        .get(&c.class)
        .filter(|j| c.choices.contains(&j.joint))
    {
        return (choice.clone(), Source::Class);
    }
    let first = c.choices.first().copied().unwrap_or(FJoint::None);
    let valid = |j: Option<FJoint>| j.filter(|j| c.choices.contains(j));
    let joint = match style {
        Style::Template => {
            let default = template
                .joint_classes
                .get(&c.class)
                .and_then(|d| serde_json::from_value::<FJoint>(serde_json::json!(d.default)).ok());
            match default {
                Some(d) => valid(Some(d))
                    .or_else(|| valid(Style::Traditional.joint(c.kind)))
                    .unwrap_or(first),
                None => first,
            }
        }
        other => valid(other.joint(c.kind)).unwrap_or(first),
    };
    (
        JointChoice {
            joint,
            params: JointParams::default(),
        },
        Source::Style,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::furniture::eval::evaluate_template;
    use crate::furniture::template::builtin_template;
    use serde_json::json;

    fn bookcase(values: serde_json::Value) -> (Template, Vec<Panel>) {
        let t = builtin_template("bookcase").expect("bookcase");
        let given = serde_json::from_value(values).expect("values");
        let e = evaluate_template(&t, &given).expect("works out");
        (t, e.panels)
    }

    fn find<'a>(list: &'a [Connection], key: &str) -> &'a Connection {
        list.iter().find(|c| c.key == key).unwrap_or_else(|| {
            panic!(
                "no {key} in {:?}",
                list.iter().map(|c| &c.key).collect::<Vec<_>>()
            )
        })
    }

    #[test]
    fn the_bookcase_connects_as_a_maker_would_sketch_it() {
        let (t, panels) = bookcase(json!({ "shelves": 2 }));
        let (list, issues) = connections(&t, &panels);
        assert!(issues.is_empty(), "{issues:?}");
        let name = |i: usize| panels[i].key.as_str();
        // A shelf stands in the middle of a side: a T, the shelf is A.
        let c = find(&list, "left~shelf#0");
        assert_eq!(c.kind, ConnType::Tee);
        assert_eq!((name(c.a), name(c.b)), ("shelf#0", "left"));
        assert_eq!(c.class, "shelf");
        assert_eq!(c.axis, 0);
        assert_eq!(c.line, 1);
        // The top sits at the top end of the sides: a corner.
        let c = find(&list, "left~top");
        assert_eq!(c.kind, ConnType::Corner);
        assert_eq!((name(c.a), name(c.b)), ("top", "left"));
        // The bottom sits 80 up for the kick: a T.
        assert_eq!(find(&list, "bottom~left").kind, ConnType::Tee);
        // The inset back meets the sides at their back ends.
        let c = find(&list, "back~left");
        assert_eq!(c.kind, ConnType::Corner);
        assert_eq!((name(c.a), name(c.b)), ("back", "left"));
        // The top's back edge lies on the thinner back: face on edges.
        let c = find(&list, "back~top");
        assert_eq!(c.kind, ConnType::FaceOnEdges);
        assert_eq!((name(c.a), name(c.b)), ("back", "top"));
        assert_eq!(c.choices, vec![FJoint::None, FJoint::Rabbet, FJoint::Dowel]);
        // The bottom is 80 up, not at the back's border: no rabbet.
        assert_eq!(find(&list, "back~bottom").choices, vec![FJoint::None]);
        // Shelves do not touch each other, sides do not touch each other.
        assert!(list
            .iter()
            .all(|c| c.key != "shelf#0~shelf#1" && c.key != "left~right"));
        // The seam of a shelf runs front to back in the middle of its
        // thickness, on the side's inner face.
        let c = find(&list, "left~shelf#0");
        let z = panels[c.a].centre()[2];
        assert_eq!(c.seam, [[18.0, 0.0, z], [18.0, 291.0, z]]);
    }

    #[test]
    fn an_overlay_back_lies_across_the_edges() {
        let (t, panels) = bookcase(json!({ "back": "overlay", "shelves": 1 }));
        let (list, _) = connections(&t, &panels);
        let c = find(&list, "back~left");
        assert_eq!(c.kind, ConnType::FaceOnEdges);
        assert_eq!(panels[c.a].key, "back");
        assert!(c.choices.contains(&FJoint::Rabbet));
        // A shelf's back edge is in the middle of the back.
        assert_eq!(find(&list, "back~shelf#0").choices, vec![FJoint::None]);
    }

    #[test]
    fn crossings_and_line_corners_are_found() {
        let t = builtin_template("bookcase").expect("bookcase");
        let panel = |key: &str, min: [f64; 3], max: [f64; 3], normal: usize| Panel {
            key: key.into(),
            id: key.into(),
            index: None,
            name: key.into(),
            material: "main".into(),
            thickness: 18.0,
            min,
            max,
            normal,
            grain: Default::default(),
            features: Vec::new(),
            outline: None,
        };
        let panels = vec![
            panel("a", [0.0, 100.0, 0.0], [400.0, 118.0, 300.0], 1),
            panel("b", [200.0, 0.0, 0.0], [218.0, 400.0, 300.0], 0),
            // Meets b only along the line x 218, z 300.
            panel("c", [218.0, 0.0, 300.0], [600.0, 400.0, 318.0], 2),
        ];
        let (list, issues) = connections(&t, &panels);
        assert!(issues.is_empty(), "{issues:?}");
        let c = find(&list, "a~b");
        assert_eq!(c.kind, ConnType::Cross);
        assert_eq!(c.axis, 2);
        assert_eq!(c.choices, vec![FJoint::CrossLap]);
        let c = find(&list, "b~c");
        assert_eq!(c.kind, ConnType::Corner);
        assert!(c.line_touch);
        assert_eq!(c.line, 1);
        // Two panels of one plane that overlap cannot be joined.
        let clash = vec![
            panel("p", [0.0, 0.0, 0.0], [100.0, 18.0, 100.0], 1),
            panel("q", [50.0, 0.0, 0.0], [150.0, 18.0, 100.0], 1),
        ];
        let (list, issues) = connections(&t, &clash);
        assert!(list.is_empty());
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn a_connection_choice_beats_its_class_which_beats_the_style() {
        let (t, panels) = bookcase(json!({ "shelves": 1 }));
        let (list, _) = connections(&t, &panels);
        let c = find(&list, "left~shelf#0");
        let none = BTreeMap::new();
        // Template style: the class default.
        let (j, s) = resolve(&t, c, Style::Template, &none, &none);
        assert_eq!((j.joint, s), (FJoint::Dado, Source::Style));
        // A style reaches every connection the maker left alone.
        let (j, _) = resolve(&t, c, Style::QuickBuild, &none, &none);
        assert_eq!(j.joint, FJoint::Dowel);
        let classes: BTreeMap<String, JointChoice> = [(
            "shelf".to_string(),
            JointChoice {
                joint: FJoint::TabSlot,
                params: JointParams::default(),
            },
        )]
        .into_iter()
        .collect();
        let (j, s) = resolve(&t, c, Style::QuickBuild, &classes, &none);
        assert_eq!((j.joint, s), (FJoint::TabSlot, Source::Class));
        let picked: BTreeMap<String, JointChoice> = [(
            c.key.clone(),
            JointChoice {
                joint: FJoint::MortiseTenon,
                params: JointParams::default(),
            },
        )]
        .into_iter()
        .collect();
        let (j, s) = resolve(&t, c, Style::QuickBuild, &classes, &picked);
        assert_eq!((j.joint, s), (FJoint::MortiseTenon, Source::Connection));
        // A choice not valid here is passed over.
        let wrong: BTreeMap<String, JointChoice> = [(
            c.key.clone(),
            JointChoice {
                joint: FJoint::CrossLap,
                params: JointParams::default(),
            },
        )]
        .into_iter()
        .collect();
        let (j, s) = resolve(&t, c, Style::Template, &none, &wrong);
        assert_eq!((j.joint, s), (FJoint::Dado, Source::Style));
        // A class default not valid for the type falls back to the
        // traditional joint of the type: the bottom of a kickless case
        // is a corner, and its class asks for a dado.
        let (t, panels) = bookcase(json!({ "kick": 0, "shelves": 0 }));
        let (list, _) = connections(&t, &panels);
        let c = find(&list, "bottom~left");
        assert_eq!(c.kind, ConnType::Corner);
        let (j, _) = resolve(&t, c, Style::Template, &none, &none);
        assert_eq!(j.joint, FJoint::Rabbet);
    }

    #[test]
    fn every_style_gives_a_valid_joint_for_every_type() {
        for style in Style::ALL.into_iter().skip(1) {
            for kind in [
                ConnType::Tee,
                ConnType::Corner,
                ConnType::Cross,
                ConnType::Edge,
                ConnType::FaceOnEdges,
            ] {
                assert!(style.joint(kind).is_some(), "{style:?} {kind:?}");
            }
        }
        assert_eq!(Style::Traditional.joint(ConnType::Tee), Some(FJoint::Dado));
        assert_eq!(Style::Laser.joint(ConnType::Corner), Some(FJoint::Finger));
        assert_eq!(
            serde_json::to_value(FJoint::StoppedDado).expect("json"),
            json!("stoppedDado")
        );
        assert_eq!(
            serde_json::to_value(Style::FlatPack).expect("json"),
            json!("flatPack")
        );
    }
}
