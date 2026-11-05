//! Warnings the kernel hands the user, as a stable code with parameters so
//! that the front end can say them in the user's language, plus an English
//! sentence for any caller that has no text of its own for the code.
//!
//! Every warning is made by one constructor here, which keeps the list of
//! codes in one place: the front end's `lib/kernelWarnings.ts` words each
//! one from `lib/strings/kernelWarnings.ts`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One warning: what happened, as a code, the numbers and names it needs,
/// and an English sentence to fall back on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    /// Stable identity, `area.name`, that the front end looks its text up by.
    pub code: String,
    /// The values the sentence is built from, by name. Lengths are in
    /// millimetres and times in seconds.
    pub params: BTreeMap<String, Value>,
    /// The sentence in English.
    pub message: String,
}

impl Warning {
    /// A warning with no parameters.
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Warning {
            code: code.to_string(),
            params: BTreeMap::new(),
            message: message.into(),
        }
    }

    /// Adds a parameter.
    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.params.insert(name.to_string(), value.into());
        self
    }

    /// True when this warning has the given code.
    pub fn is(&self, code: &str) -> bool {
        self.code == code
    }

    /// The chamfer is wider than one pass of the tool reaches.
    pub fn chamfer_multi_pass(reach: f64) -> Self {
        Warning::new(
            "chamfer.multiPass",
            format!(
                "The chamfer is wider than the tool can cut in one go ({reach:.3} mm), so it is \
                 cut in several passes stepping out."
            ),
        )
        .with("reach", reach)
    }

    /// Some chamfer passes had no room inside their vectors.
    pub fn chamfer_passes_left_out() -> Self {
        Warning::new(
            "chamfer.passesLeftOut",
            "Some chamfer passes did not fit inside their vectors and were left out.",
        )
    }

    /// The prism goes deeper than the tool's cone.
    pub fn prism_deeper_than_cone(cone_depth: f64) -> Self {
        Warning::new(
            "prism.deeperThanCone",
            format!(
                "The prism is deeper than the tool's cone reaches ({cone_depth:.3} mm), so the \
                 straight sides of the tool cut above the cone."
            ),
        )
        .with("coneDepth", cone_depth)
    }

    /// The prism is cut off at the maximum depth.
    pub fn prism_top_flat() -> Self {
        Warning::new(
            "prism.topFlat",
            "The prism is taller than the maximum depth allows, so its top is left flat.",
        )
    }

    /// The thread milling tool has another pitch than the thread.
    pub fn thread_pitch_differs(tool_pitch: f64, pitch: f64) -> Self {
        Warning::new(
            "thread.pitchDiffers",
            format!(
                "The tool's pitch ({tool_pitch:.3} mm) differs from the thread's ({pitch:.3} mm); \
                 only a single point cutter can cut it."
            ),
        )
        .with("toolPitch", tool_pitch)
        .with("pitch", pitch)
    }

    /// The tool did not fit inside some vectors.
    pub fn tool_too_big() -> Self {
        Warning::new(
            "profile.toolTooBig",
            "The tool is too big to fit inside some vectors; they were left out.",
        )
    }

    /// A moulding pass had no room on the rail.
    pub fn moulding_pass_left_out(offset: f64) -> Self {
        Warning::new(
            "moulding.passLeftOut",
            format!("The rail has no room for the pass {offset:.3} mm across; it is left out."),
        )
        .with("offset", offset)
    }

    /// No moulding pass had room on the rail.
    pub fn moulding_no_room() -> Self {
        Warning::new(
            "moulding.noRoom",
            "The rail has no room for any pass across the molding.",
        )
    }

    /// A later tool of a multi tool toolpath found nothing left to cut.
    pub fn nothing_left(tool: &str) -> Self {
        Warning::new(
            "toolpath.nothingLeft",
            format!("{tool} has nothing left to machine after the tools before it."),
        )
        .with("tool", tool)
    }

    /// A later clearance tool is no smaller than the one before it.
    pub fn no_smaller_tool(tool: &str) -> Self {
        Warning::new(
            "vgroove.noSmallerTool",
            format!(
                "{tool} is no smaller than the clearance tool before it, so it has nothing left \
                 to clear."
            ),
        )
        .with("tool", tool)
    }

    /// The toolpath enters a keep-out zone.
    pub fn keep_out(zone: &str) -> Self {
        Warning::new(
            "toolpath.keepOut",
            format!(
                "The tool goes into the keep-out zone on layer {zone}: move the clamps or change \
                 the toolpath before cutting."
            ),
        )
        .with("zone", zone)
    }

    /// Drilling left out open vectors.
    pub fn drill_open_vectors() -> Self {
        Warning::new(
            "drill.openVectors",
            "Open vectors have no center to drill and were left out.",
        )
    }

    /// Circles no drill of the bank fits.
    pub fn drill_bank_missed(count: usize) -> Self {
        Warning::new(
            "drillBank.missed",
            format!(
                "{count} circle{} fit no drill of the bank and {} left undrilled.",
                if count == 1 { "" } else { "s" },
                if count == 1 { "was" } else { "were" }
            ),
        )
    }

    /// No texture stroke fit.
    pub fn texture_no_strokes() -> Self {
        Warning::new("texture.noStrokes", "No strokes fit inside the vectors.")
    }

    /// Fluting ramps were shortened.
    pub fn fluting_ramp_shortened() -> Self {
        Warning::new(
            "fluting.rampShortened",
            "The ramp length is longer than some vectors allow; those ramps were shortened to \
             fit.",
        )
    }

    /// A carving toolpath goes deeper than the material is thick, so it cuts
    /// right through (a V-Groove with no flat depth on a wide shape, a
    /// molding whose profile is taller than the material).
    pub fn cuts_through(depth: f64, thickness: f64) -> Self {
        Warning::new(
            "toolpath.cutsThrough",
            format!(
                "The toolpath cuts {depth:.3} mm deep, right through the {thickness:.3} mm material. \
                 Set a flat depth or a thicker material if that is not meant."
            ),
        )
        .with("depth", depth)
        .with("thickness", thickness)
    }

    /// The pocket stepover is wider than the tool.
    pub fn pocket_stepover_wide() -> Self {
        Warning::new(
            "pocket.stepoverWide",
            "The stepover is wider than the tool, so ridges will be left standing.",
        )
    }

    /// Vectors that crossed themselves were repaired.
    pub fn vectors_repaired(count: usize) -> Self {
        let message = if count == 1 {
            "1 vector crossed itself and was repaired before machining.".to_string()
        } else {
            format!("{count} vectors crossed themselves and were repaired before machining.")
        };
        Warning::new("fix.repaired", message).with("count", count)
    }

    /// Geometry fixing ran out of time.
    pub fn fixing_timed_out(seconds: u64) -> Self {
        Warning::new(
            "fix.timedOut",
            format!(
                "Geometry fixing stopped after {seconds} s (Preferences, Toolpaths): the \
                 remaining vectors are machined as drawn."
            ),
        )
        .with("seconds", seconds)
    }

    /// The inlay plug boundary offset is too small.
    pub fn inlay_boundary_small() -> Self {
        Warning::new(
            "inlay.boundaryTooSmall",
            "The boundary offset is too small for the plug walls to reach the plug depth all \
             round; the plug will be shorter there.",
        )
    }

    /// V-groove flats need a pocket.
    pub fn flats_need_a_pocket() -> Self {
        Warning::new(
            FLATS_NEED_A_POCKET,
            "Some areas are wider than the flat depth allows; their middles are left at the flat \
             depth and need a pocket to clear them.",
        )
    }

    /// Import Toolpaths left out a G-code word it does not read.
    pub fn gcode_skipped(word: &str) -> Self {
        Warning::new("toolpathImport.skipped", format!("skipped {word}")).with("word", word)
    }
}

