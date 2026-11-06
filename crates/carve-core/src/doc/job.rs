//! Job setup: material size, datum and job type.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Unit the user types and reads values in. Storage is always millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Units {
    /// Millimetres.
    Mm,
    /// Inches.
    Inch,
}

/// Where Z zero sits on the material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZZero {
    /// Top surface of the material.
    #[default]
    Top,
    /// Machine bed, that is the bottom of the material.
    Bottom,
}

/// Where the XY datum sits on the job rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum XyOrigin {
    /// Centre of the job.
    Centre,
    /// Bottom left corner.
    BottomLeft,
    /// Bottom right corner.
    BottomRight,
    /// Top left corner.
    TopLeft,
    /// Top right corner.
    TopRight,
}

/// Job type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    /// One side of flat material.
    SingleSided,
    /// Flat material machined from both sides.
    DoubleSided,
    /// Cylindrical blank on a rotary axis.
    Rotary,
}

/// One face of a double sided job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    /// The face machined first, facing up when the job is set up.
    #[default]
    Top,
    /// The other face, machined after the material is turned over.
    Bottom,
}

impl Side {
    /// The other face.
    pub fn other(self) -> Side {
        match self {
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
        }
    }
}

/// How the material is turned over between the two sides of a double sided
/// job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlipAxis {
    /// Turned about the vertical centre line: left and right swap, so X
    /// mirrors across the material while Y stays.
    #[default]
    Horizontal,
    /// Turned about the horizontal centre line: top and bottom swap, so Y
    /// mirrors across the material while X stays.
    Vertical,
}

/// Settings of a double sided job. Kept while the job is single sided so
/// switching the job type back and forth does not lose them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DoubleSidedSetup {
    /// How the material is turned over between the sides.
    pub flip: FlipAxis,
    /// Where Z zero sits while the bottom side is machined, seen from that
    /// side: `Top` is the face that is up once the material is turned over.
    /// The top side uses [`JobSetup::z_zero`].
    pub bottom_z_zero: ZZero,
}

/// Axis the rotary blank turns about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RotaryAxis {
    /// Blank lies along X.
    X,
    /// Blank lies along Y.
    Y,
}

/// Cross section of a rotary blank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RotaryBlank {
    /// Round stock; `diameter` is its diameter.
    #[default]
    Cylinder,
    /// Square stock; `diameter` is the side of the square, and the job is
    /// laid out on the cylinder that side turns down to.
    Square,
}

/// Where Z zero sits on a rotary job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RotaryZZero {
    /// On the axis of rotation: the surface is at Z = radius.
    #[default]
    Centre,
    /// On the surface of the blank, at the top of the cylinder.
    Surface,
}

/// Rotary blank settings. Kept even while the job is flat so that switching
/// job type back and forth does not lose what the user typed.
///
/// A rotary job is designed and machined flat on the unwrapped surface of
/// the blank: the flat layout runs `length` along the axis and one
/// circumference (pi times the diameter) across it, and the post wraps the
/// across coordinate into an angle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotarySetup {
    /// Blank diameter (or the side of a square blank) in millimetres.
    pub diameter: f64,
    /// Axis the blank turns about.
    pub axis: RotaryAxis,
    /// Blank length along the axis, in millimetres.
    #[serde(default = "default_rotary_length")]
    pub length: f64,
    /// Cross section of the blank.
    #[serde(default)]
    pub blank: RotaryBlank,
    /// Where Z zero sits.
    #[serde(default)]
    pub z_zero: RotaryZZero,
}

fn default_rotary_length() -> f64 {
    300.0
}

impl Default for RotarySetup {
    fn default() -> Self {
        RotarySetup {
            diameter: 100.0,
            axis: RotaryAxis::Y,
            length: default_rotary_length(),
            blank: RotaryBlank::Cylinder,
            z_zero: RotaryZZero::Centre,
        }
    }
}

impl RotarySetup {
    /// Radius of the surface the flat layout wraps onto, in millimetres.
    pub fn radius(&self) -> f64 {
        self.diameter / 2.0
    }

    /// Length of the unwrapped surface across the axis: pi times the
    /// diameter, in millimetres.
    pub fn circumference(&self) -> f64 {
        std::f64::consts::PI * self.diameter
    }
}

