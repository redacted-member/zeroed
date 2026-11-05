//! RS274 G code, read block by block in the order a controller executes
//! the words of one block.

use crate::machine::{Axes, Centre, Machine, Plane};
use crate::{Kind, Severity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motion {
    Rapid,
    Feed,
    Cw,
    Ccw,
}

/// One block's words.
#[derive(Debug, Default)]
struct Block {
    g: Vec<f64>,
    m: Vec<f64>,
    words: Vec<(char, f64)>,
}

impl Block {
    fn word(&self, letter: char) -> Option<f64> {
        self.words
            .iter()
            .find(|(l, _)| *l == letter)
            .map(|(_, v)| *v)
    }

    fn has_g(&self, code: f64) -> bool {
        self.g.iter().any(|g| (g - code).abs() < 1e-6)
    }

    fn has_m(&self, code: f64) -> bool {
        self.m.iter().any(|m| (m - code).abs() < 1e-6)
    }
}

/// Removes comments and splits a line into words. `Err` names what a
/// controller would refuse.
fn parse(line: &str) -> Result<Block, String> {
    let mut text = String::new();
    let mut depth = false;
    for c in line.chars() {
        match c {
            '(' if !depth => depth = true,
            '(' => return Err("a comment inside a comment".into()),
            ')' if depth => depth = false,
            ')' => return Err("a comment closes that never opened".into()),
            ';' if !depth => break,
            _ if depth => {}
            _ => text.push(c),
        }
    }
    if depth {
        return Err("a comment is not closed".into());
    }
    let trimmed = text.trim();
    if trimmed.starts_with('%') {
        return Ok(Block::default());
    }
    let chars: Vec<char> = trimmed.chars().collect();
    let mut block = Block::default();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if !c.is_ascii_alphabetic() {
            return Err(format!("unexpected '{c}'"));
        }
        let letter = c.to_ascii_uppercase();
        i += 1;
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        let start = i;
        if i < chars.len() && (chars[i] == '-' || chars[i] == '+') {
            i += 1;
        }
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
            i += 1;
        }
        let number: String = chars[start..i].iter().collect();
        let value: f64 = number
            .parse()
            .map_err(|_| format!("the word {letter} has no number"))?;
        match letter {
            'G' => block.g.push(value),
            'M' => block.m.push(value),
            'N' | 'O' => {}
            _ => {
                if block.words.iter().any(|(l, _)| *l == letter) {
                    return Err(format!("the word {letter} appears twice"));
                }
                block.words.push((letter, value));
            }
        }
    }
    Ok(block)
}

pub(crate) fn run(text: &str, machine: &mut Machine) {
    let mut motion: Option<Motion> = None;
    for (index, raw) in text.lines().enumerate() {
        machine.set_line(index + 1);
        if machine.ended() {
            continue;
        }
        let block = match parse(raw) {
            Ok(b) => b,
            Err(e) => {
                machine.issue(Severity::Error, Kind::Syntax, e);
                continue;
            }
        };
        run_block(&block, machine, &mut motion);
    }
}

