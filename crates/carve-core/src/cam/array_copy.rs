//! Array Copy Toolpath: copies calculated toolpaths into a grid of rows and
//! columns, to the right of and above the originals. Each copy toolpath
//! keeps a link to the toolpaths it was copied from, so recalculating it
//! copies them again after they were edited.
//!
//! With Minimize Tool Changes the copies of every source cut with the same
//! tool are joined into one toolpath, so the whole grid is cut with the
//! fewest tool changes; without it each source gives its own toolpath of
//! copies.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::CoreError;
use crate::geom::Point;

use super::merge::merge;
use super::toolpath::{Move, MoveKind, Point3, Toolpath};

/// How the X and Y spacing of an array is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArraySpacing {
    /// From one copy's start to the next.
    #[default]
    Offset,
    /// Between the bounding boxes of neighbouring copies.
    Gap,
}

/// The Array Copy Toolpath form's settings. Lengths are millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ArrayCopySpec {
    /// Rows (Y).
    pub rows: u32,
    /// Columns (X).
    pub columns: u32,
    /// Offset or Gap.
    pub spacing: ArraySpacing,
    /// Spacing along X.
    pub x: f64,
    /// Spacing along Y.
    pub y: f64,
    /// Join the copies of sources that share a tool into one toolpath.
    pub minimize_tool_changes: bool,
}

impl Default for ArrayCopySpec {
    fn default() -> Self {
        ArrayCopySpec {
            rows: 2,
            columns: 2,
            spacing: ArraySpacing::Gap,
            x: 10.0,
            y: 10.0,
            minimize_tool_changes: true,
        }
    }
}

/// A toolpath moved by `(dx, dy)`: every move and arc centre; its home stays.
fn shifted(toolpath: &Toolpath, dx: f64, dy: f64) -> Toolpath {
    let mut copy = toolpath.clone();
    for m in &mut copy.moves {
        m.to = Point3::new(m.to.x + dx, m.to.y + dy, m.to.z);
        m.kind = match m.kind {
            MoveKind::ArcCw { centre } => MoveKind::ArcCw {
                centre: Point::new(centre.x + dx, centre.y + dy),
            },
            MoveKind::ArcCcw { centre } => MoveKind::ArcCcw {
                centre: Point::new(centre.x + dx, centre.y + dy),
            },
            other => other,
        };
    }
    copy
}

/// The XY step between neighbouring copies.
fn step(sources: &[&Toolpath], spec: &ArrayCopySpec) -> (f64, f64) {
    match spec.spacing {
        ArraySpacing::Offset => (spec.x, spec.y),
        ArraySpacing::Gap => {
            let mut lo = (f64::INFINITY, f64::INFINITY);
            let mut hi = (f64::NEG_INFINITY, f64::NEG_INFINITY);
            for b in sources.iter().filter_map(|t| t.bounds) {
                lo = (lo.0.min(b.min.x), lo.1.min(b.min.y));
                hi = (hi.0.max(b.max.x), hi.1.max(b.max.y));
            }
            let (w, h) = if lo.0.is_finite() {
                (hi.0 - lo.0, hi.1 - lo.1)
            } else {
                (0.0, 0.0)
            };
            (w + spec.x, h + spec.y)
        }
    }
}

/// Most moves an array may hold, over every copy: beyond this the copies
/// would not fit in memory, let alone be cut.
const MAX_MOVES: u64 = 20_000_000;

/// Checks the grid and its spacing, and that `sources` copied that many
/// times stay within [`MAX_MOVES`].
fn check(sources: &[&Toolpath], spec: &ArrayCopySpec) -> Result<(), CoreError> {
    if sources.is_empty() {
        return Err(CoreError::validation(
            "make the toolpaths to copy visible before creating an array copy",
        ));
    }
    let copies = u64::from(spec.rows) * u64::from(spec.columns);
    if copies < 2 {
        return Err(CoreError::validation(
            "an array needs at least two copies (rows times columns)",
        ));
    }
    if !spec.x.is_finite() || !spec.y.is_finite() {
        return Err(CoreError::validation("the array spacing must be a number"));
    }
    let moves: u64 = sources.iter().map(|t| t.moves.len() as u64).sum();
    if moves.saturating_mul(copies) > MAX_MOVES {
        return Err(CoreError::validation(
            "the array is too large to cut; use fewer rows or columns",
        ));
    }
    Ok(())
}

