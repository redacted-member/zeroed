//! Cutting tools: geometry, feeds and speeds, and the half section profile
//! that simulation and the 3D strategies cut with.
//!
//! Every length stored on a [`Tool`] is millimetres, whatever [`Tool::units`]
//! says: that field only records the unit the user types and reads values in.
//! Feed rates are the exception, they are stored in [`Tool::feed_units`] so
//! that the number the user typed survives a round trip, and
//! [`Tool::feed_mm_per_min`] converts on demand.

use serde::{Deserialize, Serialize};

use crate::doc::Units;
use crate::error::CoreError;
use crate::geom::{flatten_path, Path, Point, Tolerance};

/// Unit a feed or plunge rate is expressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FeedUnits {
    /// Millimetres per second.
    MmPerSec,
    /// Millimetres per minute.
    MmPerMin,
    /// Inches per second.
    InchPerSec,
    /// Inches per minute.
    InchPerMin,
    /// Metres per minute.
    MetrePerMin,
}

impl FeedUnits {
    /// Millimetres per minute for one unit of this rate.
    pub fn to_mm_per_min(self) -> f64 {
        match self {
            FeedUnits::MmPerSec => 60.0,
            FeedUnits::MmPerMin => 1.0,
            FeedUnits::InchPerSec => 25.4 * 60.0,
            FeedUnits::InchPerMin => 25.4,
            FeedUnits::MetrePerMin => 1000.0,
        }
    }

    /// Converts a rate in these units to millimetres per minute.
    pub fn as_mm_per_min(self, rate: f64) -> f64 {
        rate * self.to_mm_per_min()
    }

    /// Converts millimetres per minute to a rate in these units.
    pub fn from_mm_per_min(self, mm_per_min: f64) -> f64 {
        mm_per_min / self.to_mm_per_min()
    }
}

/// Tool shape and the geometry that goes with it. All lengths are
/// millimetres, all angles degrees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToolGeometry {
    /// Flat bottomed cylindrical cutter.
    EndMill {
        /// Cutting diameter.
        diameter: f64,
    },
    /// Hemispherical tipped cutter.
    BallNose {
        /// Cutting diameter, twice the tip radius.
        diameter: f64,
    },
    /// Conical cutter that runs to a point.
    VBit {
        /// Diameter at the top of the cone, the widest cut it can make.
        diameter: f64,
        /// Full angle across the cone, in degrees.
        included_angle: f64,
    },
    /// Conical cutter with a flat tip.
    EngravingTool {
        /// Diameter at the top of the cone.
        diameter: f64,
        /// Full angle across the cone, in degrees.
        included_angle: f64,
        /// Width of the flat at the tip.
        flat_diameter: f64,
    },
    /// Tapered cutter with a rounded tip, used for deep detail work.
    TaperedBallNose {
        /// Diameter at the top of the taper.
        diameter: f64,
        /// Radius of the ball at the tip.
        tip_radius: f64,
        /// Half angle of the taper measured from the tool axis, in degrees.
        taper_angle: f64,
    },
    /// Flat bottomed cutter with rounded corners, sometimes called a bull nose.
    RadiusedEndMill {
        /// Cutting diameter.
        diameter: f64,
        /// Radius of the rounded corner.
        corner_radius: f64,
    },
    /// Ground to a custom shape. The profile is the half section in the
    /// radius/height plane: x is the distance from the tool axis and y is the
    /// height above the lowest point of the tool.
    FormTool {
        /// Half section of the cutter.
        profile: Path,
    },
    /// Twist drill, plunged straight down.
    Drill {
        /// Drilled hole diameter.
        diameter: f64,
        /// Full angle across the point, in degrees; the hole's wall stops
        /// short of the depth, which is the depth of the point's tip.
        #[serde(default = "drill_point_angle")]
        point_angle: f64,
    },
    /// Thread milling cutter.
    ThreadMill {
        /// Diameter over the thread form.
        diameter: f64,
        /// Distance between thread crests along the axis.
        pitch: f64,
        /// Full angle across the thread form, in degrees.
        included_angle: f64,
        /// Height of the lowest tooth's point above the bottom of the
        /// cutter, in millimetres; the thread stops this far above the
        /// deepest the cutter goes.
        #[serde(default)]
        tooth_offset: f64,
        /// Number of teeth up the cutter one pitch apart: one for a single
        /// point cutter, more for a multi point cutter that cuts that many
        /// turns of the thread at once.
        #[serde(default = "one_tooth")]
        teeth: u32,
    },
    /// Sprung diamond point that scratches the surface without spinning.
    DiamondDrag {
        /// Widest mark the point can make.
        diameter: f64,
        /// Full angle across the point, in degrees.
        angle: f64,
    },
    /// Laser or other jet cutter driven by the laser module.
    LaserTool {
        /// Output power as a percentage of the machine maximum.
        power: f64,
        /// Cutting speed in millimetres per minute.
        speed: f64,
        /// Width of material the beam removes.
        kerf: f64,
        /// Maximum Burn Rate: the fastest speed, millimetres per minute, at
        /// which the beam at full power still burns the material. For the
        /// preview only; calibrate it against a burnt sample.
        #[serde(default = "default_max_burn_rate")]
        max_burn_rate: f64,
    },
}

/// A laser tool's Maximum Burn Rate when none is given, millimetres per
/// minute: full power at this speed chars the preview black.
pub const DEFAULT_MAX_BURN_RATE: f64 = 3000.0;

fn default_max_burn_rate() -> f64 {
    DEFAULT_MAX_BURN_RATE
}

fn one_tooth() -> u32 {
    1
}

/// Point angle of a twist drill when the tool database does not give one.
pub const DRILL_POINT_ANGLE: f64 = 118.0;

fn drill_point_angle() -> f64 {
    DRILL_POINT_ANGLE
}

