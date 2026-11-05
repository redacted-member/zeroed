//! Axis aligned bounding boxes, in millimetres.

use serde::{Deserialize, Serialize};

use super::point::{Point, Vec2};

/// An axis aligned box with `min` at the lower left and `max` at the upper
/// right. A box is always non empty: it contains at least one point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    /// Lower left corner, in millimetres.
    pub min: Point,
    /// Upper right corner, in millimetres.
    pub max: Point,
}

impl BBox {
    /// A box containing the single point `p`.
    pub fn from_point(p: Point) -> Self {
        BBox { min: p, max: p }
    }

    /// A box with the given corners, sorted so that `min` really is minimal.
    pub fn new(a: Point, b: Point) -> Self {
        BBox {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    /// The tight box around a set of points, or `None` when the set is empty.
    pub fn of_points<I: IntoIterator<Item = Point>>(points: I) -> Option<Self> {
        let mut iter = points.into_iter();
        let first = iter.next()?;
        let mut bbox = BBox::from_point(first);
        for p in iter {
            bbox = bbox.with_point(p);
        }
        Some(bbox)
    }

    /// The smallest box containing both `self` and `other`.
    pub fn union(self, other: BBox) -> BBox {
        BBox {
            min: Point::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    /// The smallest box containing `self` and the point `p`.
    pub fn with_point(self, p: Point) -> BBox {
        BBox {
            min: Point::new(self.min.x.min(p.x), self.min.y.min(p.y)),
            max: Point::new(self.max.x.max(p.x), self.max.y.max(p.y)),
        }
    }

    /// The box grown by `d` millimetres on every side. A negative `d` shrinks
    /// it, and never past the centre.
    pub fn expand(self, d: f64) -> BBox {
        let cx = (self.min.x + self.max.x) * 0.5;
        let cy = (self.min.y + self.max.y) * 0.5;
        BBox {
            min: Point::new((self.min.x - d).min(cx), (self.min.y - d).min(cy)),
            max: Point::new((self.max.x + d).max(cx), (self.max.y + d).max(cy)),
        }
    }

    /// True when the two boxes share at least one point, touching included.
    pub fn intersects(self, other: BBox) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
    }

    /// The overlap of two boxes, or `None` when they do not overlap.
    pub fn intersection(self, other: BBox) -> Option<BBox> {
        if !self.intersects(other) {
            return None;
        }
        Some(BBox {
            min: Point::new(self.min.x.max(other.min.x), self.min.y.max(other.min.y)),
            max: Point::new(self.max.x.min(other.max.x), self.max.y.min(other.max.y)),
        })
    }

    /// True when the point lies inside the box or on its edge.
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    /// True when `other` lies entirely inside `self`, touching included.
    pub fn contains_bbox(self, other: BBox) -> bool {
        self.contains(other.min) && self.contains(other.max)
    }

    /// The centre of the box, in millimetres.
    pub fn centre(self) -> Point {
        Point::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
        )
    }

    /// Width and height, in millimetres.
    pub fn size(self) -> Vec2 {
        Vec2::new(self.max.x - self.min.x, self.max.y - self.min.y)
    }

    /// Width in millimetres.
    pub fn width(self) -> f64 {
        self.max.x - self.min.x
    }

    /// Height in millimetres.
    pub fn height(self) -> f64 {
        self.max.y - self.min.y
    }

    /// True when the box has no area, because it is a point or a line.
    pub fn is_degenerate(self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn of_points_is_tight() {
        let b = BBox::of_points([
            Point::new(1.0, 5.0),
            Point::new(-2.0, 3.0),
            Point::new(4.0, -1.0),
        ])
        .expect("three points");
        assert_eq!(b.min, Point::new(-2.0, -1.0));
        assert_eq!(b.max, Point::new(4.0, 5.0));
    }

    #[test]
    fn of_points_on_empty_is_none() {
        assert!(BBox::of_points(Vec::<Point>::new()).is_none());
    }

    #[test]
    fn new_sorts_the_corners() {
        let b = BBox::new(Point::new(4.0, 5.0), Point::new(1.0, 2.0));
        assert_eq!(b.min, Point::new(1.0, 2.0));
        assert_eq!(b.max, Point::new(4.0, 5.0));
    }

    #[test]
    fn union_covers_both() {
        let a = BBox::new(Point::new(0.0, 0.0), Point::new(1.0, 1.0));
        let b = BBox::new(Point::new(2.0, 2.0), Point::new(3.0, 3.0));
        let u = a.union(b);
        assert!(u.contains_bbox(a) && u.contains_bbox(b));
        assert_eq!(u.size(), Vec2::new(3.0, 3.0));
    }

    #[test]
    fn expand_grows_and_shrinks() {
        let b = BBox::new(Point::new(0.0, 0.0), Point::new(10.0, 10.0));
        assert_eq!(b.expand(1.0).size(), Vec2::new(12.0, 12.0));
        assert_eq!(b.expand(-2.0).size(), Vec2::new(6.0, 6.0));
        assert!(b.expand(-100.0).is_degenerate());
    }

    #[test]
    fn intersects_counts_touching_as_true() {
        let a = BBox::new(Point::new(0.0, 0.0), Point::new(1.0, 1.0));
        let b = BBox::new(Point::new(1.0, 0.0), Point::new(2.0, 1.0));
        assert!(a.intersects(b));
        let c = BBox::new(Point::new(1.001, 0.0), Point::new(2.0, 1.0));
        assert!(!a.intersects(c));
        assert!(a.intersection(c).is_none());
    }

    #[test]
    fn intersection_is_the_overlap() {
        let a = BBox::new(Point::new(0.0, 0.0), Point::new(4.0, 4.0));
        let b = BBox::new(Point::new(2.0, 1.0), Point::new(6.0, 3.0));
        let i = a.intersection(b).expect("overlap");
        assert_eq!(i.min, Point::new(2.0, 1.0));
        assert_eq!(i.max, Point::new(4.0, 3.0));
    }

    #[test]
    fn centre_and_size_agree() {
        let b = BBox::new(Point::new(-2.0, -4.0), Point::new(2.0, 4.0));
        assert_eq!(b.centre(), Point::new(0.0, 0.0));
        assert_eq!(b.width(), 4.0);
        assert_eq!(b.height(), 8.0);
    }

    #[test]
    fn contains_includes_the_edge() {
        let b = BBox::new(Point::new(0.0, 0.0), Point::new(1.0, 1.0));
        assert!(b.contains(Point::new(0.0, 0.5)));
        assert!(!b.contains(Point::new(-0.001, 0.5)));
    }
}