/// One toolpath of copies of `group`, `step` apart. `span` lists every
/// source of the array, whose bounds a Gap spacing is measured from.
fn copy_group(
    group: &[&Toolpath],
    spec: &ArrayCopySpec,
    step: (f64, f64),
    span: &[u64],
) -> Result<Toolpath, CoreError> {
    let (dx, dy) = step;
    let first = group
        .first()
        .ok_or_else(|| CoreError::validation("the array has nothing to copy"))?;
    let mut copies = Vec::new();
    for row in 0..spec.rows {
        for column in 0..spec.columns {
            for source in group {
                copies.push(shifted(source, dx * f64::from(column), dy * f64::from(row)));
            }
        }
    }
    let parts: Vec<&Toolpath> = copies.iter().collect();
    let mut array = merge(&parts, &format!("{} Array", first.name))?;
    // Back home from the last copy, not from where the source's home
    // would have been moved to.
    while array
        .moves
        .last()
        .is_some_and(|m| m.kind == MoveKind::Rapid)
    {
        array.moves.pop();
    }
    let last = array.moves.last().map_or(array.home, |m| m.to);
    let high = array.home.z.max(array.safe_z).max(last.z);
    array
        .moves
        .push(Move::rapid(Point3::new(last.x, last.y, high)));
    array.moves.push(Move::rapid(array.home));
    array.kind = first.kind;
    array.params_json = json!({
        "arrayCopy": {
            "sources": group.iter().map(|t| t.id).collect::<Vec<_>>(),
            "span": span,
            "spec": spec,
        },
    });
    array.recompute_bounds();
    super::estimate::TimeEstimate::default().apply(&mut array);
    Ok(array)
}

/// The copy toolpaths for `sources` (in list order), named after the first
/// source of each: one per source, or one per tool with Minimize Tool
/// Changes. Copies are cut row by row from the bottom left; the first is
/// the source itself. A Gap is measured from the box round every source.
///
/// # Errors
/// Returns [`CoreError::Validation`] for no sources, fewer than one row or
/// column, a grid of one, spacing that is not a number, an array of more
/// than [`MAX_MOVES`] moves, or a source with no moves.
pub fn array_copies(
    sources: &[&Toolpath],
    spec: &ArrayCopySpec,
) -> Result<Vec<Toolpath>, CoreError> {
    check(sources, spec)?;
    let step = step(sources, spec);
    let span: Vec<u64> = sources.iter().map(|t| t.id).collect();
    // Which sources share an output toolpath.
    let mut groups: Vec<Vec<&Toolpath>> = Vec::new();
    for source in sources {
        let joined = spec.minimize_tool_changes
            && groups
                .iter_mut()
                .find(|g| g[0].tool.tool_type == source.tool.tool_type)
                .map(|g| g.push(source))
                .is_some();
        if !joined {
            groups.push(vec![source]);
        }
    }
    groups
        .iter()
        .map(|group| copy_group(group, spec, step, &span))
        .collect()
}

