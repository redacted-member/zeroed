//! Import Toolpaths: reads a G-code file back into toolpaths, one per tool
//! change, so moves saved by any post (ours or another program's) can be
//! placed in a job, previewed and saved again.
//!
//! Understood: G0, G1, G2 and G3 in the XY plane (centres by I and J, or
//! radius R), G4 dwells, G20 and G21 units, G90 and G91 distance modes, F
//! feed, S spindle, T with M6 tool changes. Line numbers and comments are
//! skipped, except that a header comment `(T1 = name)` names tool 1 and the
//! first plain comment after a tool change names that toolpath. Anything
//! else is ignored and listed once in the warnings.
//!
//! The tool's shape comes from its name when the name carries a diameter
//! ("6 mm", "0.25 in", "1/4 in") and a kind (V-bit, ball nose); otherwise
//! a 6 mm end mill stands in. It only matters for the preview.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use crate::error::CoreError;
use crate::geom::Point;
use crate::warning::Warning;

use super::estimate::TimeEstimate;
use super::tool::{Tool, ToolGeometry};
use super::toolpath::{Move, Point3, StrategyKind, Toolpath};

/// Millimetres in an inch.
const INCH: f64 = 25.4;
/// Most moves a file may make, a guard against a runaway file.
const MAX_MOVES: usize = 20_000_000;

/// What reading a G-code file gave.
#[derive(Debug, Clone)]
pub struct ImportedToolpaths {
    /// One toolpath per tool, in file order.
    pub toolpaths: Vec<Toolpath>,
    /// Words and codes that were skipped, once each.
    pub warnings: Vec<Warning>,
}

/// Something the reader left out, listed once.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Skipped {
    /// G18 and G19 arcs.
    ArcPlanes,
    /// A, B and C words.
    RotaryAxes,
    /// Any other word or code, as written.
    Word(String),
}

impl Skipped {
    fn warning(&self) -> Warning {
        match self {
            Skipped::ArcPlanes => Warning::new(
                "toolpathImport.skippedArcPlanes",
                "skipped arcs outside the XY plane (G18, G19)",
            ),
            Skipped::RotaryAxes => Warning::new(
                "toolpathImport.skippedRotary",
                "skipped rotary axes (A, B, C)",
            ),
            Skipped::Word(word) => Warning::gcode_skipped(word),
        }
    }
}

/// One word of a block: a letter and its number.
struct Word {
    letter: char,
    value: f64,
}

/// A block's words, with comments taken out; the comments come back
/// separately.
fn split_block(line: &str) -> (Vec<Word>, Vec<String>) {
    let mut words = Vec::new();
    let mut comments = Vec::new();
    let mut code = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '(' => {
                let mut text = String::new();
                for d in chars.by_ref() {
                    if d == ')' {
                        break;
                    }
                    text.push(d);
                }
                comments.push(text.trim().to_string());
            }
            ';' => {
                let rest: String = chars.by_ref().collect();
                comments.push(rest.trim().to_string());
            }
            _ => code.push(c),
        }
    }
    let bytes: Vec<char> = code.chars().filter(|c| !c.is_whitespace()).collect();
    let mut i = 0;
    while i < bytes.len() {
        let letter = bytes[i].to_ascii_uppercase();
        i += 1;
        if !letter.is_ascii_alphabetic() {
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || matches!(bytes[i], '.' | '-' | '+'))
        {
            i += 1;
        }
        let text: String = bytes[start..i].iter().collect();
        if let Ok(value) = text.parse::<f64>() {
            words.push(Word { letter, value });
        }
    }
    (words, comments)
}

/// The tool a name describes, as far as the name tells.
pub fn tool_from_name(name: &str, number: u32) -> Tool {
    let lower = name.to_ascii_lowercase();
    let diameter = diameter_in(&lower).unwrap_or(6.0);
    let tool_type = if lower.contains("v-bit") || lower.contains("v bit") || lower.contains("vbit")
    {
        let angle = angle_in(&lower).unwrap_or(90.0);
        ToolGeometry::VBit {
            diameter,
            included_angle: angle,
        }
    } else if lower.contains("ball") {
        ToolGeometry::BallNose { diameter }
    } else {
        ToolGeometry::EndMill { diameter }
    };
    Tool {
        name: if name.trim().is_empty() {
            format!("Tool {number}")
        } else {
            name.trim().to_string()
        },
        tool_type,
        number,
        ..Tool::default()
    }
}

