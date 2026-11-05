//! Joint Creator: joints between flat parts, made on the parts' vectors.
//!
//! A part is a closed vector cut through from a sheet. A joint names the
//! straight edge of part A it sits on and either an edge of part B (corner,
//! crossing and edge to edge joints) or a line on B's top face where A
//! stands (T joints). Each joint rewrites the outlines of both parts with
//! booleans (material added or cut away, the inside corners relieved for
//! the round tool) and may add vectors of its own: holes, pockets and extra
//! parts such as wedges or keys.
//!
//! Joints stay live. The set keeps the path each vector had before a joint
//! first touched it; every change puts those paths back and applies every
//! joint again in order, reusing the ids of the vectors the joints made. So
//! removing a joint restores its edges exactly, and changing the job's fit,
//! relief, tool or thickness remakes every joint in one step.

pub mod commands;
pub mod geometry;
pub mod kinds;
pub mod pose;
#[cfg(test)]
pub(crate) mod tests;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::doc::{Document, Entity, EntityKind};
use crate::error::CoreError;
use crate::geom::Path;

use geometry::{arcs_path, path_rings, Circle, Frame, ReliefShape, P2};

/// The kinds of joint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JointKind {
    /// Box joint: alternating fingers on two edges meeting at a corner.
    Finger,
    /// Housing: a pocket in B's face the edge of A sits in.
    Dado,
    /// A step along B's edge that A's edge sits in.
    Rabbet,
    /// Tabs on A's edge through slots in B.
    TabSlot,
    /// One wide tenon on A in a mortise in B, through or blind.
    MortiseTenon,
    /// Slots half way into two crossing parts (egg crate).
    CrossLap,
    /// Half depth pockets on the ends of two parts laid over each other.
    HalfLap,
    /// A through tenon held by a wedge through it (tusk tenon).
    WedgedTenon,
    /// Tails with angled sides locking two panels edge to edge.
    Dovetail,
    /// Blind dowel holes in B along the line, marks on A's edge.
    Dowel,
    /// A bolt through B into a T slot in A with a square nut.
    CaptiveNut,
    /// A bow tie key across the seam of two panels edge to edge.
    Butterfly,
    /// Round knobs locking two panels edge to edge.
    Puzzle,
    /// Not a joint between two parts: a band of staggered slots across one
    /// part so it bends (a kerf or lattice hinge).
    LivingHinge,
    /// Fingers for two panels meeting at an angle other than square, both
    /// cut square from the top.
    VJoint,
    /// A part through a hole in another, locked by a quarter turn: a neck
    /// in the first part turns in a round middle of the hole.
    TwistLock,
    /// A locking box corner: tails on A's edge, sockets in B whose sloped
    /// walls are cut from the top in steps.
    CornerDovetail,
    /// A through tenon split by a saw kerf from its end and spread by a
    /// wedge driven into the kerf (a kerf wedged or split tenon).
    KerfTenon,
    /// Knock-down cam fittings: cam bores in A's face near its edge, a
    /// bolt hole in B's face for each, the bolt's bore into A's edge
    /// drilled by hand.
    CamLock,
    /// A dado sized for thin stock (a back or a drawer bottom).
    Groove,
    /// A butt joint held by screws: marks through B for each screw's
    /// countersunk clearance hole and on A's edge for its pilot.
    Screws,
}

impl JointKind {
    /// Whether the joint works on one part only (its second pick is the
    /// first part's own edge).
    pub fn one_part(self) -> bool {
        self == JointKind::LivingHinge
    }

    /// Whether part B is picked as a line on its face (T joints) rather
    /// than as an edge.
    pub fn on_face(self) -> bool {
        matches!(
            self,
            JointKind::Dado
                | JointKind::TabSlot
                | JointKind::MortiseTenon
                | JointKind::WedgedTenon
                | JointKind::KerfTenon
                | JointKind::Dowel
                | JointKind::CaptiveNut
                | JointKind::TwistLock
                | JointKind::CamLock
                | JointKind::Groove
                | JointKind::Screws
        )
    }

