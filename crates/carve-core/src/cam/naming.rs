//! Tool name templates.
//!
//! A tool's name may hold fields in braces, such as
//! `{Tool Type} ({Diameter|F} {Units Short})`, that are filled in from the
//! tool, its cutting data and the active machine and material each time the
//! name is shown. A field can carry a modifier after a bar: `U` or `L` for
//! upper or lower case text, a pattern of zeros such as `00.00` for padding
//! and decimals, or `F` for a fraction. A field the context has no value for
//! stays as written, so the user can see what is missing.

use crate::doc::Units;

use super::tool::{FeedUnits, Tool, ToolGeometry};

/// Every built in variable, in the order the insert menu lists them.
pub const VARIABLES: [&str; 27] = [
    "Tool Type",
    "Units",
    "Units Short",
    "Diameter",
    "Included Angle",
    "Side Angle",
    "Tip Radius",
    "Flat Diameter",
    "Num Flutes",
    "Length Units",
    "Length Units Short",
    "Pass Depth",
    "Stepover",
    "Stepover Percent",
    "Kerf",
    "Rate Units",
    "Move Speed",
    "Plunge Rate",
    "Spindle Speed",
    "Chip Load",
    "Tool Number",
    "Power",
    "Material Name",
    "Machine Name",
    "Tool Notes",
    "Group Name",
    "Tool Id",
];

/// The machine and material a name is shown for. Empty means none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NameContext<'a> {
    /// Active machine.
    pub machine: &'a str,
    /// Active material.
    pub material: &'a str,
    /// Name of the group the tool sits in.
    pub group: &'a str,
}

/// The name format a new tool of this shape starts with, when the user has
/// not set one.
pub fn builtin_format(type_name: &str) -> &'static str {
    match type_name {
        "vBit" => "{Tool Type} {Included Angle} deg ({Diameter} {Units Short})",
        "engravingTool" => "{Tool Type} {Included Angle} deg ({Flat Diameter} {Units Short} tip)",
        "taperedBallNose" => "{Tool Type} ({Tip Radius} {Units Short} tip radius)",
        "radiusedEndMill" => {
            "{Tool Type} ({Diameter} {Units Short}, {Tip Radius} {Units Short} radius)"
        }
        "diamondDrag" => "{Tool Type} {Included Angle} deg",
        "laserTool" => "{Tool Type} ({Kerf} {Units Short} kerf)",
        _ => "{Tool Type} ({Diameter} {Units Short})",
    }
}

/// Readable name of a tool shape, as `{Tool Type}` shows it.
pub fn type_label(geometry: &ToolGeometry) -> &'static str {
    match geometry {
        ToolGeometry::EndMill { .. } => "End Mill",
        ToolGeometry::BallNose { .. } => "Ball Nose",
        ToolGeometry::VBit { .. } => "V-Bit",
        ToolGeometry::EngravingTool { .. } => "Engraving",
        ToolGeometry::TaperedBallNose { .. } => "Tapered Ball Nose",
        ToolGeometry::RadiusedEndMill { .. } => "Radiused End Mill",
        ToolGeometry::FormTool { .. } => "Form Cutter",
        ToolGeometry::Drill { .. } => "Drill",
        ToolGeometry::ThreadMill { .. } => "Thread Mill",
        ToolGeometry::DiamondDrag { .. } => "Diamond Drag",
        ToolGeometry::LaserTool { .. } => "Laser",
    }
}

/// Whether a name holds any template field.
pub fn is_template(name: &str) -> bool {
    name.contains('{') && name.contains('}')
}

