//! B-spline curves from CAD files turned into paths.
//!
//! A clamped, non-rational spline of degree 1 to 3 is converted exactly:
//! every interior knot is inserted (Boehm's algorithm) until the curve falls
//! apart into Bezier pieces, which become lines, or cubics (a quadratic
//! piece is raised to a cubic). Other splines (rational, unclamped or of a
//! higher degree) are evaluated with de Boor's algorithm and followed with
//! short lines. A spline given only by fit points goes smoothly through
//! them.

use crate::geom::{Point, SubPath};

/// Line pieces per knot span when a spline has to be sampled.
const SAMPLES_PER_SPAN: usize = 16;
/// Most samples taken from one spline.
const MAX_SAMPLES: usize = 20_000;

/// A spline as CAD files store it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spline {
    /// Polynomial degree.
    pub degree: usize,
    /// Knot vector, `control.len() + degree + 1` values when well formed.
    pub knots: Vec<f64>,
    /// Control points.
    pub control: Vec<Point>,
    /// Weights, one per control point; empty or all equal for a
    /// non-rational spline.
    pub weights: Vec<f64>,
    /// Points the curve passes through, used when there are no control
    /// points.
    pub fit: Vec<Point>,
    /// Whether the curve closes on itself.
    pub closed: bool,
}

/// The spline as a subpath, `None` when it has too little to draw.
pub fn subpath(spline: &Spline) -> Option<SubPath> {
    let mut sub = if spline.control.len() >= 2 && valid(spline) {
        if is_rational(spline) || spline.degree > 3 || !clamped(spline) {
            sampled(spline)?
        } else {
            bezier_pieces(spline)?
        }
    } else if spline.fit.len() >= 2 {
        crate::geom::bezier_through_points(&spline.fit)
            .ok()?
            .subpaths
            .into_iter()
            .next()?
    } else if spline.control.len() >= 2 {
        // A broken knot vector: the control polygon is the best guess.
        let mut sub = SubPath::open(spline.control[0]);
        for p in &spline.control[1..] {
            sub.line_to(*p);
        }
        sub
    } else {
        return None;
    };
    if sub.segments.is_empty() {
        return None;
    }
    sub.closed = spline.closed || sub.start.distance(sub.end()) < 1e-9;
    Some(sub)
}

fn valid(s: &Spline) -> bool {
    s.degree >= 1
        && s.knots.len() == s.control.len() + s.degree + 1
        && s.knots.windows(2).all(|w| w[1] >= w[0])
        && s.knots.iter().all(|k| k.is_finite())
        && s.knots[s.degree] < s.knots[s.control.len()]
}

fn is_rational(s: &Spline) -> bool {
    s.weights.len() == s.control.len()
        && s.weights
            .iter()
            .any(|w| (w - s.weights[0]).abs() > 1e-12 || *w <= 0.0)
}

fn clamped(s: &Spline) -> bool {
    let p = s.degree;
    let n = s.knots.len();
    s.knots[..=p].iter().all(|k| *k == s.knots[0])
        && s.knots[n - p - 1..].iter().all(|k| *k == s.knots[n - 1])
}

/// Inserts every interior knot up to the degree, then reads off the Bezier
/// pieces.
fn bezier_pieces(s: &Spline) -> Option<SubPath> {
    let p = s.degree;
    let mut knots = s.knots.clone();
    let mut control = s.control.clone();
    let interior: Vec<f64> = {
        let mut v: Vec<f64> = knots[p + 1..knots.len() - p - 1].to_vec();
        v.dedup();
        v
    };
    for u in interior {
        let have = knots.iter().filter(|k| **k == u).count();
        for _ in have..p {
            insert_knot(&mut knots, &mut control, p, u);
        }
    }
    let mut sub = SubPath::open(control[0]);
    for piece in control.windows(p + 1).step_by(p) {
        match p {
            1 => {
                sub.line_to(piece[1]);
            }
            2 => {
                let (a, c, b) = (piece[0], piece[1], piece[2]);
                sub.cubic_to(a.lerp(c, 2.0 / 3.0), b.lerp(c, 2.0 / 3.0), b);
            }
            _ => {
                sub.cubic_to(piece[1], piece[2], piece[3]);
            }
        }
    }
    Some(sub)
}

/// Boehm's knot insertion of `u` once.
fn insert_knot(knots: &mut Vec<f64>, control: &mut Vec<Point>, p: usize, u: f64) {
    let Some(k) = (p..control.len()).rev().find(|&k| knots[k] <= u) else {
        return;
    };
    let mut fresh = Vec::with_capacity(control.len() + 1);
    fresh.extend_from_slice(&control[..=k - p]);
    for i in k - p + 1..=k {
        let span = knots[i + p] - knots[i];
        let alpha = if span.abs() < 1e-300 {
            0.0
        } else {
            (u - knots[i]) / span
        };
        fresh.push(control[i - 1].lerp(control[i], alpha));
    }
    fresh.extend_from_slice(&control[k..]);
    *control = fresh;
    knots.insert(k + 1, u);
}

