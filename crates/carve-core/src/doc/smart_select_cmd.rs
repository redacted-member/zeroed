//! Smart part selection: from the right click menu, select every vector
//! that lies wholly inside the selected closed boundary vectors, so a part
//! (its outline and everything drawn inside it) is picked in one go without
//! grouping it first.
//!
//! Command:
//! - `selection.inside {ids?}` takes the boundaries given, or the selection,
//!   and selects them together with every pickable entity whose vectors all
//!   lie inside one of them. Answers `{ids, added, revision}`.

use serde::Deserialize;
use serde_json::json;

use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::{path_bbox, path_in_path, path_is_closed, BBox, Path, Tolerance};
use crate::kernel::KernelReply;

/// Distance the containment test flattens curves to, in millimetres.
const CONTAIN_TOLERANCE_MM: f64 = 0.01;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    name == "selection.inside"
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InsideArgs {
    #[serde(default)]
    ids: Option<Vec<u64>>,
}

/// Runs the smart selection command.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for another name, [`CoreError::InvalidArgs`]
/// when the arguments do not parse and [`CoreError::Validation`] when no
/// closed boundary is selected.
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    if !handles(name) {
        return Err(CoreError::UnknownCommand(name.to_string()));
    }
    let trimmed = args.trim();
    let args: InsideArgs = if trimmed.is_empty() {
        InsideArgs::default()
    } else {
        serde_json::from_str(trimmed).map_err(|error| CoreError::invalid_args(name, error))?
    };
    let boundaries = document.target_ids(args.ids.as_deref());
    let added = entities_inside(document, &boundaries)?;
    let mut ids = boundaries;
    ids.extend(added.iter().copied());
    document.set_selection(&ids);
    document.bump();
    KernelReply::ok(&json!({
        "ids": document.selection(),
        "added": added,
        "revision": document.revision(),
    }))
}

/// One closed boundary and its box.
struct Boundary {
    id: u64,
    path: Path,
    bbox: BBox,
}

fn merged_path(document: &Document, id: u64) -> Path {
    let mut merged = Path::new();
    for (_, path) in document.paths_of(id) {
        merged.subpaths.extend(path.subpaths);
    }
    merged
}

/// Pickable entities, in z-order, whose vectors all lie inside one of the
/// closed `boundaries`. The boundaries themselves are left out.
///
/// # Errors
/// [`CoreError::Validation`] when none of `boundaries` is a closed vector.
pub fn entities_inside(document: &Document, boundaries: &[u64]) -> Result<Vec<u64>, CoreError> {
    let tol = Tolerance::new(CONTAIN_TOLERANCE_MM)?;
    let closed: Vec<Boundary> = boundaries
        .iter()
        .filter_map(|id| {
            let path = merged_path(document, *id);
            if path.subpaths.is_empty() || !path_is_closed(&path) {
                return None;
            }
            path_bbox(&path).map(|bbox| Boundary {
                id: *id,
                path,
                bbox,
            })
        })
        .collect();
    if closed.is_empty() {
        return Err(CoreError::validation(
            "select a closed vector to pick the vectors inside it",
        ));
    }
    let mut inside = Vec::new();
    for id in document.selectable() {
        if boundaries.contains(&id) {
            continue;
        }
        let path = merged_path(document, id);
        let Some(bbox) = path_bbox(&path) else {
            continue;
        };
        let within = closed.iter().any(|boundary| {
            boundary.id != id
                && boundary.bbox.contains_bbox(bbox)
                && path_in_path(&path, &boundary.path, tol)
        });
        if within {
            inside.push(id);
        }
    }
    Ok(inside)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::entity::{Entity, EntityKind};
    use crate::geom::{circle, rect, Point};

    fn add(doc: &mut Document, path: Path) -> u64 {
        let entity = Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path },
        };
        doc.insert_entity(entity)
    }

    /// A 100 mm square part with a hole and a slot inside, a vector crossing
    /// its edge and one outside it.
    fn part() -> (Document, [u64; 5]) {
        let mut doc = Document::new();
        doc.begin("Create");
        let outline = add(
            &mut doc,
            rect(Point::new(0.0, 0.0), Point::new(100.0, 100.0)).expect("rect"),
        );
        let hole = add(
            &mut doc,
            circle(Point::new(30.0, 30.0), 10.0).expect("circle"),
        );
        let slot = add(
            &mut doc,
            rect(Point::new(60.0, 20.0), Point::new(80.0, 80.0)).expect("rect"),
        );
        let crossing = add(
            &mut doc,
            rect(Point::new(90.0, 40.0), Point::new(120.0, 60.0)).expect("rect"),
        );
        let outside = add(
            &mut doc,
            circle(Point::new(200.0, 50.0), 5.0).expect("circle"),
        );
        doc.commit();
        (doc, [outline, hole, slot, crossing, outside])
    }

    #[test]
    fn selects_what_lies_inside_the_boundary() {
        let (doc, [outline, hole, slot, _, _]) = part();
        let inside = entities_inside(&doc, &[outline]).expect("inside");
        assert_eq!(inside, vec![hole, slot]);
    }

    #[test]
    fn the_command_keeps_the_boundary_selected() {
        let (mut doc, [outline, hole, slot, _, _]) = part();
        doc.set_selection(&[outline]);
        dispatch(&mut doc, "selection.inside", "").expect("reply");
        assert_eq!(doc.selection(), &[outline, hole, slot]);
    }

    #[test]
    fn several_boundaries_each_collect_their_inside() {
        let (doc, [_, hole, slot, _, _]) = part();
        let inside = entities_inside(&doc, &[slot, hole]).expect("inside");
        assert!(inside.is_empty());
    }

    #[test]
    fn an_open_boundary_is_refused() {
        let mut doc = Document::new();
        doc.begin("Create");
        let mut open = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        for sub in &mut open.subpaths {
            sub.closed = false;
        }
        let id = add(&mut doc, open);
        doc.commit();
        let error = entities_inside(&doc, &[id]).expect_err("open");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }
}
