//! Geometry fixing before a toolpath is calculated: a closed vector that
//! crosses itself (a figure of eight, a loop drawn over itself) has no clear
//! inside, so area strategies would offset it into nonsense. Each one is
//! replaced by the outlines of the area it encloses. The work is bounded by
//! the Toolpath Geometry Fixing Timeout preference: once the time is spent,
//! the vectors not yet looked at are machined as drawn.

use std::time::{Duration, Instant};

use crate::geom::{subpath_self_intersects_within, weld, Path, Tolerance};
use crate::warning::Warning;

/// The vectors after fixing, and what happened.
#[derive(Debug, Clone, PartialEq)]
pub struct Fixed {
    /// The vectors to machine, in the order given.
    pub paths: Vec<Path>,
    /// How many closed shapes crossed themselves and were repaired.
    pub repaired: usize,
    /// True when the time ran out before every shape was looked at.
    pub timed_out: bool,
}

/// Repairs the closed shapes of `paths` that cross themselves, spending at
/// most `limit` on it; a zero limit repairs nothing. Open shapes and shapes
/// that do not cross themselves are kept exactly as they are.
pub fn fix_crossings(paths: Vec<Path>, limit: Duration, tol: Tolerance) -> Fixed {
    fix_crossings_until(paths, limit, tol, Instant::now)
}

/// [`fix_crossings`] with the clock given, so tests can run out of time.
fn fix_crossings_until<C>(paths: Vec<Path>, limit: Duration, tol: Tolerance, now: C) -> Fixed
where
    C: Fn() -> Instant,
{
    if limit.is_zero() {
        return Fixed {
            paths,
            repaired: 0,
            timed_out: false,
        };
    }
    let started = now();
    let mut repaired = 0;
    let mut timed_out = false;
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        if timed_out {
            out.push(path);
            continue;
        }
        let mut fixed = Path::new();
        for sub in path.subpaths {
            if timed_out || now().duration_since(started) >= limit {
                timed_out = true;
                fixed.push(sub);
                continue;
            }
            if !sub.closed {
                fixed.push(sub);
                continue;
            }
            let mut out_of_time = || now().duration_since(started) >= limit;
            match subpath_self_intersects_within(&sub, tol, &mut out_of_time) {
                Some(true) => {}
                Some(false) => {
                    fixed.push(sub);
                    continue;
                }
                None => {
                    timed_out = true;
                    fixed.push(sub);
                    continue;
                }
            }
            match weld(&[sub.clone().into_path()], tol) {
                Ok(parts) if !parts.is_empty() => {
                    for part in parts {
                        for piece in part.subpaths {
                            fixed.push(piece);
                        }
                    }
                    repaired += 1;
                }
                _ => {
                    fixed.push(sub);
                }
            }
        }
        out.push(fixed);
    }
    Fixed {
        paths: out,
        repaired,
        timed_out,
    }
}