impl ToolGeometry {
    /// Short machine readable name of the shape, used by the tool list column
    /// and by post processor tool summaries.
    pub fn type_name(&self) -> &'static str {
        match self {
            ToolGeometry::EndMill { .. } => "endMill",
            ToolGeometry::BallNose { .. } => "ballNose",
            ToolGeometry::VBit { .. } => "vBit",
            ToolGeometry::EngravingTool { .. } => "engravingTool",
            ToolGeometry::TaperedBallNose { .. } => "taperedBallNose",
            ToolGeometry::RadiusedEndMill { .. } => "radiusedEndMill",
            ToolGeometry::FormTool { .. } => "formTool",
            ToolGeometry::Drill { .. } => "drill",
            ToolGeometry::ThreadMill { .. } => "threadMill",
            ToolGeometry::DiamondDrag { .. } => "diamondDrag",
            ToolGeometry::LaserTool { .. } => "laserTool",
        }
    }

    /// Widest diameter of the cutting part, in millimetres.
    pub fn diameter(&self) -> f64 {
        match self {
            ToolGeometry::EndMill { diameter }
            | ToolGeometry::BallNose { diameter }
            | ToolGeometry::VBit { diameter, .. }
            | ToolGeometry::EngravingTool { diameter, .. }
            | ToolGeometry::TaperedBallNose { diameter, .. }
            | ToolGeometry::RadiusedEndMill { diameter, .. }
            | ToolGeometry::Drill { diameter, .. }
            | ToolGeometry::ThreadMill { diameter, .. }
            | ToolGeometry::DiamondDrag { diameter, .. } => *diameter,
            ToolGeometry::LaserTool { kerf, .. } => *kerf,
            ToolGeometry::FormTool { profile } => form_tool_radius(profile) * 2.0,
        }
    }

    /// Half the widest diameter, in millimetres.
    pub fn radius(&self) -> f64 {
        self.diameter() / 2.0
    }

    /// Height above the tip at which the tool reaches its full diameter, in
    /// millimetres. Zero for a flat bottomed cutter.
    pub fn shoulder_height(&self) -> f64 {
        let radius = self.radius();
        match self {
            ToolGeometry::EndMill { .. }
            | ToolGeometry::ThreadMill { .. }
            | ToolGeometry::LaserTool { .. } => 0.0,
            ToolGeometry::Drill { point_angle, .. } => radius / half_angle_tan(*point_angle),
            ToolGeometry::BallNose { .. } => radius,
            ToolGeometry::VBit { included_angle, .. }
            | ToolGeometry::DiamondDrag {
                angle: included_angle,
                ..
            } => radius / half_angle_tan(*included_angle),
            ToolGeometry::EngravingTool {
                included_angle,
                flat_diameter,
                ..
            } => ((radius - flat_diameter / 2.0).max(0.0)) / half_angle_tan(*included_angle),
            ToolGeometry::RadiusedEndMill { corner_radius, .. } => corner_radius.min(radius),
            ToolGeometry::TaperedBallNose {
                tip_radius,
                taper_angle,
                ..
            } => {
                let (contact_r, contact_z) = taper_contact(*tip_radius, *taper_angle);
                let taper_tan = (taper_angle.to_radians()).tan();
                if taper_tan <= f64::EPSILON || radius <= contact_r {
                    contact_z
                } else {
                    contact_z + (radius - contact_r) / taper_tan
                }
            }
            ToolGeometry::FormTool { profile } => form_tool_height(profile),
        }
    }

    /// Radius of the tool at `z` millimetres above its lowest point. Values
    /// below zero give the radius at the tip, values above the shoulder give
    /// the full radius.
    pub fn radius_at_height(&self, z: f64) -> f64 {
        let radius = self.radius();
        if z <= 0.0 {
            return self.tip_radius();
        }
        let raw = match self {
            ToolGeometry::EndMill { .. }
            | ToolGeometry::ThreadMill { .. }
            | ToolGeometry::LaserTool { .. } => radius,
            ToolGeometry::Drill { point_angle, .. } => z * half_angle_tan(*point_angle),
            ToolGeometry::BallNose { .. } => {
                if z >= radius {
                    radius
                } else {
                    (z * (2.0 * radius - z)).max(0.0).sqrt()
                }
            }
            ToolGeometry::VBit { included_angle, .. }
            | ToolGeometry::DiamondDrag {
                angle: included_angle,
                ..
            } => z * half_angle_tan(*included_angle),
            ToolGeometry::EngravingTool {
                included_angle,
                flat_diameter,
                ..
            } => flat_diameter / 2.0 + z * half_angle_tan(*included_angle),
            ToolGeometry::RadiusedEndMill {
                corner_radius,
                diameter,
            } => {
                let corner = corner_radius.min(diameter / 2.0);
                let flat = (radius - corner).max(0.0);
                if z >= corner {
                    radius
                } else {
                    flat + (z * (2.0 * corner - z)).max(0.0).sqrt()
                }
            }
            ToolGeometry::TaperedBallNose {
                tip_radius,
                taper_angle,
                ..
            } => {
                let (contact_r, contact_z) = taper_contact(*tip_radius, *taper_angle);
                if z <= contact_z {
                    (z * (2.0 * tip_radius - z)).max(0.0).sqrt()
                } else {
                    contact_r + (z - contact_z) * taper_angle.to_radians().tan()
                }
            }
            ToolGeometry::FormTool { profile } => form_tool_radius_at(profile, z),
        };
        raw.clamp(0.0, radius)
    }

    /// Radius of the flat at the very tip of the tool, in millimetres.
    pub fn tip_radius(&self) -> f64 {
        match self {
            ToolGeometry::EndMill { diameter } | ToolGeometry::ThreadMill { diameter, .. } => {
                diameter / 2.0
            }
            ToolGeometry::LaserTool { kerf, .. } => kerf / 2.0,
            ToolGeometry::EngravingTool { flat_diameter, .. } => flat_diameter / 2.0,
            ToolGeometry::RadiusedEndMill {
                diameter,
                corner_radius,
            } => (diameter / 2.0 - corner_radius).max(0.0),
            ToolGeometry::BallNose { .. }
            | ToolGeometry::VBit { .. }
            | ToolGeometry::TaperedBallNose { .. }
            | ToolGeometry::Drill { .. }
            | ToolGeometry::DiamondDrag { .. } => 0.0,
            ToolGeometry::FormTool { profile } => form_tool_radius_at(profile, 0.0),
        }
    }

    /// Checks the geometry is machinable.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] naming the first field that fails.
    pub fn validate(&self) -> Result<(), CoreError> {
        match self {
            ToolGeometry::EndMill { diameter } | ToolGeometry::BallNose { diameter } => {
                positive("diameter", *diameter)
            }
            ToolGeometry::Drill {
                diameter,
                point_angle,
            } => {
                positive("diameter", *diameter)?;
                angle("pointAngle", *point_angle)
            }
            ToolGeometry::VBit {
                diameter,
                included_angle,
            } => {
                positive("diameter", *diameter)?;
                angle("includedAngle", *included_angle)
            }
            ToolGeometry::DiamondDrag {
                diameter,
                angle: point_angle,
            } => {
                positive("diameter", *diameter)?;
                angle("angle", *point_angle)
            }
            ToolGeometry::EngravingTool {
                diameter,
                included_angle,
                flat_diameter,
            } => {
                positive("diameter", *diameter)?;
                angle("includedAngle", *included_angle)?;
                not_negative("flatDiameter", *flat_diameter)?;
                at_most("flatDiameter", *flat_diameter, *diameter)
            }
            ToolGeometry::TaperedBallNose {
                diameter,
                tip_radius,
                taper_angle,
            } => {
                positive("diameter", *diameter)?;
                not_negative("tipRadius", *tip_radius)?;
                at_most("tipRadius", *tip_radius, diameter / 2.0)?;
                angle("taperAngle", *taper_angle * 2.0)
            }
            ToolGeometry::RadiusedEndMill {
                diameter,
                corner_radius,
            } => {
                positive("diameter", *diameter)?;
                not_negative("cornerRadius", *corner_radius)?;
                at_most("cornerRadius", *corner_radius, diameter / 2.0)
            }
            ToolGeometry::ThreadMill {
                diameter,
                pitch,
                included_angle,
                tooth_offset,
                teeth,
            } => {
                positive("diameter", *diameter)?;
                positive("pitch", *pitch)?;
                not_negative("toothOffset", *tooth_offset)?;
                positive("teeth", f64::from(*teeth))?;
                angle("includedAngle", *included_angle)
            }
            ToolGeometry::LaserTool {
                power,
                speed,
                kerf,
                max_burn_rate,
            } => {
                positive("power", *power)?;
                positive("speed", *speed)?;
                positive("maxBurnRate", *max_burn_rate)?;
                positive("kerf", *kerf)
            }
            ToolGeometry::FormTool { profile } => {
                if form_tool_radius(profile) > 0.0 {
                    Ok(())
                } else {
                    Err(CoreError::validation(
                        "profile must reach a radius greater than zero",
                    ))
                }
            }
        }
    }
}