/// Rapid move heights and the home position, the "Material Setup" part of
/// the job that toolpaths use. Heights are millimetres above the material
/// surface, whichever face Z zero is on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MachineSetup {
    /// Clearance (Z1): the height rapids travel at between cuts.
    pub safe_z: f64,
    /// Plunge (Z2): the tool rapids down to this far above the surface, then
    /// plunges at the plunge rate.
    pub plunge_gap: f64,
    /// Home position X in job space.
    pub home_x: f64,
    /// Home position Y in job space.
    pub home_y: f64,
    /// Home height above the surface, where the job starts and ends.
    pub home_z: f64,
    /// Model Position in Material: how far the top of the model sits below
    /// the top side's material surface, in millimetres. 3D toolpaths,
    /// projection and the 3D view place the model this much lower; the
    /// model never goes below the material bottom. On a double sided job it
    /// is the gap above the whole two sided model: both sides share one
    /// modelling plane (see [`JobSetup::modelling_plane_depth`]). With
    /// `gap_below` set it is the gap below the model instead.
    pub model_gap: f64,
    /// Gap Below Model is the chosen radio: `model_gap` is measured up from
    /// the material bottom (a rotary job's axis) to the underside of the
    /// model, so the model keeps its place above the bottom when its height
    /// changes. Off (Gap Above Model) for older files.
    pub gap_below: bool,
}

impl Default for MachineSetup {
    fn default() -> Self {
        MachineSetup {
            safe_z: 5.0,
            plunge_gap: 2.0,
            home_x: 0.0,
            home_y: 0.0,
            home_z: 20.0,
            model_gap: 0.0,
            gap_below: false,
        }
    }
}

/// Material size, datum and job type. All lengths are millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSetup {
    /// Unit the UI displays and accepts values in.
    pub units: Units,
    /// Job width along X, in millimetres.
    pub width: f64,
    /// Job height along Y, in millimetres.
    pub height: f64,
    /// Material thickness along Z, in millimetres.
    pub thickness: f64,
    /// Which face of the material Z zero refers to.
    pub z_zero: ZZero,
    /// Which point of the job rectangle the XY datum sits on.
    pub xy_origin: XyOrigin,
    /// Datum offset along X, in millimetres.
    pub offset_x: f64,
    /// Datum offset along Y, in millimetres.
    pub offset_y: f64,
    /// Job type.
    pub kind: JobKind,
    /// Rotary blank settings, used when `kind` is [`JobKind::Rotary`].
    pub rotary: RotarySetup,
    /// Rapid heights and home position for toolpaths.
    #[serde(default)]
    pub machine: MachineSetup,
    /// Flip direction and bottom side Z zero, used when `kind` is
    /// [`JobKind::DoubleSided`].
    #[serde(default)]
    pub double_sided: DoubleSidedSetup,
    /// How the material looks in the 3D view, kept with the job.
    #[serde(default)]
    pub appearance: Appearance,
    /// How finely the 3D model is calculated.
    #[serde(default)]
    pub resolution: ModelResolution,
}

/// Modelling resolution: how many cells the 3D model grid has over the job.
/// Finer grids keep small details on large jobs but calculate more slowly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelResolution {
    /// The fastest setting, enough for most jobs.
    #[default]
    Standard,
    /// Three times the cells of standard, about three times slower.
    High,
    /// Seven times the cells of standard, about seven times slower.
    Highest,
}

impl ModelResolution {
    /// Cells of the model grid relative to [`ModelResolution::Standard`].
    pub fn cell_factor(self) -> f64 {
        match self {
            ModelResolution::Standard => 1.0,
            ModelResolution::High => 3.0,
            ModelResolution::Highest => 7.0,
        }
    }
}

/// The material's look in the 3D view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    /// A wood, board, plastic or metal look, or a solid colour.
    pub material: crate::prefs::Material3d,
    /// The colour used when `material` is solid, as `#rrggbb`.
    #[serde(with = "hex_colour")]
    pub colour: [u8; 3],
    /// A picture from the user's material textures laid over the material,
    /// by its id; zero for none.
    #[serde(default, skip_serializing_if = "is_zero_texture")]
    pub texture: u32,
}