/// A diameter written in a name: "6 mm", "6mm", "0.25 in", "1/4\"".
fn diameter_in(name: &str) -> Option<f64> {
    let tokens: Vec<&str> = name
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',')
        .filter(|t| !t.is_empty())
        .collect();
    for (i, token) in tokens.iter().enumerate() {
        let next = tokens.get(i + 1).copied().unwrap_or("");
        for (suffix, scale) in [("mm", 1.0), ("in", INCH), ("inch", INCH), ("\"", INCH)] {
            let (number, unit) = match token.strip_suffix(suffix) {
                Some(n) if !n.is_empty() => (n, true),
                _ => (*token, next == suffix),
            };
            if !unit {
                continue;
            }
            if let Some(value) = number_or_fraction(number) {
                if value > 0.0 {
                    return Some(value * scale);
                }
            }
        }
    }
    None
}

/// An included angle written in a name: "90 deg", "60°".
fn angle_in(name: &str) -> Option<f64> {
    let tokens: Vec<&str> = name.split_whitespace().collect();
    for (i, token) in tokens.iter().enumerate() {
        let next = tokens.get(i + 1).copied().unwrap_or("");
        let trimmed = token.trim_end_matches('\u{b0}').trim_end_matches("deg");
        let marked = trimmed.len() < token.len() || next.starts_with("deg");
        if marked {
            if let Ok(value) = trimmed.parse::<f64>() {
                if value > 0.0 && value < 180.0 {
                    return Some(value);
                }
            }
        }
    }
    None
}

fn number_or_fraction(text: &str) -> Option<f64> {
    if let Some((a, b)) = text.split_once('/') {
        let (a, b) = (a.parse::<f64>().ok()?, b.parse::<f64>().ok()?);
        return (b != 0.0).then(|| a / b);
    }
    text.parse().ok()
}

/// A toolpath being read.
struct Part {
    number: u32,
    name: Option<String>,
    moves: Vec<Move>,
    feed: Option<f64>,
    plunge: Option<f64>,
    spindle: Option<f64>,
}

impl Part {
    fn new(number: u32) -> Part {
        Part {
            number,
            name: None,
            moves: Vec::new(),
            feed: None,
            plunge: None,
            spindle: None,
        }
    }
}

