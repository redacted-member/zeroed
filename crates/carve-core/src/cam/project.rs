//! "Project toolpath onto 3D model": a flat toolpath calculated against the
//! material top is dropped onto the relief model, so that every cutting
//! depth is measured down from the model surface under the tool tip.

use crate::geom::Point;

use super::sim::Footprint;
use super::surface::Surface;
use super::toolpath::{Move, MoveKind, Point3};

/// Longest straight span a projected cut is split into, in millimetres, so
/// that it follows the surface between the ends of a long move.
pub const PROJECT_STEP: f64 = 0.5;

/// Drops `moves` (starting from `start`) onto `surface`. Positions at or
/// above `safe` are left alone; lower ones sink by how far the model surface
/// under them is below `top`, the material top the moves were calculated
/// against. Cuts are split into spans of at most `step` millimetres and arcs
/// become straight spans. Each piece keeps its move's laser power.
pub fn project_moves(
    moves: &[Move],
    start: Point3,
    surface: &Surface,
    top: f64,
    safe: f64,
    step: f64,
) -> Vec<Move> {
    project_moves_with(moves, start, surface, None, top, safe, step)
}

/// [`project_moves`], and with a `tool` footprint the whole tool is dropped
/// onto the surface (the "Drop Tool" preference): each position sinks only
/// as far as the tool can go without its sides cutting into the model,
/// rather than as far as the surface under the tip.
pub fn project_moves_with(
    moves: &[Move],
    start: Point3,
    surface: &Surface,
    tool: Option<&Footprint>,
    top: f64,
    safe: f64,
    step: f64,
) -> Vec<Move> {
    let step = step.max(0.01);
    let drop = |p: Point3| -> Point3 {
        if p.z >= safe - 1e-9 {
            p
        } else {
            let under = match tool {
                Some(footprint) => surface.drop_cutter(footprint, p.xy()),
                None => surface.z_at(p.xy()),
            };
            let sink = (top - under).max(0.0);
            Point3::new(p.x, p.y, p.z - sink)
        }
    };
    let mut out = Vec::with_capacity(moves.len());
    let mut from = start;
    for one in moves {
        let keep_power = |m: Move| match one.power {
            Some(power) => m.with_power(power),
            None => m,
        };
        match one.kind {
            MoveKind::Rapid => out.push(Move::rapid(drop(one.to))),
            MoveKind::Dwell { seconds } => out.push(Move::dwell(seconds, drop(one.to))),
            MoveKind::Feed | MoveKind::Plunge => {
                let pieces = (from.xy().distance(one.to.xy()) / step).ceil().max(1.0) as usize;
                for k in 1..=pieces {
                    let t = k as f64 / pieces as f64;
                    let p = Point3::new(
                        from.x + (one.to.x - from.x) * t,
                        from.y + (one.to.y - from.y) * t,
                        from.z + (one.to.z - from.z) * t,
                    );
                    let m = if one.kind == MoveKind::Plunge {
                        Move::plunge(drop(p))
                    } else {
                        Move::feed(drop(p))
                    };
                    out.push(keep_power(m));
                }
            }
            MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } => {
                let sweep = one.arc_sweep(from).unwrap_or(0.0);
                let radius = centre.distance(from.xy());
                let start_angle = centre.to(from.xy()).angle();
                let pieces = (sweep.abs() * radius / step).ceil().max(1.0) as usize;
                for k in 1..=pieces {
                    let t = k as f64 / pieces as f64;
                    let xy = if k == pieces {
                        one.to.xy()
                    } else {
                        let a = start_angle + sweep * t;
                        Point::new(centre.x + radius * a.cos(), centre.y + radius * a.sin())
                    };
                    let z = from.z + (one.to.z - from.z) * t;
                    out.push(keep_power(Move::feed(drop(Point3::new(xy.x, xy.y, z)))));
                }
            }
        }
        from = one.to;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::JobSetup;
    use crate::model::field::Field;

    /// A 100 mm square model grid rising 1 mm per 10 mm along X.
    fn ramp(job: &JobSetup) -> Surface {
        let (w, h, cell) = (101, 101, 1.0);
        let z = (0..h)
            .flat_map(|_| (0..w).map(|i| i as f32 / 10.0))
            .collect::<Vec<f32>>();
        let field = Field {
            width: w,
            height: h,
            cell,
            origin: Point::new(0.0, 0.0),
            z,
        };
        Surface::from_composite(&field, job)
    }

    #[test]
    fn cuts_follow_the_model_and_rapids_at_safe_height_stay() {
        let job = JobSetup::default();
        let top = job.surface_z();
        let surface = ramp(&job);
        let safe = top + 5.0;
        let moves = vec![
            Move::rapid(Point3::new(0.0, 50.0, safe)),
            Move::plunge(Point3::new(0.0, 50.0, top - 1.0)),
            Move::feed(Point3::new(100.0, 50.0, top - 1.0)).with_power(40.0),
            Move::rapid(Point3::new(100.0, 50.0, safe)),
        ];
        let out = project_moves(
            &moves,
            Point3::new(0.0, 0.0, safe),
            &surface,
            top,
            safe,
            0.5,
        );
        assert_eq!(out[0], moves[0]);
        assert_eq!(*out.last().expect("moves"), moves[3]);
        // The model peaks at the material top at X 100 and is 10 mm down at X 0.
        let plunged = out
            .iter()
            .find(|m| m.kind == MoveKind::Plunge)
            .expect("a plunge");
        assert!((plunged.to.z - (top - 11.0)).abs() < 1e-6, "{plunged:?}");
        let cuts: Vec<&Move> = out.iter().filter(|m| m.kind == MoveKind::Feed).collect();
        assert_eq!(cuts.len(), 200, "100 mm in half millimetre spans");
        for m in &cuts {
            let expected = top - 1.0 - (10.0 - m.to.x / 10.0);
            assert!((m.to.z - expected).abs() < 1e-6, "{m:?}");
            assert_eq!(m.power, Some(40.0));
        }
    }

    #[test]
    fn arcs_become_straight_spans_on_the_surface() {
        let job = JobSetup::default();
        let top = job.surface_z();
        let surface = ramp(&job);
        let from = Point3::new(60.0, 50.0, top - 0.5);
        let moves = vec![Move::arc_ccw(
            Point::new(50.0, 50.0),
            Point3::new(40.0, 50.0, top - 0.5),
        )];
        let out = project_moves(&moves, from, &surface, top, top + 5.0, 0.5);
        assert!(
            out.len() >= 60,
            "half a 10 mm radius circle in 0.5 mm spans"
        );
        assert!(out.iter().all(|m| m.kind == MoveKind::Feed));
        let last = out.last().expect("moves").to;
        assert!((last.x - 40.0).abs() < 1e-9 && (last.y - 50.0).abs() < 1e-9);
        assert!((last.z - (top - 0.5 - 6.0)).abs() < 1e-6, "{last:?}");
        let mid = out[out.len() / 2].to;
        assert!(mid.y > 55.0, "the arc bulges upwards: {mid:?}");
    }

    #[test]
    fn a_dropped_tool_rides_on_the_rising_side_instead_of_its_tip_height() {
        let job = JobSetup::default();
        let top = job.surface_z();
        let surface = ramp(&job);
        let safe = top + 5.0;
        let moves = vec![Move::feed(Point3::new(50.0, 50.0, top - 1.0))];
        let start = Point3::new(50.0, 50.0, top - 1.0);
        let tip = project_moves(&moves, start, &surface, top, safe, 0.5);
        let tool = crate::cam::Tool {
            tool_type: crate::cam::ToolGeometry::EndMill { diameter: 10.0 },
            ..crate::cam::Tool::default()
        };
        let footprint = Footprint::of(&tool, 1.0).expect("footprint");
        let dropped = project_moves_with(&moves, start, &surface, Some(&footprint), top, safe, 0.5);
        let (a, b) = (
            tip.last().expect("move").to.z,
            dropped.last().expect("move").to.z,
        );
        // A flat 10 mm end mill at X 50 rests on the model at X 55, half a
        // millimetre higher than the surface under its centre.
        assert!((b - a - 0.5).abs() < 1e-3, "tip {a} dropped {b}");
    }
}
