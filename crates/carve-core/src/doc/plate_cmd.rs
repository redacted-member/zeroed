//! Plate Production: copies of a master template (badges, name plates)
//! laid out in rows across the material, with the template's text
//! variables filled from a data file or a counter.
//!
//! A variable is written into the template's text as `!!name!!`. Each
//! variable takes either a column of the data file or a counter (start,
//! step and a number of digits padded with leading zeros). The data file is
//! plain text with one plate per line, split by commas, tabs, semicolons or
//! spaces; with `header` on, the first line names the columns. Quoted
//! fields (`"a, b"`, with `""` for a quote) keep their separators.
//!
//! The plate size is the template's bounding box. Plates fill the material
//! (the job size inside the margins) row by row from the top left, with the
//! spacing between them. Plates that do not fit go on new sheets, or with
//! `mode: "layers"` on new layers of the same sheet (`Plates 1`,
//! `Plates 2`, ...), or every plate gets a sheet of its own with `ownSheets`. The template stays where
//! it is and stays selected, so the run can be repeated. Commands, all
//! under `plates.*`:
//! - `plates.variables {ids}` answers `{variables, toolpaths}`, the names
//!   used in the selected text, in order of first use, and how many
//!   calculated toolpaths machine the template.
//! - `plates.data {path | text, separator, header}` answers `{columns,
//!   rows, sample}`: column names (or `Column 1`...), row count and the
//!   first rows.
//! - `plates.produce {ids, path | text, separator, header, fields, count,
//!   margins, spacing, ownSheets, toolpaths}` makes the plates in one undo
//!   step and answers `{ids, plates, sheets, perSheet, toolpaths}`. With
//!   `toolpaths`, every toolpath calculated for the template is calculated
//!   again for the plates of each sheet, one per operation per sheet, named
//!   `S1 - Name` after the sheet number and the template's toolpath.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::json;

use super::ops;
use super::text_cmd::straight_text;
use super::{Document, EntityKind, TextEntity};
use crate::cam::compute::{self as cam_compute, Request};
use crate::error::CoreError;
use crate::geom::{self, FontLibrary, Mat3, Vec2};
use crate::kernel::{edit, parse_args, KernelReply};

/// Most plates one run makes, so a typo in a count cannot flood the job.
pub const MAX_PLATES: usize = 5000;

/// How the columns of the data file are split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Separator {
    /// `,` (the default).
    #[default]
    Comma,
    /// A tab.
    Tab,
    /// `;`.
    Semicolon,
    /// One or more spaces.
    Space,
}