    /// The name used in vector names and messages.
    pub fn title(self) -> &'static str {
        match self {
            JointKind::Finger => "Finger joint",
            JointKind::Dado => "Dado",
            JointKind::Rabbet => "Rabbet",
            JointKind::TabSlot => "Tab and slot",
            JointKind::MortiseTenon => "Mortise and tenon",
            JointKind::CrossLap => "Cross lap",
            JointKind::HalfLap => "Half lap",
            JointKind::WedgedTenon => "Wedged tenon",
            JointKind::Dovetail => "Dovetail",
            JointKind::Dowel => "Dowels",
            JointKind::CaptiveNut => "Captive nut",
            JointKind::Butterfly => "Butterfly key",
            JointKind::Puzzle => "Puzzle joint",
            JointKind::LivingHinge => "Living hinge",
            JointKind::VJoint => "V joint",
            JointKind::TwistLock => "Twist lock",
            JointKind::CornerDovetail => "Corner dovetail",
            JointKind::KerfTenon => "Kerf tenon",
            JointKind::CamLock => "Cam lock",
            JointKind::Groove => "Groove",
            JointKind::Screws => "Screws",
        }
    }
}

/// How tight the parts go together, as a clearance on each side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Fit {
    /// Slides together by hand: 0.2 mm a side.
    Slip,
    /// Pressed or tapped together: 0.1 mm a side.
    #[default]
    Press,
    /// Room for glue: 0.15 mm a side.
    Glue,
    /// Goes together and apart again and again: 0.25 mm a side.
    KnockDown,
    /// The clearance typed in the settings.
    Custom,
}

impl Fit {
    /// The clearance on each side, in millimetres.
    pub fn clearance(self, custom: f64) -> f64 {
        match self {
            Fit::Slip => 0.2,
            Fit::Press => 0.1,
            Fit::Glue => 0.15,
            Fit::KnockDown => 0.25,
            Fit::Custom => custom,
        }
    }
}

/// How the inside corners a round tool cannot cut are relieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Relief {
    /// Left round (for a laser, or parts rounded by hand).
    None,
    /// A circle along the corner's bisector.
    #[default]
    DogBone,
    /// A circle along one wall; the faces the parts bear on stay straight.
    TBone,
}

impl Relief {
    fn shape(self) -> ReliefShape {
        match self {
            Relief::None => ReliefShape::None,
            Relief::DogBone => ReliefShape::DogBone,
            Relief::TBone => ReliefShape::TBone,
        }
    }
}

fn default_tool() -> f64 {
    6.35
}

fn default_custom() -> f64 {
    0.1
}

/// Settings every joint of the job shares; changing one remakes them all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JointSettings {
    /// How tight the joints are.
    pub fit: Fit,
    /// The clearance per side for [`Fit::Custom`], in millimetres.
    #[serde(default = "default_custom")]
    pub custom_clearance: f64,
    /// Corner relief.
    pub relief: Relief,
    /// Diameter of the end mill that cuts the joints, in millimetres.
    #[serde(default = "default_tool")]
    pub tool_diameter: f64,
    /// Measured stock thickness per sheet, in millimetres; a sheet left out
    /// uses its material thickness.
    pub thickness: BTreeMap<u64, f64>,
}

impl Default for JointSettings {
    fn default() -> Self {
        JointSettings {
            fit: Fit::Press,
            custom_clearance: default_custom(),
            relief: Relief::DogBone,
            tool_diameter: default_tool(),
            thickness: BTreeMap::new(),
        }
    }
}

impl JointSettings {
    /// The clearance per side, in millimetres.
    pub fn clearance(&self) -> f64 {
        self.fit.clearance(self.custom_clearance)
    }

    /// The thickness parts on `sheet` are cut from, in millimetres.
    pub fn thickness_of(&self, document: &Document, sheet: u64) -> f64 {
        if let Some(t) = self.thickness.get(&sheet) {
            return *t;
        }
        document
            .sheet_material(sheet)
            .map(|m| m.thickness)
            .unwrap_or_else(|_| document.job_setup().thickness)
    }