/// Feeds and speeds for one machine and material pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CuttingData {
    /// Machine this data belongs to, empty for any machine.
    pub machine: String,
    /// Material this data belongs to, empty for any material.
    pub material: String,
    /// Unit the two rates are given in.
    pub feed_units: FeedUnits,
    /// Rate the tool travels at while cutting.
    pub feed_rate: f64,
    /// Rate the tool travels at while moving down into the material.
    pub plunge_rate: f64,
    /// Spindle speed in revolutions per minute.
    pub spindle_speed: f64,
    /// Depth taken by one pass, in millimetres.
    pub pass_depth: f64,
    /// Sideways step between passes, in millimetres.
    pub stepover: f64,
    /// Tool number on this machine, where it differs from the tool's own
    /// (the slot in an automatic tool changer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
}

/// A user defined value a tool name template can show, written `{name}` in
/// the template. It only labels the tool; nothing is cut differently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolVariable {
    /// Name used inside the braces.
    pub name: String,
    /// Text the braces are replaced by.
    pub value: String,
}

/// One entry of the tool database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    /// Database identity. Zero means the tool has not been saved yet.
    pub id: u64,
    /// Name shown in the tool list and written into toolpath names. It may
    /// be a template holding `{Variable}` fields, see [`super::naming`].
    pub name: String,
    /// Shape and geometry.
    pub tool_type: ToolGeometry,
    /// Free text, shown on the tool form.
    pub notes: String,
    /// Unit the user types geometry in. Storage stays millimetres.
    pub units: Units,
    /// Tool number sent to the machine on a tool change.
    pub number: u32,
    /// Unit the feed and plunge rates are given in.
    pub feed_units: FeedUnits,
    /// Rate the tool travels at while cutting, in `feed_units`.
    pub feed_rate: f64,
    /// Rate the tool travels at while moving down, in `feed_units`.
    pub plunge_rate: f64,
    /// Spindle speed in revolutions per minute.
    pub spindle_speed: f64,
    /// Depth taken by one pass, in millimetres.
    pub pass_depth: f64,
    /// Sideways step between passes, in millimetres. Kept in step with
    /// `stepover_percent`.
    pub stepover: f64,
    /// Sideways step between passes as a percentage of the tool diameter.
    pub stepover_percent: f64,
    /// Extra clearance left around the tool when clearing a pocket, as a
    /// percentage of the tool diameter.
    pub clearance: f64,
    /// Feeds and speeds for machine and material pairs, sorted by machine
    /// then material so that the file is stable.
    pub cutting_data: Vec<CuttingData>,
    /// Number of cutting flutes, used for the chip load. Zero when unknown.
    #[serde(default)]
    pub flutes: u32,
    /// The tool's own naming variables, sorted by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<ToolVariable>,
    /// Length of the cutting flutes above the tip, millimetres; zero when
    /// not known, which leaves the flute length and shank unchecked.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub flute_length: f64,
    /// Diameter of the plain shank above the flutes, millimetres; zero for
    /// the cutting diameter.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub shank_diameter: f64,
    /// How far the tool reaches below the holder, from the tip, millimetres.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub length_below_holder: f64,
    /// Diameter of the tool holder, millimetres; zero leaves the holder
    /// unchecked.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub holder_diameter: f64,
    /// Length of the tool holder, millimetres.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub holder_length: f64,
    /// Whether the tool cuts at its centre, so it can plunge straight down
    /// into material.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub center_cutting: bool,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl Default for Tool {
    fn default() -> Self {
        let mut tool = Tool {
            id: 0,
            name: String::new(),
            tool_type: ToolGeometry::EndMill { diameter: 6.0 },
            notes: String::new(),
            units: Units::Mm,
            number: 1,
            feed_units: FeedUnits::MmPerMin,
            feed_rate: 2400.0,
            plunge_rate: 600.0,
            spindle_speed: 18000.0,
            pass_depth: 3.0,
            stepover: 2.4,
            stepover_percent: 40.0,
            clearance: 0.0,
            cutting_data: Vec::new(),
            flutes: 2,
            variables: Vec::new(),
            flute_length: 0.0,
            shank_diameter: 0.0,
            length_below_holder: 0.0,
            holder_diameter: 0.0,
            holder_length: 0.0,
            center_cutting: true,
        };
        tool.sync_stepover_from_percent();
        tool
    }
}