/// The point at `u` by de Boor's algorithm, weights included.
fn point_at(s: &Spline, u: f64) -> Point {
    let p = s.degree;
    let n = s.control.len();
    let k = (p..n)
        .rev()
        .find(|&k| s.knots[k] <= u)
        .unwrap_or(p)
        .min(n - 1);
    let weight = |i: usize| {
        if s.weights.len() == n {
            s.weights[i]
        } else {
            1.0
        }
    };
    let mut d: Vec<(f64, f64, f64)> = (0..=p)
        .map(|j| {
            let i = j + k - p;
            let w = weight(i);
            (s.control[i].x * w, s.control[i].y * w, w)
        })
        .collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = j + k - p;
            let span = s.knots[i + p + 1 - r] - s.knots[i];
            let a = if span.abs() < 1e-300 {
                0.0
            } else {
                (u - s.knots[i]) / span
            };
            d[j] = (
                (1.0 - a) * d[j - 1].0 + a * d[j].0,
                (1.0 - a) * d[j - 1].1 + a * d[j].1,
                (1.0 - a) * d[j - 1].2 + a * d[j].2,
            );
        }
    }
    let (x, y, w) = d[p];
    if w.abs() < 1e-300 {
        Point::new(x, y)
    } else {
        Point::new(x / w, y / w)
    }
}

fn sampled(s: &Spline) -> Option<SubPath> {
    let p = s.degree;
    let (lo, hi) = (s.knots[p], s.knots[s.control.len()]);
    let spans = s.knots[p..=s.control.len()]
        .windows(2)
        .filter(|w| w[1] > w[0])
        .count()
        .max(1);
    let count = (spans * SAMPLES_PER_SPAN).clamp(8, MAX_SAMPLES);
    let mut sub = SubPath::open(point_at(s, lo));
    for i in 1..=count {
        let u = lo + (hi - lo) * i as f64 / count as f64;
        let u = if i == count {
            hi - (hi - lo) * 1e-12
        } else {
            u
        };
        let q = point_at(s, u);
        if q.distance(sub.end()) > 1e-12 {
            sub.line_to(q);
        }
    }
    Some(sub)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{path_bbox, Segment};

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn a_single_cubic_piece_passes_straight_through() {
        let control = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 2.0),
            Point::new(3.0, 2.0),
            Point::new(4.0, 0.0),
        ];
        let s = Spline {
            degree: 3,
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            control: control.clone(),
            ..Spline::default()
        };
        let sub = subpath(&s).expect("a curve");
        assert_eq!(sub.segments.len(), 1);
        assert!(matches!(sub.segments[0], Segment::Cubic { .. }));
        assert!(close(sub.end(), control[3]));
    }

    #[test]
    fn interior_knots_split_the_curve_into_pieces_on_the_curve() {
        let s = Spline {
            degree: 3,
            knots: vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
            control: vec![
                Point::new(0.0, 0.0),
                Point::new(1.0, 3.0),
                Point::new(2.0, -1.0),
                Point::new(4.0, 2.0),
                Point::new(5.0, 0.0),
            ],
            ..Spline::default()
        };
        let sub = subpath(&s).expect("a curve");
        assert_eq!(sub.segments.len(), 2);
        // The joint is the curve at the inserted knot.
        let joint = sub.segments[0].end();
        assert!(close(joint, point_at(&s, 0.5)), "{joint:?}");
        assert!(close(sub.end(), Point::new(5.0, 0.0)));
    }

    #[test]
    fn quadratics_become_cubics_and_lines_stay_lines() {
        let quad = Spline {
            degree: 2,
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            control: vec![
                Point::new(0.0, 0.0),
                Point::new(1.0, 1.0),
                Point::new(2.0, 0.0),
            ],
            ..Spline::default()
        };
        let sub = subpath(&quad).expect("a curve");
        let Segment::Cubic { c1, .. } = sub.segments[0] else {
            panic!("not a cubic");
        };
        assert!(close(c1, Point::new(2.0 / 3.0, 2.0 / 3.0)));
        let line = Spline {
            degree: 1,
            knots: vec![0.0, 0.0, 1.0, 2.0, 2.0],
            control: vec![
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(1.0, 1.0),
            ],
            ..Spline::default()
        };
        let sub = subpath(&line).expect("lines");
        assert_eq!(sub.segments.len(), 2);
        assert!(sub
            .segments
            .iter()
            .all(|s| matches!(s, Segment::Line { .. })));
    }

    #[test]
    fn a_rational_circle_is_sampled_on_the_circle() {
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let s = Spline {
            degree: 2,
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 4.0],
            control: vec![
                Point::new(1.0, 0.0),
                Point::new(1.0, 1.0),
                Point::new(0.0, 1.0),
                Point::new(-1.0, 1.0),
                Point::new(-1.0, 0.0),
                Point::new(-1.0, -1.0),
                Point::new(0.0, -1.0),
                Point::new(1.0, -1.0),
                Point::new(1.0, 0.0),
            ],
            weights: vec![1.0, w, 1.0, w, 1.0, w, 1.0, w, 1.0],
            closed: true,
            ..Spline::default()
        };
        let sub = subpath(&s).expect("a circle");
        assert!(sub.closed);
        for (from, _) in sub.iter() {
            assert!((from.distance(Point::ORIGIN) - 1.0).abs() < 1e-9);
        }
        let bbox = path_bbox(&sub.into_path()).expect("bbox");
        assert!((bbox.width() - 2.0).abs() < 1e-3);
    }

    #[test]
    fn fit_points_alone_are_passed_through() {
        let s = Spline {
            degree: 3,
            fit: vec![
                Point::new(0.0, 0.0),
                Point::new(5.0, 5.0),
                Point::new(10.0, 0.0),
            ],
            ..Spline::default()
        };
        let sub = subpath(&s).expect("a curve");
        assert!(close(sub.start, Point::new(0.0, 0.0)));
        assert!(close(sub.end(), Point::new(10.0, 0.0)));
        assert!(subpath(&Spline::default()).is_none());
    }
}