    /// Checks the values are usable.
    ///
    /// # Errors
    /// [`CoreError::Validation`] naming the value.
    pub fn validate(&self) -> Result<(), CoreError> {
        if !(0.0..=5.0).contains(&self.custom_clearance) || !self.custom_clearance.is_finite() {
            return Err(CoreError::validation(
                "the clearance must be between 0 and 5 mm a side",
            ));
        }
        if !(self.tool_diameter > 0.0 && self.tool_diameter < 100.0) {
            return Err(CoreError::validation(
                "the tool diameter must be more than 0 and less than 100 mm",
            ));
        }
        for t in self.thickness.values() {
            if !(*t > 0.0 && t.is_finite()) {
                return Err(CoreError::validation(
                    "a measured thickness must be more than 0",
                ));
            }
        }
        Ok(())
    }
}

/// A straight edge of a part: the closed vector and the edge's ends, taken
/// with the part on the left (counter-clockwise), as first picked.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeLine {
    /// The part's closed vector.
    pub vector: u64,
    /// Where the edge starts, job millimetres.
    pub from: P2,
    /// Where it ends.
    pub to: P2,
}

/// What part B of a joint is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Target {
    /// An edge of B.
    Edge(EdgeLine),
    /// A line on B's top face, parallel to one of B's edges (`edge`), where
    /// A stands: A's near face `offset` in from that edge, A's middle
    /// `along` from the edge's start.
    Face {
        /// B's edge the line runs along.
        edge: EdgeLine,
        /// From that edge to the near face of A, millimetres.
        offset: f64,
        /// From the edge's start to the middle of A, millimetres.
        along: f64,
    },
}

impl Target {
    /// B's closed vector.
    pub fn vector(&self) -> u64 {
        match self {
            Target::Edge(e) | Target::Face { edge: e, .. } => e.vector,
        }
    }

    /// B's edge (the picked one, or the one the face line runs along).
    pub fn edge(&self) -> EdgeLine {
        match self {
            Target::Edge(e) | Target::Face { edge: e, .. } => *e,
        }
    }
}

/// Where fingers start on part A's edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Start {
    /// A keeps both ends of its edge (the corners).
    Finger,
    /// B keeps them.
    Gap,
}

/// The slots of a living hinge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HingePattern {
    /// Straight staggered slots: bends one way.
    Straight,
    /// Wavy staggered slots: bends one way, softer, and twists a little.
    Sinuous,
    /// A grid of spiral cells: bends both ways, for domes and curves.
    Spiral,
}

/// Tab and tenon ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Ends {
    Square,
    Rounded,
}

/// A joint's own sizes, in millimetres. Each kind reads some of them; left
/// out, the kernel works them out from the parts and reports what it used.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JointParams {
    /// Fingers, tabs, dowels, bolts or keys along the edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// Width of each finger, tab or key along the edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// Pocket, dado or blind depth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<f64>,
    /// Where the fingers start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<Start>,
    /// Makes the first finger or tab this share wider so the parts fit one
    /// way only (0 to 0.5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bias: Option<f64>,
    /// Blind instead of through.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blind: Option<bool>,
    /// A stopped dado stops this far short of B's front edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<f64>,
    /// How far tabs and tenons stand out past B.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proud: Option<f64>,
    /// Tab and tenon ends.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends: Option<Ends>,
    /// Snap detent bump height a side (cross lap).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detent: Option<f64>,
    /// Dovetail side angle, degrees from square; the angle between the
    /// panels of a V joint; how far a living hinge bends, degrees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
    /// The inside radius a living hinge bends round, millimetres.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
    /// The slots of a living hinge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<HingePattern>,
    /// Dowel or bolt diameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diameter: Option<f64>,
    /// Distance of the outer dowels, tabs or bolts from the ends of the edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inset: Option<f64>,
    /// Bolt length, key length or knob size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// Square nut width across the flats.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nut_width: Option<f64>,
    /// Square nut thickness.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nut_thickness: Option<f64>,
    /// Where along A's edge a crossing slot sits, from the edge's start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_a: Option<f64>,
    /// Where along B's edge a crossing slot sits, from the edge's start;
    /// for a rabbet, where along B's edge A's shorter edge starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_b: Option<f64>,
    /// The depth of B's slot of a cross lap (half of B where it crosses
    /// when left out).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth_b: Option<f64>,
}