fn is_zero_texture(texture: &u32) -> bool {
    *texture == 0
}

impl Default for Appearance {
    fn default() -> Self {
        Appearance {
            material: crate::prefs::Material3d::Theme,
            colour: [0xc9, 0xb2, 0x8f],
            texture: 0,
        }
    }
}

/// `#rrggbb` text for a colour held as three bytes, so the job stays `Copy`.
mod hex_colour {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(rgb: &[u8; 3], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 3], D::Error> {
        let text = String::deserialize(d)?;
        parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("{text} is not a #rrggbb color")))
    }

    /// The bytes of a `#rrggbb` colour.
    pub fn parse(text: &str) -> Option<[u8; 3]> {
        let hex = text.strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some([byte(0)?, byte(2)?, byte(4)?])
    }
}

/// The bytes of a `#rrggbb` colour.
pub fn hex_colour_bytes(text: &str) -> Option<[u8; 3]> {
    hex_colour::parse(text)
}

impl Default for JobSetup {
    fn default() -> Self {
        JobSetup {
            units: Units::Mm,
            width: 300.0,
            height: 200.0,
            thickness: 18.0,
            z_zero: ZZero::Top,
            xy_origin: XyOrigin::BottomLeft,
            offset_x: 0.0,
            offset_y: 0.0,
            kind: JobKind::SingleSided,
            rotary: RotarySetup::default(),
            machine: MachineSetup::default(),
            double_sided: DoubleSidedSetup::default(),
            appearance: Appearance::default(),
            resolution: ModelResolution::Standard,
        }
    }
}

impl JobSetup {
    /// Checks the job is machinable: positive finite size and thickness, a
    /// finite datum offset, and a positive diameter for a rotary job.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] naming the first field that fails.
    pub fn validate(&self) -> Result<(), CoreError> {
        positive("width", self.width)?;
        positive("height", self.height)?;
        positive("thickness", self.thickness)?;
        finite("offsetX", self.offset_x)?;
        finite("offsetY", self.offset_y)?;
        if self.kind == JobKind::Rotary {
            positive("rotary.diameter", self.rotary.diameter)?;
            positive("rotary.length", self.rotary.length)?;
        }
        let m = &self.machine;
        positive("machine.safeZ", m.safe_z)?;
        finite("machine.plungeGap", m.plunge_gap)?;
        if m.plunge_gap < 0.0 || m.plunge_gap > m.safe_z {
            return Err(CoreError::validation(
                "the plunge gap (Z2) must be between zero and the clearance height (Z1)",
            ));
        }
        finite("machine.homeX", m.home_x)?;
        finite("machine.homeY", m.home_y)?;
        positive("machine.homeZ", m.home_z)?;
        finite("machine.modelGap", m.model_gap)?;
        if m.model_gap < 0.0 || m.model_gap > self.thickness {
            return Err(CoreError::validation(
                "the gap between the model and the material must be between zero and the material thickness",
            ));
        }
        Ok(())
    }

    /// The job with its flat size and Z zero derived from the rotary blank
    /// when it is a rotary job, and unchanged otherwise. The flat layout of a
    /// rotary job is the unwrapped surface: `length` along the axis and one
    /// circumference across it. The thickness is the radius, so that a cut
    /// can reach the axis, and Z zero on the axis is Z zero on the bottom of
    /// that thickness. A gap above the model (Material Setup) deeper than
    /// the material is brought up to its thickness.
    pub fn normalised(&self) -> JobSetup {
        let mut job = self.rotary_normalised();
        // A thinner material takes the gap above the model down with it,
        // so the model still sits inside it.
        if job.machine.model_gap > job.thickness && job.thickness > 0.0 {
            job.machine.model_gap = job.thickness;
        }
        job
    }

    fn rotary_normalised(&self) -> JobSetup {
        let mut job = *self;
        if job.kind != JobKind::Rotary {
            return job;
        }
        let rotary = job.rotary;
        let around = rotary.circumference();
        match rotary.axis {
            RotaryAxis::X => {
                job.width = rotary.length;
                job.height = around;
            }
            RotaryAxis::Y => {
                job.width = around;
                job.height = rotary.length;
            }
        }
        job.thickness = rotary.radius();
        job.z_zero = match rotary.z_zero {
            RotaryZZero::Centre => ZZero::Bottom,
            RotaryZZero::Surface => ZZero::Top,
        };
        job
    }

