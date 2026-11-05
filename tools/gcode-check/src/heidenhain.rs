//! Heidenhain conversational programs: numbered blocks of L lines, CC
//! circle centres and C arcs, with TOOL CALL for tool changes.

use crate::machine::{Axes, Centre, Machine};
use crate::{Kind, Severity, Units};

/// An axis word such as `X+10.000` or `IX-2.5`: its axis, value and
/// whether it is incremental.
fn axis_word(token: &str) -> Option<(usize, f64, bool)> {
    let (inc, rest) = match token.strip_prefix('I') {
        Some(rest) if rest.starts_with(['X', 'Y', 'Z', 'A', 'B']) => (true, rest),
        _ => (false, token),
    };
    let mut chars = rest.chars();
    let axis = match chars.next()? {
        'X' => Axes::X,
        'Y' => Axes::Y,
        'Z' => Axes::Z,
        'A' => Axes::A,
        'B' => Axes::B,
        _ => return None,
    };
    let value: f64 = chars.as_str().parse().ok()?;
    Some((axis, value, inc))
}

pub(crate) fn run(text: &str, machine: &mut Machine) {
    // The last feed programmed, units per minute; FMAX lasts one block.
    let mut feed: Option<f64> = None;
    let mut centre: Option<(f64, f64)> = None;
    for (index, raw) in text.lines().enumerate() {
        machine.set_line(index + 1);
        if machine.ended() {
            continue;
        }
        let mut line = raw.trim();
        // The block number.
        let digits = line.chars().take_while(char::is_ascii_digit).count();
        line = line[digits..].trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('*') {
            continue;
        }
        let upper = line.to_ascii_uppercase();
        let tokens: Vec<&str> = upper.split_whitespace().collect();
        match tokens.first().copied().unwrap_or("") {
            "BEGIN" => {
                if upper.ends_with(" MM") {
                    machine.set_units(Units::Mm);
                } else if upper.ends_with(" INCH") {
                    machine.set_units(Units::Inch);
                }
                machine.set_absolute(true);
            }
            "END" => machine.end(),
            "TOOL" if tokens.get(1) == Some(&"CALL") => {
                // A TOOL CALL with only a spindle speed changes the speed and
                // nothing else; with a tool number it changes the tool.
                let tool = tokens.get(2).and_then(|t| t.parse::<f64>().ok());
                let speed_only = tool.is_none()
                    && tokens.len() == 3
                    && tokens[2]
                        .strip_prefix('S')
                        .is_some_and(|s| s.parse::<f64>().is_ok());
                match tool {
                    Some(t) => machine.select_tool(t.round() as u32),
                    None if speed_only => {}
                    None => machine.issue(Severity::Error, Kind::Syntax, "TOOL CALL needs a tool"),
                }
                for t in &tokens[2..] {
                    if let Some(s) = t.strip_prefix('S').and_then(|s| s.parse::<f64>().ok()) {
                        machine.set_speed(s);
                    }
                }
                if !speed_only {
                    machine.change_tool();
                }
            }
            "CYCL" => {
                if upper.contains("9.1") {
                    if let Some(t) = tokens.last().and_then(|t| t.parse::<f64>().ok()) {
                        machine.dwell(t);
                    }
                } else if !upper.contains("9.0") {
                    machine.issue(
                        Severity::Error,
                        Kind::Unsupported,
                        "a cycle the checker does not read",
                    );
                }
            }
            "CC" => {
                let here = machine.position_in_units();
                let (mut cx, mut cy) = centre.unwrap_or((here[0], here[1]));
                for t in &tokens[1..] {
                    match axis_word(t) {
                        Some((Axes::X, v, inc)) => cx = if inc { here[0] + v } else { v },
                        Some((Axes::Y, v, inc)) => cy = if inc { here[1] + v } else { v },
                        _ => machine.issue(Severity::Error, Kind::Syntax, "a CC word"),
                    }
                }
                centre = Some((cx, cy));
            }
            "L" | "C" => {
                let arc = tokens[0] == "C";
                let mut axes = Axes::default();
                let mut rapid = false;
                let mut cw: Option<bool> = None;
                let mut ms: Vec<f64> = Vec::new();
                for t in &tokens[1..] {
                    if let Some((axis, v, inc)) = axis_word(t) {
                        axes.set(axis, v);
                        axes.inc[axis] = inc;
                    } else if *t == "FMAX" {
                        rapid = true;
                    } else if let Some(f) = t.strip_prefix('F') {
                        match f.parse::<f64>() {
                            Ok(v) => feed = Some(v),
                            Err(_) => machine.issue(Severity::Error, Kind::Syntax, "a feed word"),
                        }
                    } else if *t == "DR-" {
                        cw = Some(true);
                    } else if *t == "DR+" {
                        cw = Some(false);
                    } else if *t == "R0" {
                    } else if *t == "RL" || *t == "RR" {
                        machine.issue(
                            Severity::Error,
                            Kind::Unsupported,
                            "radius compensation moves the tool off the programmed path",
                        );
                    } else if let Some(m) = t.strip_prefix('M').and_then(|m| m.parse().ok()) {
                        ms.push(m);
                    } else {
                        machine.issue(
                            Severity::Error,
                            Kind::Syntax,
                            format!("{t} is not a word the checker reads"),
                        );
                    }
                }
                // Spindle on acts at the start of the block, off and the
                // program end at its end.
                if ms.contains(&3.0) || ms.contains(&4.0) {
                    machine.spindle(true);
                }
                if let Some(f) = feed {
                    machine.set_feed(f);
                }
                if arc {
                    let Some((cx, cy)) = centre else {
                        machine.issue(Severity::Error, Kind::Syntax, "an arc with no CC");
                        continue;
                    };
                    let Some(cw) = cw else {
                        machine.issue(Severity::Error, Kind::Syntax, "an arc with no DR");
                        continue;
                    };
                    machine.arc(cw, &axes, Centre::Absolute(cx, cy), None);
                } else if axes.any() {
                    machine.linear(rapid, &axes, None);
                }
                if ms.contains(&5.0) {
                    machine.spindle(false);
                }
                if ms.contains(&0.0) {
                    machine.pause();
                }
                if ms.contains(&2.0) || ms.contains(&30.0) {
                    machine.end();
                }
            }
            first => {
                if let Some(m) = first.strip_prefix('M').and_then(|m| m.parse::<f64>().ok()) {
                    match m as i64 {
                        3 | 4 => machine.spindle(true),
                        5 => machine.spindle(false),
                        0 | 1 => machine.pause(),
                        2 | 30 => machine.end(),
                        _ => {}
                    }
                } else if first == "STOP" {
                    machine.pause();
                } else {
                    machine.issue(
                        Severity::Error,
                        Kind::Unsupported,
                        format!("{first} is not read by the checker"),
                    );
                }
            }
        }
    }
}
