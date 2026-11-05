//! Keep-out zones: closed vectors on layers marked keep-out stand for clamps
//! and fixtures. A calculated toolpath whose tool comes below the clearance
//! height over one of them gets a warning.

use crate::doc::Document;
use crate::geom::{Path, Point, Region, Tolerance};

use super::toolpath::{Move, MoveKind, Point3};

/// Distance between the points a move is checked at, in millimetres.
const STEP: f64 = 0.5;

/// A keep-out area: the layer's name and its closed vectors.
#[derive(Debug, Clone)]
pub struct KeepOut {
    /// Layer name, for the warning.
    pub name: String,
    /// The area the tool must stay out of.
    pub region: Region,
}

/// The keep-out areas of the active sheet: one per keep-out layer that has
/// closed vectors on it. Hidden layers still count.
pub fn keep_outs(doc: &Document, tol: Tolerance) -> Vec<KeepOut> {
    let sheet = doc.active_sheet();
    doc.layers()
        .iter()
        .filter(|layer| layer.keep_out)
        .filter_map(|layer| {
            let paths: Vec<Path> = doc
                .order()
                .iter()
                .filter_map(|id| doc.entity(*id).ok())
                .filter(|e| e.layer == layer.id && e.sheet == sheet)
                .filter_map(|e| e.path())
                .collect();
            let region = Region::from_paths(&paths, tol);
            (!region.is_empty()).then(|| KeepOut {
                name: layer.name.clone(),
                region,
            })
        })
        .collect()
}

/// Points along a move from `from`, every [`STEP`] millimetres, arcs
/// followed round their sweep.
fn samples(from: Point3, one: &Move) -> Vec<Point3> {
    match one.kind {
        MoveKind::Dwell { .. } => vec![one.to],
        MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } => {
            let sweep = one.arc_sweep(from).unwrap_or(0.0);
            let radius = centre.distance(from.xy());
            let start = centre.to(from.xy()).angle();
            let n = (sweep.abs() * radius / STEP).ceil().max(1.0) as usize;
            (1..=n)
                .map(|k| {
                    let t = k as f64 / n as f64;
                    let a = start + sweep * t;
                    Point3::new(
                        centre.x + radius * a.cos(),
                        centre.y + radius * a.sin(),
                        from.z + (one.to.z - from.z) * t,
                    )
                })
                .collect()
        }
        _ => {
            let n = (from.xy().distance(one.to.xy()) / STEP).ceil().max(1.0) as usize;
            (1..=n)
                .map(|k| {
                    let t = k as f64 / n as f64;
                    Point3::new(
                        from.x + (one.to.x - from.x) * t,
                        from.y + (one.to.y - from.y) * t,
                        from.z + (one.to.z - from.z) * t,
                    )
                })
                .collect()
        }
    }
}

/// The names of the keep-out areas the tool enters: anywhere the tool is
/// below `safe` with its edge (`radius` from the centre) over the area.
pub fn zones_entered(
    moves: &[Move],
    start: Point3,
    safe: f64,
    radius: f64,
    zones: &[KeepOut],
    tol: Tolerance,
) -> Vec<String> {
    let grown: Vec<(&str, Region)> = zones
        .iter()
        .map(|z| {
            let region = if radius > 0.0 {
                z.region.offset(-radius, tol)
            } else {
                z.region.clone()
            };
            (z.name.as_str(), region)
        })
        .collect();
    let mut hit = vec![false; grown.len()];
    let mut from = start;
    for one in moves {
        for p in samples(from, one) {
            if p.z >= safe - 1e-6 {
                continue;
            }
            for (i, (_, region)) in grown.iter().enumerate() {
                if !hit[i] && region.contains(Point::new(p.x, p.y), tol) {
                    hit[i] = true;
                }
            }
        }
        from = one.to;
    }
    grown
        .iter()
        .zip(hit)
        .filter(|(_, h)| *h)
        .map(|((name, _), _)| (*name).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::rect;

    fn zone() -> Vec<KeepOut> {
        let path = rect(Point::new(40.0, 0.0), Point::new(60.0, 20.0)).expect("rect");
        let tol = Tolerance::new(0.01).expect("tol");
        vec![KeepOut {
            name: "Clamps".to_string(),
            region: Region::from_paths(&[path], tol),
        }]
    }

    #[test]
    fn a_cut_through_a_zone_is_reported_and_a_rapid_above_it_is_not() {
        let tol = Tolerance::new(0.01).expect("tol");
        let start = Point3::new(0.0, 10.0, 10.0);
        let over = vec![Move::rapid(Point3::new(100.0, 10.0, 10.0))];
        assert!(zones_entered(&over, start, 5.0, 3.0, &zone(), tol).is_empty());
        let through = vec![
            Move::plunge(Point3::new(0.0, 10.0, -1.0)),
            Move::feed(Point3::new(100.0, 10.0, -1.0)),
        ];
        assert_eq!(
            zones_entered(&through, start, 5.0, 3.0, &zone(), tol),
            vec!["Clamps".to_string()]
        );
    }

    #[test]
    fn the_tool_radius_counts_and_arcs_are_followed() {
        let tol = Tolerance::new(0.01).expect("tol");
        let start = Point3::new(0.0, 22.0, -1.0);
        // Passes 2 mm above the zone: only a tool wider than 4 mm touches it.
        let beside = vec![Move::feed(Point3::new(100.0, 22.0, -1.0))];
        assert!(zones_entered(&beside, start, 5.0, 1.5, &zone(), tol).is_empty());
        assert_eq!(
            zones_entered(&beside, start, 5.0, 3.0, &zone(), tol).len(),
            1
        );
        // A half circle under (50, 25) dips to y 15, inside the zone, while
        // its chord at y 25 stays clear of it.
        let arc = vec![Move::arc_ccw(
            Point::new(50.0, 25.0),
            Point3::new(60.0, 25.0, -1.0),
        )];
        let from = Point3::new(40.0, 25.0, -1.0);
        assert_eq!(zones_entered(&arc, from, 5.0, 0.0, &zone(), tol).len(), 1);
        let chord = vec![Move::feed(Point3::new(60.0, 25.0, -1.0))];
        assert!(zones_entered(&chord, from, 5.0, 0.0, &zone(), tol).is_empty());
    }
}
