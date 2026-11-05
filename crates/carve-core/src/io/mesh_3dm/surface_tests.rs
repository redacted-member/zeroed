//! Whole files with surfaces, polysurfaces and extrusions, written the way
//! openNURBS lays them out.

use super::brep::{ON_BREP, ON_EXTRUSION};
use super::geometry::{
    length, sub, ARC, LINE, NURBS_CURVE, NURBS_SURFACE, PLANE_SURFACE, POLYCURVE, POLYLINE,
    REV_SURFACE, SUM_SURFACE,
};
use super::tests::{cube, Writer};
use super::{read, ANONYMOUS, CLASS_DATA, CLASS_UUID, ON_MESH, OPENNURBS_CLASS};
use crate::io::mesh::Triangle;

fn f(out: &mut Vec<u8>, values: &[f64]) {
    for v in values {
        out.extend(v.to_le_bytes());
    }
}

fn i(out: &mut Vec<u8>, values: &[i32]) {
    for v in values {
        out.extend(v.to_le_bytes());
    }
}

impl Writer {
    fn class(&self, uuid: [u8; 16], data: &[u8]) -> Vec<u8> {
        let mut class = self.chunk(CLASS_UUID, &uuid);
        class.extend(self.chunk(CLASS_DATA, data));
        class.extend(self.short(0x0002_7fff, 0));
        self.chunk(OPENNURBS_CLASS, &class)
    }

    /// An `ON_CurveArray` or `ON_SurfaceArray`.
    fn objects(&self, items: &[([u8; 16], Vec<u8>)]) -> Vec<u8> {
        let mut body = vec![0x10];
        i(&mut body, &[items.len() as i32]);
        for (uuid, data) in items {
            i(&mut body, &[1]);
            body.extend(self.class(*uuid, data));
        }
        self.chunk(ANONYMOUS, &body)
    }

    /// An array of fixed records (vertices, edges, trims, loops, faces).
    fn records(&self, records: &[Vec<u8>]) -> Vec<u8> {
        let mut body = vec![0x10];
        i(&mut body, &[records.len() as i32]);
        body.extend(records.concat());
        self.chunk(ANONYMOUS, &body)
    }

    /// A brep: trim curves, surfaces, trims `(curve, domain)`, loops
    /// `(trims, kind)`, faces `(loops, surface, reversed)` and optionally a
    /// render mesh per face.
    #[allow(clippy::too_many_arguments)]
    fn brep(
        &self,
        curves: &[([u8; 16], Vec<u8>)],
        surfaces: &[([u8; 16], Vec<u8>)],
        trims: &[(i32, [f64; 2])],
        loops: &[(Vec<i32>, i32)],
        faces: &[(Vec<i32>, i32, bool)],
        render: Option<Vec<Option<Vec<u8>>>>,
    ) -> Vec<u8> {
        let mut out = vec![if render.is_some() { 0x32 } else { 0x30 }];
        out.extend(self.objects(curves));
        out.extend(self.objects(&[]));
        out.extend(self.objects(surfaces));
        out.extend(self.records(&[]));
        out.extend(self.records(&[]));
        let trim_records: Vec<Vec<u8>> = trims
            .iter()
            .enumerate()
            .map(|(k, (curve, domain))| {
                let mut r = Vec::new();
                i(&mut r, &[k as i32, *curve]);
                f(&mut r, domain);
                i(&mut r, &[-1, 0, 0, 0, 1, 0, 0]);
                f(&mut r, &[0.0; 2]);
                r.extend([0u8; 64]);
                r
            })
            .collect();
        out.extend(self.records(&trim_records));
        let loop_records: Vec<Vec<u8>> = loops
            .iter()
            .enumerate()
            .map(|(k, (trims, kind))| {
                let mut r = Vec::new();
                i(&mut r, &[k as i32, trims.len() as i32]);
                i(&mut r, trims);
                i(&mut r, &[*kind, 0]);
                r
            })
            .collect();
        out.extend(self.records(&loop_records));
        let face_records: Vec<Vec<u8>> = faces
            .iter()
            .enumerate()
            .map(|(k, (loops, surface, reversed))| {
                let mut r = Vec::new();
                i(&mut r, &[k as i32, loops.len() as i32]);
                i(&mut r, loops);
                i(&mut r, &[*surface, i32::from(*reversed), 0]);
                r
            })
            .collect();
        out.extend(self.records(&face_records));
        f(&mut out, &[0.0, 0.0, 0.0, 100.0, 100.0, 100.0]);
        if let Some(render) = render {
            let mut body = Vec::new();
            for mesh in &render {
                match mesh {
                    Some(data) => {
                        body.push(1);
                        body.extend(self.class(ON_MESH, data));
                    }
                    None => body.push(0),
                }
            }
            out.extend(self.chunk(ANONYMOUS, &body));
            out.extend(self.chunk(ANONYMOUS, &vec![0u8; render.len()]));
            i(&mut out, &[0]);
        }
        out
    }
}