/// Code of the warning that a flat bottomed V-groove left areas for a pocket.
pub const FLATS_NEED_A_POCKET: &str = "vgroove.flatsNeedPocket";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_warning_carries_its_code_parameters_and_english() {
        let warning = Warning::nothing_left("End Mill 3 mm");
        assert!(warning.is("toolpath.nothingLeft"));
        assert_eq!(warning.params["tool"], "End Mill 3 mm");
        assert!(warning.message.starts_with("End Mill 3 mm has nothing"));
        let json = serde_json::to_value(&warning).expect("serialises");
        assert_eq!(json["code"], "toolpath.nothingLeft");
        assert_eq!(json["params"]["tool"], "End Mill 3 mm");
        assert!(json["message"].is_string());
    }

    #[test]
    fn counts_pick_the_right_english() {
        assert_eq!(
            Warning::vectors_repaired(1).message,
            "1 vector crossed itself and was repaired before machining."
        );
        assert_eq!(
            Warning::vectors_repaired(3).message,
            "3 vectors crossed themselves and were repaired before machining."
        );
        assert_eq!(Warning::vectors_repaired(3).params["count"], 3);
    }

    #[test]
    fn the_front_end_words_every_code() {
        let front = include_str!("../../../app/src/lib/kernelWarnings.ts");
        let mut all = vec![
            Warning::chamfer_multi_pass(1.0),
            Warning::chamfer_passes_left_out(),
            Warning::prism_deeper_than_cone(1.0),
            Warning::prism_top_flat(),
            Warning::thread_pitch_differs(1.0, 2.0),
            Warning::tool_too_big(),
            Warning::moulding_pass_left_out(1.0),
            Warning::moulding_no_room(),
            Warning::nothing_left("t"),
            Warning::no_smaller_tool("t"),
            Warning::keep_out("z"),
            Warning::drill_open_vectors(),
            Warning::texture_no_strokes(),
            Warning::fluting_ramp_shortened(),
            Warning::pocket_stepover_wide(),
            Warning::cuts_through(20.0, 18.0),
            Warning::vectors_repaired(2),
            Warning::fixing_timed_out(5),
            Warning::inlay_boundary_small(),
            Warning::flats_need_a_pocket(),
            Warning::gcode_skipped("G41"),
            Warning::new("toolpathImport.skippedArcPlanes", ""),
            Warning::new("toolpathImport.skippedRotary", ""),
        ];
        use crate::io::ImportWarning as Iw;
        for kind in [
            Iw::TextSkipped,
            Iw::ImageSkipped,
            Iw::UseSkipped,
            Iw::BlockMissing,
            Iw::BlockTooDeep,
            Iw::FontSubstituted,
        ] {
            all.push(kind.to_warning());
        }
        for warning in all {
            assert!(
                front.contains(&format!("case '{}':", warning.code)),
                "lib/kernelWarnings.ts has no sentence for {}",
                warning.code
            );
        }
    }

    #[test]
    fn lengths_are_parameters_in_millimetres() {
        let warning = Warning::thread_pitch_differs(1.5, 2.0);
        assert_eq!(warning.params["toolPitch"], 1.5);
        assert_eq!(warning.params["pitch"], 2.0);
        assert!(warning.message.contains("(1.500 mm)"));
    }
}