    /// How far the corners of a square rotary blank reach past the surface
    /// the design wraps onto, in millimetres; zero for a round blank or a flat
    /// job.
    pub fn rotary_corner_rise(&self) -> f64 {
        if self.is_rotary() && self.rotary.blank == RotaryBlank::Square {
            self.rotary.radius() * (std::f64::consts::SQRT_2 - 1.0)
        } else {
            0.0
        }
    }

    /// Whether the job is a rotary job.
    pub fn is_rotary(&self) -> bool {
        self.kind == JobKind::Rotary
    }

    /// Job Z of the material's top surface: zero when Z zero is on top, the
    /// thickness when it is on the machine bed.
    pub fn surface_z(&self) -> f64 {
        match self.z_zero {
            ZZero::Top => 0.0,
            ZZero::Bottom => self.thickness,
        }
    }

    /// Position of the XY datum in job space, where (0, 0) is the bottom left
    /// corner of the job rectangle and Y points up. Millimetres.
    pub fn datum(&self) -> (f64, f64) {
        let (x, y) = match self.xy_origin {
            XyOrigin::Centre => (self.width / 2.0, self.height / 2.0),
            XyOrigin::BottomLeft => (0.0, 0.0),
            XyOrigin::BottomRight => (self.width, 0.0),
            XyOrigin::TopLeft => (0.0, self.height),
            XyOrigin::TopRight => (self.width, self.height),
        };
        (x + self.offset_x, y + self.offset_y)
    }
}

impl JobSetup {
    /// The material rectangle in job space, in millimetres.
    pub fn material_bounds(&self) -> crate::geom::BBox {
        let (x, y) = self.datum();
        crate::geom::BBox::new(
            crate::geom::Point::new(-x, -y),
            crate::geom::Point::new(self.width - x, self.height - y),
        )
    }
}

/// Mapping between the coordinates of the two sides of a double sided job.
///
/// Each side's vectors, model and toolpaths live in that side's own job
/// space: X and Y as seen looking down at the face that is up while the side
/// is machined, with the same XY datum rule on both sides, and Z measured
/// from that side's Z zero. The mappings are their own inverse.
impl JobSetup {
    /// Depth of the modelling plane (the model's zero plane) below the top
    /// side's material surface, in millimetres, for a top side model whose
    /// highest point is `top_peak` above that plane and, on a double sided
    /// job, a bottom side model `bottom_peak` deep under it. With Gap Above
    /// Model the top peak sits the Material Setup gap below the surface;
    /// with Gap Below Model the lowest point of the model (the plane, or
    /// the bottom side's peak) sits the gap above the material bottom. The
    /// plane never leaves the material. A double sided job's two sides
    /// share this plane: the bottom side's model stands on it facing the
    /// other way.
    pub fn modelling_plane_depth(&self, top_peak: f64, bottom_peak: f64) -> f64 {
        let gap = self.machine.model_gap.max(0.0);
        let depth = if self.machine.gap_below {
            let under = if self.is_double_sided() {
                bottom_peak.max(0.0)
            } else {
                0.0
            };
            self.thickness - gap - under
        } else {
            gap + top_peak.max(0.0)
        };
        depth.min(self.thickness).max(0.0)
    }

    /// Job Z of the modelling plane while `side` is machined (this job being
    /// that side's view of the setup, as [`JobSetup::for_side`] gives it),
    /// for a top side model `top_peak` high and a bottom side model
    /// `bottom_peak` high. On the bottom side the plane lies
    /// `thickness - depth` under that side's surface.
    pub fn model_zero(&self, side: Side, top_peak: f64, bottom_peak: f64) -> f64 {
        let depth = self.modelling_plane_depth(top_peak, bottom_peak);
        match side {
            Side::Bottom if self.is_double_sided() => self.surface_z() - (self.thickness - depth),
            _ => self.surface_z() - depth,
        }
    }

    /// Whether the job is machined from both faces.
    pub fn is_double_sided(&self) -> bool {
        self.kind == JobKind::DoubleSided
    }