fn line(from: [f64; 3], to: [f64; 3], domain: [f64; 2]) -> ([u8; 16], Vec<u8>) {
    let mut d = vec![0x10];
    f(&mut d, &from);
    f(&mut d, &to);
    f(&mut d, &domain);
    i(&mut d, &[2]);
    (LINE, d)
}

fn polyline(points: &[[f64; 3]]) -> ([u8; 16], Vec<u8>) {
    let mut d = vec![0x10];
    i(&mut d, &[points.len() as i32]);
    for p in points {
        f(&mut d, p);
    }
    i(&mut d, &[points.len() as i32]);
    for k in 0..points.len() {
        f(&mut d, &[k as f64]);
    }
    i(&mut d, &[2]);
    (POLYLINE, d)
}

/// A NURBS curve from stored knots and homogeneous points `[x, y, z, w]`.
fn nurbs_curve(
    dim: usize,
    order: i32,
    knots: &[f64],
    cvs: &[[f64; 4]],
    rational: bool,
) -> ([u8; 16], Vec<u8>) {
    let mut d = vec![0x10];
    i(
        &mut d,
        &[
            dim as i32,
            i32::from(rational),
            order,
            cvs.len() as i32,
            0,
            0,
        ],
    );
    f(&mut d, &[0.0; 6]);
    i(&mut d, &[knots.len() as i32]);
    f(&mut d, knots);
    i(&mut d, &[cvs.len() as i32]);
    for cv in cvs {
        f(&mut d, &cv[..dim]);
        if rational {
            f(&mut d, &[cv[3]]);
        }
    }
    (NURBS_CURVE, d)
}

/// A full circle as a rational quadratic of nine points.
fn circle(centre: [f64; 2], r: f64) -> ([u8; 16], Vec<u8>) {
    let w = std::f64::consts::FRAC_1_SQRT_2;
    let corners = [
        (1.0, 0.0, 1.0),
        (1.0, 1.0, w),
        (0.0, 1.0, 1.0),
        (-1.0, 1.0, w),
        (-1.0, 0.0, 1.0),
        (-1.0, -1.0, w),
        (0.0, -1.0, 1.0),
        (1.0, -1.0, w),
        (1.0, 0.0, 1.0),
    ];
    let cvs: Vec<[f64; 4]> = corners
        .iter()
        .map(|(x, y, w)| [(centre[0] + r * x) * w, (centre[1] + r * y) * w, 0.0, *w])
        .collect();
    let knots = [0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0];
    nurbs_curve(2, 3, &knots, &cvs, true)
}

fn plane(size: f64) -> ([u8; 16], Vec<u8>) {
    let mut d = vec![0x10];
    f(
        &mut d,
        &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    );
    f(&mut d, &[0.0, 0.0, 1.0, 0.0]);
    f(&mut d, &[0.0, size, 0.0, size]);
    (PLANE_SURFACE, d)
}

/// Half a cylinder of radius 10 about the x axis, x from 0 to 40: a
/// rational quadratic in u (the half turn), linear in v (along x).
fn half_cylinder() -> ([u8; 16], Vec<u8>) {
    let w = std::f64::consts::FRAC_1_SQRT_2;
    let arc = [
        (0.0, 10.0, 1.0),
        (10.0, 10.0, w),
        (10.0, 0.0, 1.0),
        (10.0, -10.0, w),
        (0.0, -10.0, 1.0),
    ];
    let mut d = vec![0x10];
    i(&mut d, &[3, 1, 3, 2, 5, 2, 0, 0]);
    f(&mut d, &[0.0; 6]);
    i(&mut d, &[6]);
    f(&mut d, &[0.0, 0.0, 1.0, 1.0, 2.0, 2.0]);
    i(&mut d, &[2]);
    f(&mut d, &[0.0, 40.0]);
    i(&mut d, &[10]);
    for (z, y, w) in arc {
        for x in [0.0, 40.0] {
            f(&mut d, &[x * w, y * w, z * w, w]);
        }
    }
    (NURBS_SURFACE, d)
}

