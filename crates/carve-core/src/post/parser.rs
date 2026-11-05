//! Reader for the post processor text format.
//!
//! The grammar this build supports, in short:
//!
//! ```text
//! ' a comment, and so is a line that starts with #
//! POST_NAME = "Generic G code (mm) (*.gcode)"
//! FILE_EXTENSION = "gcode"
//! UNITS = MM                  ' or INCHES
//! RAPID_RATE = 3000           ' post units per minute
//! SPINDLE_SPEED_RANGE = 6000 24000
//! PRINT_TOOLLIST = YES
//! LINE_ENDING = "[13][10]"    ' character codes
//! DIRECT_OUTPUT = "Controller"
//! ROTARY_WRAP = A             ' wraps rotary jobs, see `rotary`
//! SUBSTITUTE = "[124] [47]"   ' write 47 wherever 124 would be written
//! LINE_NUMBER_START = 10
//! LINE_NUMBER_INCREMENT = 10
//!
//! VAR X_POSITION = [X|A|X|1.3]
//! VAR FEED_RATE = [F|AF|F|1.0]
//!
//! begin HEADER
//! + "G90"
//! + "G21"
//! end
//! ```
//!
//! A `VAR` declaration names the token that stands for it in output lines,
//! then flags, then the text written in front of the number, then the number
//! format, and optionally a scale factor the value is multiplied by on the
//! way out. Flags are `A` absolute, `I` incremental (the change since the last
//! time the variable was written), `F` always write it even when it has not
//! changed, `Z` pad the whole number part with zeros and `P` write a plus
//! sign on positive values. The format `1.3` means at least one digit in
//! front of the decimal point and exactly three after it, and `1.0` writes a
//! whole number.
//!
//! Inside a `begin NAME` block every line that starts with `+` is an output
//! line. The rest of the line is a quoted string, and tokens in square
//! brackets inside it are replaced when the program is written. A variable
//! that has not changed since it was last written is replaced with nothing,
//! which is why the format has separate `FIRST_` blocks: they are the ones
//! that carry the feed rate.
//!
//! Also read: `LINE_NUMBER` (the same as `LINE_NUMBER_START`),
//! `LINE_NUMBER_MAXIMUM` (numbering starts again after it), `MIN_ARC_RADIUS`
//! and `MAX_ARC_RADIUS` (arcs outside them are written as straight moves),
//! `LASER_SUPPORT`, `ROTARY_WRAP_X` and `ROTARY_WRAP_Y` (flat X or Y wraps onto
//! the letter named), `MAXIMUM_ROTATION` (the longest turn in one move, longer
//! ones are broken up), `INVERSE_TIME_MODE`
//! (inverse time feeds on a wrapped rotary job: a declared `[FI]` variable
//! carries the rate, otherwise the feed words do; `[FEED_MODE]` writes G93
//! while those rates are in force and G94 otherwise) and `TAPE_SPLITTING` (long
//! programs cut into several files); `PRINT_DIRECT` and
//! `RAPID_PLUNGE_TO_STARTZ` are read and listed but not acted on. The blocks `INITIAL_RAPID_MOVE`,
//! `SPINDLE_ON`, `FEED_RATE_CHANGE` and the `CW_`/`CCW_HELICAL_ARC_MOVE`
//! pairs (with `FIRST_` versions) are written when defined. Token names are
//! matched in any case, so `[ArcStartX]` and `[ARCSTARTX]` are the same.
//!
//! Tool changes: `[ZCLEAR]` is the higher of the toolpath's safe Z and the
//! home Z. A tool change block lifts to it straight up before anything else
//! moves, because after a manual change the new cutter may sit on the
//! material where it was zeroed; the first rapid after the change then
//! crosses at that height and comes down only once over its start.
//!
//! Drill banks: `SETUP_DRILL_BANK` and `FINAL_DRILL_BANK_ACTION` open and
//! close each drill bank toolpath; between them either `DRILL_BANK_PATTERN`
//! selects the whole set of drills at once, or `DRILL_BANK_EXTEND_DRILL` and
//! `DRILL_BANK_RETRACT_DRILL` lower and raise one drill at a time. The
//! numbers `[DB]`, `[DBX]` (horizontal arm) and `[DBY]` (vertical arm) are
//! bit patterns of the drills down and `[CD]` is the one drill moving; they
//! are declared with `VAR` like any number. `[DL]` lists the drills down as
//! text, shaped by `VAR DRILL_LIST = [DL|prefix|separator|first separator]`
//! (a comma separated list when not declared).