    /// Where Z zero sits while `side` is machined.
    pub fn z_zero_of(&self, side: Side) -> ZZero {
        match side {
            Side::Top => self.z_zero,
            Side::Bottom => self.double_sided.bottom_z_zero,
        }
    }

    /// The job as seen while machining `side`: the same material with that
    /// side's Z zero. A single sided job always answers itself.
    pub fn for_side(&self, side: Side) -> JobSetup {
        let mut job = *self;
        if self.is_double_sided() {
            job.z_zero = self.z_zero_of(side);
        }
        job
    }

    /// Maps a point in one side's job space to the other side's, in
    /// millimetres. The material is turned over about its centre line, so
    /// the datum lands on the same corner of the face that is up.
    pub fn flip_xy(&self, x: f64, y: f64) -> (f64, f64) {
        let (datum_x, datum_y) = self.datum();
        match self.double_sided.flip {
            FlipAxis::Horizontal => (self.width - 2.0 * datum_x - x, y),
            FlipAxis::Vertical => (x, self.height - 2.0 * datum_y - y),
        }
    }

    /// Maps a job Z on `from` to the job Z of the same material point seen
    /// from the other side, in millimetres. A point `d` below one face is
    /// `thickness - d` below the other.
    pub fn flip_z(&self, z: f64, from: Side) -> f64 {
        let above_bed = match self.z_zero_of(from) {
            ZZero::Top => self.thickness + z,
            ZZero::Bottom => z,
        };
        let turned = self.thickness - above_bed;
        match self.z_zero_of(from.other()) {
            ZZero::Top => turned - self.thickness,
            ZZero::Bottom => turned,
        }
    }
}

fn finite(field: &str, value: f64) -> Result<(), CoreError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(CoreError::validation(format!("{field} must be a number")))
    }
}