/// The tool's name with every field it can fill in replaced. `tool` should
/// already carry the cutting data of the context (see
/// [`Tool::with_cutting_data`]).
pub fn evaluate(template: &str, tool: &Tool, context: NameContext<'_>) -> String {
    if !is_template(template) {
        return template.to_string();
    }
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let field = &after[..close];
        match expand(field, tool, context) {
            Some(text) => out.push_str(&text),
            None => {
                out.push('{');
                out.push_str(field);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

enum Value {
    Text(String),
    Number(f64),
}

fn expand(field: &str, tool: &Tool, context: NameContext<'_>) -> Option<String> {
    let (name, modifier) = match field.split_once('|') {
        Some((name, modifier)) => (name.trim(), Some(modifier.trim())),
        None => (field.trim(), None),
    };
    let value = lookup(name, tool, context)?;
    Some(match (value, modifier) {
        (Value::Text(text), Some("U")) => text.to_uppercase(),
        (Value::Text(text), Some("L")) => text.to_lowercase(),
        (Value::Text(text), _) => text,
        (Value::Number(n), Some("F")) => fraction(n),
        (Value::Number(n), Some(pattern)) if is_number_pattern(pattern) => pad(n, pattern),
        (Value::Number(n), _) => plain(n),
    })
}

fn lookup(name: &str, tool: &Tool, context: NameContext<'_>) -> Option<Value> {
    let inch = tool.units == Units::Inch;
    let length = |mm: f64| Value::Number(if inch { mm / 25.4 } else { mm });
    let text = |s: &str| Value::Text(s.to_string());
    let geometry = &tool.tool_type;
    let value = match name {
        "Tool Type" => text(type_label(geometry)),
        "Units" | "Length Units" => text(if inch { "inch" } else { "mm" }),
        "Units Short" | "Length Units Short" => text(if inch { "in" } else { "mm" }),
        "Diameter" => length(tool.diameter()),
        "Included Angle" => Value::Number(match geometry {
            ToolGeometry::VBit { included_angle, .. }
            | ToolGeometry::EngravingTool { included_angle, .. }
            | ToolGeometry::ThreadMill { included_angle, .. } => *included_angle,
            ToolGeometry::DiamondDrag { angle, .. } => *angle,
            ToolGeometry::TaperedBallNose { taper_angle, .. } => taper_angle * 2.0,
            _ => return None,
        }),
        "Side Angle" => Value::Number(match geometry {
            ToolGeometry::VBit { included_angle, .. }
            | ToolGeometry::EngravingTool { included_angle, .. } => included_angle / 2.0,
            ToolGeometry::DiamondDrag { angle, .. } => angle / 2.0,
            ToolGeometry::TaperedBallNose { taper_angle, .. } => *taper_angle,
            _ => return None,
        }),
        "Tip Radius" => match geometry {
            ToolGeometry::TaperedBallNose { tip_radius, .. } => length(*tip_radius),
            ToolGeometry::RadiusedEndMill { corner_radius, .. } => length(*corner_radius),
            ToolGeometry::BallNose { diameter } => length(diameter / 2.0),
            _ => return None,
        },
        "Flat Diameter" => match geometry {
            ToolGeometry::EngravingTool { flat_diameter, .. } => length(*flat_diameter),
            _ => return None,
        },
        "Kerf" => match geometry {
            ToolGeometry::LaserTool { kerf, .. } => length(*kerf),
            _ => return None,
        },
        "Power" => match geometry {
            ToolGeometry::LaserTool { power, .. } => Value::Number(*power),
            _ => return None,
        },
        "Num Flutes" if tool.flutes > 0 => Value::Number(f64::from(tool.flutes)),
        "Pass Depth" => length(tool.pass_depth),
        "Stepover" => length(tool.stepover),
        "Stepover Percent" => Value::Number(tool.stepover_percent),
        "Rate Units" => text(rate_units(tool.feed_units)),
        "Move Speed" => Value::Number(tool.feed_rate),
        "Plunge Rate" => Value::Number(tool.plunge_rate),
        "Spindle Speed" => Value::Number(tool.spindle_speed),
        "Chip Load" => length(tool.chip_load()?),
        "Tool Number" => Value::Number(f64::from(tool.number)),
        "Material Name" if !context.material.is_empty() => text(context.material),
        "Machine Name" if !context.machine.is_empty() => text(context.machine),
        "Group Name" if !context.group.is_empty() => text(context.group),
        "Tool Notes" if !tool.notes.trim().is_empty() => {
            text(tool.notes.lines().next().unwrap_or_default().trim())
        }
        "Tool Id" if tool.id > 0 => Value::Number(tool.id as f64),
        custom => {
            let found = tool.variables.iter().find(|v| v.name == custom)?;
            text(&found.value)
        }
    };
    Some(value)
}

/// Short label of a feed rate unit.
pub fn rate_units(units: FeedUnits) -> &'static str {
    match units {
        FeedUnits::MmPerSec => "mm/sec",
        FeedUnits::MmPerMin => "mm/min",
        FeedUnits::InchPerSec => "inch/sec",
        FeedUnits::InchPerMin => "inch/min",
        FeedUnits::MetrePerMin => "m/min",
    }
}

/// A number with up to three decimals and no trailing zeros.
fn plain(value: f64) -> String {
    let text = format!("{:.3}", round_to(value, 3));
    let text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    if text == "-0" {
        "0".to_string()
    } else {
        text
    }
}

fn round_to(value: f64, decimals: i32) -> f64 {
    let scale = 10f64.powi(decimals);
    (value * scale).round() / scale
}

fn is_number_pattern(pattern: &str) -> bool {
    !pattern.is_empty()
        && pattern.chars().all(|c| c == '0' || c == '.')
        && pattern.matches('.').count() <= 1
}

/// Pads a number to a pattern of zeros: the zeros before the point are the
/// least number of whole digits, those after it the decimals.
fn pad(value: f64, pattern: &str) -> String {
    let (whole, decimals) = match pattern.split_once('.') {
        Some((whole, decimals)) => (whole.len(), decimals.len()),
        None => (pattern.len(), 0),
    };
    let text = format!("{:.*}", decimals, value.abs());
    let (int_part, frac_part) = match text.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (text.clone(), None),
    };
    let mut out = String::new();
    if value < 0.0 && text.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.push('-');
    }
    for _ in int_part.len()..whole {
        out.push('0');
    }
    out.push_str(&int_part);
    if let Some(frac) = frac_part {
        out.push('.');
        out.push_str(&frac);
    }
    out
}