/// Reads a G-code program. `file_name` names toolpaths that no comment names.
///
/// # Errors
/// [`CoreError::Validation`] for a program with no moves, a number that is
/// not finite, or more than [`MAX_MOVES`] moves.
pub fn read(text: &str, file_name: &str) -> Result<ImportedToolpaths, CoreError> {
    let mut tool_names: BTreeMap<u32, String> = BTreeMap::new();
    let mut skipped: BTreeSet<Skipped> = BTreeSet::new();
    let mut parts: Vec<Part> = vec![Part::new(1)];
    let mut at = Point3::new(0.0, 0.0, 0.0);
    let mut seen_position = false;
    let mut scale = 1.0;
    let mut absolute = true;
    let mut motion = 0_u8;
    let mut feed = 0.0;
    let mut pending_tool: Option<u32> = None;
    let mut awaiting_name = false;
    let mut count = 0_usize;

    for line in text.lines() {
        let (words, comments) = split_block(line);
        for comment in &comments {
            if let Some((left, right)) = comment.split_once('=') {
                let left = left.trim();
                if let Some(n) = left
                    .strip_prefix('T')
                    .and_then(|n| n.trim().parse::<u32>().ok())
                {
                    tool_names.insert(n, right.trim().to_string());
                    continue;
                }
            }
            let part = parts
                .last_mut()
                .ok_or_else(|| CoreError::validation("no toolpath"))?;
            if awaiting_name && !comment.is_empty() && part.name.is_none() {
                part.name = Some(comment.clone());
                awaiting_name = false;
            }
        }
        if words.is_empty() {
            continue;
        }
        let mut target = (None, None, None);
        let (mut i_off, mut j_off, mut radius) = (None, None, None);
        let mut dwell = None;
        let mut change = false;
        let mut moved = false;
        for word in &words {
            if !word.value.is_finite() {
                return Err(CoreError::validation(
                    "the file holds a number that is not finite",
                ));
            }
            match word.letter {
                'G' => {
                    let code = (word.value * 10.0).round() as i64;
                    match code {
                        0 => motion = 0,
                        10 => motion = 1,
                        20 => motion = 2,
                        30 => motion = 3,
                        40 => dwell = Some(0.0),
                        200 => scale = INCH,
                        210 => scale = 1.0,
                        900 => absolute = true,
                        910 => absolute = false,
                        170 | 400 | 490 | 540 | 800 | 610 | 640 | 940 => {}
                        180 | 190 => {
                            skipped.insert(Skipped::ArcPlanes);
                        }
                        _ => {
                            skipped.insert(Skipped::Word(format!("G{}", word.value)));
                        }
                    }
                }
                'X' => {
                    target.0 = Some(word.value);
                    moved = true;
                }
                'Y' => {
                    target.1 = Some(word.value);
                    moved = true;
                }
                'Z' => {
                    target.2 = Some(word.value);
                    moved = true;
                }
                'I' => i_off = Some(word.value),
                'J' => j_off = Some(word.value),
                'R' => radius = Some(word.value),
                'F' => feed = word.value * scale,
                'S' => {
                    if let Some(part) = parts.last_mut() {
                        part.spindle = Some(word.value);
                    }
                }
                'P' => {
                    if dwell.is_some() {
                        dwell = Some(word.value);
                    }
                }
                'T' => pending_tool = Some(word.value.round().max(0.0) as u32),
                'M' => {
                    if (word.value - 6.0).abs() < 1e-9 {
                        change = true;
                    }
                }
                'N' | 'O' => {}
                'A' | 'B' | 'C' => {
                    skipped.insert(Skipped::RotaryAxes);
                }
                other => {
                    skipped.insert(Skipped::Word(other.to_string()));
                }
            }
        }
        if change || (pending_tool.is_some() && !moved) {
            if let Some(number) = pending_tool.take() {
                let current = parts.last().map(|p| (p.number, p.moves.is_empty()));
                match current {
                    Some((_, true)) => {
                        if let Some(part) = parts.last_mut() {
                            part.number = number;
                        }
                    }
                    Some((n, false)) if n == number => {}
                    _ => parts.push(Part::new(number)),
                }
                awaiting_name = true;
            }
        }
        if let Some(seconds) = dwell {
            if let Some(part) = parts.last_mut() {
                part.moves.push(Move::dwell(seconds.max(0.0), at));
            }
            continue;
        }
        if !moved {
            continue;
        }
        let coordinate = |value: Option<f64>, from: f64| match value {
            Some(v) if absolute => v * scale,
            Some(v) => from + v * scale,
            None => from,
        };
        let to = Point3::new(
            coordinate(target.0, at.x),
            coordinate(target.1, at.y),
            coordinate(target.2, at.z),
        );
        if !seen_position {
            // Where the machine starts is not known: the first move goes
            // there as a rapid, from straight above.
            seen_position = true;
            at = Point3::new(to.x, to.y, to.z);
        }
        let part = parts
            .last_mut()
            .ok_or_else(|| CoreError::validation("no toolpath"))?;
        let one = match motion {
            0 => Move::rapid(to),
            1 => {
                let plunging =
                    (to.x - at.x).abs() < 1e-9 && (to.y - at.y).abs() < 1e-9 && to.z < at.z;
                if plunging {
                    part.plunge.get_or_insert(feed);
                    Move::plunge(to)
                } else {
                    part.feed.get_or_insert(feed);
                    Move::feed(to)
                }
            }
            _ => {
                let centre = match (i_off, j_off, radius) {
                    (None, None, Some(r)) => arc_centre(at, to, r * scale, motion == 2),
                    _ => Some(Point::new(
                        at.x + i_off.unwrap_or(0.0) * scale,
                        at.y + j_off.unwrap_or(0.0) * scale,
                    )),
                };
                part.feed.get_or_insert(feed);
                match centre {
                    Some(c) if motion == 2 => Move::arc_cw(c, to),
                    Some(c) => Move::arc_ccw(c, to),
                    None => Move::feed(to),
                }
            }
        };
        part.moves.push(one);
        at = to;
        count += 1;
        if count > MAX_MOVES {
            return Err(CoreError::validation(
                "the file has too many moves to import",
            ));
        }
    }

    let base = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name)
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem)
        .to_string();
    let many = parts.iter().filter(|p| !p.moves.is_empty()).count() > 1;
    let mut toolpaths = Vec::new();
    for part in parts.into_iter().filter(|p| !p.moves.is_empty()) {
        let tool_name = tool_names.get(&part.number).cloned().unwrap_or_default();
        let mut tool = tool_from_name(&tool_name, part.number);
        let name = part.name.clone().unwrap_or_else(|| {
            if many {
                format!("{base} T{}", part.number)
            } else {
                base.clone()
            }
        });
        let mut toolpath = Toolpath::new(0, name, tool.clone(), StrategyKind::Imported);
        if let Some(feed) = part.feed.filter(|f| *f > 0.0) {
            toolpath.feed = feed;
            tool.feed_rate = feed;
        }
        if let Some(plunge) = part.plunge.filter(|f| *f > 0.0) {
            toolpath.plunge = plunge;
        }
        if let Some(spindle) = part.spindle.filter(|s| *s > 0.0) {
            toolpath.spindle = spindle;
        }
        toolpath.tool = tool;
        let top = part
            .moves
            .iter()
            .map(|m| m.to.z)
            .fold(f64::NEG_INFINITY, f64::max);
        toolpath.safe_z = top;
        let first = part.moves[0].to;
        toolpath.home = Point3::new(first.x, first.y, top);
        toolpath.moves = part.moves;
        toolpath.params_json = json!({
            "imported": { "file": file_name, "offsetX": 0.0, "offsetY": 0.0, "startDepth": 0.0 }
        });
        toolpath.recompute_bounds();
        TimeEstimate::default().apply(&mut toolpath);
        toolpaths.push(toolpath);
    }
    if toolpaths.is_empty() {
        return Err(CoreError::validation("the file holds no moves to import"));
    }
    Ok(ImportedToolpaths {
        toolpaths,
        warnings: skipped.iter().map(Skipped::warning).collect(),
    })
}

