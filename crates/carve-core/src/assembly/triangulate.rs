//! Triangulation of a simple polygon with holes by ear clipping. Holes are
//! joined to the outer ring through a bridge to a visible vertex first.

/// A 2D point in millimetres.
pub type P2 = [f64; 2];

#[derive(Debug, Clone, Copy)]
struct Node {
    /// Index into the caller's vertex list (outer ring, then each hole).
    index: usize,
    p: P2,
    prev: usize,
    next: usize,
}

/// Twice the signed area of a ring; positive when counter-clockwise.
pub fn signed_area2(ring: &[P2]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

fn cross(a: P2, b: P2, c: P2) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn same(a: P2, b: P2) -> bool {
    (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12
}

/// Whether segments `a b` and `c d` cross or touch anywhere other than at a
/// shared endpoint.
fn segments_meet(a: P2, b: P2, c: P2, d: P2) -> bool {
    if same(a, c) || same(a, d) || same(b, c) || same(b, d) {
        return false;
    }
    let d1 = cross(a, b, c);
    let d2 = cross(a, b, d);
    let d3 = cross(c, d, a);
    let d4 = cross(c, d, b);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    let on = |p: P2, q: P2, r: P2| {
        cross(p, q, r).abs() < 1e-12
            && r[0] >= p[0].min(q[0]) - 1e-12
            && r[0] <= p[0].max(q[0]) + 1e-12
            && r[1] >= p[1].min(q[1]) - 1e-12
            && r[1] <= p[1].max(q[1]) + 1e-12
    };
    on(a, b, c) || on(a, b, d) || on(c, d, a) || on(c, d, b)
}

fn inside_triangle(a: P2, b: P2, c: P2, p: P2) -> bool {
    cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
}

/// Triangulates `outer` (any orientation) minus `holes`. Answers triangles
/// as index triples into the vertex list `outer ++ holes[0] ++ holes[1] ...`,
/// each counter-clockwise. Rings with fewer than three points are ignored.
pub fn triangulate(outer: &[P2], holes: &[Vec<P2>]) -> Vec<[usize; 3]> {
    if outer.len() < 3 {
        return Vec::new();
    }
    let mut nodes: Vec<Node> = Vec::new();
    let outer_start = push_ring(&mut nodes, outer, 0, true);
    let mut offset = outer.len();
    let mut hole_starts = Vec::new();
    for hole in holes {
        if hole.len() >= 3 {
            let start = push_ring(&mut nodes, hole, offset, false);
            // The hole's leftmost node, where its bridge starts.
            let mut left = start;
            let mut i = nodes[start].next;
            while i != start {
                if nodes[i].p[0] < nodes[left].p[0] {
                    left = i;
                }
                i = nodes[i].next;
            }
            hole_starts.push(left);
        }
        offset += hole.len();
    }
    hole_starts.sort_by(|a, b| nodes[*a].p[0].total_cmp(&nodes[*b].p[0]));
    let mut remaining: Vec<usize> = hole_starts.clone();
    for &h in &hole_starts {
        remaining.retain(|&r| r != h);
        if let Some(v) = bridge_target(&nodes, outer_start, h, &remaining) {
            splice(&mut nodes, v, h);
        }
    }
    clip(&mut nodes, outer_start)
}

fn push_ring(nodes: &mut Vec<Node>, ring: &[P2], offset: usize, ccw: bool) -> usize {
    let forward = (signed_area2(ring) > 0.0) == ccw;
    let n = ring.len();
    let start = nodes.len();
    for k in 0..n {
        let i = if forward { k } else { n - 1 - k };
        nodes.push(Node {
            index: offset + i,
            p: ring[i],
            prev: start + (k + n - 1) % n,
            next: start + (k + 1) % n,
        });
    }
    start
}

fn ring_nodes(nodes: &[Node], start: usize) -> Vec<usize> {
    let mut out = vec![start];
    let mut i = nodes[start].next;
    while i != start && out.len() <= nodes.len() {
        out.push(i);
        i = nodes[i].next;
    }
    out
}

/// A node of the ring through `outer` that `h` can see: the nearest one
/// whose connecting segment crosses no edge of that ring or of the holes
/// still waiting.
fn bridge_target(nodes: &[Node], outer: usize, h: usize, waiting: &[usize]) -> Option<usize> {
    let ring = ring_nodes(nodes, outer);
    let hp = nodes[h].p;
    let mut edges: Vec<(P2, P2)> = ring
        .iter()
        .map(|&i| (nodes[i].p, nodes[nodes[i].next].p))
        .collect();
    for &w in waiting.iter().chain(std::iter::once(&h)) {
        for i in ring_nodes(nodes, w) {
            edges.push((nodes[i].p, nodes[nodes[i].next].p));
        }
    }
    let mut candidates = ring.clone();
    let dist = |i: &usize| {
        let p = nodes[*i].p;
        (p[0] - hp[0]).powi(2) + (p[1] - hp[1]).powi(2)
    };
    candidates.sort_by(|a, b| dist(a).total_cmp(&dist(b)));
    candidates.into_iter().find(|&v| {
        let vp = nodes[v].p;
        // The bridge must leave v into the polygon's interior.
        let a = nodes[nodes[v].prev].p;
        let b = nodes[nodes[v].next].p;
        let convex = cross(a, vp, b) >= 0.0;
        let inward = if convex {
            cross(a, vp, hp) >= 0.0 && cross(vp, b, hp) >= 0.0
        } else {
            !(cross(a, vp, hp) < 0.0 && cross(vp, b, hp) < 0.0)
        };
        inward && !edges.iter().any(|&(e0, e1)| segments_meet(hp, vp, e0, e1))
    })
}

/// Joins the hole ring through `h` into the ring through `v` with a two way
/// bridge: v, h, (hole), h', v', (rest).
fn splice(nodes: &mut Vec<Node>, v: usize, h: usize) {
    let v2 = nodes.len();
    let h2 = v2 + 1;
    let v_next = nodes[v].next;
    let h_prev = nodes[h].prev;
    nodes.push(Node {
        prev: h2,
        next: v_next,
        ..nodes[v]
    });
    nodes.push(Node {
        prev: h_prev,
        next: v2,
        ..nodes[h]
    });
    nodes[v].next = h;
    nodes[h].prev = v;
    nodes[h_prev].next = h2;
    nodes[v_next].prev = v2;
}

fn clip(nodes: &mut [Node], start: usize) -> Vec<[usize; 3]> {
    let mut out = Vec::new();
    let mut ring = ring_nodes(nodes, start);
    let mut current = start;
    let mut stalled = 0;
    while ring.len() > 3 {
        let a = nodes[current].prev;
        let c = nodes[current].next;
        let (pa, pb, pc) = (nodes[a].p, nodes[current].p, nodes[c].p);
        let area = cross(pa, pb, pc);
        let degenerate = area.abs() < 1e-14;
        let ear = !degenerate
            && area > 0.0
            && !ring.iter().any(|&i| {
                i != a
                    && i != current
                    && i != c
                    && !same(nodes[i].p, pa)
                    && !same(nodes[i].p, pb)
                    && !same(nodes[i].p, pc)
                    && inside_triangle(pa, pb, pc, nodes[i].p)
            });
        if ear || degenerate || stalled > ring.len() {
            if !degenerate {
                out.push([nodes[a].index, nodes[current].index, nodes[c].index]);
            }
            nodes[a].next = c;
            nodes[c].prev = a;
            ring.retain(|&i| i != current);
            current = c;
            stalled = 0;
        } else {
            current = c;
            stalled += 1;
        }
    }
    if ring.len() == 3 {
        let (a, b, c) = (ring[0], ring[1], ring[2]);
        if cross(nodes[a].p, nodes[b].p, nodes[c].p).abs() >= 1e-14 {
            out.push([nodes[a].index, nodes[b].index, nodes[c].index]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(points: &[P2], tris: &[[usize; 3]]) -> f64 {
        tris.iter()
            .map(|t| cross(points[t[0]], points[t[1]], points[t[2]]) / 2.0)
            .sum()
    }

    #[test]
    fn a_square_makes_two_triangles() {
        let sq = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let tris = triangulate(&sq, &[]);
        assert_eq!(tris.len(), 2);
        assert!((area(&sq, &tris) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn a_square_with_a_hole_covers_the_ring_only() {
        let outer = vec![[0.0, 0.0], [0.0, 10.0], [10.0, 10.0], [10.0, 0.0]];
        let hole = vec![[3.0, 3.0], [7.0, 3.0], [7.0, 7.0], [3.0, 7.0]];
        let tris = triangulate(&outer, std::slice::from_ref(&hole));
        let mut all = outer.clone();
        all.extend(hole);
        assert!(
            (area(&all, &tris) - 84.0).abs() < 1e-9,
            "{}",
            area(&all, &tris)
        );
        assert!(tris
            .iter()
            .all(|t| cross(all[t[0]], all[t[1]], all[t[2]]) > 0.0));
    }

    #[test]
    fn two_holes_and_a_concave_outline() {
        let outer = vec![
            [0.0, 0.0],
            [30.0, 0.0],
            [30.0, 20.0],
            [15.0, 10.0],
            [0.0, 20.0],
        ];
        let h1 = vec![[3.0, 3.0], [8.0, 3.0], [8.0, 6.0], [3.0, 6.0]];
        let h2 = vec![[20.0, 3.0], [26.0, 3.0], [26.0, 6.0], [20.0, 6.0]];
        let tris = triangulate(&outer, &[h1.clone(), h2.clone()]);
        let mut all = outer.clone();
        all.extend(h1);
        all.extend(h2);
        let expected = signed_area2(&outer) / 2.0 - 15.0 - 18.0;
        assert!((area(&all, &tris) - expected).abs() < 1e-9);
    }
}
