//! Functional part assembly: flat parts cut from sheets, placed as instances
//! in 3D assembly space and joined by mates. A mate places its child
//! instance against a face of its parent; hinge and slide mates carry a
//! drivable value (an angle or a distance) within limits, so doors open and
//! drawers pull out. Mates form a tree, so poses come from a walk down it.
//!
//! The 2D vectors on the sheets stay the source of truth: a part keeps the
//! ids of its vectors and its solid is rebuilt from them.

pub mod commands;
pub mod drop;
pub mod exact;
pub mod export;
pub mod fit;
pub mod interference;
pub mod joint_mates;
pub mod mate_faces;
pub mod mates;
pub mod measure;
pub mod pieces;
pub mod pieces_cmd;
pub mod pose;
pub mod solid;
pub mod steps;
pub mod triangulate;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

use pose::{Pose, Vec3};
use solid::Face;

/// Which way the grain runs on a part's sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Grain {
    /// Along the sheet's X axis.
    #[default]
    X,
    /// Along the sheet's Y axis.
    Y,
    /// No grain (MDF, plastics).
    None,
}

/// A pocket of a part: a closed vector and how deep it goes from the top.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartPocket {
    /// The pocket boundary vector.
    pub vector: u64,
    /// Depth below the top in millimetres.
    pub depth: f64,
}

/// A flat part cut from a sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// Id, unique among the assembly's parts, instances and mates.
    pub id: u64,
    /// Name shown in the part list and the cut list.
    pub name: String,
    /// The closed vector that is the part's outline.
    pub outline: u64,
    /// Closed vectors inside the outline cut right through.
    #[serde(default)]
    pub holes: Vec<u64>,
    /// Closed vectors inside the outline sunk from the top.
    #[serde(default)]
    pub pockets: Vec<PartPocket>,
    /// Thickness in millimetres; the sheet's thickness when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness: Option<f64>,
    /// Grain direction.
    #[serde(default)]
    pub grain: Grain,
    /// The outline ring (sheet coordinates) before a joint (older files) rewrote the
    /// outline vector; face frames, and so mates and poses, come from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_outline: Option<Vec<[f64; 2]>>,
}

/// A placed copy of a part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    /// Id.
    pub id: u64,
    /// The part it is a copy of.
    pub part: u64,
    /// Name shown in the assembly tree.
    pub name: String,
    /// Position of an instance no mate places, in millimetres.
    #[serde(default)]
    pub position: Vec3,
    /// Rotation of an instance no mate places: degrees about X, Y, Z.
    #[serde(default)]
    pub rotation: Vec3,
}

/// How a mate lets its child move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MateKind {
    /// Fixed against the parent face.
    #[default]
    Flush,
    /// Turns about an axis on the parent face (a door).
    Hinge,
    /// Moves along an axis of the parent face (a drawer).
    Slide,
}

/// A mate: the child's face against the parent's face.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mate {
    /// Id.
    pub id: u64,
    /// Kind of joint.
    #[serde(default)]
    pub kind: MateKind,
    /// The instance the child is placed against.
    pub parent: u64,
    /// The parent's face.
    pub parent_face: Face,
    /// The instance this mate places.
    pub child: u64,
    /// The child's face, laid against the parent face (normals opposite).
    pub child_face: Face,
    /// How far the child face's centre sits from the parent face's centre,
    /// along the parent face's u and v axes, in millimetres.
    #[serde(default)]
    pub offset: [f64; 2],
    /// Turn of the child about the parent face normal, in degrees.
    #[serde(default)]
    pub angle: f64,
    /// Distance along the parent face normal; negative sinks the child in.
    #[serde(default)]
    pub gap: f64,
    /// Hinge axis point or slide origin, in parent face coordinates.
    #[serde(default)]
    pub axis_point: Vec3,
    /// Hinge axis or slide direction, in parent face coordinates.
    #[serde(default = "default_axis")]
    pub axis_dir: Vec3,
    /// Hinge angle in degrees or slide distance in millimetres.
    #[serde(default)]
    pub value: f64,
    /// Lower limit of `value`.
    #[serde(default)]
    pub min: f64,
    /// Upper limit of `value`.
    #[serde(default)]
    pub max: f64,
}

fn default_axis() -> Vec3 {
    [0.0, 1.0, 0.0]
}

impl Mate {
    /// The motion the joint's current value gives, in parent face
    /// coordinates.
    fn motion(&self) -> Pose {
        let value = self
            .value
            .clamp(self.min.min(self.max), self.max.max(self.min));
        match self.kind {
            MateKind::Flush => Pose::IDENTITY,
            MateKind::Hinge => Pose::rotation_about(self.axis_point, self.axis_dir, value),
            MateKind::Slide => {
                Pose::translation(pose::scale(pose::normalized(self.axis_dir), value))
            }
        }
    }
}