fn run_block(block: &Block, machine: &mut Machine, motion: &mut Option<Motion>) {
    for g in &block.g {
        let known = [
            0.0, 1.0, 2.0, 3.0, 4.0, 17.0, 18.0, 19.0, 20.0, 21.0, 28.0, 40.0, 43.0, 49.0, 54.0,
            55.0, 56.0, 57.0, 58.0, 59.0, 61.0, 61.1, 64.0, 80.0, 90.0, 90.1, 91.0, 91.1, 93.0,
            94.0, 98.0, 99.0,
        ];
        if !known.iter().any(|k| (k - g).abs() < 1e-6) {
            machine.issue(
                Severity::Error,
                Kind::Unsupported,
                format!("G{g} is not read by the checker"),
            );
        }
    }
    for m in &block.m {
        let known = [
            0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 30.0, 400.0,
        ];
        if !known.iter().any(|k| (k - m).abs() < 1e-6) {
            machine.issue(
                Severity::Warning,
                Kind::Unsupported,
                format!("M{m} is not read by the checker"),
            );
        }
    }
    // Feed mode, feed, speed and tool come first.
    if block.has_g(93.0) {
        machine.set_inverse_time(true);
    }
    if block.has_g(94.0) {
        machine.set_inverse_time(false);
    }
    if let Some(f) = block.word('F') {
        machine.set_feed(f);
    }
    if let Some(s) = block.word('S') {
        machine.set_speed(s);
    }
    if let Some(t) = block.word('T') {
        machine.select_tool(t.round() as u32);
    }
    if block.has_m(6.0) {
        machine.change_tool();
    }
    if block.has_g(43.0) {
        machine.table_length_offset(block.word('H').map(|h| h.round() as u32));
    }
    if block.has_g(49.0) {
        machine.cancel_length_offset();
    }
    if block.has_g(40.0) {
        machine.compensation_off();
    }
    if block.has_m(3.0) || block.has_m(4.0) {
        machine.spindle(true);
    }
    if block.has_m(5.0) {
        machine.spindle(false);
    }
    if block.has_g(4.0) {
        match block.word('P').or(block.word('X')).or(block.word('S')) {
            Some(p) => machine.dwell(p),
            None => machine.issue(Severity::Error, Kind::Syntax, "a dwell with no time"),
        }
    }
    if block.has_g(17.0) {
        machine.set_plane(Plane::Xy);
    }
    if block.has_g(18.0) {
        machine.set_plane(Plane::Zx);
    }
    if block.has_g(19.0) {
        machine.set_plane(Plane::Yz);
    }
    if block.has_g(20.0) {
        machine.set_units(crate::Units::Inch);
    }
    if block.has_g(21.0) {
        machine.set_units(crate::Units::Mm);
    }
    if block.has_g(90.0) {
        machine.set_absolute(true);
    }
    if block.has_g(91.0) {
        machine.set_absolute(false);
    }
    if block.has_g(90.1) {
        machine.set_arc_absolute(true);
    }
    if block.has_g(91.1) {
        machine.set_arc_absolute(false);
    }
    if block.has_g(80.0) {
        *motion = None;
    }
    for (code, mode) in [
        (0.0, Motion::Rapid),
        (1.0, Motion::Feed),
        (2.0, Motion::Cw),
        (3.0, Motion::Ccw),
    ] {
        if block.has_g(code) {
            *motion = Some(mode);
        }
    }
    let mut axes = Axes::default();
    for (letter, axis) in [
        ('X', Axes::X),
        ('Y', Axes::Y),
        ('Z', Axes::Z),
        ('A', Axes::A),
        ('B', Axes::B),
    ] {
        if let Some(v) = block.word(letter) {
            axes.set(axis, v);
        }
    }
    // In a dwell block X is the time, not a move.
    if block.has_g(4.0) && block.word('P').is_none() {
        axes.v[Axes::X] = None;
    }
    if block.has_g(28.0) {
        // Through the given point to the reference; only a Z reference
        // return is modelled, which lifts the tool clear.
        if axes.any() {
            machine.linear(true, &axes, None);
        }
        if axes.v[Axes::Z].is_some() {
            machine.home_z();
        }
    } else if axes.any() {
        match *motion {
            Some(Motion::Rapid) => machine.linear(true, &axes, None),
            Some(Motion::Feed) => machine.linear(false, &axes, None),
            Some(mode @ (Motion::Cw | Motion::Ccw)) => {
                let centre = match block.word('R') {
                    Some(r) => Centre::Radius(r),
                    None => {
                        let ijk = [block.word('I'), block.word('J'), block.word('K')];
                        if ijk.iter().all(Option::is_none) {
                            machine.issue(
                                Severity::Error,
                                Kind::Syntax,
                                "an arc with neither a centre nor a radius",
                            );
                        }
                        Centre::Offsets(ijk)
                    }
                };
                machine.arc(mode == Motion::Cw, &axes, centre, None);
            }
            None => {
                machine.issue(
                    Severity::Error,
                    Kind::NoMotionMode,
                    "axis words with no G0, G1, G2 or G3 in force: the controller would use \
                     its power on motion mode",
                );
                machine.linear(true, &axes, None);
            }
        }
    }
    if block.has_m(0.0) || block.has_m(1.0) {
        machine.pause();
    }
    // Marlin firmware lists no M2 or M30: its files end by finishing their moves.
    if block.has_m(2.0) || block.has_m(30.0) || block.has_m(400.0) {
        machine.end();
    }
}