/// What a vector a joint made is for, so its toolpaths can be made.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Role {
    /// Cut right through (an inside profile).
    Hole,
    /// Cleared from the top to a depth.
    Pocket {
        /// Depth below the top, millimetres.
        depth: f64,
    },
    /// A new part (wedge, key), cut out from stock this thick.
    Part {
        /// Stock thickness, millimetres.
        thickness: f64,
    },
    /// A mark to drill or cut by hand; no toolpath.
    Mark,
    /// A socket wall leaning back from its foot at `depth` below the top,
    /// cut as a tapered wall with the tool on its right, moved as a whole
    /// as it rises.
    Taper {
        /// Depth of the foot below the top, millimetres.
        depth: f64,
        /// Lean from upright, degrees.
        angle: f64,
    },
}

/// A vector a joint made, with its role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JointVector {
    /// The vector.
    pub id: u64,
    /// What it is for.
    pub role: Role,
    /// For an extra part the joint seats (a key in its recess): its part
    /// space as drawn to part A's once assembled, a column major 4 by 4
    /// matrix, as last made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat: Option<[f64; 16]>,
    /// The lower left corner of the part's box where it was drawn (job
    /// millimetres), the origin of `seat`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner: Option<P2>,
}

/// A joint of the job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Joint {
    /// Id, unique in the job.
    pub id: u64,
    /// What kind of joint.
    pub kind: JointKind,
    /// Part A's edge.
    pub a: EdgeLine,
    /// Part B.
    pub b: Target,
    /// The sizes asked for; the rest are worked out.
    #[serde(default)]
    pub params: JointParams,
    /// The vectors the joint made, in the order it makes them.
    #[serde(default)]
    pub vectors: Vec<JointVector>,
    /// The sizes it was last made with, the worked out ones filled in.
    #[serde(default)]
    pub resolved: JointParams,
    /// B's part space to A's once assembled (column major 4 by 4), as last
    /// made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b_to_a: Option<[f64; 16]>,
    /// For a joint on B's face: A turned half round about its own middle,
    /// its top towards B's edge (see [`kinds`]). Chosen when the joint is
    /// made so that it agrees with the joints already holding A and B
    /// together (a stretcher between two legs a top holds).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flip: bool,
}

/// The joints of a job, the settings they share and the paths the vectors
/// they touch had before.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JointSet {
    /// Shared settings.
    pub settings: JointSettings,
    /// The joints in the order they apply.
    pub joints: Vec<Joint>,
    /// Each outline vector a joint rewrote, as it was before any joint.
    pub originals: BTreeMap<u64, Path>,
    /// Next joint id.
    pub next_id: u64,
}

impl JointSet {
    /// Whether the set holds nothing worth saving.
    pub fn is_empty(&self) -> bool {
        self.joints.is_empty()
            && self.originals.is_empty()
            && self.settings == JointSettings::default()
    }

    /// Hands out the next joint id.
    pub fn take_id(&mut self) -> u64 {
        self.next_id = self.next_id.max(1);
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// The joint with `id`.
    ///
    /// # Errors
    /// [`CoreError::Validation`] when there is none.
    pub fn joint(&self, id: u64) -> Result<&Joint, CoreError> {
        self.joints
            .iter()
            .find(|j| j.id == id)
            .ok_or_else(|| CoreError::validation(format!("there is no joint {id}")))
    }
}

/// How one joint came out when the set was applied.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// The joint.
    pub id: u64,
    /// The sizes it used, the worked out ones filled in.
    pub resolved: JointParams,
    /// Plain notes about what was worked out.
    pub notes: Vec<String>,
    /// Why it could not be made (a part is gone); it is left as it was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broken: Option<String>,
    /// B's part space to A's once assembled, a column major 4 by 4 matrix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b_to_a: Option<[f64; 16]>,
}