fn units(code: i32) -> Vec<u8> {
    let mut d = Vec::new();
    i(&mut d, &[100, code]);
    f(&mut d, &[0.001, 0.01, 0.01]);
    d
}

fn bounds(triangles: &[Triangle]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in triangles.iter().flatten() {
        for c in 0..3 {
            lo[c] = lo[c].min(p[c]);
            hi[c] = hi[c].max(p[c]);
        }
    }
    (lo, hi)
}

fn area(triangles: &[Triangle]) -> f64 {
    triangles
        .iter()
        .map(|t| {
            let (a, b) = (sub(t[1], t[0]), sub(t[2], t[0]));
            length([
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]) / 2.0
        })
        .sum()
}

#[test]
fn a_plate_with_a_hole_comes_in_trimmed_and_in_millimetres() {
    for version in [4, 50, 70, 80] {
        let w = Writer { version };
        let square = |a: f64, b: f64| {
            [
                line([a, a, 0.0], [b, a, 0.0], [0.0, 1.0]),
                line([b, a, 0.0], [b, b, 0.0], [0.0, 1.0]),
                line([b, b, 0.0], [a, b, 0.0], [0.0, 1.0]),
                line([a, b, 0.0], [a, a, 0.0], [0.0, 1.0]),
            ]
        };
        let mut curves = square(0.0, 4.0).to_vec();
        // The hole runs clockwise, as inner loops do.
        let mut hole = square(1.0, 3.0).to_vec();
        hole.reverse();
        for (_, data) in &mut hole {
            let mut r = super::Reader::new(data, 1, 49);
            let from = r.point().expect("point");
            let to = r.point().expect("point");
            *data = line(to, from, [0.0, 1.0]).1;
        }
        curves.extend(hole);
        let trims: Vec<(i32, [f64; 2])> = (0..8).map(|k| (k, [0.0, 1.0])).collect();
        let brep = w.brep(
            &curves,
            &[plane(10.0)],
            &trims,
            &[(vec![0, 1, 2, 3], 1), (vec![4, 5, 6, 7], 2)],
            &[(vec![0, 1], 0, false)],
            None,
        );
        let file = w.file_in(&[(ON_BREP, brep)], Some(units(8)));
        let read = read(&file).expect("reads");
        assert_eq!((read.surfaces, read.skipped), (1, 0), "{version}");
        assert_eq!(read.mm_per_unit, 25.4);
        let inch2 = 25.4 * 25.4;
        assert!(
            (area(&read.triangles) - 12.0 * inch2).abs() < 1e-6 * inch2,
            "{version}"
        );
        let (lo, hi) = bounds(&read.triangles);
        assert!((hi[0] - 4.0 * 25.4).abs() < 1e-9 && lo[0].abs() < 1e-9);
    }
}