/// Messages for the user about a fix, if there is anything to say.
pub fn fix_warnings(fixed: &Fixed, limit: Duration) -> Vec<Warning> {
    let mut warnings = Vec::new();
    if fixed.repaired > 0 {
        warnings.push(Warning::vectors_repaired(fixed.repaired));
    }
    if fixed.timed_out {
        warnings.push(Warning::fixing_timed_out(limit.as_secs()));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::geom::subpath_self_intersects;
    use crate::geom::{Point, Segment, SubPath};

    fn tol() -> Tolerance {
        Tolerance::new(0.01).expect("tolerance")
    }

    fn polygon(points: &[(f64, f64)]) -> Path {
        let start = Point::new(points[0].0, points[0].1);
        let segments = points[1..]
            .iter()
            .map(|p| Segment::Line {
                to: Point::new(p.0, p.1),
            })
            .collect();
        SubPath::new(start, segments, true).into_path()
    }

    fn bowtie() -> Path {
        polygon(&[(0.0, 0.0), (10.0, 10.0), (10.0, 0.0), (0.0, 10.0)])
    }

    fn square() -> Path {
        polygon(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)])
    }

    #[test]
    fn a_shape_that_crosses_itself_becomes_simple_loops() {
        let fixed = fix_crossings(vec![bowtie(), square()], Duration::from_secs(10), tol());
        assert_eq!(fixed.repaired, 1);
        assert!(!fixed.timed_out);
        assert_eq!(fixed.paths.len(), 2);
        let repaired = &fixed.paths[0];
        assert_eq!(repaired.subpaths.len(), 2, "one loop per lobe");
        assert!(repaired
            .subpaths
            .iter()
            .all(|s| s.closed && !subpath_self_intersects(s, tol())));
        assert_eq!(fixed.paths[1], square(), "a simple shape is untouched");
        let words = fix_warnings(&fixed, Duration::from_secs(10));
        assert_eq!(words.len(), 1);
        assert!(
            words[0]
                .message
                .starts_with("1 vector crossed itself and was repaired"),
            "{words:?}"
        );
    }

    #[test]
    fn a_zero_timeout_machines_everything_as_drawn() {
        let fixed = fix_crossings(vec![bowtie()], Duration::ZERO, tol());
        assert_eq!(fixed.paths, vec![bowtie()]);
        assert_eq!(fixed.repaired, 0);
        assert!(fix_warnings(&fixed, Duration::ZERO).is_empty());
    }

    #[test]
    fn running_out_of_time_leaves_the_rest_as_drawn() {
        // A clock that moves 4 s on every reading: the first shape is
        // repaired, the second is reached after the 5 s limit.
        let base = Instant::now();
        let ticks = Cell::new(0u64);
        let clock = || {
            let t = ticks.get();
            ticks.set(t + 1);
            base + Duration::from_secs(4 * t)
        };
        let limit = Duration::from_secs(5);
        let fixed = fix_crossings_until(vec![bowtie(), bowtie()], limit, tol(), clock);
        assert_eq!(fixed.repaired, 1);
        assert!(fixed.timed_out);
        assert_eq!(fixed.paths[1], bowtie());
        let words = fix_warnings(&fixed, limit);
        assert!(words[1].message.contains("stopped after 5 s"), "{words:?}");
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use crate::geom::{Point, Segment, SubPath};

    /// A simple closed vector with many nodes, like a traced bitmap outline.
    fn dense_ring(nodes: usize) -> Path {
        let at = |i: usize| {
            let a = std::f64::consts::TAU * i as f64 / nodes as f64;
            let r = 100.0 + 5.0 * (a * 37.0).sin();
            Point::new(r * a.cos(), r * a.sin())
        };
        let segments = (1..nodes).map(|i| Segment::Line { to: at(i) }).collect();
        SubPath::new(at(0), segments, true).into_path()
    }

    #[test]
    fn a_dense_vector_is_checked_within_the_time_limit() {
        let tol = Tolerance::new(0.01).expect("tolerance");
        let limit = Duration::from_secs(1);
        let started = Instant::now();
        let fixed = fix_crossings(vec![dense_ring(20_000)], limit, tol);
        let spent = started.elapsed();
        assert_eq!(fixed.repaired, 0);
        assert!(
            spent < Duration::from_secs(3),
            "took {spent:?} with a 1 s limit"
        );
    }

    #[test]
    fn the_time_limit_holds_inside_one_long_vector() {
        // A clock that moves 1 s on every reading: the check of the one
        // dense vector asks it often enough to stop at the 5 s limit.
        let base = Instant::now();
        let ticks = std::cell::Cell::new(0u64);
        let clock = || {
            let t = ticks.get();
            ticks.set(t + 1);
            base + Duration::from_secs(t)
        };
        let tol = Tolerance::new(0.01).expect("tolerance");
        let ring = dense_ring(20_000);
        let fixed = fix_crossings_until(vec![ring.clone()], Duration::from_secs(5), tol, clock);
        assert!(fixed.timed_out);
        assert_eq!(fixed.paths, vec![ring]);
        assert!(ticks.get() < 20, "asked the clock {} times", ticks.get());
    }

    #[test]
    fn the_sweep_finds_the_same_crossings_as_every_pair() {
        let tol = Tolerance::new(0.01).expect("tolerance");
        let mut crossed = dense_ring(400);
        // Pull one node across the ring so its two segments cross others.
        if let Some(Segment::Line { to }) = crossed.subpaths[0].segments.get_mut(100) {
            *to = Point::new(-to.x, -to.y);
        }
        assert!(
            subpath_self_intersects_within(&crossed.subpaths[0], tol, &mut || false)
                .expect("no time limit")
        );
        assert_eq!(
            subpath_self_intersects_within(&dense_ring(400).subpaths[0], tol, &mut || false),
            Some(false)
        );
    }
}
