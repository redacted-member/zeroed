//! ShopBot part files: one command per line, arguments after commas,
//! speeds in units per second.

use crate::machine::{Axes, Machine};
use crate::{Kind, Severity, Units};

pub(crate) fn run(text: &str, machine: &mut Machine) {
    // Move speeds along X and Y and along Z, units per second.
    let mut speed_xy: Option<f64> = None;
    let mut speed_z: Option<f64> = None;
    for (index, raw) in text.lines().enumerate() {
        machine.set_line(index + 1);
        if machine.ended() {
            continue;
        }
        let line = raw.trim();
        if line.is_empty() || line.starts_with('\'') {
            continue;
        }
        if line.ends_with(':') && !line.contains(',') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('&') {
            let Some((name, value)) = rest.split_once('=') else {
                machine.issue(Severity::Error, Kind::Syntax, "a variable with no value");
                continue;
            };
            if name.trim().eq_ignore_ascii_case("tool") {
                match value.trim().parse::<f64>() {
                    Ok(v) => machine.select_tool(v.round() as u32),
                    Err(_) => machine.issue(Severity::Error, Kind::Syntax, "a tool number"),
                }
            }
            continue;
        }
        let upper = line.to_ascii_uppercase();
        if let Some(rest) = upper.strip_prefix("IF ") {
            // The unit check a part file makes before it moves: the machine
            // setting in system variable 25 (0 inches, 1 millimetres) that
            // sends the file to its error label is the one it refuses.
            let compact: String = rest.chars().filter(|c| !c.is_whitespace()).collect();
            if compact.starts_with("%(25)=0THENGOTO") {
                machine.set_units(Units::Mm);
            } else if compact.starts_with("%(25)=1THENGOTO") {
                machine.set_units(Units::Inch);
            } else {
                machine.issue(
                    Severity::Warning,
                    Kind::Unsupported,
                    "a condition the checker does not follow",
                );
            }
            continue;
        }
        if let Some(rest) = upper.strip_prefix("PAUSE") {
            // PAUSE with a time waits that many seconds; without one it
            // waits for the operator.
            let time = rest.trim_start_matches([' ', ',']).trim();
            if time.is_empty() {
                machine.pause();
            } else {
                match time.parse::<f64>() {
                    Ok(t) => machine.dwell(t),
                    Err(_) => machine.issue(Severity::Error, Kind::Syntax, "a PAUSE time"),
                }
            }
            continue;
        }
        let mut parts = upper.split(',');
        let command = parts.next().unwrap_or("").trim().to_string();
        let mut args: Vec<Option<f64>> = Vec::new();
        let mut bad = false;
        for part in parts {
            let part = part.trim();
            if part.is_empty() {
                args.push(None);
            } else {
                match part.parse::<f64>() {
                    Ok(v) => args.push(Some(v)),
                    Err(_) => {
                        bad = true;
                        args.push(None);
                    }
                }
            }
        }
        if bad {
            machine.issue(
                Severity::Error,
                Kind::Syntax,
                format!("{command} has an argument that is not a number"),
            );
        }
        let arg = |i: usize| args.get(i).copied().flatten();
        let axes_of = |letters: &[usize]| {
            let mut axes = Axes::default();
            for (i, axis) in letters.iter().enumerate() {
                if let Some(v) = arg(i) {
                    axes.set(*axis, v);
                }
            }
            axes
        };
        let per_minute = |s: Option<f64>| s.map(|v| v * 60.0);
        match command.as_str() {
            "SA" => machine.set_absolute(true),
            "SR" => machine.set_absolute(false),
            "JS" => {}
            "MS" => {
                if arg(0).is_some() {
                    speed_xy = arg(0);
                }
                if arg(1).is_some() {
                    speed_z = arg(1);
                }
            }
            "J3" => machine.linear(true, &axes_of(&[Axes::X, Axes::Y, Axes::Z]), None),
            "J2" => machine.linear(true, &axes_of(&[Axes::X, Axes::Y]), None),
            "JX" => machine.linear(true, &axes_of(&[Axes::X]), None),
            "JY" => machine.linear(true, &axes_of(&[Axes::Y]), None),
            "JZ" => machine.linear(true, &axes_of(&[Axes::Z]), None),
            "M3" | "M2" | "MX" | "MY" | "MZ" => {
                let letters: &[usize] = match command.as_str() {
                    "M3" => &[Axes::X, Axes::Y, Axes::Z],
                    "M2" => &[Axes::X, Axes::Y],
                    "MX" => &[Axes::X],
                    "MY" => &[Axes::Y],
                    _ => &[Axes::Z],
                };
                let speed = if command == "MZ" { speed_z } else { speed_xy };
                if speed.is_none() {
                    machine.issue(
                        Severity::Error,
                        Kind::NoFeed,
                        "a move before MS sets its speed",
                    );
                }
                let feed = per_minute(speed).or(Some(0.0));
                machine.linear(false, &axes_of(letters), feed);
            }
            "TR" => machine.set_speed(arg(0).unwrap_or(0.0)),
            "C6" => machine.spindle(true),
            "C7" => machine.spindle(false),
            "C9" => machine.change_tool(),
            "END" => machine.end(),
            _ => machine.issue(
                Severity::Error,
                Kind::Unsupported,
                format!("{command} is not read by the checker"),
            ),
        }
    }
}