/// Parts, instances and mates of the document's assembly.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Assembly {
    /// Parts in list order.
    pub parts: Vec<Part>,
    /// Instances in list order.
    pub instances: Vec<Instance>,
    /// Mates in list order.
    pub mates: Vec<Mate>,
    /// Pieces of the Assembly View that have been placed, one per part of a
    /// sheet at most; a part without an entry lies where its sheet puts it.
    pub pieces: Vec<pieces::Piece>,
    /// Sheets whose Assembly View piece is their raw model (the visible
    /// components baked) instead of their toolpath simulation.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub model_sheets: Vec<u64>,
    /// Components shown in the Assembly View as pieces of their own, in the
    /// order they were added; they are left out of their sheet's raw model.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<u64>,
    /// Mates between the Assembly View's pieces, in the order they were made.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub piece_mates: Vec<mates::PieceMate>,
    /// Named explode steps of the Assembly View, in the order they play.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub explode_steps: Vec<steps::ExplodeStep>,
    /// The Joint Creator's joints and the settings they share.
    #[serde(skip_serializing_if = "crate::joints::JointSet::is_empty")]
    pub joint_set: crate::joints::JointSet,
    /// The Furniture Designer's designs, in the order they were made.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub designs: Vec<crate::furniture::FurnitureDesign>,
    /// Next id for a part, instance or mate.
    pub next_id: u64,
}

/// Face frames of the parts, which is all the pose walk needs of them.
pub trait FaceFrames {
    /// The frame of `face` on `part` and the face's length along its u and v
    /// axes, or `None` when the part or the face does not exist.
    fn face(&self, part: u64, face: Face) -> Option<(Pose, [f64; 2])>;
}

impl Assembly {
    /// Whether the assembly has nothing in it.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
            && self.instances.is_empty()
            && self.mates.is_empty()
            && self.pieces.is_empty()
            && self.model_sheets.is_empty()
            && self.piece_mates.is_empty()
            && self.explode_steps.is_empty()
            && self.joint_set.is_empty()
            && self.designs.is_empty()
    }

    /// Hands out the next id.
    pub fn take_id(&mut self) -> u64 {
        self.next_id = self.next_id.max(1);
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// The part with `id`.
    ///
    /// # Errors
    /// [`CoreError::Validation`] when there is none.
    pub fn part(&self, id: u64) -> Result<&Part, CoreError> {
        self.parts
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| CoreError::validation(format!("there is no part {id}")))
    }

    /// The instance with `id`.
    ///
    /// # Errors
    /// [`CoreError::Validation`] when there is none.
    pub fn instance(&self, id: u64) -> Result<&Instance, CoreError> {
        self.instances
            .iter()
            .find(|i| i.id == id)
            .ok_or_else(|| CoreError::validation(format!("there is no instance {id}")))
    }

    /// The mate that places `child`, if any.
    pub fn mate_of(&self, child: u64) -> Option<&Mate> {
        self.mates.iter().find(|m| m.child == child)
    }

    /// Whether `ancestor` is `instance` or places it through a chain of
    /// mates.
    pub fn is_ancestor(&self, ancestor: u64, instance: u64) -> bool {
        let mut current = instance;
        for _ in 0..=self.mates.len() {
            if current == ancestor {
                return true;
            }
            match self.mate_of(current) {
                Some(mate) => current = mate.parent,
                None => return false,
            }
        }
        false
    }

    /// Checks a mate before it is added: both instances exist, the child is
    /// not already placed by another mate, and no loop forms.
    ///
    /// # Errors
    /// [`CoreError::Validation`] naming the problem.
    pub fn check_mate(&self, mate: &Mate) -> Result<(), CoreError> {
        self.instance(mate.parent)?;
        self.instance(mate.child)?;
        if mate.parent == mate.child {
            return Err(CoreError::validation("a part cannot be mated to itself"));
        }
        if self
            .mates
            .iter()
            .any(|m| m.child == mate.child && m.id != mate.id)
        {
            return Err(CoreError::validation(
                "that part is already placed by a mate; remove it first",
            ));
        }
        if self.is_ancestor(mate.child, mate.parent) {
            return Err(CoreError::validation(
                "that mate would make a loop: the parent is already placed from this part",
            ));
        }
        Ok(())
    }

    /// Every instance's pose in assembly space. `explode` moves each mated
    /// instance that many millimetres further out along its parent face
    /// normal, for an exploded view.
    ///
    /// # Errors
    /// [`CoreError::Validation`] when a mate names a missing face.
    pub fn poses(
        &self,
        frames: &dyn FaceFrames,
        explode: f64,
    ) -> Result<BTreeMap<u64, Pose>, CoreError> {
        let mut out = BTreeMap::new();
        for instance in &self.instances {
            self.pose_of(instance.id, frames, explode, &mut out, 0)?;
        }
        Ok(out)
    }

    fn pose_of(
        &self,
        id: u64,
        frames: &dyn FaceFrames,
        explode: f64,
        done: &mut BTreeMap<u64, Pose>,
        depth: usize,
    ) -> Result<Pose, CoreError> {
        if let Some(pose) = done.get(&id) {
            return Ok(*pose);
        }
        if depth > self.instances.len() {
            return Err(CoreError::validation("the mates form a loop"));
        }
        let instance = self.instance(id)?;
        let pose = match self.mate_of(id) {
            None => {
                Pose::translation(instance.position).then_local(&Pose::euler(instance.rotation))
            }
            Some(mate) => {
                let parent = self.pose_of(mate.parent, frames, explode, done, depth + 1)?;
                let parent_instance = self.instance(mate.parent)?;
                let missing = || {
                    CoreError::validation(format!(
                        "mate {} names a face its part does not have",
                        mate.id
                    ))
                };
                let (parent_frame, parent_size) = frames
                    .face(parent_instance.part, mate.parent_face)
                    .ok_or_else(missing)?;
                let (child_frame, child_size) = frames
                    .face(instance.part, mate.child_face)
                    .ok_or_else(missing)?;
                // Parent face frame, then the joint motion, then the face
                // centres brought together with the offset, gap and turn, then
                // a half turn about v so the child face looks back at the
                // parent face (v stays put, which keeps panel thicknesses in
                // line when edge meets edge).
                let flip = Pose::frame(
                    [0.0; 3],
                    [-1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, -1.0],
                );
                parent
                    .then_local(&parent_frame)
                    .then_local(&mate.motion())
                    .then_local(&Pose::translation([
                        parent_size[0] / 2.0 + mate.offset[0],
                        parent_size[1] / 2.0 + mate.offset[1],
                        mate.gap + explode,
                    ]))
                    .then_local(&Pose::rotation([0.0, 0.0, 1.0], mate.angle))
                    .then_local(&flip)
                    .then_local(&Pose::translation([
                        -child_size[0] / 2.0,
                        -child_size[1] / 2.0,
                        0.0,
                    ]))
                    .then_local(&child_frame.inverse())
            }
        };
        done.insert(id, pose);
        Ok(pose)
    }
}