/// The closed rings of a part vector.
///
/// # Errors
/// [`CoreError::Validation`] when the vector is gone or not a closed path.
pub fn part_rings(document: &Document, vector: u64) -> Result<Vec<Vec<P2>>, CoreError> {
    let entity = document
        .entity(vector)
        .map_err(|_| CoreError::validation("one of the joint's parts has been deleted"))?;
    let EntityKind::Path { path } = &entity.kind else {
        return Err(CoreError::validation(
            "a joint's part must be a closed vector",
        ));
    };
    let rings = path_rings(path);
    if rings.is_empty() {
        return Err(CoreError::validation(
            "a joint's part must be a closed vector",
        ));
    }
    Ok(rings)
}

fn path_of(document: &Document, vector: u64) -> Option<Path> {
    match &document.entity(vector).ok()?.kind {
        EntityKind::Path { path } => Some(path.clone()),
        _ => None,
    }
}

fn set_path(document: &mut Document, vector: u64, path: Path) -> Result<(), CoreError> {
    let mut entity = document.entity(vector)?.clone();
    if let EntityKind::Path { path: old } = &entity.kind {
        if *old == path {
            return Ok(());
        }
    }
    entity.kind = EntityKind::Path { path };
    document.replace_entity(vector, entity)
}

/// Puts back every vector the joints rewrote, as it was before them.
fn restore(document: &mut Document, set: &JointSet) -> Result<(), CoreError> {
    for (id, path) in &set.originals {
        if document.entity(*id).is_ok() {
            set_path(document, *id, path.clone())?;
        }
    }
    Ok(())
}

/// Applies every joint of `set` in order to the vectors as they were before
/// any joint, updating the vectors the joints made (and their ids in the
/// set). `strict` fails on the first joint that cannot be made; otherwise a
/// joint whose part is gone is skipped and reported.
///
/// # Errors
/// The joint's own error, prefixed with its name when there are several.
pub fn rebuild(
    document: &mut Document,
    set: &mut JointSet,
    strict: bool,
) -> Result<Vec<Outcome>, CoreError> {
    rebuild_with(document, set, strict, false)
}

/// [`rebuild`] that leaves out each joint that cannot be made (its parts
/// gone, or sizes that do not work), reporting why in its outcome's
/// `broken`, and makes the rest.
///
/// # Errors
/// Only what writing the vectors reports.
pub fn rebuild_skipping(
    document: &mut Document,
    set: &mut JointSet,
) -> Result<Vec<Outcome>, CoreError> {
    rebuild_with(document, set, false, true)
}