use crate::error::CoreError;

use super::{BlockKind, Post, PostUnits, TapeSplitting, ValueSource, VarDef};

/// Tokens that stand for text rather than a number, and so need no `VAR`.
pub const TEXT_TOKENS: [&str; 20] = [
    "TOOL_NAME",
    "TOOLNAME",
    "TOOL_NOTES",
    "TP_DIR",
    "TP_EXT",
    "FILE_NOTES",
    "PRODUCT",
    "TOOLPATHS_OUTPUT",
    "TOOLS_USED",
    "TOOLPATH_NAME",
    "TOOLPATH_NOTES",
    "TP_FILENAME",
    "PATHNAME",
    "DATE",
    "TIME",
    "POST_NAME",
    "FILE_EXTENSION",
    "DL",
    "FEED_MODE",
    LENGTH_LINE_TOKEN,
];

/// Marks a line written only when the M6 macro leaves the tool length to
/// the program (the "M6 macro applies tool length" option off), such as a
/// `G43 H` after the change or a `G49` at the end. It writes nothing itself.
pub const LENGTH_LINE_TOKEN: &str = "IF_LENGTH_BY_PROGRAM";

/// Reads a post processor. `id` names it in the post list and is normally the
/// file name without its extension.
///
/// # Errors
/// Returns [`CoreError::Validation`] naming the line that could not be read.
pub fn parse_post(id: &str, text: &str) -> Result<Post, CoreError> {
    let mut post = Post {
        id: id.to_string(),
        ..Post::default()
    };
    let mut open: Option<(BlockKind, Vec<String>, usize)> = None;

    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('\'') || line.starts_with('#') {
            continue;
        }
        if let Some((kind, lines, _)) = open.as_mut() {
            if line == "end" || line == "END" {
                let kind = *kind;
                let lines = std::mem::take(lines);
                post.blocks.insert(kind, lines);
                open = None;
                continue;
            }
            match line.strip_prefix('+') {
                Some(rest) => lines.push(unquote(rest)),
                None => {
                    return Err(error(
                        id,
                        number,
                        format!(
                            "expected an output line starting with + or the word end, got {line}"
                        ),
                    ))
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("begin ") {
            let name = rest.trim();
            let kind = BlockKind::from_name(name)
                .ok_or_else(|| error(id, number, format!("unknown block {name}")))?;
            if post.blocks.contains_key(&kind) {
                return Err(error(id, number, format!("block {name} is defined twice")));
            }
            open = Some((kind, Vec::new(), number));
            continue;
        }
        let var = line
            .get(..4)
            .filter(|head| head.eq_ignore_ascii_case("VAR "))
            .map(|_| &line[4..]);
        if let Some(rest) = var {
            if let Some(format) = drill_list_format(rest) {
                post.drill_list = Some(format);
                continue;
            }
            let def = parse_var(id, number, rest)?;
            if post.variables.contains_key(&def.token) {
                return Err(error(
                    id,
                    number,
                    format!("token [{}] is declared twice", def.token),
                ));
            }
            post.variables.insert(def.token.clone(), def);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(error(id, number, format!("cannot read {line}")));
        };
        apply_setting(&mut post, id, number, key.trim(), value.trim())?;
    }

    if let Some((kind, _, start)) = open {
        return Err(error(
            id,
            start,
            format!("block {} is never closed with end", kind.name()),
        ));
    }
    check_tokens(&post, id)?;
    post.validate()?;
    Ok(post)
}

fn apply_setting(
    post: &mut Post,
    id: &str,
    number: usize,
    key: &str,
    value: &str,
) -> Result<(), CoreError> {
    match key {
        "POST_NAME" => post.name = unquote(value),
        "FILE_EXTENSION" => {
            post.file_extension = unquote(value).trim_start_matches('.').to_string()
        }
        "UNITS" => {
            post.units = match unquote(value).to_uppercase().as_str() {
                "MM" | "MILLIMETRES" | "MILLIMETERS" => PostUnits::Mm,
                "INCH" | "INCHES" => PostUnits::Inch,
                other => return Err(error(id, number, format!("unknown UNITS {other}"))),
            }
        }
        "RAPID_RATE" => post.rapid_rate = number_setting(id, number, key, value)?,
        "LINE_NUMBER_START" | "LINE_NUMBER" => {
            post.line_number_start = number_setting(id, number, key, value)?
        }
        "LINE_NUMBER_MAXIMUM" => {
            post.line_number_maximum = Some(number_setting(id, number, key, value)?)
        }
        "MIN_ARC_RADIUS" => post.min_arc_radius = Some(number_setting(id, number, key, value)?),
        "MAX_ARC_RADIUS" => post.max_arc_radius = Some(number_setting(id, number, key, value)?),
        "LASER_SUPPORT" => post.laser_support = flag_setting(value),
        "INVERSE_TIME_MODE" => post.inverse_time = flag_setting(value),
        "TAPE_SPLITTING" => post.tape_splitting = Some(tape_splitting(id, number, value)?),
        // Read so that posts written for other programs load; this build
        // does not act on them.
        "PRINT_DIRECT" | "RAPID_PLUNGE_TO_STARTZ" => {
            if !post.ignored_settings.iter().any(|s| s == key) {
                post.ignored_settings.push(key.to_string());
            }
        }
        "LINE_NUMBER_INCREMENT" => {
            post.line_number_increment = number_setting(id, number, key, value)?
        }
        "SPINDLE_SPEED_RANGE" => {
            let numbers = number_list(id, number, key, value)?;
            let (low, high) = match numbers.as_slice() {
                [low, high] => (*low, *high),
                // Some posts name the range first, which we ignore.
                [_, low, high] => (*low, *high),
                _ => {
                    return Err(error(
                        id,
                        number,
                        "SPINDLE_SPEED_RANGE wants a lowest and a highest speed",
                    ))
                }
            };
            post.spindle_speed_range = Some((low.min(high), low.max(high)));
        }
        "PRINT_TOOLLIST" => post.print_toollist = flag_setting(value),
        "LINE_ENDING" => post.line_ending = char_codes(&unquote(value)),
        "DIRECT_OUTPUT" => post.direct_output = Some(unquote(value)),
        "ROTARY_WRAP" | "ROTARY_WRAP_X" | "ROTARY_WRAP_Y" => {
            use super::rotary::RotaryWord;
            let word = RotaryWord::from_name(&unquote(value))
                .ok_or_else(|| error(id, number, "ROTARY_WRAP names the axis A or B"))?;
            // The plain form: A wraps flat Y and B wraps flat X.
            if key == "ROTARY_WRAP_Y" || (key == "ROTARY_WRAP" && word == RotaryWord::A) {
                post.rotary_wrap_y = Some(word);
            } else {
                post.rotary_wrap_x = Some(word);
            }
        }
        "MAXIMUM_ROTATION" => {
            let degrees = number_setting(id, number, key, &unquote(value))?;
            if !(degrees > 0.0 && degrees <= 179.0) {
                return Err(error(
                    id,
                    number,
                    "MAXIMUM_ROTATION wants a turn of more than 0 and at most 179 degrees",
                ));
            }
            post.maximum_rotation = Some(degrees);
        }
        "SUBSTITUTE" => {
            let text = unquote(value);
            let (from, to) = text
                .split_once(' ')
                .ok_or_else(|| error(id, number, "SUBSTITUTE wants two character codes"))?;
            let from = char_codes(from.trim());
            let to = char_codes(to.trim());
            let mut chars = from.chars();
            let (Some(first), None) = (chars.next(), chars.next()) else {
                return Err(error(
                    id,
                    number,
                    "SUBSTITUTE replaces one character at a time",
                ));
            };
            post.substitutions.push((first, to));
        }
        other => {
            return Err(error(id, number, format!("unknown setting {other}")));
        }
    }
    Ok(())
}

/// The `[DL]` declaration, `NAME = [DL|prefix|separator|first separator]`,
/// whose fields are text rather than flags and a number format. The fields
/// keep their spaces, as a separator may be one. None for any other `VAR`.
fn drill_list_format(rest: &str) -> Option<super::DrillListFormat> {
    let (_, body) = rest.split_once('=')?;
    let body = body.trim_start().strip_prefix('[')?;
    let body = body.trim_end().strip_suffix(']')?;
    let mut fields = body.split('|');
    if !fields.next()?.trim().eq_ignore_ascii_case("DL") {
        return None;
    }
    let prefix = fields.next().unwrap_or_default().to_string();
    let separator = fields.next().unwrap_or(",").to_string();
    let first_separator = fields
        .next()
        .map_or_else(|| separator.clone(), str::to_string);
    Some(super::DrillListFormat {
        prefix,
        separator,
        first_separator,
    })
}

fn parse_var(id: &str, number: usize, rest: &str) -> Result<VarDef, CoreError> {
    let (name, body) = rest
        .split_once('=')
        .ok_or_else(|| error(id, number, "a VAR declaration wants a name and a format"))?;
    let body = body.trim();
    let body = body
        .strip_prefix('[')
        .and_then(|b| b.strip_suffix(']'))
        .ok_or_else(|| error(id, number, "a VAR format is written in square brackets"))?;
    let fields: Vec<&str> = body.split('|').collect();
    let (token, flags, output, format, scale) = match fields.as_slice() {
        [token, flags, output, format] => (token, flags, output, format, None),
        [token, flags, output, format, scale] => (token, flags, output, format, Some(scale)),
        _ => {
            return Err(error(
                id,
                number,
                "a VAR format wants four fields: token, flags, output and number format, \
                 and may end with a scale factor",
            ))
        }
    };
    let token = token.trim().to_uppercase();
    let source = value_source(&token)
        .ok_or_else(|| error(id, number, format!("unknown variable token {token}")))?;
    let mut def = VarDef {
        name: name.trim().to_string(),
        token,
        source,
        absolute: true,
        always: false,
        zero_pad: false,
        force_sign: false,
        output: (*output).to_string(),
        int_digits: 1,
        decimals: 0,
        scale: 1.0,
    };
    if let Some(scale) = scale {
        def.scale = scale
            .trim()
            .parse()
            .map_err(|_| error(id, number, format!("{scale} is not a scale factor")))?;
        if !def.scale.is_finite() {
            return Err(error(id, number, "a scale factor must be a number"));
        }
    }
    for flag in flags.trim().chars() {
        match flag.to_ascii_uppercase() {
            'A' => def.absolute = true,
            'I' => def.absolute = false,
            'F' => def.always = true,
            'Z' => def.zero_pad = true,
            'P' => def.force_sign = true,
            other => {
                return Err(error(id, number, format!("unknown variable flag {other}")));
            }
        }
    }
    let (whole, decimals) = format.trim().split_once('.').ok_or_else(|| {
        error(
            id,
            number,
            "a number format is written as whole digits, a dot, then decimal digits",
        )
    })?;
    def.int_digits = whole
        .trim()
        .parse()
        .map_err(|_| error(id, number, format!("{whole} is not a digit count")))?;
    def.decimals = decimals
        .trim()
        .parse()
        .map_err(|_| error(id, number, format!("{decimals} is not a digit count")))?;
    if def.int_digits > 12 || def.decimals > 12 {
        return Err(error(id, number, "a number format is at most 12 digits"));
    }
    if def.name.is_empty() {
        return Err(error(id, number, "a VAR declaration wants a name"));
    }
    Ok(def)
}

/// Checks every token used in an output line is one the post declared or one
/// the engine knows about.
fn check_tokens(post: &Post, id: &str) -> Result<(), CoreError> {
    for (kind, lines) in &post.blocks {
        for line in lines {
            for token in tokens_in(line) {
                let token = token.to_uppercase();
                if post.variables.contains_key(&token) || TEXT_TOKENS.contains(&token.as_str()) {
                    continue;
                }
                return Err(CoreError::validation(format!(
                    "post {id}: block {} uses [{token}], which is not declared",
                    kind.name()
                )));
            }
        }
    }
    Ok(())
}

/// Every token name used in one output line, in the order they appear.
pub fn tokens_in(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        match after.find(']') {
            Some(end) => {
                out.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    out
}

/// Maps a token name to where its number comes from.
pub fn value_source(token: &str) -> Option<ValueSource> {
    let source = match token {
        "X" => ValueSource::X,
        "Y" => ValueSource::Y,
        "Z" => ValueSource::Z,
        "A" => ValueSource::AngleA,
        "B" => ValueSource::AngleB,
        "F" => ValueSource::Feed,
        "FP" => ValueSource::PlungeFeed,
        "FR" => ValueSource::RapidRate,
        "S" => ValueSource::Spindle,
        "T" => ValueSource::ToolNumber,
        "N" => ValueSource::LineNumber,
        "I" => ValueSource::ArcCentreX,
        "J" => ValueSource::ArcCentreY,
        "K" => ValueSource::ArcCentreZ,
        "R" => ValueSource::ArcRadius,
        "DWELL" => ValueSource::Dwell,
        "POWER" => ValueSource::Power,
        "SAFEZ" => ValueSource::SafeZ,
        "ZCLEAR" => ValueSource::ClearanceZ,
        "XH" => ValueSource::HomeX,
        "YH" => ValueSource::HomeY,
        "ZH" => ValueSource::HomeZ,
        "XMIN" => ValueSource::MinX,
        "YMIN" => ValueSource::MinY,
        "ZMIN" => ValueSource::MinZ,
        "XMAX" => ValueSource::MaxX,
        "YMAX" => ValueSource::MaxY,
        "ZMAX" => ValueSource::MaxZ,
        "XLENGTH" => ValueSource::LengthX,
        "YLENGTH" => ValueSource::LengthY,
        "ZLENGTH" => ValueSource::LengthZ,
        "TOOL_DIA" => ValueSource::ToolDiameter,
        "STEPOVER" => ValueSource::ToolStepover,
        "PASS_DEPTH" => ValueSource::ToolPassDepth,
        "TDIA" => ValueSource::ToolDiameter,
        "FC" => ValueSource::CutFeed,
        "TP" => ValueSource::PreviousToolNumber,
        "IA" => ValueSource::ArcCentreAbsX,
        "JA" => ValueSource::ArcCentreAbsY,
        "ARCSTARTX" => ValueSource::ArcStartX,
        "ARCSTARTY" => ValueSource::ArcStartY,
        "ARCMIDX" => ValueSource::ArcMidX,
        "ARCMIDY" => ValueSource::ArcMidY,
        "ARCMIDXI" => ValueSource::ArcMidXInc,
        "ARCMIDYI" => ValueSource::ArcMidYInc,
        "RADIUS" => ValueSource::ArcRadius,
        "ANGLE" => ValueSource::ArcAngle,
        "WRAP_DIA" => ValueSource::WrapDiameter,
        "FI" => ValueSource::InverseTime,
        "DB" => ValueSource::DrillPattern,
        "DBX" => ValueSource::DrillPatternX,
        "DBY" => ValueSource::DrillPatternY,
        "CD" => ValueSource::CurrentDrill,
        _ => return None,
    };
    Some(source)
}

/// Drops one pair of surrounding quotes and trims what is left.
fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    match trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        Some(inner) => inner.to_string(),
        None => trimmed.to_string(),
    }
}

/// Turns `[13][10]` into the characters those codes stand for. Text that
/// holds no codes is handed back as it is.
fn char_codes(value: &str) -> String {
    if !value.contains('[') {
        return value.to_string();
    }
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find(']') {
            Some(end) => {
                match after[..end]
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                {
                    Some(found) => out.push(found),
                    None => {
                        out.push('[');
                        out.push_str(&after[..end]);
                        out.push(']');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(after);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Reads `MAX_NUM_LINES LINE_TOL "FILENAME_FORMAT" START_INDEX
/// INDEX_ON_FIRST_FILE`; the parts after the line count are optional.
fn tape_splitting(id: &str, number: usize, value: &str) -> Result<TapeSplitting, CoreError> {
    let mut words: Vec<String> = Vec::new();
    let mut rest = value.trim();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('"') {
            let end = after.find('"').unwrap_or(after.len());
            words.push(after[..end].to_string());
            rest = after.get(end + 1..).unwrap_or("").trim_start();
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            words.push(rest[..end].to_string());
            rest = rest[end..].trim_start();
        }
    }
    let whole = |index: usize, default: usize| -> Result<usize, CoreError> {
        match words.get(index) {
            Some(word) => word.parse().map_err(|_| {
                error(
                    id,
                    number,
                    format!("TAPE_SPLITTING wants whole numbers, got {word}"),
                )
            }),
            None => Ok(default),
        }
    };
    let max_lines = whole(0, 0)?;
    if max_lines == 0 {
        return Err(error(
            id,
            number,
            "TAPE_SPLITTING wants the most lines in a file",
        ));
    }
    Ok(TapeSplitting {
        max_lines,
        line_tolerance: whole(1, 0)?.min(max_lines - 1),
        name_format: words
            .get(2)
            .cloned()
            .unwrap_or_else(|| "%s_%d.[TP_EXT]".to_string()),
        start_index: whole(3, 1)?,
        index_on_first: words.get(4).is_none_or(|w| flag_setting(w)),
    })
}

fn flag_setting(value: &str) -> bool {
    matches!(
        unquote(value).to_uppercase().as_str(),
        "YES" | "TRUE" | "ON" | "1"
    )
}

fn number_setting(id: &str, number: usize, key: &str, value: &str) -> Result<f64, CoreError> {
    unquote(value)
        .trim()
        .parse()
        .map_err(|_| error(id, number, format!("{key} wants a number, got {value}")))
}

fn number_list(id: &str, number: usize, key: &str, value: &str) -> Result<Vec<f64>, CoreError> {
    unquote(value)
        .split_whitespace()
        .map(|part| {
            part.parse()
                .map_err(|_| error(id, number, format!("{key} wants numbers, got {part}")))
        })
        .collect()
}

fn error(id: &str, line: usize, message: impl std::fmt::Display) -> CoreError {
    CoreError::validation(format!("post {id} line {line}: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRICKY: &str = r#"
' A post with everything awkward in it.
# another comment style

POST_NAME = "Tricky post (mm) (*.tap)"
FILE_EXTENSION = ".tap"
UNITS = MM
RAPID_RATE = 2500
SPINDLE_SPEED_RANGE = 1 6000 24000
PRINT_TOOLLIST = YES
LINE_ENDING = "[13][10]"
DIRECT_OUTPUT = "Some Controller"
SUBSTITUTE = "[124] [47]"
LINE_NUMBER_START = 10
LINE_NUMBER_INCREMENT = 5

VAR LINE_NUMBER = [N|AF|N|1.0]
VAR X_POSITION = [X|A|X|1.3]
VAR Y_POSITION = [Y|A|Y|1.3]
VAR Z_POSITION = [Z|A|Z|1.3]
VAR FEED_RATE = [F|AF|F|4.1]
VAR ARC_CENTRE_X = [I|IFZ|I|2.3]
VAR SPINDLE = [S|AP|S|1.0]
VAR TOOL_NUMBER = [T|AF|T|1.0]

begin HEADER
+ "[N]G90"
+ "(T[T] = [TOOL_NAME])"
+ ""
end

begin RAPID_MOVE
+ "[N]G0[X][Y][Z]"
end

begin FEED_MOVE
+ "[N]G1[X][Y][Z][F]"
end
"#;

    #[test]
    fn a_tricky_post_parses() {
        let post = parse_post("tricky", TRICKY).expect("post parses");
        assert_eq!(post.name, "Tricky post (mm) (*.tap)");
        assert_eq!(post.file_extension, "tap");
        assert_eq!(post.units, PostUnits::Mm);
        assert_eq!(post.rapid_rate, 2500.0);
        assert_eq!(post.spindle_speed_range, Some((6000.0, 24000.0)));
        assert!(post.print_toollist);
        assert_eq!(post.line_ending, "\r\n");
        assert_eq!(post.direct_output.as_deref(), Some("Some Controller"));
        // The post list tells Save Toolpaths which driver direct output uses.
        assert_eq!(
            post.summary().direct_output.as_deref(),
            Some("Some Controller")
        );
        assert_eq!(post.substitutions, vec![('|', "/".to_string())]);
        assert_eq!(post.line_number_start, 10.0);
        assert_eq!(post.line_number_increment, 5.0);
        assert_eq!(post.variables.len(), 8);
        assert_eq!(post.blocks.len(), 3);
    }

    #[test]
    fn variable_flags_and_formats_are_read() {
        let post = parse_post("tricky", TRICKY).expect("post parses");
        let x = post.variables.get("X").expect("X is declared");
        assert!(x.absolute);
        assert!(!x.always);
        assert_eq!(x.output, "X");
        assert_eq!((x.int_digits, x.decimals), (1, 3));
        let feed = post.variables.get("F").expect("F is declared");
        assert!(feed.always);
        assert_eq!((feed.int_digits, feed.decimals), (4, 1));
        let centre = post.variables.get("I").expect("I is declared");
        assert!(!centre.absolute);
        assert!(centre.always);
        assert!(centre.zero_pad);
        let spindle = post.variables.get("S").expect("S is declared");
        assert!(spindle.force_sign);
        assert_eq!(spindle.name, "SPINDLE");
    }

    #[test]
    fn output_lines_keep_their_text_including_empty_ones() {
        let post = parse_post("tricky", TRICKY).expect("post parses");
        let header = post.block(BlockKind::Header).expect("header");
        assert_eq!(header.len(), 3);
        assert_eq!(header[0], "[N]G90");
        assert_eq!(header[1], "(T[T] = [TOOL_NAME])");
        assert_eq!(header[2], "");
    }

    #[test]
    fn inch_units_and_plain_values_are_accepted() {
        let text = "POST_NAME = Plain inch post\nFILE_EXTENSION = nc\nUNITS = INCHES\n\
                    PRINT_TOOLLIST = NO\nVAR X_POSITION = [X|A|X|1.4]\n\
                    begin RAPID_MOVE\n+ G0[X]\nend\nbegin FEED_MOVE\n+ G1[X]\nend\n";
        let post = parse_post("plain", text).expect("post parses");
        assert_eq!(post.units, PostUnits::Inch);
        assert_eq!(post.name, "Plain inch post");
        assert_eq!(post.file_extension, "nc");
        assert!(!post.print_toollist);
        assert_eq!(post.line_ending, "\n");
    }

    #[test]
    fn an_unknown_block_is_refused_with_its_line() {
        let text = "POST_NAME = x\nFILE_EXTENSION = nc\nbegin NONSENSE\n+ \"a\"\nend\n";
        let error = parse_post("bad", text).expect_err("unknown block");
        let message = error.to_string();
        assert!(message.contains("line 3"), "{message}");
        assert!(message.contains("NONSENSE"), "{message}");
    }

    #[test]
    fn an_unclosed_block_is_refused() {
        let text = "POST_NAME = x\nFILE_EXTENSION = nc\nbegin HEADER\n+ \"a\"\n";
        let error = parse_post("bad", text).expect_err("unclosed block");
        assert!(error.to_string().contains("never closed"), "{error}");
    }

    #[test]
    fn a_stray_line_inside_a_block_is_refused() {
        let text = "POST_NAME = x\nFILE_EXTENSION = nc\nbegin HEADER\nG90\nend\n";
        let error = parse_post("bad", text).expect_err("stray line");
        assert!(error.to_string().contains("line 4"), "{error}");
    }

    #[test]
    fn an_undeclared_token_is_refused() {
        let text = "POST_NAME = x\nFILE_EXTENSION = nc\n\
                    begin RAPID_MOVE\n+ \"G0[X]\"\nend\nbegin FEED_MOVE\n+ \"G1\"\nend\n";
        let error = parse_post("bad", text).expect_err("undeclared token");
        assert!(error.to_string().contains("[X]"), "{error}");
    }

    #[test]
    fn a_duplicate_block_or_token_is_refused() {
        let base = "POST_NAME = x\nFILE_EXTENSION = nc\nbegin RAPID_MOVE\n+ \"G0\"\nend\n\
                    begin FEED_MOVE\n+ \"G1\"\nend\n";
        let twice = format!("{base}begin FEED_MOVE\n+ \"G1\"\nend\n");
        assert!(parse_post("bad", &twice).is_err());
        let vars = format!("VAR A = [X|A|X|1.3]\nVAR B = [X|A|X|1.3]\n{base}");
        assert!(parse_post("bad", &vars).is_err());
    }

    #[test]
    fn broken_variable_declarations_are_refused() {
        let base = "POST_NAME = x\nFILE_EXTENSION = nc\nbegin RAPID_MOVE\n+ \"G0\"\nend\n\
                    begin FEED_MOVE\n+ \"G1\"\nend\n";
        for var in [
            "VAR X_POSITION = X|A|X|1.3",
            "VAR X_POSITION = [X|A|X]",
            "VAR X_POSITION = [NOPE|A|X|1.3]",
            "VAR X_POSITION = [X|Q|X|1.3]",
            "VAR X_POSITION = [X|A|X|13]",
            "VAR X_POSITION = [X|A|X|1.99]",
            "VAR  = [X|A|X|1.3]",
        ] {
            let text = format!("{var}\n{base}");
            assert!(parse_post("bad", &text).is_err(), "{var} should be refused");
        }
    }

    #[test]
    fn a_post_with_no_way_to_cut_is_refused() {
        let text = "POST_NAME = x\nFILE_EXTENSION = nc\nbegin HEADER\n+ \"G90\"\nend\n";
        assert!(parse_post("bad", text).is_err());
        let no_name = "FILE_EXTENSION = nc\nbegin RAPID_MOVE\n+ \"G0\"\nend\n\
                       begin FEED_MOVE\n+ \"G1\"\nend\n";
        assert!(parse_post("bad", no_name).is_err());
    }

    #[test]
    fn an_unknown_setting_is_refused() {
        let text = "POST_NAME = x\nWHAT_IS_THIS = 4\nFILE_EXTENSION = nc\n";
        let error = parse_post("bad", text).expect_err("unknown setting");
        assert!(error.to_string().contains("WHAT_IS_THIS"), "{error}");
    }

    #[test]
    fn character_codes_are_decoded() {
        assert_eq!(char_codes("[13][10]"), "\r\n");
        assert_eq!(char_codes("[9]"), "\t");
        assert_eq!(char_codes("plain"), "plain");
        assert_eq!(char_codes("[nope]"), "[nope]");
        assert_eq!(char_codes("a[32]b"), "a b");
    }

    #[test]
    fn tokens_are_found_in_order() {
        assert_eq!(tokens_in("G1[X][Y][F]"), vec!["X", "Y", "F"]);
        assert_eq!(tokens_in("no tokens"), Vec::<String>::new());
        assert_eq!(tokens_in("[unclosed"), Vec::<String>::new());
    }
}