#[cfg(test)]
mod tests {
    use super::solid::Profile;
    use super::*;

    struct Panels(BTreeMap<u64, Profile>);

    impl FaceFrames for Panels {
        fn face(&self, part: u64, face: Face) -> Option<(Pose, [f64; 2])> {
            let profile = self.0.get(&part)?;
            Some((profile.face_frame(face)?, profile.face_size(face)?))
        }
    }

    fn rect(w: f64, h: f64) -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
    }

    fn instance(id: u64, part: u64) -> Instance {
        Instance {
            id,
            part,
            name: format!("i{id}"),
            position: [0.0; 3],
            rotation: [0.0; 3],
        }
    }

    fn mate(id: u64, parent: u64, pf: Face, child: u64, cf: Face) -> Mate {
        Mate {
            id,
            kind: MateKind::Flush,
            parent,
            parent_face: pf,
            child,
            child_face: cf,
            offset: [0.0; 2],
            angle: 0.0,
            gap: 0.0,
            axis_point: [0.0; 3],
            axis_dir: [0.0, 1.0, 0.0],
            value: 0.0,
            min: 0.0,
            max: 0.0,
        }
    }

    /// A 600 x 400 x 18 base panel with a side panel standing on its top.
    fn cabinet() -> (Assembly, Panels) {
        let mut profiles = BTreeMap::new();
        profiles.insert(1, Profile::new(rect(600.0, 400.0), vec![], vec![], 18.0));
        profiles.insert(2, Profile::new(rect(700.0, 400.0), vec![], vec![], 18.0));
        let assembly = Assembly {
            parts: Vec::new(),
            instances: vec![instance(10, 1), instance(11, 2)],
            // The side's bottom edge (edge 0) on the base's top face.
            mates: vec![mate(20, 10, Face::Top, 11, Face::Edge(0))],
            pieces: Vec::new(),
            model_sheets: Vec::new(),
            components: Vec::new(),
            piece_mates: Vec::new(),
            explode_steps: Vec::new(),
            joint_set: crate::joints::JointSet::default(),
            designs: Vec::new(),
            next_id: 30,
        };
        (assembly, Panels(profiles))
    }

    #[test]
    fn a_flush_mate_stands_the_side_on_the_base() {
        let (assembly, frames) = cabinet();
        let poses = assembly.poses(&frames, 0.0).expect("poses");
        let side = poses[&11];
        // The side's bottom edge face lies on the base top (z = 18) and the
        // side rises 700 mm above it.
        let bottom_edge_mid = side.apply([350.0, 0.0, 9.0]);
        assert!(
            (bottom_edge_mid[2] - 18.0).abs() < 1e-6,
            "{bottom_edge_mid:?}"
        );
        let top_edge = side.apply([350.0, 400.0, 9.0]);
        assert!((top_edge[2] - 18.0 - 400.0).abs() < 1e-6 || (top_edge[2] - 18.0).abs() < 1e-6);
        let far = side.apply([700.0, 0.0, 0.0]);
        assert!((far[2] - 18.0).abs() < 1e-6);
    }

    #[test]
    fn a_hinge_turns_the_child_about_its_axis() {
        let (mut assembly, frames) = cabinet();
        let m = &mut assembly.mates[0];
        m.kind = MateKind::Hinge;
        m.axis_point = [0.0, 0.0, 0.0];
        m.axis_dir = [0.0, 1.0, 0.0];
        m.min = 0.0;
        m.max = 90.0;
        m.value = 0.0;
        let closed = assembly.poses(&frames, 0.0).expect("closed")[&11];
        assembly.mates[0].value = 90.0;
        let open = assembly.poses(&frames, 0.0).expect("open")[&11];
        // The motion from closed to open turns about the axis: a point on the
        // axis (the base top face's origin, running along its v) stays put
        // and a point off it moves.
        let swing = open.then_local(&closed.inverse());
        let on_axis = [0.0, 150.0, 18.0];
        assert!(
            close(swing.apply(on_axis), on_axis),
            "{:?}",
            swing.apply(on_axis)
        );
        let off = closed.apply([700.0, 0.0, 0.0]);
        assert!(!close(swing.apply(off), off));
        // Past the limit the value is held at the limit.
        assembly.mates[0].value = 200.0;
        let held = assembly.poses(&frames, 0.0).expect("held")[&11];
        assert!(close(
            held.apply([700.0, 0.0, 0.0]),
            open.apply([700.0, 0.0, 0.0])
        ));
    }

    #[test]
    fn a_slide_moves_along_its_axis_within_limits() {
        let (mut assembly, frames) = cabinet();
        let m = &mut assembly.mates[0];
        m.kind = MateKind::Slide;
        m.axis_dir = [1.0, 0.0, 0.0];
        m.min = 0.0;
        m.max = 300.0;
        m.value = 500.0;
        let base = assembly.poses(&frames, 0.0).expect("poses")[&11];
        assembly.mates[0].kind = MateKind::Flush;
        let rest = assembly.poses(&frames, 0.0).expect("rest")[&11];
        let moved = base.apply([0.0; 3]);
        let still = rest.apply([0.0; 3]);
        assert!((moved[0] - still[0] - 300.0).abs() < 1e-6);
    }

    #[test]
    fn loops_and_second_parents_are_refused() {
        let (assembly, _) = cabinet();
        let back = mate(21, 11, Face::Top, 10, Face::Bottom);
        assert!(assembly.check_mate(&back).is_err());
        let second = mate(22, 10, Face::Bottom, 11, Face::Top);
        assert!(assembly.check_mate(&second).is_err());
    }

    #[test]
    fn older_files_with_joints_and_one_piece_per_sheet_still_open() {
        let assembly: Assembly = serde_json::from_value(serde_json::json!({
            "joints": [{ "id": 4, "mate": 3, "kind": "dado" }],
            "pieces": [{ "sheet": 2, "position": [1.0, 2.0, 3.0], "rotation": [0.0, 0.0, 0.0] }],
            "nextId": 5
        }))
        .expect("an older assembly reads");
        assert_eq!(assembly.pieces[0].part, 1);
        assert_eq!(assembly.next_id, 5);
    }

    #[test]
    fn explode_moves_children_off_their_parent_face() {
        let (assembly, frames) = cabinet();
        let near = assembly.poses(&frames, 0.0).expect("near")[&11];
        let far = assembly.poses(&frames, 50.0).expect("far")[&11];
        let d = pose::sub(far.apply([0.0; 3]), near.apply([0.0; 3]));
        assert!(close(d, [0.0, 0.0, 50.0]));
    }
}