/// A number as a fraction to the nearest 1/64, such as `1/8` or `1 1/4`.
fn fraction(value: f64) -> String {
    let negative = value < 0.0;
    let sixty_fourths = (value.abs() * 64.0).round() as u64;
    let whole = sixty_fourths / 64;
    let mut numerator = sixty_fourths % 64;
    let mut denominator = 64;
    while numerator > 0 && numerator % 2 == 0 {
        numerator /= 2;
        denominator /= 2;
    }
    let sign = if negative && sixty_fourths > 0 {
        "-"
    } else {
        ""
    };
    match (whole, numerator) {
        (w, 0) => format!("{sign}{w}"),
        (0, n) => format!("{sign}{n}/{denominator}"),
        (w, n) => format!("{sign}{w} {n}/{denominator}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end_mill(diameter: f64, units: Units) -> Tool {
        Tool {
            name: "cutter".to_string(),
            tool_type: ToolGeometry::EndMill { diameter },
            units,
            ..Tool::default()
        }
    }

    #[test]
    fn a_plain_name_is_left_alone() {
        let tool = end_mill(6.0, Units::Mm);
        assert_eq!(
            evaluate("End Mill 6 mm", &tool, NameContext::default()),
            "End Mill 6 mm"
        );
    }

    #[test]
    fn geometry_fields_follow_the_tool_units() {
        let tool = end_mill(12.7, Units::Inch);
        let name = evaluate(
            "{Tool Type} {Diameter} {Units}",
            &tool,
            NameContext::default(),
        );
        assert_eq!(name, "End Mill 0.5 inch");
        let metric = end_mill(6.0, Units::Mm);
        assert_eq!(
            evaluate("{Diameter}{Units Short}", &metric, NameContext::default()),
            "6mm"
        );
    }

    #[test]
    fn material_and_machine_come_from_the_context() {
        let tool = end_mill(12.7, Units::Inch);
        let template = "{Tool Type} {Diameter} {Material Name}";
        let oak = NameContext {
            material: "Oak",
            ..NameContext::default()
        };
        assert_eq!(evaluate(template, &tool, oak), "End Mill 0.5 Oak");
        // Without a material the field stays as written.
        assert_eq!(
            evaluate(template, &tool, NameContext::default()),
            "End Mill 0.5 {Material Name}"
        );
        let shop = NameContext {
            machine: "Shop",
            ..NameContext::default()
        };
        assert_eq!(evaluate("{Machine Name|U}", &tool, shop), "SHOP");
    }

    #[test]
    fn modifiers_change_case_padding_and_fractions() {
        let tool = end_mill(3.175, Units::Inch);
        assert_eq!(
            evaluate("{Tool Type|U}", &tool, NameContext::default()),
            "END MILL"
        );
        assert_eq!(
            evaluate("{Tool Type|L}", &tool, NameContext::default()),
            "end mill"
        );
        assert_eq!(
            evaluate("{Diameter|F}", &tool, NameContext::default()),
            "1/8"
        );
        assert_eq!(
            evaluate("{Diameter|00.000}", &tool, NameContext::default()),
            "00.125"
        );
        let metric = end_mill(1.5101, Units::Mm);
        assert_eq!(
            evaluate("{Diameter|0.00}", &metric, NameContext::default()),
            "1.51"
        );
        assert_eq!(fraction(1.25), "1 1/4");
        assert_eq!(fraction(2.0), "2");
    }

    #[test]
    fn angles_and_radii_come_from_the_shape() {
        let mut tool = end_mill(6.0, Units::Mm);
        tool.tool_type = ToolGeometry::VBit {
            diameter: 12.0,
            included_angle: 60.0,
        };
        assert_eq!(
            evaluate(
                "{Included Angle}/{Side Angle}",
                &tool,
                NameContext::default()
            ),
            "60/30"
        );
        // A V-Bit has no tip radius, so that field stays.
        assert_eq!(
            evaluate("{Tip Radius}", &tool, NameContext::default()),
            "{Tip Radius}"
        );
        tool.tool_type = ToolGeometry::TaperedBallNose {
            diameter: 6.0,
            tip_radius: 0.5,
            taper_angle: 5.0,
        };
        assert_eq!(
            evaluate(
                "{Tip Radius} {Included Angle}",
                &tool,
                NameContext::default()
            ),
            "0.5 10"
        );
    }

    #[test]
    fn custom_variables_fill_their_own_fields() {
        let mut tool = end_mill(6.0, Units::Mm);
        tool.set_variable("Maker", "Acme").expect("variable");
        tool.set_variable("Coating", "TiN").expect("variable");
        assert!(tool.set_variable("bad{name", "x").is_err());
        assert_eq!(tool.variables[0].name, "Coating");
        assert_eq!(
            evaluate("{Maker|U} {Coating} {Other}", &tool, NameContext::default()),
            "ACME TiN {Other}"
        );
    }

    #[test]
    fn feeds_and_an_unclosed_brace_are_handled() {
        let tool = end_mill(6.0, Units::Mm);
        assert_eq!(
            evaluate(
                "{Spindle Speed} {Rate Units} {Tool Number}",
                &tool,
                NameContext::default()
            ),
            "18000 mm/min 1"
        );
        assert_eq!(
            evaluate("{Diameter} {oops", &tool, NameContext::default()),
            "6 {oops"
        );
    }

    #[test]
    fn every_builtin_format_fills_in_for_its_own_shape() {
        let shapes = [
            ToolGeometry::EndMill { diameter: 6.0 },
            ToolGeometry::VBit {
                diameter: 12.0,
                included_angle: 90.0,
            },
            ToolGeometry::EngravingTool {
                diameter: 6.0,
                included_angle: 30.0,
                flat_diameter: 0.1,
            },
            ToolGeometry::TaperedBallNose {
                diameter: 6.0,
                tip_radius: 0.8,
                taper_angle: 3.0,
            },
            ToolGeometry::RadiusedEndMill {
                diameter: 6.0,
                corner_radius: 1.0,
            },
            ToolGeometry::DiamondDrag {
                diameter: 1.0,
                angle: 90.0,
            },
            ToolGeometry::LaserTool {
                power: 80.0,
                speed: 1000.0,
                kerf: 0.2,
                max_burn_rate: crate::cam::tool::DEFAULT_MAX_BURN_RATE,
            },
            ToolGeometry::Drill {
                diameter: 3.0,
                point_angle: 118.0,
            },
        ];
        for shape in shapes {
            let tool = Tool {
                tool_type: shape.clone(),
                ..Tool::default()
            };
            let name = evaluate(
                builtin_format(shape.type_name()),
                &tool,
                NameContext::default(),
            );
            assert!(!name.contains('{'), "{name}");
        }
    }
}