fn positive(field: &str, value: f64) -> Result<(), CoreError> {
    finite(field, value)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(CoreError::validation(format!(
            "{field} must be greater than zero"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_job_is_valid() {
        assert!(JobSetup::default().validate().is_ok());
    }

    #[test]
    fn the_modelling_plane_is_shared_by_both_sides() {
        let mut job = JobSetup {
            kind: JobKind::DoubleSided,
            thickness: 18.0,
            ..JobSetup::default()
        };
        job.machine.model_gap = 1.0;
        assert_eq!(job.modelling_plane_depth(5.0, 0.0), 6.0);
        assert_eq!(job.modelling_plane_depth(40.0, 0.0), 18.0);
        let top = job.for_side(Side::Top);
        let bottom = job.for_side(Side::Bottom);
        // Physical height of the plane above the bottom face agrees.
        let from_bottom_top = top.model_zero(Side::Top, 5.0, 0.0) - (top.surface_z() - 18.0);
        let from_bottom_bottom = bottom.surface_z() - bottom.model_zero(Side::Bottom, 5.0, 0.0);
        assert!((from_bottom_top - 12.0).abs() < 1e-9);
        assert!((from_bottom_bottom - 12.0).abs() < 1e-9);
        // A single sided job ignores the side.
        job.kind = JobKind::SingleSided;
        assert_eq!(
            job.model_zero(Side::Bottom, 5.0, 0.0),
            job.model_zero(Side::Top, 5.0, 0.0)
        );
    }

    #[test]
    fn gap_below_keeps_the_model_above_the_material_bottom() {
        let mut job = JobSetup {
            thickness: 18.0,
            ..JobSetup::default()
        };
        job.machine.model_gap = 3.0;
        job.machine.gap_below = true;
        // The plane stays 3 above the bottom whatever the model's height.
        assert_eq!(job.modelling_plane_depth(5.0, 0.0), 15.0);
        assert_eq!(job.modelling_plane_depth(9.0, 0.0), 15.0);
        assert_eq!(job.model_zero(Side::Top, 9.0, 0.0), -15.0);
        // A double sided job measures to the underside of the bottom model.
        job.kind = JobKind::DoubleSided;
        assert_eq!(job.modelling_plane_depth(5.0, 4.0), 11.0);
        // Never out of the material.
        job.machine.model_gap = 18.0;
        assert_eq!(job.modelling_plane_depth(5.0, 4.0), 0.0);
        // Older files have no flag and keep the gap above.
        let old: MachineSetup = serde_json::from_str(r#"{"safeZ":5,"modelGap":2}"#).expect("json");
        assert!(!old.gap_below);
    }

    #[test]
    fn a_thinner_material_brings_the_gap_above_the_model_with_it() {
        let mut doc = crate::doc::Document::new();
        let mut job = JobSetup::default();
        job.machine.model_gap = 10.0;
        doc.set_job(job).expect("a 10 mm gap in 18 mm");
        // Job Setup sends the material setup unchanged with a new thickness.
        job.thickness = 6.0;
        doc.set_job(job).expect("thinner material");
        assert_eq!(doc.job().machine.model_gap, 6.0);
        // A rotary job's thickness is its radius.
        let mut rotary = job;
        rotary.kind = JobKind::Rotary;
        rotary.rotary.diameter = 8.0;
        rotary.machine.model_gap = 6.0;
        doc.set_job(rotary).expect("rotary");
        assert_eq!(doc.job().machine.model_gap, 4.0);
    }

    #[test]
    fn rejects_non_positive_sizes() {
        for mutate in [
            |j: &mut JobSetup| j.width = 0.0,
            |j: &mut JobSetup| j.height = -1.0,
            |j: &mut JobSetup| j.thickness = 0.0,
        ] {
            let mut job = JobSetup::default();
            mutate(&mut job);
            assert!(job.validate().is_err());
        }
    }

    #[test]
    fn rejects_non_finite_values() {
        let mut job = JobSetup {
            offset_x: f64::NAN,
            ..JobSetup::default()
        };
        assert!(job.validate().is_err());
        job.offset_x = 0.0;
        job.width = f64::INFINITY;
        assert!(job.validate().is_err());
    }

    #[test]
    fn rotary_diameter_only_checked_for_rotary_jobs() {
        let mut job = JobSetup {
            rotary: RotarySetup {
                diameter: 0.0,
                ..RotarySetup::default()
            },
            ..JobSetup::default()
        };
        assert!(job.validate().is_ok());
        job.kind = JobKind::Rotary;
        assert!(job.validate().is_err());
        job.rotary.diameter = 80.0;
        assert!(job.validate().is_ok());
    }

    #[test]
    fn datum_follows_origin_and_offset() {
        let mut job = JobSetup::default();
        assert_eq!(job.datum(), (0.0, 0.0));
        job.xy_origin = XyOrigin::Centre;
        assert_eq!(job.datum(), (150.0, 100.0));
        job.xy_origin = XyOrigin::TopRight;
        job.offset_x = -10.0;
        assert_eq!(job.datum(), (290.0, 200.0));
    }

    fn double_sided(flip: FlipAxis) -> JobSetup {
        JobSetup {
            width: 300.0,
            height: 200.0,
            thickness: 18.0,
            kind: JobKind::DoubleSided,
            double_sided: DoubleSidedSetup {
                flip,
                bottom_z_zero: ZZero::Top,
            },
            ..JobSetup::default()
        }
    }

    #[test]
    fn a_horizontal_flip_mirrors_x_across_the_material() {
        let job = double_sided(FlipAxis::Horizontal);
        assert_eq!(job.flip_xy(10.0, 20.0), (290.0, 20.0));
        let mut centred = job;
        centred.xy_origin = XyOrigin::Centre;
        assert_eq!(centred.flip_xy(-140.0, 20.0), (140.0, 20.0));
        centred.xy_origin = XyOrigin::BottomLeft;
        centred.offset_x = 5.0;
        // Datum 5 mm in: the point 10 mm from the datum sits 15 mm from the
        // left edge, so 15 mm from the right edge once turned over.
        assert_eq!(centred.flip_xy(10.0, 0.0), (280.0, 0.0));
    }

    #[test]
    fn a_vertical_flip_mirrors_y_across_the_material() {
        let job = double_sided(FlipAxis::Vertical);
        assert_eq!(job.flip_xy(10.0, 20.0), (10.0, 180.0));
    }

    #[test]
    fn flipping_twice_is_the_identity() {
        for flip in [FlipAxis::Horizontal, FlipAxis::Vertical] {
            let mut job = double_sided(flip);
            job.xy_origin = XyOrigin::TopRight;
            job.offset_y = 3.0;
            let (x, y) = job.flip_xy(-37.5, -12.25);
            assert_eq!(job.flip_xy(x, y), (-37.5, -12.25));
            for side in [Side::Top, Side::Bottom] {
                let z = job.flip_z(-4.0, side);
                assert!((job.flip_z(z, side.other()) + 4.0).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn depth_below_one_face_is_the_rest_of_the_thickness_below_the_other() {
        let mut job = double_sided(FlipAxis::Horizontal);
        // 5 mm into an 18 mm board from the top is 13 mm into it from the
        // bottom.
        assert_eq!(job.flip_z(-5.0, Side::Top), -13.0);
        job.double_sided.bottom_z_zero = ZZero::Bottom;
        assert_eq!(job.flip_z(-5.0, Side::Top), 5.0);
        job.z_zero = ZZero::Bottom;
        assert_eq!(job.flip_z(13.0, Side::Top), 5.0);
    }

    #[test]
    fn each_side_has_its_own_z_zero() {
        let mut job = double_sided(FlipAxis::Horizontal);
        job.double_sided.bottom_z_zero = ZZero::Bottom;
        assert_eq!(job.for_side(Side::Top).surface_z(), 0.0);
        assert_eq!(job.for_side(Side::Bottom).surface_z(), 18.0);
        job.kind = JobKind::SingleSided;
        assert_eq!(job.for_side(Side::Bottom).surface_z(), 0.0);
    }

    #[test]
    fn older_jobs_without_double_sided_settings_still_read() {
        let json = r#"{"units":"mm","width":300,"height":200,"thickness":18,"zZero":"top",
            "xyOrigin":"bottomLeft","offsetX":0,"offsetY":0,"kind":"doubleSided",
            "rotary":{"diameter":100,"axis":"x"}}"#;
        let job: JobSetup = serde_json::from_str(json).expect("reads");
        assert_eq!(job.double_sided, DoubleSidedSetup::default());
    }

    fn rotary(axis: RotaryAxis, z_zero: RotaryZZero) -> JobSetup {
        JobSetup {
            kind: JobKind::Rotary,
            rotary: RotarySetup {
                diameter: 50.0,
                axis,
                length: 240.0,
                blank: RotaryBlank::Cylinder,
                z_zero,
            },
            ..JobSetup::default()
        }
    }

    #[test]
    fn a_rotary_job_along_x_lays_out_length_by_circumference() {
        let job = rotary(RotaryAxis::X, RotaryZZero::Centre).normalised();
        assert_eq!(job.width, 240.0);
        assert!((job.height - std::f64::consts::PI * 50.0).abs() < 1e-12);
        assert_eq!(job.thickness, 25.0);
        // Z zero on the axis: the surface is one radius up.
        assert_eq!(job.z_zero, ZZero::Bottom);
        assert_eq!(job.surface_z(), 25.0);
        assert!(job.validate().is_ok());
    }

    #[test]
    fn a_rotary_job_along_y_swaps_the_layout() {
        let job = rotary(RotaryAxis::Y, RotaryZZero::Surface).normalised();
        assert_eq!(job.height, 240.0);
        assert!((job.width - std::f64::consts::PI * 50.0).abs() < 1e-12);
        assert_eq!(job.z_zero, ZZero::Top);
        assert_eq!(job.surface_z(), 0.0);
    }

    #[test]
    fn flat_jobs_are_not_normalised() {
        let job = JobSetup::default();
        assert_eq!(job.normalised(), job);
        assert!(!job.is_rotary());
    }

    #[test]
    fn a_square_blank_measures_its_heights_from_the_corners_sweep() {
        use crate::cam::common::Heights;
        let mut job = rotary(RotaryAxis::X, RotaryZZero::Centre);
        job.rotary.diameter = 100.0;
        let round = job.normalised();
        assert_eq!(round.rotary_corner_rise(), 0.0);
        let heights = Heights::from_job(&round);
        assert!((heights.safe - 55.0).abs() < 1e-9, "{heights:?}");
        assert!((heights.plunge_from - 52.0).abs() < 1e-9, "{heights:?}");

        job.rotary.blank = RotaryBlank::Square;
        let square = job.normalised();
        // The corners reach 50 x sqrt(2) = 70.711 from the axis.
        let corner = 50.0 * std::f64::consts::SQRT_2;
        assert!((square.rotary_corner_rise() - (corner - 50.0)).abs() < 1e-9);
        let heights = Heights::from_job(&square);
        assert!((heights.surface - 50.0).abs() < 1e-9, "{heights:?}");
        assert!((heights.safe - (corner + 5.0)).abs() < 1e-9, "{heights:?}");
        assert!((heights.plunge_from - (corner + 2.0)).abs() < 1e-9);
        assert!((heights.home.z - (corner + 20.0)).abs() < 1e-9);

        let flat = JobSetup::default();
        assert_eq!(flat.rotary_corner_rise(), 0.0);
    }

    #[test]
    fn rotary_length_must_be_positive() {
        let mut job = rotary(RotaryAxis::X, RotaryZZero::Centre);
        job.rotary.length = 0.0;
        assert!(job.validate().is_err());
    }

    #[test]
    fn older_rotary_settings_still_read() {
        let json = r#"{"units":"mm","width":300,"height":200,"thickness":18,"zZero":"top",
            "xyOrigin":"bottomLeft","offsetX":0,"offsetY":0,"kind":"rotary",
            "rotary":{"diameter":80,"axis":"y"}}"#;
        let job: JobSetup = serde_json::from_str(json).expect("reads");
        assert_eq!(job.rotary.diameter, 80.0);
        assert_eq!(job.rotary.axis, RotaryAxis::Y);
        assert_eq!(job.rotary.length, 300.0);
        assert_eq!(job.rotary.blank, RotaryBlank::Cylinder);
        assert_eq!(job.rotary.z_zero, RotaryZZero::Centre);
    }

    #[test]
    fn serialises_with_camel_case_names() {
        let json = serde_json::to_string(&JobSetup::default()).expect("job serialises");
        assert!(json.contains("\"zZero\":\"top\""), "{json}");
        assert!(json.contains("\"xyOrigin\":\"bottomLeft\""), "{json}");
        assert!(json.contains("\"kind\":\"singleSided\""), "{json}");
        assert!(json.contains("\"units\":\"mm\""), "{json}");
        let back: JobSetup = serde_json::from_str(&json).expect("job round trips");
        assert_eq!(back, JobSetup::default());
    }

    #[test]
    fn the_appearance_is_kept_with_the_job_and_old_jobs_get_the_default() {
        let job = JobSetup {
            appearance: Appearance {
                material: crate::prefs::Material3d::Walnut,
                colour: [0x12, 0xab, 0xef],
                texture: 0,
            },
            ..JobSetup::default()
        };
        let text = serde_json::to_string(&job).expect("writes");
        assert!(text.contains("\"colour\":\"#12abef\""), "{text}");
        let back: JobSetup = serde_json::from_str(&text).expect("reads");
        assert_eq!(back.appearance, job.appearance);
        let mut old = serde_json::to_value(JobSetup::default()).expect("value");
        old.as_object_mut().expect("object").remove("appearance");
        let read: JobSetup = serde_json::from_value(old).expect("old job reads");
        assert_eq!(read.appearance, Appearance::default());
        let mut bad = serde_json::to_value(job).expect("value");
        bad["appearance"]["colour"] = serde_json::json!("red");
        assert!(serde_json::from_value::<JobSetup>(bad).is_err());
    }

    #[test]
    fn the_modelling_resolution_is_kept_and_old_jobs_are_standard() {
        let job = JobSetup {
            resolution: ModelResolution::Highest,
            ..JobSetup::default()
        };
        let text = serde_json::to_string(&job).expect("writes");
        assert!(text.contains("\"resolution\":\"highest\""), "{text}");
        let back: JobSetup = serde_json::from_str(&text).expect("reads");
        assert_eq!(back.resolution, ModelResolution::Highest);
        let mut old = serde_json::to_value(JobSetup::default()).expect("value");
        old.as_object_mut().expect("object").remove("resolution");
        let read: JobSetup = serde_json::from_value(old).expect("old job reads");
        assert_eq!(read.resolution, ModelResolution::Standard);
        assert!(ModelResolution::High.cell_factor() > ModelResolution::Standard.cell_factor());
    }
}