impl Tool {
    /// Cutting diameter in millimetres.
    pub fn diameter(&self) -> f64 {
        self.tool_type.diameter()
    }

    /// Cutting radius in millimetres.
    pub fn radius(&self) -> f64 {
        self.tool_type.radius()
    }

    /// Cutting feed rate in millimetres per minute.
    pub fn feed_mm_per_min(&self) -> f64 {
        self.feed_units.as_mm_per_min(self.feed_rate)
    }

    /// Plunge feed rate in millimetres per minute.
    pub fn plunge_mm_per_min(&self) -> f64 {
        self.feed_units.as_mm_per_min(self.plunge_rate)
    }

    /// Sets the stepover in millimetres and recomputes the percentage.
    pub fn set_stepover(&mut self, stepover_mm: f64) {
        self.stepover = stepover_mm;
        let diameter = self.diameter();
        if diameter > 0.0 {
            self.stepover_percent = stepover_mm / diameter * 100.0;
        }
    }

    /// Sets the stepover as a percentage of the diameter and recomputes the
    /// millimetre value.
    pub fn set_stepover_percent(&mut self, percent: f64) {
        self.stepover_percent = percent;
        self.stepover = self.diameter() * percent / 100.0;
    }

    /// Recomputes the millimetre stepover from the percentage, which is what
    /// a change of diameter calls for.
    pub fn sync_stepover_from_percent(&mut self) {
        let percent = self.stepover_percent;
        self.set_stepover_percent(percent);
    }

    /// Half section of the tool as `samples` pairs of radius and height above
    /// the tip, evenly spaced from the axis out to the full radius.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when fewer than two samples are
    /// asked for.
    pub fn profile(&self, samples: usize) -> Result<Vec<(f64, f64)>, CoreError> {
        if samples < 2 {
            return Err(CoreError::validation(
                "a tool profile needs at least two samples",
            ));
        }
        let radius = self.radius();
        let shoulder = self.tool_type.shoulder_height();
        let steps = samples - 1;
        let mut out = Vec::with_capacity(samples);
        for index in 0..=steps {
            let r = radius * index as f64 / steps as f64;
            out.push((r, self.height_at_radius(r, shoulder)));
        }
        Ok(out)
    }

    /// Radius of the tool at `z` millimetres below the surface the tool tip
    /// touches, that is the width of the cut a plunge to that depth makes.
    /// `z` is negative below the surface, matching job space.
    pub fn effective_radius_at_depth(&self, z: f64) -> f64 {
        self.tool_type.radius_at_height((-z).max(0.0))
    }

    /// Feeds and speeds for a machine and material pair, falling back to an
    /// entry that leaves either field empty, then to the tool's own values.
    pub fn cutting_data_for(&self, machine: &str, material: &str) -> CuttingData {
        let exact = self
            .cutting_data
            .iter()
            .find(|d| d.machine == machine && d.material == material);
        let partial = || {
            self.cutting_data.iter().find(|d| {
                (d.machine == machine && d.material.is_empty())
                    || (d.machine.is_empty() && d.material == material)
            })
        };
        match exact.or_else(partial) {
            Some(found) => found.clone(),
            None => CuttingData {
                machine: machine.to_string(),
                material: material.to_string(),
                feed_units: self.feed_units,
                feed_rate: self.feed_rate,
                plunge_rate: self.plunge_rate,
                spindle_speed: self.spindle_speed,
                pass_depth: self.pass_depth,
                stepover: self.stepover,
                number: None,
            },
        }
    }

    /// Stores feeds and speeds for a machine and material pair, replacing any
    /// entry already there and keeping the list sorted.
    pub fn set_cutting_data(&mut self, data: CuttingData) {
        self.cutting_data
            .retain(|d| !(d.machine == data.machine && d.material == data.material));
        self.cutting_data.push(data);
        self.sort_cutting_data();
    }

