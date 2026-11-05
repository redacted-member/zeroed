//! Toolpath settings from the preferences, and the filter that thins a
//! calculated toolpath to its tolerance.
//!
//! Every toolpath is filtered once it is calculated: runs of straight cutting
//! moves whose middle points lie within the tolerance of the line through
//! their neighbours are merged, which makes the saved file smaller without
//! changing the cut by more than the tolerance.

use super::toolpath::{Move, MoveKind, Point3};

/// Default of [`CamSettings::geometry_fix_seconds`].
pub const DEFAULT_GEOMETRY_FIX_SECONDS: u32 = 10;

/// The toolpath preferences the calculation reads. Session state set from
/// the preferences, not saved with a job.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CamSettings {
    /// Filter tolerance of 2D toolpaths, in millimetres.
    pub tolerance_2d: f64,
    /// Filter tolerance of 3D and projected toolpaths, in millimetres.
    pub tolerance_3d: f64,
    /// Filter tolerance of V-groove toolpaths, in millimetres.
    pub tolerance_vgroove: f64,
    /// Projecting a toolpath onto the model drops the whole tool onto the
    /// surface instead of moving its tip to the surface height.
    pub drop_tool: bool,
    /// Raster clearing cuts each separate strip of the area back and forth
    /// before moving to the next, so the cutting direction stays the same on
    /// complex shapes.
    pub new_raster: bool,
    /// Seconds a calculation may spend repairing vectors that cross
    /// themselves before it machines the rest as drawn; zero repairs none.
    pub geometry_fix_seconds: u32,
    /// After the job size changes, every toolpath's depth is checked
    /// against the material, not only the visible toolpaths'.
    pub check_all_depths: bool,
    /// A duplicated toolpath goes to the end of the list instead of
    /// straight after the toolpath it copies.
    pub append_duplicated: bool,
    /// New toolpaths go to the end of the toolpath group selected in the
    /// list instead of the end of the list.
    pub append_to_selected_group: bool,
}

impl Default for CamSettings {
    fn default() -> Self {
        CamSettings {
            tolerance_2d: 0.025,
            tolerance_3d: 0.01,
            tolerance_vgroove: 0.025,
            drop_tool: false,
            new_raster: true,
            geometry_fix_seconds: DEFAULT_GEOMETRY_FIX_SECONDS,
            check_all_depths: false,
            append_duplicated: false,
            append_to_selected_group: true,
        }
    }
}

/// `moves`, starting from `start`, with every straight feed move that lies
/// within `tolerance` millimetres of the line joining its neighbours merged
/// into them. Only runs of feed moves with the same laser power are thinned;
/// rapids, plunges, arcs and dwells stay as they are and end a run.
pub fn filter_moves(moves: &[Move], start: Point3, tolerance: f64) -> Vec<Move> {
    if !(tolerance.is_finite() && tolerance > 0.0) || moves.len() < 2 {
        return moves.to_vec();
    }
    let mut out = Vec::with_capacity(moves.len());
    let mut from = start;
    let mut i = 0;
    while i < moves.len() {
        if moves[i].kind != MoveKind::Feed {
            out.push(moves[i]);
            from = moves[i].to;
            i += 1;
            continue;
        }
        let power = moves[i].power;
        let mut end = i;
        while end + 1 < moves.len()
            && moves[end + 1].kind == MoveKind::Feed
            && moves[end + 1].power == power
        {
            end += 1;
        }
        let mut points = Vec::with_capacity(end - i + 2);
        points.push(from);
        points.extend(moves[i..=end].iter().map(|m| m.to));
        let mut keep = vec![false; points.len()];
        keep[0] = true;
        keep[points.len() - 1] = true;
        simplify(&points, 0, points.len() - 1, tolerance, &mut keep);
        for (k, kept) in keep.iter().enumerate().skip(1) {
            if *kept {
                out.push(moves[i + k - 1]);
            }
        }
        from = moves[end].to;
        i = end + 1;
    }
    out
}

/// Douglas-Peucker: marks the points between `a` and `b` that must stay for
/// the polyline to keep within `tolerance` of the original.
fn simplify(points: &[Point3], a: usize, b: usize, tolerance: f64, keep: &mut [bool]) {
    let mut stack = vec![(a, b)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let mut worst = 0.0;
        let mut at = a;
        for k in a + 1..b {
            let d = distance_to_segment(points[k], points[a], points[b]);
            if d > worst {
                worst = d;
                at = k;
            }
        }
        if worst > tolerance {
            keep[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
}

fn distance_to_segment(p: Point3, a: Point3, b: Point3) -> f64 {
    let (dx, dy, dz) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let len2 = dx * dx + dy * dy + dz * dz;
    let t = if len2 <= 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy + (p.z - a.z) * dz) / len2).clamp(0.0, 1.0)
    };
    let (qx, qy, qz) = (a.x + dx * t, a.y + dy * t, a.z + dz * t);
    ((p.x - qx).powi(2) + (p.y - qy).powi(2) + (p.z - qz).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn collinear_feeds_merge_into_one_move() {
        let moves: Vec<Move> = (1..=10)
            .map(|i| Move::feed(p(f64::from(i), 0.0, -1.0)))
            .collect();
        let out = filter_moves(&moves, p(0.0, 0.0, -1.0), 0.025);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to, p(10.0, 0.0, -1.0));
    }

    #[test]
    fn a_bend_beyond_the_tolerance_stays() {
        let moves = vec![
            Move::feed(p(5.0, 0.0, 0.0)),
            Move::feed(p(5.0, 5.0, 0.0)),
            Move::feed(p(5.0, 10.0, 0.0)),
        ];
        let out = filter_moves(&moves, p(0.0, 0.0, 0.0), 0.025);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].to, p(5.0, 0.0, 0.0));
    }

    #[test]
    fn small_wobbles_go_within_the_tolerance_and_other_moves_stay() {
        let moves = vec![
            Move::rapid(p(0.0, 0.0, 5.0)),
            Move::plunge(p(0.0, 0.0, -1.0)),
            Move::feed(p(1.0, 0.01, -1.0)),
            Move::feed(p(2.0, -0.01, -1.0)),
            Move::feed(p(3.0, 0.0, -1.0)),
            Move::rapid(p(3.0, 0.0, 5.0)),
        ];
        let loose = filter_moves(&moves, p(0.0, 0.0, 10.0), 0.025);
        assert_eq!(loose.len(), 4);
        let tight = filter_moves(&moves, p(0.0, 0.0, 10.0), 0.001);
        assert_eq!(tight.len(), 6);
    }

    #[test]
    fn laser_power_changes_end_a_run() {
        let moves = vec![
            Move::feed(p(1.0, 0.0, 0.0)).with_power(10.0),
            Move::feed(p(2.0, 0.0, 0.0)).with_power(50.0),
            Move::feed(p(3.0, 0.0, 0.0)).with_power(50.0),
        ];
        let out = filter_moves(&moves, p(0.0, 0.0, 0.0), 0.025);
        assert_eq!(out.len(), 2);
    }
}