fn rebuild_with(
    document: &mut Document,
    set: &mut JointSet,
    strict: bool,
    skip_failing: bool,
) -> Result<Vec<Outcome>, CoreError> {
    restore(document, set)?;
    let settings = set.settings.clone();
    let mut outcomes = Vec::with_capacity(set.joints.len());
    let many = set.joints.len() > 1;
    let mut touched: Vec<u64> = Vec::new();
    // The relief circles on each outline so far, so later joints keep
    // them as arcs.
    let mut circles: BTreeMap<u64, Vec<Circle>> = BTreeMap::new();
    for index in 0..set.joints.len() {
        let joint = set.joints[index].clone();
        let gone =
            document.entity(joint.a.vector).is_err() || document.entity(joint.b.vector()).is_err();
        if gone && !strict {
            outcomes.push(Outcome {
                id: joint.id,
                resolved: joint.params.clone(),
                notes: Vec::new(),
                broken: Some("one of its parts has been deleted".into()),
                b_to_a: None,
            });
            continue;
        }
        let inch = document.job().units == crate::doc::Units::Inch;
        let planned = kinds::plan(document, &set.originals, &settings, &joint).map_err(|e| {
            let e = match e {
                CoreError::Validation(message) => {
                    CoreError::Validation(in_job_units(&message, inch))
                }
                other => other,
            };
            if many && !skip_failing {
                CoreError::validation(format!("{} {}: {e}", joint.kind.title(), joint.id))
            } else {
                e
            }
        });
        let plan = match planned {
            Ok(plan) => plan,
            Err(error) if skip_failing => {
                outcomes.push(Outcome {
                    id: joint.id,
                    resolved: joint.params.clone(),
                    notes: Vec::new(),
                    broken: Some(error.to_string()),
                    b_to_a: None,
                });
                continue;
            }
            Err(error) => return Err(error),
        };
        for (vector, outline) in [(joint.a.vector, &plan.a), (joint.b.vector(), &plan.b)] {
            let Some(outline) = outline else { continue };
            if let std::collections::btree_map::Entry::Vacant(slot) = set.originals.entry(vector) {
                if let Some(path) = path_of(document, vector) {
                    slot.insert(path);
                }
            }
            touched.push(vector);
            let on = circles.entry(vector).or_default();
            on.extend(outline.circles.iter().copied());
            set_path(document, vector, arcs_path(&outline.rings, on))?;
        }
        let vectors = write_vectors(document, &joint, &plan.vectors, &plan.seats)?;
        let mut notes = plan.notes;
        let parts = [
            (joint.a.vector, "the first part"),
            (joint.b.vector(), "the second part"),
        ];
        let extra = vectors
            .iter()
            .filter(|v| matches!(v.role, Role::Part { .. }))
            .map(|v| (v.id, "a part the joint makes"));
        let mut seen = Vec::new();
        // The vectors the joints cut into their parts (holes, pockets,
        // marks) are not parts of their own.
        let cuts: Vec<u64> = set
            .joints
            .iter()
            .chain(std::iter::once(&joint))
            .flat_map(|j| j.vectors.iter())
            .chain(vectors.iter())
            .filter(|v| !matches!(v.role, Role::Part { .. }))
            .map(|v| v.id)
            .collect();
        for (id, what) in parts.into_iter().chain(extra) {
            if !seen.contains(&id) && off_material(document, id) {
                notes.push(format!(
                    "{what} runs past the edge of the material, so no toolpath cuts it free; move it onto the material"
                ));
            } else if !seen.contains(&id)
                && runs_into_a_part(document, id, set.originals.get(&id), &cuts)
            {
                notes.push(format!(
                    "{what} now runs into another part on its sheet, so cutting one spoils the other; move them apart before making the toolpaths"
                ));
            }
            seen.push(id);
        }
        set.joints[index].vectors = vectors;
        let notes = notes.iter().map(|n| in_job_units(n, inch)).collect();
        outcomes.push(Outcome {
            id: joint.id,
            b_to_a: Some(pose::b_to_a(&plan.fa, &plan.fb, &plan.map).to_matrix()),
            resolved: plan.resolved,
            notes,
            broken: None,
        });
    }
    // Vectors no joint touches any more are back as they were.
    set.originals.retain(|id, _| touched.contains(id));
    Ok(outcomes)
}