#[test]
fn a_curved_face_trimmed_by_a_circle_stays_on_its_surface() {
    let w = Writer { version: 70 };
    // The face is the half cylinder cut to a disc in its parameter plane
    // (u 0 to 2 around, v 0 to 40 along), inside a polycurve outline.
    let outline = {
        let (_, a) = polyline(&[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 40.0, 0.0]]);
        let (_, b) = line([2.0, 40.0, 0.0], [0.0, 40.0, 0.0], [0.0, 1.0]);
        let (_, c) = line([0.0, 40.0, 0.0], [0.0, 0.0, 0.0], [5.0, 6.0]);
        let mut d = vec![0x10];
        i(&mut d, &[3, 0, 0]);
        f(&mut d, &[0.0; 6]);
        i(&mut d, &[4]);
        f(&mut d, &[0.0, 2.0, 3.0, 4.0]);
        d.extend(w.class(POLYLINE, &a));
        d.extend(w.class(LINE, &b));
        d.extend(w.class(LINE, &c));
        (POLYCURVE, d)
    };
    let mut hole = circle([1.0, 20.0], 0.5);
    // Clockwise: swap the circle's y about its centre.
    hole = {
        let data = hole.1;
        let mut r = super::Reader::new(&data, 1 + 24 + 48, data.len());
        let _ = r.i32();
        let knots: Vec<f64> = (0..10).map(|_| r.f64().expect("knot")).collect();
        let _ = r.i32();
        let cvs: Vec<[f64; 4]> = (0..9)
            .map(|_| {
                let x = r.f64().expect("x");
                let y = r.f64().expect("y");
                let w = r.f64().expect("w");
                [x, 40.0 * w - y, 0.0, w]
            })
            .collect();
        nurbs_curve(2, 3, &knots, &cvs, true)
    };
    let brep = w.brep(
        &[outline, hole],
        &[half_cylinder()],
        &[(0, [0.0, 4.0]), (1, [0.0, 4.0])],
        &[(vec![0], 1), (vec![1], 2)],
        &[(vec![0, 1], 0, false)],
        None,
    );
    let file = w.file(&[(ON_BREP, brep)]);
    let read = read(&file).expect("reads");
    assert_eq!(read.surfaces, 1);
    assert!(read.triangles.len() > 40);
    for p in read.triangles.iter().flatten() {
        assert!((p[1].hypot(p[2]) - 10.0).abs() < 0.05, "{p:?}");
        assert!(p[2] >= -1e-9);
    }
    // The hole: no point near the top of the arch at mid length.
    let near_hole = read
        .triangles
        .iter()
        .flatten()
        .any(|p| (p[0] - 20.0).abs() < 0.3 && p[2] > 9.9);
    assert!(!near_hole);
}

#[test]
fn faces_of_unknown_surfaces_use_the_render_mesh() {
    let w = Writer { version: 60 };
    let (v, f4) = cube();
    let mesh = w.mesh(&v, &f4, true);
    let unknown = [0x33; 16];
    let square = [
        line([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0]),
        line([1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0]),
        line([1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 1.0]),
        line([0.0, 1.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0]),
    ];
    let trims: Vec<(i32, [f64; 2])> = (0..4).map(|k| (k, [0.0, 1.0])).collect();
    let with_mesh = w.brep(
        &square,
        &[(unknown, vec![0x10])],
        &trims,
        &[(vec![0, 1, 2, 3], 1)],
        &[(vec![0], 0, false)],
        Some(vec![Some(mesh)]),
    );
    let without = w.brep(
        &square,
        &[(unknown, vec![0x10])],
        &trims,
        &[(vec![0, 1, 2, 3], 1)],
        &[(vec![0], 0, false)],
        None,
    );
    let read_one = read(&w.file(&[(ON_BREP, with_mesh.clone())])).expect("reads");
    assert_eq!((read_one.surfaces, read_one.skipped), (1, 0));
    assert_eq!(read_one.triangles.len(), 12);
    // Without a render mesh the face cannot be read, which fails the file.
    let error = read(&w.file(&[(ON_BREP, with_mesh), (ON_BREP, without)]))
        .expect_err("refused")
        .to_string();
    assert!(error.contains("polysurface"), "{error}");
}