    /// Drops the entry for a machine and material pair. Answers whether one
    /// was there.
    pub fn remove_cutting_data(&mut self, machine: &str, material: &str) -> bool {
        let before = self.cutting_data.len();
        self.cutting_data
            .retain(|d| !(d.machine == machine && d.material == material));
        before != self.cutting_data.len()
    }

    /// Copy of the tool with the feeds and speeds of a machine and material
    /// pair applied to its own fields, which is what a toolpath cuts with.
    pub fn with_cutting_data(&self, machine: &str, material: &str) -> Tool {
        let data = self.cutting_data_for(machine, material);
        let mut tool = self.clone();
        tool.feed_units = data.feed_units;
        tool.feed_rate = data.feed_rate;
        tool.plunge_rate = data.plunge_rate;
        tool.spindle_speed = data.spindle_speed;
        tool.pass_depth = data.pass_depth;
        tool.set_stepover(data.stepover);
        if let Some(number) = data.number {
            tool.number = number;
        }
        tool
    }

    /// Chip load in millimetres per tooth: the feed shared out over every
    /// flute of every revolution. `None` without flutes or a spindle speed.
    pub fn chip_load(&self) -> Option<f64> {
        if self.flutes == 0 || self.spindle_speed <= 0.0 {
            return None;
        }
        Some(self.feed_mm_per_min() / (self.spindle_speed * f64::from(self.flutes)))
    }

    /// Sets a naming variable, replacing one of the same name, and keeps
    /// the list sorted.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for an empty name or one holding a brace.
    pub fn set_variable(&mut self, name: &str, value: &str) -> Result<(), CoreError> {
        let name = name.trim();
        if name.is_empty() || name.contains(['{', '}', '|']) {
            return Err(CoreError::validation(
                "a variable needs a name without braces or bars",
            ));
        }
        self.variables.retain(|v| v.name != name);
        self.variables.push(ToolVariable {
            name: name.to_string(),
            value: value.to_string(),
        });
        self.variables.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(())
    }

    /// Sorts the cutting data so that the database file is byte stable.
    pub fn sort_cutting_data(&mut self) {
        self.cutting_data
            .sort_by(|a, b| (&a.machine, &a.material).cmp(&(&b.machine, &b.material)));
    }

    /// Checks the tool can be cut with.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] naming the first field that fails.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.name.trim().is_empty() {
            return Err(CoreError::validation("a tool needs a name"));
        }
        self.tool_type.validate()?;
        positive("feedRate", self.feed_rate)?;
        positive("plungeRate", self.plunge_rate)?;
        not_negative("spindleSpeed", self.spindle_speed)?;
        positive("passDepth", self.pass_depth)?;
        positive("stepover", self.stepover)?;
        not_negative("clearance", self.clearance)?;
        not_negative("fluteLength", self.flute_length)?;
        not_negative("shankDiameter", self.shank_diameter)?;
        not_negative("lengthBelowHolder", self.length_below_holder)?;
        not_negative("holderDiameter", self.holder_diameter)?;
        not_negative("holderLength", self.holder_length)?;
        Ok(())
    }

    /// Radius of the shank above the flutes, millimetres: the cutting
    /// radius unless a shank diameter is set.
    pub fn shank_radius(&self) -> f64 {
        if self.shank_diameter > 0.0 {
            self.shank_diameter / 2.0
        } else {
            self.radius()
        }
    }

    /// The holder as (radius, height of its bottom above the tip, height of
    /// its top), when the holder is to be checked: a diameter and a length
    /// below it are both set.
    pub fn holder(&self) -> Option<(f64, f64, f64)> {
        (self.holder_diameter > 0.0 && self.length_below_holder > 0.0).then(|| {
            let bottom = self.length_below_holder;
            let length = if self.holder_length > 0.0 {
                self.holder_length
            } else {
                self.holder_diameter
            };
            (self.holder_diameter / 2.0, bottom, bottom + length)
        })
    }

    fn height_at_radius(&self, r: f64, shoulder: f64) -> f64 {
        if shoulder <= 0.0 {
            return 0.0;
        }
        // The profile is monotonic in r, so a bisection on radius_at_height
        // inverts it for every shape including a form tool.
        let (mut low, mut high) = (0.0, shoulder);
        for _ in 0..60 {
            let mid = (low + high) / 2.0;
            if self.tool_type.radius_at_height(mid) < r {
                low = mid;
            } else {
                high = mid;
            }
        }
        (low + high) / 2.0
    }
}

/// Turns a drawn vector into a form cutter's half section: the drawing is
/// the right hand side of the cutter, so it is moved until its leftmost
/// point sits on the tool axis and its lowest point on the tip.
///
/// # Errors
/// [`CoreError::Validation`] when the vector is empty or has no width.
pub fn form_profile_from(path: &Path) -> Result<Path, CoreError> {
    let points = form_tool_points(path);
    let (min_x, min_y, max_x) = points.iter().fold(
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY),
        |(lx, ly, hx), p| (lx.min(p.x), ly.min(p.y), hx.max(p.x)),
    );
    if points.is_empty() || max_x - min_x <= 1e-6 || max_x.is_nan() {
        return Err(CoreError::validation(
            "draw the right hand side of the cutter as one vector with some width",
        ));
    }
    let shift = crate::geom::Mat3::translate(crate::geom::Vec2 {
        x: -min_x,
        y: -min_y,
    });
    let profile = crate::geom::transform_path(path, &shift);
    ToolGeometry::FormTool {
        profile: profile.clone(),
    }
    .validate()?;
    Ok(profile)
}

fn half_angle_tan(included_angle_deg: f64) -> f64 {
    let half = (included_angle_deg / 2.0).to_radians();
    let tan = half.tan();
    if tan.is_finite() && tan > 1e-9 {
        tan
    } else {
        1e-9
    }
}

