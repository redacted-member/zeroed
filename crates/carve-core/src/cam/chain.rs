//! Cut chains: the XY track a tool follows for one continuous cut, made of
//! straight and circular spans. Bezier spans are flattened on the way in,
//! since machines cut lines and arcs.

use crate::geom::{
    bulge_from_sweep, flatten_segment_into, ArcGeom, Point, Segment, SubPath, Tolerance,
};

/// Bulges smaller than this are straight lines.
const STRAIGHT: f64 = 1e-12;

/// One straight or circular span of a chain, in millimetres. `bulge` follows
/// the path convention: zero for a line, positive for a counter clockwise
/// arc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutSeg {
    /// Where the span starts.
    pub from: Point,
    /// Where the span ends.
    pub to: Point,
    /// Arc bulge, zero for a straight span.
    pub bulge: f64,
}

impl CutSeg {
    /// The circle the span lies on, or `None` for a straight span.
    pub fn arc(&self) -> Option<ArcGeom> {
        if self.bulge.abs() < STRAIGHT {
            None
        } else {
            ArcGeom::from_bulge(self.from, self.to, self.bulge)
        }
    }

    /// Length along the span, in millimetres.
    pub fn length(&self) -> f64 {
        match self.arc() {
            Some(arc) => arc.radius * arc.sweep.abs(),
            None => self.from.distance(self.to),
        }
    }