/// The centre of an arc from `from` to `to` with radius `r` (negative for
/// the long way round), or None when the ends are too far apart.
fn arc_centre(from: Point3, to: Point3, r: f64, clockwise: bool) -> Option<Point> {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let chord = (dx * dx + dy * dy).sqrt();
    let radius = r.abs();
    if chord < 1e-12 || chord > 2.0 * radius + 1e-9 {
        return None;
    }
    let h = (radius * radius - chord * chord / 4.0).max(0.0).sqrt();
    let (mx, my) = ((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
    let (nx, ny) = (-dy / chord, dx / chord);
    // Left of the chord for a short counter clockwise arc.
    let left = (r > 0.0) != clockwise;
    let sign = if left { 1.0 } else { -1.0 };
    Some(Point::new(mx + sign * nx * h, my + sign * ny * h))
}

/// Moves `toolpath` by `dx`, `dy` and deepens every cutting move and the
/// moves between them by `dz` millimetres (rapids above the start keep
/// their height), keeping the import's recorded offsets in step.
pub fn reposition(toolpath: &mut Toolpath, dx: f64, dy: f64, dz: f64) {
    let safe = toolpath.safe_z;
    for m in &mut toolpath.moves {
        m.to.x += dx;
        m.to.y += dy;
        let travel = m.kind == super::toolpath::MoveKind::Rapid && m.to.z >= safe - 1e-9;
        if !travel {
            m.to.z -= dz;
        }
        match &mut m.kind {
            super::toolpath::MoveKind::ArcCw { centre }
            | super::toolpath::MoveKind::ArcCcw { centre } => {
                centre.x += dx;
                centre.y += dy;
            }
            _ => {}
        }
    }
    toolpath.home.x += dx;
    toolpath.home.y += dy;
    if let Some(imported) = toolpath.params_json.get_mut("imported") {
        let add = |value: &serde_json::Value, by: f64| json!(value.as_f64().unwrap_or(0.0) + by);
        imported["offsetX"] = add(&imported["offsetX"], dx);
        imported["offsetY"] = add(&imported["offsetY"], dy);
        imported["startDepth"] = add(&imported["startDepth"], dz);
    }
    toolpath.recompute_bounds();
    TimeEstimate::default().apply(toolpath);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::toolpath::MoveKind;

    const PROGRAM: &str = "(sample.gcode)\n(T1 = End Mill 6 mm)\n(T2 = V-Bit 90 deg 1/2 in)\n\
        N10G90\nN20G21\nN30T1M6\nN40S18000M3\n(Profile 1)\nN50G0X10Y10Z5\nN60G1Z-3F600\n\
        N70G1X50F2400\nN80G3X60Y20I0J10\nN90G2X50Y30R10\nN100G0Z5\nN110T2M6\n(Carve)\n\
        N120G0X0Y0\nN130G1Z-1F300\nN140G4P0.5\nN150G1X5A10\nM30\n";

    #[test]
    fn a_program_splits_into_a_toolpath_per_tool() {
        let read = read(PROGRAM, "/jobs/sample.gcode").expect("reads");
        assert_eq!(read.toolpaths.len(), 2);
        let first = &read.toolpaths[0];
        assert_eq!(first.name, "Profile 1");
        assert_eq!(first.kind, StrategyKind::Imported);
        assert_eq!(first.tool.name, "End Mill 6 mm");
        assert!((first.tool.diameter() - 6.0).abs() < 1e-9);
        assert_eq!(first.spindle, 18000.0);
        assert_eq!(first.feed, 2400.0);
        assert_eq!(first.plunge, 600.0);
        assert_eq!(first.moves[1].kind, MoveKind::Plunge);
        assert_eq!(
            first.moves[3].kind,
            MoveKind::ArcCcw {
                centre: Point::new(50.0, 20.0)
            }
        );
        // R10 from (60, 20) to (50, 30) clockwise: the centre is (60, 30).
        match first.moves[4].kind {
            MoveKind::ArcCw { centre } => {
                assert!((centre.x - 60.0).abs() < 1e-9 && (centre.y - 30.0).abs() < 1e-9);
            }
            other => panic!("expected a clockwise arc, got {other:?}"),
        }
        assert_eq!(first.safe_z, 5.0);
        let second = &read.toolpaths[1];
        assert_eq!(second.name, "Carve");
        assert_eq!(second.tool_number, 2);
        match &second.tool.tool_type {
            ToolGeometry::VBit {
                diameter,
                included_angle,
            } => {
                assert!((diameter - 12.7).abs() < 1e-9);
                assert_eq!(*included_angle, 90.0);
            }
            other => panic!("expected a V-bit, got {other:?}"),
        }
        assert!(second
            .moves
            .iter()
            .any(|m| matches!(m.kind, MoveKind::Dwell { .. })));
        assert_eq!(read.warnings.len(), 1);
        assert!(read.warnings[0].is("toolpathImport.skippedRotary"));
        assert_eq!(read.warnings[0].message, "skipped rotary axes (A, B, C)");
    }

    #[test]
    fn inches_and_relative_moves_are_read_in_millimetres() {
        let read = read("G20 G91\nG0 X1 Y1 Z0.5\nG1 X1 F10\n", "a.nc").expect("reads");
        let t = &read.toolpaths[0];
        assert_eq!(t.name, "a");
        assert!((t.moves[1].to.x - 50.8).abs() < 1e-9);
        assert!((t.feed - 254.0).abs() < 1e-9);
        assert!(read.warnings.is_empty());
    }

    #[test]
    fn an_empty_program_is_refused() {
        assert!(read("(nothing)\nM30\n", "x.nc").is_err());
    }

    #[test]
    fn repositioning_moves_and_deepens_the_cuts() {
        let mut t = read(PROGRAM, "s.nc").expect("reads").toolpaths.remove(0);
        reposition(&mut t, 100.0, -5.0, 2.0);
        assert_eq!(
            t.moves[0].to,
            Point3::new(110.0, 5.0, 5.0),
            "a rapid at safe height stays"
        );
        assert_eq!(t.moves[1].to, Point3::new(110.0, 5.0, -5.0));
        assert_eq!(
            t.moves[3].kind,
            MoveKind::ArcCcw {
                centre: Point::new(150.0, 15.0)
            }
        );
        assert_eq!(t.params_json["imported"]["offsetX"], 100.0);
        assert_eq!(t.params_json["imported"]["startDepth"], 2.0);
        reposition(&mut t, -100.0, 5.0, -2.0);
        assert_eq!(t.moves[1].to, Point3::new(10.0, 10.0, -3.0));
    }

    #[test]
    fn tool_names_give_their_shape() {
        assert!((tool_from_name("Ball Nose 1/8\"", 3).diameter() - 3.175).abs() < 1e-9);
        assert!(matches!(
            tool_from_name("Ball Nose 3mm", 3).tool_type,
            ToolGeometry::BallNose { .. }
        ));
        assert_eq!(tool_from_name("", 4).name, "Tool 4");
        assert!((tool_from_name("mystery", 1).diameter() - 6.0).abs() < 1e-9);
    }
}