fn taper_contact(tip_radius: f64, taper_angle_deg: f64) -> (f64, f64) {
    let angle = taper_angle_deg.to_radians();
    let contact_r = tip_radius * angle.cos();
    let contact_z = tip_radius - tip_radius * angle.sin();
    (contact_r, contact_z)
}

fn form_tool_points(profile: &Path) -> Vec<Point> {
    let mut points: Vec<Point> = flatten_path(profile, Tolerance::FLATTEN)
        .into_iter()
        .flat_map(|line| line.points.into_iter())
        .filter(|p| p.is_finite())
        .collect();
    points.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    points
}

fn form_tool_radius(profile: &Path) -> f64 {
    form_tool_points(profile)
        .iter()
        .map(|p| p.x.abs())
        .fold(0.0_f64, f64::max)
}

fn form_tool_height(profile: &Path) -> f64 {
    let points = form_tool_points(profile);
    match (points.first(), points.last()) {
        (Some(first), Some(last)) => (last.y - first.y).max(0.0),
        _ => 0.0,
    }
}

fn form_tool_radius_at(profile: &Path, z: f64) -> f64 {
    let points = form_tool_points(profile);
    let Some(first) = points.first() else {
        return 0.0;
    };
    let base = first.y;
    let target = base + z;
    let mut previous = *first;
    for point in &points {
        if point.y >= target {
            let span = point.y - previous.y;
            let t = if span > 1e-12 {
                (target - previous.y) / span
            } else {
                0.0
            };
            return (previous.x.abs() + (point.x.abs() - previous.x.abs()) * t).max(0.0);
        }
        previous = *point;
    }
    previous.x.abs()
}

fn positive(field: &str, value: f64) -> Result<(), CoreError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(CoreError::validation(format!(
            "{field} must be greater than zero"
        )))
    }
}

fn not_negative(field: &str, value: f64) -> Result<(), CoreError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(CoreError::validation(format!(
            "{field} must not be negative"
        )))
    }
}

fn at_most(field: &str, value: f64, limit: f64) -> Result<(), CoreError> {
    if value <= limit {
        Ok(())
    } else {
        Err(CoreError::validation(format!(
            "{field} must not be greater than {limit}"
        )))
    }
}

