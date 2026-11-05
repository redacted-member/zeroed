//! Join Vectors in node editing mode (the J key): an open end node of one
//! vector is joined to the nearest open end of another open vector, or of the
//! same vector, in one undo step.
//!
//! Ends that already meet are merged into one node; ends apart are joined by a
//! straight span. Joining the two ends of the same open span closes it. The
//! other vector's subpath moves into the edited vector; the other vector is
//! removed when nothing else is left in it.

use super::ops::set_path;
use super::{Document, EntityKind};
use crate::error::CoreError;
use crate::geom::{reverse_subpath, Path, Point, Segment, SubPath, Tolerance};

/// One open end: the entity, its subpath and whether it is the start node.
#[derive(Debug, Clone, Copy, PartialEq)]
struct End {
    id: u64,
    subpath: usize,
    at_start: bool,
    point: Point,
}

fn open_ends(id: u64, path: &Path) -> Vec<End> {
    let mut out = Vec::new();
    for (index, sub) in path.subpaths.iter().enumerate() {
        if sub.closed || sub.segments.is_empty() {
            continue;
        }
        out.push(End {
            id,
            subpath: index,
            at_start: true,
            point: sub.start,
        });
        out.push(End {
            id,
            subpath: index,
            at_start: false,
            point: sub.end(),
        });
    }
    out
}

/// Every open end the edited end may join: the other ends of the same vector
/// and the ends of the other plain vectors on visible, unlocked layers of the
/// same sheet.
fn candidates(doc: &Document, from: End, sheet: u64) -> Vec<End> {
    let mut out = Vec::new();
    for (id, entity) in doc.entities() {
        let EntityKind::Path { path } = &entity.kind else {
            continue;
        };
        if *id != from.id {
            if entity.parent.is_some() || entity.sheet != sheet {
                continue;
            }
            let usable = doc
                .layer(entity.layer)
                .map(|layer| layer.visible && !layer.locked)
                .unwrap_or(false);
            if !usable {
                continue;
            }
        }
        out.extend(
            open_ends(*id, path)
                .into_iter()
                .filter(|end| !(end.id == from.id && end.subpath == from.subpath)),
        );
    }
    out.extend(own_other_end(doc, from));
    out
}

/// The other end of the edited span itself, when closing it makes a shape
/// (a single straight span cannot close on itself).
fn own_other_end(doc: &Document, from: End) -> Option<End> {
    let path = doc.entity(from.id).ok()?.path()?;
    let sub = path.subpaths.get(from.subpath)?;
    let single_line = sub.segments.len() == 1 && matches!(sub.segments[0], Segment::Line { .. });
    if sub.closed || single_line {
        return None;
    }
    Some(End {
        id: from.id,
        subpath: from.subpath,
        at_start: !from.at_start,
        point: if from.at_start { sub.end() } else { sub.start },
    })
}

/// Appends `tail` after `head`, which ends where `tail` starts or is joined to
/// it by a straight span.
fn chain(head: &SubPath, tail: &SubPath, tol: Tolerance) -> SubPath {
    let mut segments = head.segments.clone();
    if !head.end().approx_eq(tail.start, tol) {
        segments.push(Segment::line(tail.start));
    } else if let Some(last) = segments.last_mut() {
        last.set_end(tail.start);
    }
    segments.extend_from_slice(&tail.segments);
    SubPath::new(head.start, segments, false)
}