/// Where a variable's text comes from.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum FieldSource {
    /// A column of the data file, from 0.
    Column {
        /// Column index.
        column: usize,
    },
    /// A number counting up (or down) from plate to plate.
    Counter {
        /// Value on the first plate.
        start: i64,
        /// Added for each further plate.
        step: i64,
        /// Pad with leading zeros to this many digits (0 for none).
        #[serde(default)]
        digits: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Margins {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

impl Default for Margins {
    fn default() -> Self {
        Margins {
            left: 10.0,
            right: 10.0,
            top: 10.0,
            bottom: 10.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Spacing {
    x: f64,
    y: f64,
}

impl Default for Spacing {
    fn default() -> Self {
        Spacing { x: 5.0, y: 5.0 }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VariablesArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataArgs {
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    separator: Separator,
    #[serde(default)]
    header: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProduceArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(flatten)]
    data: DataArgs,
    #[serde(default)]
    fields: BTreeMap<String, FieldSource>,
    /// Plates to make when no data file is given.
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    margins: Margins,
    #[serde(default)]
    spacing: Spacing,
    #[serde(default)]
    own_sheets: bool,
    /// Where plates that do not fit go: new sheets or new layers.
    #[serde(default)]
    mode: PlateMode,
    /// Calculate the template's toolpaths for the plates of every sheet.
    #[serde(default)]
    toolpaths: bool,
}

/// Where the plates that overflow a sheet go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum PlateMode {
    /// Each sheet worth of plates on a sheet of its own.
    #[default]
    Sheets,
    /// Each sheet worth of plates on a layer of its own, on the same sheet.
    Layers,
}

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(name, "plates.variables" | "plates.data" | "plates.produce")
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for another name, [`CoreError::InvalidArgs`]
/// for arguments that do not parse, and [`CoreError::Validation`] for a
/// template, data file or layout that cannot make plates.
pub fn dispatch(
    document: &mut Document,
    fonts: &FontLibrary,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "plates.variables" => {
            let args: VariablesArgs = parse_args(name, args)?;
            let ids = document.target_ids(args.ids.as_deref());
            KernelReply::ok(&json!({
                "variables": template_variables(document, &ids),
                "toolpaths": template_toolpaths(document, &ids).len(),
            }))
        }
        "plates.data" => {
            let args: DataArgs = parse_args(name, args)?;
            let table = read_table(&args)?;
            KernelReply::ok(&json!({
                "columns": table.columns,
                "rows": table.rows.len(),
                "sample": table.rows.iter().take(5).collect::<Vec<_>>(),
            }))
        }
        "plates.produce" => {
            let args: ProduceArgs = parse_args(name, args)?;
            produce(document, fonts, &args)
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// The variable names used in `text`, in order of first use.
pub fn variables_in(text: &str, out: &mut Vec<String>) {
    let mut rest = text;
    while let Some(open) = rest.find("!!") {
        let after = &rest[open + 2..];
        let Some(close) = after.find("!!") else {
            break;
        };
        let name = &after[..close];
        if !name.is_empty() && !name.contains(char::is_whitespace) {
            if !out.iter().any(|n| n == name) {
                out.push(name.to_string());
            }
            rest = &after[close + 2..];
        } else {
            rest = after;
        }
    }
}

/// `text` with every `!!name!!` whose name `values` holds replaced.
pub fn substitute(text: &str, values: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("!!") {
        let after = &rest[open + 2..];
        let Some(close) = after.find("!!") else {
            break;
        };
        let name = &after[..close];
        match values.get(name) {
            Some(value) => {
                out.push_str(&rest[..open]);
                out.push_str(value);
                rest = &after[close + 2..];
            }
            None => {
                out.push_str(&rest[..open + 2]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn template_variables(document: &Document, ids: &[u64]) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        for leaf in document.leaves(*id) {
            if let Ok(entity) = document.entity(leaf) {
                if let EntityKind::Text(text) = &entity.kind {
                    variables_in(&text.spec.text, &mut out);
                }
            }
        }
    }
    out
}

/// A data file split into rows.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// Column names: the header row, or `Column 1` and so on.
    pub columns: Vec<String>,
    /// The data rows.
    pub rows: Vec<Vec<String>>,
}

fn read_table(args: &DataArgs) -> Result<Table, CoreError> {
    let text = match (&args.text, &args.path) {
        (Some(text), _) => text.clone(),
        (None, Some(path)) => {
            std::fs::read_to_string(path).map_err(|e| CoreError::io(path.display(), e))?
        }
        (None, None) => return Err(CoreError::validation("choose a data file")),
    };
    Ok(parse_table(&text, args.separator, args.header))
}

/// Splits a data file into rows. Blank lines are skipped.
pub fn parse_table(text: &str, separator: Separator, header: bool) -> Table {
    let mut rows: Vec<Vec<String>> = text
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.trim().is_empty())
        .map(|line| split_line(line, separator))
        .collect();
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let columns = if header && !rows.is_empty() {
        let mut names = rows.remove(0);
        names.resize_with(width, String::new);
        names
            .into_iter()
            .enumerate()
            .map(|(i, n)| {
                if n.trim().is_empty() {
                    format!("Column {}", i + 1)
                } else {
                    n.trim().to_string()
                }
            })
            .collect()
    } else {
        (1..=width).map(|i| format!("Column {i}")).collect()
    };
    Table { columns, rows }
}

fn split_line(line: &str, separator: Separator) -> Vec<String> {
    if separator == Separator::Space {
        return line.split_whitespace().map(str::to_string).collect();
    }
    let sep = match separator {
        Separator::Comma => ',',
        Separator::Tab => '\t',
        _ => ';',
    };
    let mut out = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' if quoted => quoted = false,
            '"' if field.trim().is_empty() => {
                field.clear();
                quoted = true;
            }
            c if c == sep && !quoted => {
                out.push(field.trim().to_string());
                field.clear();
            }
            c => field.push(c),
        }
    }
    out.push(field.trim().to_string());
    out
}

/// The counter's text on plate `index`.
fn counter_text(start: i64, step: i64, digits: usize, index: usize) -> String {
    let value = start.saturating_add(step.saturating_mul(index as i64));
    if value < 0 {
        format!("-{:0width$}", value.unsigned_abs(), width = digits)
    } else {
        format!("{value:0digits$}")
    }
}

/// How plates sit on the material: columns and rows per sheet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// Plates across.
    pub columns: usize,
    /// Plates down.
    pub rows: usize,
}

/// Plates of `plate` size that fit in `space` with `gap` between them.
pub fn plate_grid(space: (f64, f64), plate: (f64, f64), gap: (f64, f64)) -> Grid {
    let fit = |room: f64, size: f64, gap: f64| {
        if size <= 0.0 || room + 1e-9 < size {
            0
        } else {
            (((room - size) / (size + gap)) + 1e-9).floor() as usize + 1
        }
    };
    Grid {
        columns: fit(space.0, plate.0, gap.0),
        rows: fit(space.1, plate.1, gap.1),
    }
}

fn produce(
    document: &mut Document,
    fonts: &FontLibrary,
    args: &ProduceArgs,
) -> Result<KernelReply, CoreError> {
    let ids = document.target_ids(args.ids.as_deref());
    if ids.is_empty() {
        return Err(CoreError::validation("select the template to copy"));
    }
    let template = document
        .bbox_of_many(&ids)
        .ok_or_else(|| CoreError::validation("the template has no size"))?;
    let has_data = args.data.path.is_some() || args.data.text.is_some();
    let table = if has_data {
        Some(read_table(&args.data)?)
    } else {
        None
    };
    let count = match (&table, args.count) {
        (Some(table), _) => table.rows.len(),
        (None, Some(count)) => count,
        (None, None) => 0,
    };
    if count == 0 {
        return Err(CoreError::validation(
            "there are no plates to make; choose a data file or a number of plates",
        ));
    }
    if count > MAX_PLATES {
        return Err(CoreError::validation(format!(
            "at most {MAX_PLATES} plates can be made at once"
        )));
    }
    for (name, source) in &args.fields {
        if let (FieldSource::Column { column }, Some(table)) = (source, &table) {
            if *column >= table.columns.len() {
                return Err(CoreError::validation(format!(
                    "the data file has no column {} for {name}",
                    column + 1
                )));
            }
        }
    }
    let m = args.margins;
    let s = args.spacing;
    for v in [m.left, m.right, m.top, m.bottom, s.x, s.y] {
        if !v.is_finite() || v < 0.0 {
            return Err(CoreError::validation(
                "margins and spacing must be zero or more millimeters",
            ));
        }
    }
    let job = document.job_bounds();
    let space = (
        job.width() - m.left - m.right,
        job.height() - m.top - m.bottom,
    );
    let plate = (template.width(), template.height());
    let grid = plate_grid(space, plate, (s.x, s.y));
    let per_sheet = if args.own_sheets {
        1
    } else {
        grid.columns * grid.rows
    };
    if grid.columns == 0 || grid.rows == 0 {
        return Err(CoreError::validation(
            "the template is larger than the material inside the margins",
        ));
    }

    // Every text entity of the template that uses a variable.
    let mut texts: Vec<u64> = Vec::new();
    for id in &ids {
        for leaf in document.leaves(*id) {
            if let Ok(entity) = document.entity(leaf) {
                if let EntityKind::Text(text) = &entity.kind {
                    let mut names = Vec::new();
                    variables_in(&text.spec.text, &mut names);
                    if !names.is_empty() {
                        texts.push(leaf);
                    }
                }
            }
        }
    }

    let first_sheet = document.active_sheet();
    let machining = if args.toolpaths {
        template_toolpaths(document, &ids)
    } else {
        Vec::new()
    };
    let layers_mode = args.mode == PlateMode::Layers && !args.own_sheets;
    edit(document, "Plate Production", |doc| {
        let mut sheets = vec![first_sheet];
        // Layers mode: one layer per sheet worth of plates, all on this sheet.
        let mut layers: Vec<u64> = Vec::new();
        let mut made = Vec::new();
        // For each sheet, every template id (top level and leaf) with the
        // copies of it on that sheet.
        let mut copies: Vec<BTreeMap<u64, Vec<u64>>> = Vec::new();
        for index in 0..count {
            let values: BTreeMap<String, String> = args
                .fields
                .iter()
                .map(|(name, source)| {
                    let value = match source {
                        FieldSource::Column { column } => table
                            .as_ref()
                            .and_then(|t| t.rows.get(index))
                            .and_then(|row| row.get(*column))
                            .cloned()
                            .unwrap_or_default(),
                        FieldSource::Counter {
                            start,
                            step,
                            digits,
                        } => counter_text(*start, *step, *digits, index),
                    };
                    (name.clone(), value)
                })
                .collect();
            let sheet_index = index / per_sheet;
            if layers_mode {
                while layers.len() <= sheet_index {
                    let name = format!("Plates {}", layers.len() + 1);
                    layers.push(doc.add_layer(Some(name), None));
                }
                while sheets.len() <= sheet_index {
                    sheets.push(first_sheet);
                }
            }
            while sheets.len() <= sheet_index {
                sheets.push(doc.add_sheet(None));
            }
            let slot = index % per_sheet;
            let (col, row) = (slot % grid.columns, slot / grid.columns);
            let left = job.min.x + m.left + col as f64 * (plate.0 + s.x);
            let top = job.max.y - m.top - row as f64 * (plate.1 + s.y);
            let shift = Mat3::translate(Vec2::new(left - template.min.x, top - template.max.y));
            for id in &ids {
                let copy = ops::duplicate(doc, *id)?;
                // Duplicates keep the template's leaf order, so the text
                // leaves pair up with the template's.
                let pairs: Vec<(u64, u64)> =
                    doc.leaves(*id).into_iter().zip(doc.leaves(copy)).collect();
                for (original, leaf) in pairs {
                    if texts.contains(&original) {
                        fill_text(doc, fonts, leaf, &values)?;
                    }
                }
                doc.transform_entities(&[copy], &shift);
                doc.set_entity_sheet(&[copy], sheets[sheet_index])?;
                if let Some(layer) = layers.get(sheet_index) {
                    doc.set_entity_layer(&[copy], *layer)?;
                }
                while copies.len() <= sheet_index {
                    copies.push(BTreeMap::new());
                }
                let on_sheet = &mut copies[sheet_index];
                on_sheet.entry(*id).or_default().push(copy);
                for (original, leaf) in doc.leaves(*id).into_iter().zip(doc.leaves(copy)) {
                    if original != *id {
                        on_sheet.entry(original).or_default().push(leaf);
                    }
                }
                made.push(copy);
            }
        }
        let mut toolpaths = Vec::new();
        for (index, on_sheet) in copies.iter().enumerate() {
            for request in &machining {
                let mut plate = request.clone();
                plate.ids = request
                    .ids
                    .iter()
                    .flat_map(|id| on_sheet.get(id).cloned().unwrap_or_default())
                    .collect();
                if plate.ids.is_empty() {
                    continue;
                }
                // The plates are picked by id, not by the template's filter.
                if let Some(params) = plate.params.as_object_mut() {
                    params.remove("selector");
                }
                let base = request.name.clone().unwrap_or_default();
                let prefix = if layers_mode { "L" } else { "S" };
                plate.name = Some(format!("{prefix}{} - {base}", index + 1));
                let done = cam_compute::calculate_parts(doc, &plate)?;
                for mut toolpath in done.clearance.into_iter().chain([done.main]) {
                    toolpath.sheet = sheets[index];
                    toolpaths.push(doc.add_toolpath(toolpath));
                }
            }
        }
        Ok(json!({
            "ids": made,
            "plates": count,
            "sheets": if layers_mode { 1 } else { sheets.len() },
            "layers": layers.len(),
            "perSheet": per_sheet,
            "toolpaths": toolpaths,
        }))
    })
}

/// The requests of the calculated toolpaths that machine only the template:
/// every vector they cut is one of `ids` or inside one of them. Toolpaths
/// that cannot be recalculated (imported or merged ones) are left out.
fn template_toolpaths(document: &Document, ids: &[u64]) -> Vec<Request> {
    let mut inside: Vec<u64> = ids.to_vec();
    for id in ids {
        inside.extend(document.leaves(*id));
    }
    document
        .toolpaths()
        .into_iter()
        // A clearance or later tool part is made again with its main toolpath.
        .filter(|toolpath| {
            !cam_compute::is_clearance(toolpath) && cam_compute::rest_index(toolpath).is_none()
        })
        .filter_map(|toolpath| Request::of(toolpath).ok())
        .filter(|request| {
            !request.ids.is_empty() && request.ids.iter().all(|id| inside.contains(id))
        })
        .collect()
}

/// Fills the variables of text entity `id` and lays it out again where it
/// is, turned by its kept rotation.
fn fill_text(
    doc: &mut Document,
    fonts: &FontLibrary,
    id: u64,
    values: &BTreeMap<String, String>,
) -> Result<(), CoreError> {
    let mut entity = doc.entity(id)?.clone();
    let EntityKind::Text(text) = &entity.kind else {
        return Ok(());
    };
    let mut spec = text.spec.clone();
    spec.text = substitute(&spec.text, values);
    let origin = text.origin;
    let mut paths = if spec.text.trim().is_empty() {
        Vec::new()
    } else {
        straight_text(fonts, &spec, origin)?
    };
    if entity.rotation != 0.0 {
        let turn = Mat3::rotate_about(entity.rotation.to_radians(), origin);
        paths = paths
            .iter()
            .map(|p| geom::transform_path(p, &turn))
            .collect();
    }
    entity.kind = EntityKind::Text(TextEntity {
        spec,
        origin,
        paths,
    });
    doc.replace_entity(id, entity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_are_found_and_filled() {
        let mut names = Vec::new();
        variables_in("Hello !!name!!, !!dept!! !!name!! !!not closed", &mut names);
        assert_eq!(names, vec!["name".to_string(), "dept".to_string()]);
        let values = BTreeMap::from([
            ("name".to_string(), "Ann".to_string()),
            ("dept".to_string(), "Sales".to_string()),
        ]);
        assert_eq!(
            substitute("Hello !!name!!, !!dept!! !!other!!", &values),
            "Hello Ann, Sales !!other!!"
        );
    }

    #[test]
    fn data_files_split_by_every_separator() {
        let table = parse_table(
            "Name,Dept\nAnn,\"Sales, North\"\n\nBob,\"Say \"\"hi\"\"\"\n",
            Separator::Comma,
            true,
        );
        assert_eq!(table.columns, vec!["Name", "Dept"]);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0][1], "Sales, North");
        assert_eq!(table.rows[1][1], "Say \"hi\"");
        let tabs = parse_table("a\tb\nc\td", Separator::Tab, false);
        assert_eq!(tabs.columns, vec!["Column 1", "Column 2"]);
        assert_eq!(tabs.rows[1], vec!["c", "d"]);
        let spaces = parse_table("a  b   c", Separator::Space, false);
        assert_eq!(spaces.rows[0], vec!["a", "b", "c"]);
        let semi = parse_table("x;y", Separator::Semicolon, false);
        assert_eq!(semi.rows[0], vec!["x", "y"]);
    }

    #[test]
    fn counters_pad_and_step() {
        assert_eq!(counter_text(1, 1, 3, 0), "001");
        assert_eq!(counter_text(1, 5, 0, 2), "11");
        assert_eq!(counter_text(0, -1, 2, 3), "-03");
    }

    #[test]
    fn plates_fit_the_material_with_their_gaps() {
        let grid = plate_grid((100.0, 50.0), (30.0, 20.0), (5.0, 5.0));
        assert_eq!(
            grid,
            Grid {
                columns: 3,
                rows: 2
            }
        );
        let exact = plate_grid((65.0, 20.0), (30.0, 20.0), (5.0, 0.0));
        assert_eq!(
            exact,
            Grid {
                columns: 2,
                rows: 1
            }
        );
        assert_eq!(
            plate_grid((10.0, 10.0), (30.0, 20.0), (0.0, 0.0)).columns,
            0
        );
    }
}