/// `text` (a note or a refusal, its sizes written "12.5 mm") with every
/// size in inches when `inch`, as the job shows them: "0.4921 inch".
fn in_job_units(text: &str, inch: bool) -> String {
    if !inch {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(" mm") {
        let (before, after) = rest.split_at(at);
        let tail = &after[3..];
        let word_ends = tail.chars().next().is_none_or(|c| !c.is_alphanumeric());
        let start = before
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_digit() || *c == '.' || *c == '-')
            .last()
            .map(|(i, _)| i);
        match (start, word_ends) {
            (Some(i), true) if before[i..].parse::<f64>().is_ok() => {
                let mm: f64 = before[i..].parse().unwrap_or(0.0);
                let inches = format!("{:.4}", mm / 25.4);
                let inches = inches.trim_end_matches('0').trim_end_matches('.');
                out.push_str(&before[..i]);
                out.push_str(inches);
                out.push_str(" inch");
            }
            _ => {
                out.push_str(before);
                out.push_str(" mm");
            }
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// Whether the material a joint grew on part `id` (all of it for a part
/// the joint makes; what is not in `original`, the outline before any
/// joint, for a part it changed: tenons, tabs) overlaps another closed
/// vector on its sheet, not one of the joints' `cuts`, by more than a
/// square millimetre: cutting one would spoil the other.
fn runs_into_a_part(document: &Document, id: u64, original: Option<&Path>, cuts: &[u64]) -> bool {
    let (Ok(entity), Some(b)) = (document.entity(id), document.bbox_of(id)) else {
        return false;
    };
    let EntityKind::Path { path } = &entity.kind else {
        return false;
    };
    let Ok(tol) = crate::geom::Tolerance::new(0.01) else {
        return false;
    };
    let grown = match original {
        Some(before) => {
            match crate::geom::boolean::subtract(
                std::slice::from_ref(path),
                std::slice::from_ref(before),
                tol,
            ) {
                Ok(grown) => grown,
                Err(_) => return false,
            }
        }
        None => vec![path.clone()],
    };
    if grown.is_empty() {
        return false;
    }
    document.order().iter().any(|other| {
        if *other == id || cuts.contains(other) {
            return false;
        }
        let (Ok(e), Some(ob)) = (document.entity(*other), document.bbox_of(*other)) else {
            return false;
        };
        let EntityKind::Path { path: other_path } = &e.kind else {
            return false;
        };
        if e.sheet != entity.sheet || !other_path.is_closed() || b.intersection(ob).is_none() {
            return false;
        }
        crate::geom::boolean::intersect_paths(&grown, std::slice::from_ref(other_path), tol)
            .is_ok_and(|shared| crate::geom::Region::from_paths(&shared, tol).area() > 1.0)
    })
}

/// Whether vector `id` reaches past the material of its sheet (a part
/// there is never cut free, so it is not a piece of the Assembly View).
fn off_material(document: &Document, id: u64) -> bool {
    let (Ok(entity), Some(b)) = (document.entity(id), document.bbox_of(id)) else {
        return false;
    };
    let material = document.sheet_material(entity.sheet).map_or_else(
        |_| document.job_bounds(),
        |m| m.applied_to(document.job_setup()).material_bounds(),
    );
    let slack = 1e-6;
    b.min.x < material.min.x - slack
        || b.min.y < material.min.y - slack
        || b.max.x > material.max.x + slack
        || b.max.y > material.max.y + slack
}

/// Writes the vectors a joint makes, reusing the ids it had.
fn write_vectors(
    document: &mut Document,
    joint: &Joint,
    made: &[kinds::Made],
    seats: &[(usize, crate::assembly::pose::Pose)],
) -> Result<Vec<JointVector>, CoreError> {
    let mut out = Vec::with_capacity(made.len());
    for (k, m) in made.iter().enumerate() {
        let host = match m.on {
            kinds::On::A => joint.a.vector,
            kinds::On::B => joint.b.vector(),
        };
        let (layer, sheet) = document
            .entity(host)
            .map(|e| (e.layer, e.sheet))
            .unwrap_or((document.active_layer(), document.active_sheet()));
        let name = Some(format!("{} {} {}", joint.kind.title(), joint.id, m.name));
        let reuse = joint
            .vectors
            .get(k)
            .map(|v| v.id)
            .filter(|id| document.entity(*id).is_ok());
        let path = match m.role {
            // A part the joint makes (a wedge, a key) stays where it is
            // once drawn, so it can be moved; a new one goes to a free
            // place on the sheet.
            Role::Part { .. } => {
                let own: Vec<u64> = joint.vectors.iter().map(|v| v.id).collect();
                place_part(document, sheet, &m.path, reuse, &own)
            }
            _ => m.path.clone(),
        };
        // The seat from the part's own frame (from the lower left corner
        // of its box), so it holds wherever the part is drawn or moved.
        let seat = seats.iter().find(|(i, _)| *i == k).map(|(_, pose)| {
            let corner =
                crate::geom::path_bbox(&m.path).map_or([0.0; 3], |b| [b.min.x, b.min.y, 0.0]);
            pose.then_local(&crate::assembly::pose::Pose::translation(corner))
                .to_matrix()
        });
        let corner = seat
            .and_then(|_| crate::geom::path_bbox(&path))
            .map(|b| [b.min.x, b.min.y]);
        let entity = Entity {
            id: 0,
            layer,
            sheet,
            revision: 0,
            name,
            parent: None,
            rotation: 0.0,
            kind: EntityKind::Path { path },
        };
        let id = match reuse {
            Some(id) => {
                let unchanged = document.entity(id).is_ok_and(|e| {
                    e.kind == entity.kind
                        && e.layer == entity.layer
                        && e.sheet == entity.sheet
                        && e.name == entity.name
                });
                if !unchanged {
                    document.replace_entity(id, entity)?;
                }
                id
            }
            None => document.insert_entity(entity),
        };
        out.push(JointVector {
            id,
            role: m.role,
            seat,
            corner,
        });
    }
    let surplus: Vec<u64> = joint
        .vectors
        .iter()
        .skip(made.len())
        .map(|v| v.id)
        .filter(|id| document.entity(*id).is_ok())
        .collect();
    document.delete_entities(&surplus);
    Ok(out)
}

/// Room left round the parts on a sheet when a new part is drawn there,
/// for the tenons and tabs joints add to them.
const PART_ROOM: f64 = 40.0;

/// `path` moved to where its part goes: where the part `reuse` lies now,
/// else the first free place on the sheet (scanning the material from its
/// lower left, clear of everything but `own` by [`PART_ROOM`]), else
/// where it was laid out.
fn place_part(
    document: &Document,
    sheet: u64,
    path: &Path,
    reuse: Option<u64>,
    own: &[u64],
) -> Path {
    use crate::geom::{path_bbox, transform_path, BBox, Mat3, Point, Vec2};
    let Some(bb) = path_bbox(path) else {
        return path.clone();
    };
    let target = match reuse.and_then(|id| document.bbox_of(id)) {
        Some(now) => Some([now.min.x, now.min.y]),
        None => {
            let (w, h) = (bb.max.x - bb.min.x, bb.max.y - bb.min.y);
            let job = document.job_bounds();
            let taken: Vec<BBox> = document
                .order()
                .iter()
                .filter(|id| !own.contains(id))
                .filter(|id| document.entity(**id).is_ok_and(|e| e.sheet == sheet))
                .filter_map(|id| document.bbox_of(*id))
                .collect();
            let step = 5.0;
            let mut found = None;
            let mut y = job.min.y + 5.0;
            while found.is_none() && y + h <= job.max.y - 5.0 {
                let mut x = job.min.x + 5.0;
                while x + w <= job.max.x - 5.0 {
                    let spot = BBox {
                        min: Point::new(x - PART_ROOM, y - PART_ROOM),
                        max: Point::new(x + w + PART_ROOM, y + h + PART_ROOM),
                    };
                    if !taken.iter().any(|b| b.intersects(spot)) {
                        found = Some([x, y]);
                        break;
                    }
                    x += step;
                }
                y += step;
            }
            found
        }
    };
    match target {
        Some([x, y]) => transform_path(
            path,
            &Mat3::translate(Vec2::new(x - bb.min.x, y - bb.min.y)),
        ),
        None => path.clone(),
    }
}

/// Deletes the vectors a joint made.
pub fn delete_made(document: &mut Document, joint: &Joint) {
    let ids: Vec<u64> = joint
        .vectors
        .iter()
        .map(|v| v.id)
        .filter(|id| document.entity(*id).is_ok())
        .collect();
    document.delete_entities(&ids);
}

/// The frame of an edge line on the current outline of its part.
///
/// # Errors
/// [`CoreError::Validation`] when the part is gone or the edge is too short.
pub fn edge_frame(
    document: &Document,
    set_originals: &BTreeMap<u64, Path>,
    edge: &EdgeLine,
) -> Result<Frame, CoreError> {
    // The side the material is on comes from the outline before any joint,
    // which a notch at the edge's middle cannot fool.
    let rings = match set_originals.get(&edge.vector) {
        Some(path) => path_rings(path),
        None => part_rings(document, edge.vector)?,
    };
    let frame = Frame::of_edge(edge.from, edge.to, &rings);
    if frame.len < 1.0 {
        return Err(CoreError::validation(
            "the picked edge is shorter than 1 mm; pick a longer straight edge",
        ));
    }
    Ok(frame)
}