fn angle(field: &str, value: f64) -> Result<(), CoreError> {
    if value.is_finite() && value > 0.0 && value < 180.0 {
        Ok(())
    } else {
        Err(CoreError::validation(format!(
            "{field} must be between 0 and 180 degrees"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Segment, SubPath};

    fn tool(geometry: ToolGeometry) -> Tool {
        Tool {
            name: "test".to_string(),
            tool_type: geometry,
            ..Tool::default()
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn end_mill_profile_is_flat() {
        let tool = tool(ToolGeometry::EndMill { diameter: 6.0 });
        let profile = tool.profile(5).expect("profile");
        assert_eq!(profile.len(), 5);
        assert!(profile.iter().all(|(_, z)| close(*z, 0.0)), "{profile:?}");
        assert!(close(profile[4].0, 3.0));
        assert!(close(tool.effective_radius_at_depth(-2.0), 3.0));
    }

    #[test]
    fn ball_nose_profile_follows_the_sphere() {
        let tool = tool(ToolGeometry::BallNose { diameter: 6.0 });
        assert!(close(tool.tool_type.shoulder_height(), 3.0));
        assert!(close(tool.tool_type.radius_at_height(3.0), 3.0));
        assert!(close(tool.tool_type.radius_at_height(0.0), 0.0));
        // At half the tip radius the width is sqrt(r^2 - (r - z)^2).
        assert!(close(
            tool.tool_type.radius_at_height(1.5),
            (3.0f64 * 3.0 - 1.5 * 1.5).sqrt()
        ));
        let profile = tool.profile(4).expect("profile");
        assert!(close(profile[0].1, 0.0));
        assert!(close(profile[3].1, 3.0));
    }

    #[test]
    fn v_bit_profile_is_a_cone() {
        let tool = tool(ToolGeometry::VBit {
            diameter: 12.0,
            included_angle: 90.0,
        });
        // A 90 degree V cuts as wide as it is deep.
        assert!(close(tool.tool_type.radius_at_height(2.0), 2.0));
        assert!(close(tool.effective_radius_at_depth(-2.0), 2.0));
        assert!(close(tool.tool_type.shoulder_height(), 6.0));
        // Past full depth the tool cannot widen further.
        assert!(close(tool.effective_radius_at_depth(-20.0), 6.0));
        let profile = tool.profile(3).expect("profile");
        assert!(close(profile[1].1, 3.0), "{profile:?}");
    }

    #[test]
    fn engraving_tool_has_a_flat_tip() {
        let tool = tool(ToolGeometry::EngravingTool {
            diameter: 6.0,
            included_angle: 90.0,
            flat_diameter: 0.5,
        });
        assert!(close(tool.tool_type.tip_radius(), 0.25));
        assert!(close(tool.tool_type.radius_at_height(1.0), 1.25));
        let profile = tool.profile(3).expect("profile");
        assert!(close(profile[0].1, 0.0));
    }

    #[test]
    fn tapered_ball_nose_joins_ball_to_taper() {
        let tool = tool(ToolGeometry::TaperedBallNose {
            diameter: 6.0,
            tip_radius: 0.75,
            taper_angle: 5.0,
        });
        let (contact_r, contact_z) = taper_contact(0.75, 5.0);
        assert!(close(tool.tool_type.radius_at_height(contact_z), contact_r));
        let above = tool.tool_type.radius_at_height(contact_z + 1.0);
        assert!(close(above, contact_r + 5.0f64.to_radians().tan()));
        assert!(tool.tool_type.shoulder_height() > 20.0);
    }

    #[test]
    fn radiused_end_mill_rounds_its_corner() {
        let tool = tool(ToolGeometry::RadiusedEndMill {
            diameter: 6.0,
            corner_radius: 1.0,
        });
        assert!(close(tool.tool_type.tip_radius(), 2.0));
        assert!(close(tool.tool_type.radius_at_height(1.0), 3.0));
        assert!(close(tool.tool_type.radius_at_height(2.0), 3.0));
        let mid = tool.tool_type.radius_at_height(1.0 - 3.0f64.sqrt() / 2.0);
        assert!(close(mid, 2.5), "{mid}");
    }

    #[test]
    fn diamond_drag_behaves_like_a_cone() {
        let tool = tool(ToolGeometry::DiamondDrag {
            diameter: 2.0,
            angle: 120.0,
        });
        let expected = 0.5 * (60.0f64.to_radians().tan());
        assert!(close(tool.tool_type.radius_at_height(0.5), expected));
    }

    #[test]
    fn a_drill_has_a_point_and_the_laser_is_flat() {
        let drill = tool(ToolGeometry::Drill {
            diameter: 3.0,
            point_angle: DRILL_POINT_ANGLE,
        });
        assert!(close(drill.effective_radius_at_depth(-5.0), 1.5));
        // A 118 degree point on a 3 mm drill is 0.901 mm long: 0.5 mm into
        // the hole it cuts a radius of 0.5 * tan(59).
        let point = 1.5 / 59.0f64.to_radians().tan();
        assert!(close(drill.tool_type.shoulder_height(), point));
        assert!(close(drill.tool_type.tip_radius(), 0.0));
        assert!(close(
            drill.effective_radius_at_depth(-0.5),
            0.5 * 59.0f64.to_radians().tan()
        ));
        // A tool database entry without an angle gets the usual 118.
        let read: ToolGeometry =
            serde_json::from_str(r#"{"kind":"drill","diameter":3.0}"#).expect("reads");
        assert_eq!(read, drill.tool_type);
        assert!(ToolGeometry::Drill {
            diameter: 3.0,
            point_angle: 180.0
        }
        .validate()
        .is_err());
        let laser = tool(ToolGeometry::LaserTool {
            power: 80.0,
            speed: 1200.0,
            kerf: 0.2,
            max_burn_rate: DEFAULT_MAX_BURN_RATE,
        });
        assert!(close(laser.diameter(), 0.2));
        assert!(close(laser.effective_radius_at_depth(0.0), 0.1));
    }

    #[test]
    fn form_tool_reads_its_half_section() {
        // A half section that steps out to 2 mm over 1 mm of height.
        let profile = Path::from_subpaths(vec![SubPath::new(
            Point::new(0.0, 0.0),
            vec![
                Segment::line(Point::new(1.0, 0.5)),
                Segment::line(Point::new(2.0, 1.0)),
            ],
            false,
        )]);
        let tool = tool(ToolGeometry::FormTool { profile });
        assert!(close(tool.diameter(), 4.0));
        assert!(close(tool.tool_type.shoulder_height(), 1.0));
        assert!(close(tool.tool_type.radius_at_height(0.5), 1.0));
        assert!(close(tool.tool_type.radius_at_height(0.25), 0.5));
        let sampled = tool.profile(5).expect("profile");
        assert!(close(sampled[4].0, 2.0));
        assert!(close(sampled[4].1, 1.0), "{sampled:?}");
    }

    #[test]
    fn stepover_percent_and_absolute_stay_in_step() {
        let mut tool = tool(ToolGeometry::EndMill { diameter: 10.0 });
        tool.set_stepover_percent(45.0);
        assert!(close(tool.stepover, 4.5));
        tool.set_stepover(2.5);
        assert!(close(tool.stepover_percent, 25.0));
        tool.tool_type = ToolGeometry::EndMill { diameter: 20.0 };
        tool.sync_stepover_from_percent();
        assert!(close(tool.stepover, 5.0));
    }

    #[test]
    fn feed_units_convert_to_mm_per_minute() {
        let mut tool = tool(ToolGeometry::EndMill { diameter: 6.0 });
        tool.feed_units = FeedUnits::InchPerMin;
        tool.feed_rate = 100.0;
        tool.plunge_rate = 30.0;
        assert!(close(tool.feed_mm_per_min(), 2540.0));
        assert!(close(tool.plunge_mm_per_min(), 762.0));
        tool.feed_units = FeedUnits::MmPerSec;
        tool.feed_rate = 40.0;
        assert!(close(tool.feed_mm_per_min(), 2400.0));
        assert!(close(FeedUnits::MetrePerMin.from_mm_per_min(3000.0), 3.0));
    }

    #[test]
    fn cutting_data_is_scoped_by_machine_and_material() {
        let mut tool = tool(ToolGeometry::EndMill { diameter: 6.0 });
        tool.set_cutting_data(CuttingData {
            machine: "Shark".to_string(),
            material: "Oak".to_string(),
            feed_units: FeedUnits::MmPerMin,
            feed_rate: 1800.0,
            plunge_rate: 400.0,
            spindle_speed: 16000.0,
            pass_depth: 2.0,
            stepover: 1.8,
            number: None,
        });
        tool.set_cutting_data(CuttingData {
            machine: String::new(),
            material: "Pine".to_string(),
            feed_units: FeedUnits::MmPerMin,
            feed_rate: 3000.0,
            plunge_rate: 800.0,
            spindle_speed: 18000.0,
            pass_depth: 4.0,
            stepover: 3.0,
            number: None,
        });
        assert!(close(
            tool.cutting_data_for("Shark", "Oak").feed_rate,
            1800.0
        ));
        assert!(close(
            tool.cutting_data_for("Other", "Pine").feed_rate,
            3000.0
        ));
        // No match at all falls back to the tool's own values.
        assert!(close(
            tool.cutting_data_for("Other", "Ash").feed_rate,
            tool.feed_rate
        ));
        let applied = tool.with_cutting_data("Shark", "Oak");
        assert!(close(applied.feed_mm_per_min(), 1800.0));
        assert!(close(applied.stepover_percent, 30.0));
        assert!(tool.remove_cutting_data("Shark", "Oak"));
        assert!(!tool.remove_cutting_data("Shark", "Oak"));
    }

    #[test]
    fn cutting_data_stays_sorted_for_stable_files() {
        let mut tool = tool(ToolGeometry::EndMill { diameter: 6.0 });
        for (machine, material) in [("B", "Oak"), ("A", "Pine"), ("A", "Ash")] {
            tool.set_cutting_data(CuttingData {
                machine: machine.to_string(),
                material: material.to_string(),
                feed_units: FeedUnits::MmPerMin,
                feed_rate: 1000.0,
                plunge_rate: 300.0,
                spindle_speed: 12000.0,
                pass_depth: 1.0,
                stepover: 1.0,
                number: None,
            });
        }
        let order: Vec<String> = tool
            .cutting_data
            .iter()
            .map(|d| format!("{}/{}", d.machine, d.material))
            .collect();
        assert_eq!(order, vec!["A/Ash", "A/Pine", "B/Oak"]);
    }

    #[test]
    fn validation_rejects_impossible_tools() {
        let mut tool = tool(ToolGeometry::EndMill { diameter: 0.0 });
        assert!(tool.validate().is_err());
        tool.tool_type = ToolGeometry::EndMill { diameter: 6.0 };
        assert!(tool.validate().is_ok());
        tool.name = "  ".to_string();
        assert!(tool.validate().is_err());
        tool.name = "cutter".to_string();
        tool.feed_rate = 0.0;
        assert!(tool.validate().is_err());
        tool.feed_rate = 1000.0;
        tool.tool_type = ToolGeometry::VBit {
            diameter: 12.0,
            included_angle: 200.0,
        };
        assert!(tool.validate().is_err());
        tool.tool_type = ToolGeometry::RadiusedEndMill {
            diameter: 6.0,
            corner_radius: 5.0,
        };
        assert!(tool.validate().is_err());
    }

    #[test]
    fn profile_needs_at_least_two_samples() {
        let tool = tool(ToolGeometry::EndMill { diameter: 6.0 });
        assert!(tool.profile(1).is_err());
        assert!(tool.profile(2).is_ok());
    }

    #[test]
    fn tool_json_uses_camel_case_and_round_trips() {
        let mut tool = tool(ToolGeometry::VBit {
            diameter: 12.7,
            included_angle: 60.0,
        });
        tool.name = "V 60".to_string();
        let json = serde_json::to_string(&tool).expect("tool serialises");
        assert!(json.contains("\"toolType\":{\"kind\":\"vBit\""), "{json}");
        assert!(json.contains("\"includedAngle\":60.0"), "{json}");
        assert!(json.contains("\"stepoverPercent\""), "{json}");
        let back: Tool = serde_json::from_str(&json).expect("tool round trips");
        assert_eq!(back, tool);
    }

    #[test]
    fn a_drawn_vector_becomes_a_form_profile_on_the_axis() {
        let drawn = Path::from_subpaths(vec![SubPath::new(
            Point::new(50.0, 20.0),
            vec![
                Segment::line(Point::new(52.0, 21.0)),
                Segment::line(Point::new(53.0, 25.0)),
            ],
            false,
        )]);
        let profile = form_profile_from(&drawn).expect("profile");
        let tool = tool(ToolGeometry::FormTool { profile });
        assert!(close(tool.diameter(), 6.0));
        assert!(close(tool.tool_type.shoulder_height(), 5.0));
        let flat = Path::from_subpaths(vec![SubPath::new(
            Point::new(1.0, 0.0),
            vec![Segment::line(Point::new(1.0, 5.0))],
            false,
        )]);
        assert!(form_profile_from(&flat).is_err());
        assert!(form_profile_from(&Path::new()).is_err());
    }

    #[test]
    fn chip_load_shares_the_feed_over_the_flutes() {
        let mut tool = tool(ToolGeometry::EndMill { diameter: 6.0 });
        tool.feed_rate = 2400.0;
        tool.spindle_speed = 12000.0;
        tool.flutes = 2;
        assert!(close(tool.chip_load().expect("chip load"), 0.1));
        tool.flutes = 0;
        assert!(tool.chip_load().is_none());
    }

    #[test]
    fn a_tool_without_the_new_fields_still_reads() {
        let json = r#"{"id":1,"name":"old","toolType":{"kind":"endMill","diameter":6.0},
            "notes":"","units":"mm","number":1,"feedUnits":"mmPerMin","feedRate":1000.0,
            "plungeRate":300.0,"spindleSpeed":12000.0,"passDepth":1.0,"stepover":2.0,
            "stepoverPercent":33.0,"clearance":0.0,"cuttingData":[]}"#;
        let tool: Tool = serde_json::from_str(json).expect("an old tool reads");
        assert_eq!(tool.flutes, 0);
        assert!(tool.variables.is_empty());
    }

    #[test]
    fn type_names_cover_every_shape() {
        assert_eq!(
            ToolGeometry::EndMill { diameter: 6.0 }.type_name(),
            "endMill"
        );
        assert_eq!(
            ToolGeometry::ThreadMill {
                diameter: 6.0,
                pitch: 1.0,
                included_angle: 60.0,
                tooth_offset: 0.0,
                teeth: 1,
            }
            .type_name(),
            "threadMill"
        );
    }
}