#[test]
fn lone_surfaces_come_in_whole() {
    let w = Writer { version: 50 };
    let profile = line([5.0, 0.0, 0.0], [5.0, 0.0, 10.0], [0.0, 1.0]);
    let mut rev = vec![0x20];
    f(&mut rev, &[0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    f(&mut rev, &[0.0, std::f64::consts::TAU, 0.0, 1.0]);
    f(&mut rev, &[0.0; 6]);
    i(&mut rev, &[0]);
    rev.push(1);
    rev.extend(w.class(profile.0, &profile.1));
    let mut sum = vec![0x10];
    f(&mut sum, &[0.0, 0.0, 50.0]);
    f(&mut sum, &[0.0; 6]);
    let a = line([0.0; 3], [10.0, 0.0, 0.0], [0.0, 1.0]);
    let b = line([0.0; 3], [0.0, 10.0, 0.0], [0.0, 1.0]);
    sum.extend(w.class(a.0, &a.1));
    sum.extend(w.class(b.0, &b.1));
    let file = w.file(&[
        (REV_SURFACE, rev),
        (SUM_SURFACE, sum),
        (half_cylinder().0, half_cylinder().1),
    ]);
    let read = read(&file).expect("reads");
    assert_eq!((read.surfaces, read.skipped), (3, 0));
    let tube: Vec<Triangle> = read
        .triangles
        .iter()
        .copied()
        .filter(|t| t.iter().all(|p| (p[0].hypot(p[1]) - 5.0).abs() < 1e-6))
        .collect();
    let tube_area = 2.0 * std::f64::consts::PI * 5.0 * 10.0;
    assert!((area(&tube) - tube_area).abs() / tube_area < 0.01);
    let flat: f64 = area(
        &read
            .triangles
            .iter()
            .copied()
            .filter(|t| t.iter().all(|p| (p[2] - 50.0).abs() < 1e-9))
            .collect::<Vec<_>>(),
    );
    assert!((flat - 100.0).abs() < 1e-9);
}

#[test]
fn a_capped_extrusion_is_a_closed_cylinder() {
    let w = Writer { version: 70 };
    let mut arc = vec![0x10];
    f(
        &mut arc,
        &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    );
    f(&mut arc, &[0.0, 0.0, 1.0, 0.0, 5.0]);
    f(&mut arc, &[5.0, 0.0, 0.0, 0.0, 5.0, 0.0, -5.0, 0.0, 0.0]);
    f(&mut arc, &[0.0, std::f64::consts::TAU, 0.0, 1.0]);
    i(&mut arc, &[2]);
    let mut body = Vec::new();
    i(&mut body, &[1, 2]);
    body.extend(w.class(ARC, &arc));
    f(
        &mut body,
        &[0.0, 0.0, 0.0, 0.0, 0.0, 20.0, 0.0, 1.0, 0.0, 1.0, 0.0],
    );
    body.extend([0, 0]);
    f(&mut body, &[0.0; 6]);
    f(&mut body, &[0.0, 20.0]);
    body.push(0);
    i(&mut body, &[1]);
    body.extend([1, 1]);
    let data = w.chunk(ANONYMOUS, &body);
    let read = read(&w.file_in(&[(ON_EXTRUSION, data)], Some(units(2)))).expect("reads");
    assert_eq!(read.surfaces, 1);
    let (lo, hi) = bounds(&read.triangles);
    assert!(lo[2].abs() < 1e-9 && (hi[2] - 20.0).abs() < 1e-9);
    assert!((hi[0] - 5.0).abs() < 1e-9 && (lo[1] + 5.0).abs() < 0.01);
    let top: Vec<Triangle> = read
        .triangles
        .iter()
        .copied()
        .filter(|t| t.iter().all(|p| (p[2] - 20.0).abs() < 1e-9))
        .collect();
    let disc = std::f64::consts::PI * 25.0;
    assert!((area(&top) - disc).abs() / disc < 0.01);
}

#[test]
fn custom_units_scale_by_their_metres() {
    let w = Writer { version: 70 };
    let (v, f4) = cube();
    let mut custom = Vec::new();
    i(&mut custom, &[102, 11]);
    f(&mut custom, &[0.001, 0.01, 0.01]);
    i(&mut custom, &[0, 3]);
    f(&mut custom, &[0.1]);
    i(&mut custom, &[0]);
    let file = w.file_in(&[(ON_MESH, w.mesh(&v, &f4, true))], Some(custom));
    let read = read(&file).expect("reads");
    assert_eq!(read.mm_per_unit, 100.0);
    assert_eq!(bounds(&read.triangles).1[2], 1000.0);
    // Files with no units, or millimetres, keep their numbers.
    let plain = read_top(&w.file(&[(ON_MESH, w.mesh(&v, &f4, true))]));
    assert_eq!(plain, 10.0);
}

fn read_top(file: &[u8]) -> f64 {
    bounds(&read(file).expect("reads").triangles).1[2]
}

/// Files written by an independent 3DM writer (cadmpeg's Rhino codec,
/// Apache-2.0): a 10 mm cube polysurface, a 40 x 30 plate with a 10 mm
/// square hole and a lone bicubic NURBS patch over 60 x 60.
#[test]
fn files_from_another_writer_come_in() {
    let files: [(&str, &[u8]); 6] = [
        (
            "cube-v5",
            include_bytes!("../../../tests/fixtures/3dm/cube-v5.3dm"),
        ),
        (
            "cube-v8",
            include_bytes!("../../../tests/fixtures/3dm/cube-v8.3dm"),
        ),
        (
            "plate-v6",
            include_bytes!("../../../tests/fixtures/3dm/plate-v6.3dm"),
        ),
        (
            "plate-v7",
            include_bytes!("../../../tests/fixtures/3dm/plate-v7.3dm"),
        ),
        (
            "dome-v5",
            include_bytes!("../../../tests/fixtures/3dm/dome-v5.3dm"),
        ),
        (
            "dome-v8",
            include_bytes!("../../../tests/fixtures/3dm/dome-v8.3dm"),
        ),
    ];
    for (name, bytes) in files {
        let read = read(bytes).expect(name);
        assert_eq!(
            (read.meshes, read.surfaces, read.skipped),
            (0, 1, 0),
            "{name}"
        );
        assert_eq!(read.mm_per_unit, 1.0);
        let (lo, hi) = bounds(&read.triangles);
        let a = area(&read.triangles);
        if name.starts_with("cube") {
            assert!((a - 600.0).abs() < 1e-6, "{name} {a}");
            assert_eq!((lo, hi), ([0.0; 3], [10.0; 3]), "{name}");
        } else if name.starts_with("plate") {
            assert!((a - 1100.0).abs() < 1e-6, "{name} {a}");
            assert_eq!((lo, hi), ([0.0; 3], [40.0, 30.0, 0.0]), "{name}");
        } else {
            assert!((hi[2] - 11.25).abs() < 1e-9, "{name} {hi:?}");
            assert_eq!((lo[0], hi[0], lo[1], hi[1]), (0.0, 60.0, 0.0, 60.0));
            // Finer than one quad.
            assert!(read.triangles.len() > 50, "{name}");
        }
    }
}

mod mesh_3dm_review_tests {
    use super::*;
    use crate::io::mesh_3dm::tests::{cube, Writer};

    fn samples() -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = [
            &include_bytes!("../../../tests/fixtures/3dm/cube-v5.3dm")[..],
            &include_bytes!("../../../tests/fixtures/3dm/cube-v8.3dm")[..],
            &include_bytes!("../../../tests/fixtures/3dm/plate-v6.3dm")[..],
            &include_bytes!("../../../tests/fixtures/3dm/plate-v7.3dm")[..],
            &include_bytes!("../../../tests/fixtures/3dm/dome-v5.3dm")[..],
        ]
        .iter()
        .map(|b| b.to_vec())
        .collect();
        let (v, f) = cube();
        for (version, compressed) in [(4, false), (70, true)] {
            let w = Writer { version };
            out.push(w.file(&[(super::super::ON_MESH, w.mesh(&v, &f, compressed))]));
        }
        out
    }

    fn read_quietly(bytes: &[u8]) -> bool {
        std::panic::catch_unwind(|| {
            let _ = read(bytes);
        })
        .is_ok()
    }

    #[test]
    fn truncated_files_are_refused_without_a_panic() {
        for (n, sample) in samples().iter().enumerate() {
            for end in 0..sample.len() {
                assert!(read_quietly(&sample[..end]), "sample {n} cut at {end}");
            }
        }
    }

    #[test]
    fn scrambled_files_never_panic() {
        let mut seed = 0x9e37_79b9_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for (n, sample) in samples().iter().enumerate() {
            for round in 0..2000 {
                let mut bytes = sample.clone();
                let flips = 1 + next() as usize % 6;
                for _ in 0..flips {
                    let at = 32 + next() as usize % (bytes.len() - 32);
                    bytes[at] = match next() % 4 {
                        0 => 0xff,
                        1 => 0,
                        2 => 0x7f,
                        _ => next() as u8,
                    };
                }
                let started = std::time::Instant::now();
                assert!(read_quietly(&bytes), "sample {n} round {round}");
                assert!(
                    started.elapsed().as_secs_f64() < 5.0,
                    "sample {n} round {round} took {:?}",
                    started.elapsed()
                );
            }
        }
    }
}