/// Joins the open end `(subpath, at_start)` of vector `id` to the nearest
/// other open end. Answers the id of the joined vector.
///
/// # Errors
/// [`CoreError::Validation`] when the entity is not a plain vector, the node
/// is not an open end, or there is no other open end to join.
pub fn join_end(
    doc: &mut Document,
    id: u64,
    subpath: usize,
    at_start: bool,
) -> Result<u64, CoreError> {
    let entity = doc.entity(id)?;
    let sheet = entity.sheet;
    let EntityKind::Path { path } = &entity.kind else {
        return Err(CoreError::validation(
            "only vectors can be joined in node editing",
        ));
    };
    let path = path.clone();
    let sub = path
        .subpaths
        .get(subpath)
        .filter(|sub| !sub.closed && !sub.segments.is_empty())
        .ok_or_else(|| CoreError::validation("pick the end node of an open vector to join"))?
        .clone();
    let from = End {
        id,
        subpath,
        at_start,
        point: if at_start { sub.start } else { sub.end() },
    };
    let target = candidates(doc, from, sheet)
        .into_iter()
        .min_by(|a, b| {
            a.point
                .distance_squared(from.point)
                .total_cmp(&b.point.distance_squared(from.point))
        })
        .ok_or_else(|| CoreError::validation("there is no other open vector end to join to"))?;
    let tol = Tolerance::default();

    // The edited subpath, running towards the picked end.
    let head = if at_start {
        reverse_subpath(&sub)
    } else {
        sub.clone()
    };

    if target.id == id && target.subpath == subpath {
        // The two ends of one span: close it.
        let mut closed = head.clone();
        if closed.end().approx_eq(closed.start, tol) {
            if let Some(last) = closed.segments.last_mut() {
                last.set_end(closed.start);
            }
        } else {
            closed.segments.push(Segment::line(closed.start));
        }
        closed.closed = true;
        let mut next = path.clone();
        next.subpaths[subpath] = closed;
        set_path(doc, id, next)?;
        return Ok(id);
    }

    let other_path = if target.id == id {
        path.clone()
    } else {
        doc.entity(target.id)?
            .path()
            .ok_or_else(|| CoreError::validation("the other vector has no path"))?
    };
    let other = other_path
        .subpaths
        .get(target.subpath)
        .ok_or_else(|| CoreError::geometry("no such subpath"))?;
    // The other subpath, running away from its picked end.
    let tail = if target.at_start {
        other.clone()
    } else {
        reverse_subpath(other)
    };
    let joined = chain(&head, &tail, tol);

    let mut next = Path::new();
    for (index, existing) in path.subpaths.iter().enumerate() {
        if index == subpath {
            next.push(joined.clone());
        } else if !(target.id == id && index == target.subpath) {
            next.push(existing.clone());
        }
    }
    set_path(doc, id, next)?;
    if target.id != id {
        let rest: Vec<SubPath> = other_path
            .subpaths
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != target.subpath)
            .map(|(_, sub)| sub.clone())
            .collect();
        if rest.is_empty() {
            doc.delete_entities(&[target.id]);
        } else {
            set_path(doc, target.id, Path::from_subpaths(rest))?;
        }
    }
    doc.set_selection(&[id]);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Entity;

    fn line(a: (f64, f64), b: (f64, f64)) -> Path {
        Path::from_subpaths(vec![SubPath::new(
            Point::new(a.0, a.1),
            vec![Segment::line(Point::new(b.0, b.1))],
            false,
        )])
    }

    fn add(doc: &mut Document, path: Path) -> u64 {
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path },
        })
    }

    fn path(doc: &Document, id: u64) -> Path {
        doc.entity(id).expect("entity").path().expect("path")
    }

    #[test]
    fn touching_ends_merge_and_the_other_vector_goes() {
        let mut doc = Document::new();
        doc.begin("Create");
        let a = add(&mut doc, line((0.0, 0.0), (10.0, 0.0)));
        let b = add(&mut doc, line((10.0, 0.0), (10.0, 10.0)));
        let far = add(&mut doc, line((50.0, 50.0), (60.0, 50.0)));
        doc.commit();
        doc.begin("Join");
        assert_eq!(join_end(&mut doc, a, 0, false).expect("join"), a);
        doc.commit();
        assert!(doc.entity(b).is_err());
        assert!(doc.entity(far).is_ok());
        let joined = path(&doc, a);
        assert_eq!(joined.subpaths.len(), 1);
        let nodes = joined.subpaths[0].nodes();
        assert_eq!(nodes.len(), 3);
        assert!(nodes[2].approx_eq(Point::new(10.0, 10.0), Tolerance::default()));
        doc.undo();
        assert!(doc.entity(b).is_ok());
    }

    #[test]
    fn ends_apart_get_a_straight_span_and_reverse_as_needed() {
        let mut doc = Document::new();
        doc.begin("Create");
        let a = add(&mut doc, line((0.0, 0.0), (10.0, 0.0)));
        // Its end, not its start, is nearest the start of `a`.
        let b = add(&mut doc, line((-20.0, 0.0), (-5.0, 0.0)));
        doc.commit();
        join_end(&mut doc, a, 0, true).expect("join");
        assert!(doc.entity(b).is_err());
        let sub = &path(&doc, a).subpaths[0];
        let nodes = sub.nodes();
        assert_eq!(nodes.len(), 4);
        assert!(nodes[0].approx_eq(Point::new(10.0, 0.0), Tolerance::default()));
        assert!(nodes[3].approx_eq(Point::new(-20.0, 0.0), Tolerance::default()));
        assert!(!sub.closed);
    }

    #[test]
    fn the_two_ends_of_one_span_close_it() {
        let mut doc = Document::new();
        let mut open = SubPath::new(Point::new(0.0, 0.0), Vec::new(), false);
        open.segments.push(Segment::line(Point::new(10.0, 0.0)));
        open.segments.push(Segment::line(Point::new(10.0, 10.0)));
        let a = add(&mut doc, Path::from_subpaths(vec![open]));
        join_end(&mut doc, a, 0, false).expect("join");
        let sub = &path(&doc, a).subpaths[0];
        assert!(sub.closed);
        assert_eq!(sub.segments.len(), 3);
    }

    #[test]
    fn a_closed_vector_or_a_lone_line_is_refused() {
        let mut doc = Document::new();
        let square =
            crate::geom::rect(Point::new(20.0, 0.0), Point::new(30.0, 10.0)).expect("rect");
        let s = add(&mut doc, square);
        assert!(join_end(&mut doc, s, 0, true).is_err());
        // A single straight span cannot close on itself.
        let a = add(&mut doc, line((0.0, 0.0), (10.0, 0.0)));
        assert!(join_end(&mut doc, a, 0, true).is_err());
    }
}