/// One copy toolpath made again from its own sources (`group`), spaced as
/// the whole array is: a Gap is measured from `span`, every source of the
/// array (the group when none is left).
///
/// # Errors
/// As [`array_copies`], and when the sources no longer share a tool (one
/// toolpath cuts with one tool, so the array must be made again).
pub fn array_copy_again(
    group: &[&Toolpath],
    span: &[&Toolpath],
    spec: &ArrayCopySpec,
) -> Result<Toolpath, CoreError> {
    check(group, spec)?;
    if group
        .windows(2)
        .any(|pair| pair[0].tool.tool_type != pair[1].tool.tool_type)
    {
        return Err(CoreError::validation(
            "the toolpaths this array copies no longer share a tool; make the array copy again",
        ));
    }
    let spaced = if span.is_empty() { group } else { span };
    let ids: Vec<u64> = spaced.iter().map(|t| t.id).collect();
    copy_group(group, spec, step(spaced, spec), &ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::{Tool, ToolGeometry};
    use crate::cam::toolpath::StrategyKind;

    fn tool(diameter: f64) -> Tool {
        Tool {
            name: format!("End Mill {diameter}"),
            tool_type: ToolGeometry::EndMill { diameter },
            ..Tool::default()
        }
    }

    fn slot(id: u64, tool: &Tool) -> Toolpath {
        let mut t = Toolpath::new(
            id,
            format!("Slot {id}"),
            tool.clone(),
            StrategyKind::Profile,
        );
        t.home = Point3::new(0.0, 0.0, 20.0);
        t.safe_z = 5.0;
        t.moves = vec![
            Move::rapid(Point3::new(10.0, 10.0, 20.0)),
            Move::rapid(Point3::new(10.0, 10.0, 2.0)),
            Move::plunge(Point3::new(10.0, 10.0, -3.0)),
            Move::feed(Point3::new(30.0, 10.0, -3.0)),
            Move::rapid(Point3::new(30.0, 10.0, 20.0)),
            Move::rapid(Point3::new(0.0, 0.0, 20.0)),
        ];
        t.recompute_bounds();
        t
    }

    fn plunges(t: &Toolpath) -> Vec<(f64, f64)> {
        t.moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .map(|m| (m.to.x, m.to.y))
            .collect()
    }

    #[test]
    fn copies_go_right_and_up_by_the_offset() {
        let mill = tool(6.0);
        let source = slot(1, &mill);
        let spec = ArrayCopySpec {
            rows: 2,
            columns: 3,
            spacing: ArraySpacing::Offset,
            x: 50.0,
            y: 40.0,
            ..ArrayCopySpec::default()
        };
        let out = array_copies(&[&source], &spec).expect("array");
        assert_eq!(out.len(), 1);
        assert_eq!(
            plunges(&out[0]),
            vec![
                (10.0, 10.0),
                (60.0, 10.0),
                (110.0, 10.0),
                (10.0, 50.0),
                (60.0, 50.0),
                (110.0, 50.0)
            ]
        );
        assert_eq!(out[0].moves.last().expect("home").to, source.home);
        assert_eq!(out[0].params_json["arrayCopy"]["sources"], json!([1]));
    }

    #[test]
    fn a_gap_is_measured_between_the_bounding_boxes() {
        let mill = tool(6.0);
        let source = slot(1, &mill);
        let width = source.bounds.map(|b| b.max.x - b.min.x).expect("bounds");
        let spec = ArrayCopySpec {
            rows: 1,
            columns: 2,
            spacing: ArraySpacing::Gap,
            x: 5.0,
            ..ArrayCopySpec::default()
        };
        let out = array_copies(&[&source], &spec).expect("array");
        let xs: Vec<f64> = plunges(&out[0]).iter().map(|p| p.0).collect();
        assert!((xs[1] - xs[0] - (width + 5.0)).abs() < 1e-9, "{xs:?}");
    }

    #[test]
    fn minimize_tool_changes_joins_the_sources_that_share_a_tool() {
        let (six, three) = (tool(6.0), tool(3.0));
        let (a, b, c) = (slot(1, &six), slot(2, &three), slot(3, &six));
        let spec = ArrayCopySpec {
            rows: 1,
            columns: 2,
            ..ArrayCopySpec::default()
        };
        let joined = array_copies(&[&a, &b, &c], &spec).expect("array");
        assert_eq!(joined.len(), 2);
        assert_eq!(joined[0].params_json["arrayCopy"]["sources"], json!([1, 3]));
        let apart = array_copies(
            &[&a, &b, &c],
            &ArrayCopySpec {
                minimize_tool_changes: false,
                ..spec
            },
        )
        .expect("array");
        assert_eq!(apart.len(), 3);
    }

    #[test]
    fn a_grid_of_one_or_no_sources_is_refused() {
        let mill = tool(6.0);
        let source = slot(1, &mill);
        let one = ArrayCopySpec {
            rows: 1,
            columns: 1,
            ..ArrayCopySpec::default()
        };
        assert!(array_copies(&[&source], &one).is_err());
        assert!(array_copies(&[], &ArrayCopySpec::default()).is_err());
    }
}