    /// The point `d` millimetres along the span.
    pub fn point_at(&self, d: f64) -> Point {
        let length = self.length();
        let t = if length > 0.0 {
            (d / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        match self.arc() {
            Some(arc) => arc.point_at(t),
            None => self.from.lerp(self.to, t),
        }
    }

    /// The span cut in two, `d` millimetres from its start.
    pub fn split(&self, d: f64) -> (CutSeg, CutSeg) {
        let length = self.length();
        let t = if length > 0.0 {
            (d / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mid = self.point_at(d);
        match self.arc() {
            Some(arc) => (
                CutSeg {
                    from: self.from,
                    to: mid,
                    bulge: bulge_from_sweep(arc.sweep * t),
                },
                CutSeg {
                    from: mid,
                    to: self.to,
                    bulge: bulge_from_sweep(arc.sweep * (1.0 - t)),
                },
            ),
            None => (
                CutSeg {
                    from: self.from,
                    to: mid,
                    bulge: 0.0,
                },
                CutSeg {
                    from: mid,
                    to: self.to,
                    bulge: 0.0,
                },
            ),
        }
    }

    /// The point of the span nearest `p`: how far along the span it is and
    /// how far from `p`, in millimetres.
    pub fn nearest(&self, p: Point) -> (f64, f64) {
        let length = self.length();
        let along = match self.arc() {
            Some(arc) => {
                let angle = (p.y - arc.centre.y).atan2(p.x - arc.centre.x);
                // Angle travelled from the start, in the sweep's direction.
                let turn = std::f64::consts::TAU;
                let mut off = (angle - arc.start_angle) * arc.sweep.signum();
                off = off.rem_euclid(turn);
                let sweep = arc.sweep.abs();
                if off > sweep {
                    // Past the end: the nearer end.
                    if off - sweep < turn - off {
                        length
                    } else {
                        0.0
                    }
                } else {
                    off * arc.radius
                }
            }
            None => {
                let (dx, dy) = (self.to.x - self.from.x, self.to.y - self.from.y);
                let len2 = dx * dx + dy * dy;
                if len2 <= 0.0 {
                    0.0
                } else {
                    let t = ((p.x - self.from.x) * dx + (p.y - self.from.y) * dy) / len2;
                    t.clamp(0.0, 1.0) * length
                }
            }
        };
        (along, self.point_at(along).distance(p))
    }

    /// The same span travelled the other way.
    pub fn reversed(&self) -> CutSeg {
        CutSeg {
            from: self.to,
            to: self.from,
            bulge: -self.bulge,
        }
    }
}

/// A continuous cut: spans end to end, closed when the last ends on the
/// first's start.
#[derive(Debug, Clone, PartialEq)]
pub struct Chain {
    /// Spans in cutting order.
    pub segs: Vec<CutSeg>,
    /// True when the chain is a loop.
    pub closed: bool,
}

impl Chain {
    /// The chain for a subpath, with bezier spans flattened to `tol`.
    pub fn from_subpath(sub: &SubPath, tol: Tolerance) -> Chain {
        let mut segs = Vec::with_capacity(sub.segments.len());
        let mut from = sub.start;
        for segment in &sub.segments {
            match *segment {
                Segment::Line { to } => segs.push(CutSeg {
                    from,
                    to,
                    bulge: 0.0,
                }),
                Segment::Arc { to, bulge } => segs.push(CutSeg { from, to, bulge }),
                Segment::Cubic { .. } => {
                    let mut points = Vec::new();
                    flatten_segment_into(from, segment, tol, &mut points);
                    let mut at = from;
                    for p in points {
                        segs.push(CutSeg {
                            from: at,
                            to: p,
                            bulge: 0.0,
                        });
                        at = p;
                    }
                }
            }
            from = segment.end();
        }
        segs.retain(|s| s.length() > 1e-9);
        Chain {
            segs,
            closed: sub.closed,
        }
    }

    /// True when the chain has nothing to cut.
    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }

    /// The point of the chain nearest `p`: how far along the chain it is
    /// and how far from `p`, in millimetres; `None` for an empty chain.
    pub fn nearest(&self, p: Point) -> Option<(f64, f64)> {
        let mut walked = 0.0;
        let mut best: Option<(f64, f64)> = None;
        for seg in &self.segs {
            let (along, gap) = seg.nearest(p);
            if best.is_none_or(|(_, b)| gap < b) {
                best = Some((walked + along, gap));
            }
            walked += seg.length();
        }
        best
    }

    /// The point `d` millimetres along the chain (clamped to its ends).
    pub fn point_at(&self, d: f64) -> Option<Point> {
        let mut left = d.max(0.0);
        for seg in &self.segs {
            let length = seg.length();
            if left <= length {
                return Some(seg.point_at(left));
            }
            left -= length;
        }
        self.end()
    }

    /// Where the chain starts.
    pub fn start(&self) -> Option<Point> {
        self.segs.first().map(|s| s.from)
    }

    /// Where the chain ends.
    pub fn end(&self) -> Option<Point> {
        self.segs.last().map(|s| s.to)
    }

    /// Length along the chain, in millimetres.
    pub fn length(&self) -> f64 {
        self.segs.iter().map(CutSeg::length).sum()
    }

    /// The same chain travelled the other way.
    pub fn reversed(&self) -> Chain {
        Chain {
            segs: self.segs.iter().rev().map(CutSeg::reversed).collect(),
            closed: self.closed,
        }
    }

    /// A closed chain restarted at the node nearest `p`. Open chains are
    /// returned as they are, since their ends are fixed.
    pub fn starting_near(&self, p: Point) -> Chain {
        if !self.closed || self.segs.is_empty() {
            return self.clone();
        }
        let best = self
            .segs
            .iter()
            .enumerate()
            .min_by(|a, b| {
                a.1.from
                    .distance_squared(p)
                    .total_cmp(&b.1.from.distance_squared(p))
            })
            .map_or(0, |(i, _)| i);
        let mut segs = self.segs[best..].to_vec();
        segs.extend_from_slice(&self.segs[..best]);
        Chain { segs, closed: true }
    }

    /// The chain cut into consecutive pieces at the given distances along
    /// it, which must be increasing. Answers `cuts.len() + 1` pieces; a piece
    /// can be empty when two cuts coincide.
    pub fn split_at(&self, cuts: &[f64]) -> Vec<Vec<CutSeg>> {
        let mut pieces: Vec<Vec<CutSeg>> = vec![Vec::new()];
        let mut travelled = 0.0;
        let mut next = cuts.iter().copied().peekable();
        for seg in &self.segs {
            let mut rest = *seg;
            let mut rest_start = travelled;
            let end = travelled + seg.length();
            while let Some(&cut) = next.peek() {
                if cut >= end - 1e-9 {
                    break;
                }
                next.next();
                if cut <= rest_start + 1e-9 {
                    pieces.push(Vec::new());
                    continue;
                }
                let (head, tail) = rest.split(cut - rest_start);
                if let Some(piece) = pieces.last_mut() {
                    piece.push(head);
                }
                pieces.push(Vec::new());
                rest = tail;
                rest_start = cut;
            }
            if let Some(piece) = pieces.last_mut() {
                piece.push(rest);
            }
            travelled = end;
        }
        // Cuts at or past the end still make their (empty) pieces.
        for _ in next {
            pieces.push(Vec::new());
        }
        pieces
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{circle, rect};

    fn tol() -> Tolerance {
        Tolerance::new(1e-3).expect("tolerance")
    }

    fn square() -> Chain {
        let path = rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect");
        Chain::from_subpath(&path.subpaths[0], tol())
    }

    #[test]
    fn a_square_is_four_straight_spans() {
        let chain = square();
        assert!(chain.closed);
        assert_eq!(chain.segs.len(), 4);
        assert!((chain.length() - 40.0).abs() < 1e-9);
    }

    #[test]
    fn a_circle_keeps_its_arcs_and_their_length() {
        let path = circle(Point::new(0.0, 0.0), 5.0).expect("circle");
        let chain = Chain::from_subpath(&path.subpaths[0], tol());
        assert!(chain.segs.iter().all(|s| s.bulge != 0.0));
        let want = std::f64::consts::TAU * 5.0;
        assert!((chain.length() - want).abs() < 1e-9);
    }

    #[test]
    fn splitting_an_arc_keeps_both_halves_on_the_circle() {
        let path = circle(Point::new(0.0, 0.0), 5.0).expect("circle");
        let chain = Chain::from_subpath(&path.subpaths[0], tol());
        let first = chain.segs[0];
        let (a, b) = first.split(first.length() / 3.0);
        assert!((a.length() + b.length() - first.length()).abs() < 1e-9);
        assert!((a.to.distance(Point::ORIGIN) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn split_at_makes_pieces_whose_lengths_add_up() {
        let pieces = square().split_at(&[5.0, 15.0, 15.0, 39.0]);
        assert_eq!(pieces.len(), 5);
        let lengths: Vec<f64> = pieces
            .iter()
            .map(|p| p.iter().map(CutSeg::length).sum())
            .collect();
        let want = [5.0, 10.0, 0.0, 24.0, 1.0];
        for (got, want) in lengths.iter().zip(want) {
            assert!((got - want).abs() < 1e-9, "{lengths:?}");
        }
    }

    #[test]
    fn a_closed_chain_can_start_at_its_nearest_node() {
        let moved = square().starting_near(Point::new(11.0, 11.0));
        assert_eq!(moved.start(), Some(Point::new(10.0, 10.0)));
        assert!((moved.length() - 40.0).abs() < 1e-9);
    }

    #[test]
    fn reversing_runs_the_chain_backwards() {
        let back = square().reversed();
        assert_eq!(back.start(), square().end());
        assert!((back.length() - 40.0).abs() < 1e-9);
    }
}
