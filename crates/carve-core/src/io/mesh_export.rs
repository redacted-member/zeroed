//! Mesh exports of the model: binary STL, Wavefront OBJ (with a material
//! file and a texture of the component colours) and a POV-Ray scene holding
//! the mesh as a `mesh2`. The relief's surface, all of it or reduced to a
//! tolerance or a triangle budget ([`super::mesh_reduce`]), is closed on
//! the back as the export form asks: left open, with the front mirrored, or
//! with a flat plane. Lengths in millimetres.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path as FsPath;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::model::field::Field;

use super::mesh::{Triangle, Vertex};
use super::mesh_reduce::{self, Reduction, Surface};
use super::stl_export;

/// How the back of an exported model is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackFace {
    /// No back: a shell of the relief surface only.
    Open,
    /// The front mirrored below the base plane, a two sided solid.
    Inverted,
    /// Walls down to a flat plane.
    #[default]
    Flat,
    /// The other side of a double sided job's model as the back: `other`
    /// gives how deep its surface lies below this side's zero plane.
    OtherSide,
    /// A rotary job's model wrapped round its blank, the seam joined and
    /// both ends closed to the axis.
    Sides,
}

/// How a rotary job's model wraps round its blank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotaryWrap {
    /// Blank radius, mm: a distance round the blank of `radius` is one
    /// radian.
    pub radius: f64,
    /// True when the blank lies along X (the model's Y goes round it).
    pub along_x: bool,
    /// Radius of the model's zero plane, mm: height `h` lies at `zero + h`.
    pub zero: f64,
}

/// What a back other than a plane needs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Backing<'a> {
    /// For [`BackFace::OtherSide`]: how deep the other side's surface lies
    /// below this side's zero plane, cell for cell.
    pub other: Option<&'a Field>,
    /// For [`BackFace::Sides`]: the blank to wrap round.
    pub wrap: Option<RotaryWrap>,
}

/// The export's triangle count and how far it strays from the model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshStats {
    /// Triangles written, back and walls included.
    pub triangles: usize,
    /// Largest height difference between the model and the surface at any
    /// cell centre, millimetres.
    pub deviation: f64,
}

/// A picture laid over the model from above, one pixel per cell, for the
/// OBJ texture: RGB rows from the top (highest Y) down.
#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    /// Pixels across, the field's width.
    pub width: u32,
    /// Pixels down, the field's height.
    pub height: u32,
    /// RGB bytes, rows from the top.
    pub rgb: Vec<u8>,
}

/// The surface of `field` closed as `back` says, `base_z` being the flat
/// plane (and the mirror plane of an inverted back), with the stats.
///
/// A [`BackFace::OtherSide`] back needs `backing.other` and a
/// [`BackFace::Sides`] back `backing.wrap`; empty cells of `field` then sit
/// on the zero plane. Without them there are no triangles.
pub fn model_triangles(
    field: &Field,
    backing: Backing<'_>,
    base_z: f64,
    back: BackFace,
    reduction: Reduction,
) -> (Vec<Triangle>, MeshStats) {
    let (surface, triangles) = match (back, backing.other, backing.wrap) {
        (BackFace::OtherSide, Some(other), _) => {
            let surface = mesh_reduce::surface_pair(field, other, 0.0, reduction);
            let triangles = close(&surface, back, base_z);
            (surface, triangles)
        }
        (BackFace::Sides, _, Some(wrap)) => {
            let step = wrap_step(field, wrap, reduction);
            let keep = mesh_reduce::Keep::Wrap {
                rows: wrap.along_x,
                step,
            };
            let mut surface = mesh_reduce::surface_keeping(field, 0.0, reduction, keep);
            // Straight edges round the blank cut inside it by their sag.
            let peak = f64::from(field.max().unwrap_or(0.0)).max(0.0);
            let angle = step as f64 * field.cell / wrap.radius;
            let sag = (wrap.zero + peak) * (1.0 - (angle / 2.0).cos());
            if reduction != Reduction::Full {
                surface.deviation = surface.deviation.max(sag);
            }
            let triangles = wrap_rotary(&surface, wrap);
            (surface, triangles)
        }
        (BackFace::OtherSide | BackFace::Sides, _, _) => (Surface::default(), Vec::new()),
        _ => {
            let surface = mesh_reduce::surface(field, base_z, reduction);
            let triangles = close(&surface, back, base_z);
            (surface, triangles)
        }
    };
    let stats = MeshStats {
        triangles: triangles.len(),
        deviation: surface.deviation,
    };
    (triangles, stats)
}

/// How many cells round the blank a reduced wrapped surface may span with
/// one triangle: few enough that the chord of a triangle edge sags no more
/// than the tolerance, or, for a triangle budget, a lattice using half of
/// it. Every cell for a full surface.
fn wrap_step(field: &Field, wrap: RotaryWrap, reduction: Reduction) -> usize {
    let cells = field.width.max(1) * field.height.max(1);
    match reduction {
        Reduction::Full => 1,
        Reduction::Tolerance(tolerance) => {
            let peak = f64::from(field.max().unwrap_or(0.0)).max(0.0);
            let reach = (wrap.zero + peak).max(1e-9);
            // Sag of a chord across angle a is about reach a^2 / 8.
            let angle = (8.0 * tolerance / reach).sqrt();
            let per_cell = field.cell / wrap.radius;
            ((angle / per_cell).floor() as usize).max(1)
        }
        Reduction::MaxTriangles(budget) => {
            let lattice = (budget / 4).max(1);
            ((cells as f64 / lattice as f64).sqrt().ceil() as usize).max(1)
        }
    }
}

/// Wraps a surface round a rotary blank: every corner goes round the axis
/// by its distance across the blank, out to the zero plane's radius plus
/// its height; the first and last rows round the blank are joined and both
/// ends closed with a fan to the axis. The surface must keep its whole
/// edge rows (or columns for a blank along Y).
pub fn wrap_rotary(surface: &Surface, wrap: RotaryWrap) -> Vec<Triangle> {
    let v = &surface.vertices;
    if v.is_empty() || wrap.radius <= 0.0 {
        return Vec::new();
    }
    // (along, around) of a corner.
    let split = |p: &[f64; 3]| {
        if wrap.along_x {
            (p[0], p[1])
        } else {
            (p[1], p[0])
        }
    };
    let place = |p: &[f64; 3]| -> Vertex {
        let (along, around) = split(p);
        let (s, c) = (around / wrap.radius).sin_cos();
        let rho = (wrap.zero + p[2]).max(0.0);
        if wrap.along_x {
            [along, rho * s, rho * c]
        } else {
            [rho * s, along, rho * c]
        }
    };
    let mut out: Vec<Triangle> = surface
        .triangles
        .iter()
        .map(|t| [place(&v[t[0]]), place(&v[t[1]]), place(&v[t[2]])])
        .collect();
    let (lo_around, hi_around, lo_along, hi_along) = v.iter().fold(
        (f64::MAX, f64::MIN, f64::MAX, f64::MIN),
        |(a, b, c, d), p| {
            let (along, around) = split(p);
            (a.min(around), b.max(around), c.min(along), d.max(along))
        },
    );
    // Corners of a row round the blank, or an end, in order.
    let row = |around: f64| -> Vec<usize> {
        let mut at: Vec<usize> = (0..v.len()).filter(|&k| split(&v[k]).1 == around).collect();
        at.sort_by(|&a, &b| split(&v[a]).0.total_cmp(&split(&v[b]).0));
        at
    };
    let end = |along: f64| -> Vec<usize> {
        let mut at: Vec<usize> = (0..v.len()).filter(|&k| split(&v[k]).0 == along).collect();
        at.sort_by(|&a, &b| split(&v[a]).1.total_cmp(&split(&v[b]).1));
        at
    };
    // The seam: the last row round meets the first.
    let (first, last) = (row(lo_around), row(hi_around));
    if first.len() == last.len() {
        for k in 0..first.len().saturating_sub(1) {
            let (a, b) = (place(&v[last[k]]), place(&v[last[k + 1]]));
            let (c, d) = (place(&v[first[k + 1]]), place(&v[first[k]]));
            if wrap.along_x {
                out.push([a, b, c]);
                out.push([a, c, d]);
            } else {
                // Around runs along X here, so the square turns the other way.
                out.push([a, d, c]);
                out.push([a, c, b]);
            }
        }
    }
    // The ends: fans to the axis, each edge run the other way from the
    // surface's border so the solid is closed and faces out. Round the far
    // end of a blank along X the border runs forwards round the blank, so
    // the fan runs backwards; along Y it is the other way about.
    for (along, far) in [(hi_along, true), (lo_along, false)] {
        let ring = end(along);
        let hub = if wrap.along_x {
            [along, 0.0, 0.0]
        } else {
            [0.0, along, 0.0]
        };
        for k in 0..ring.len() {
            let a = place(&v[ring[k]]);
            let b = place(&v[ring[(k + 1) % ring.len()]]);
            if far == wrap.along_x {
                out.push([hub, b, a]);
            } else {
                out.push([hub, a, b]);
            }
        }
    }
    out
}

/// Closes a surface into triangles, counter clockwise seen from outside. An
/// [`BackFace::OtherSide`] back takes its heights from [`Surface::back`].
pub fn close(surface: &Surface, back: BackFace, base_z: f64) -> Vec<Triangle> {
    let v = &surface.vertices;
    let mut out: Vec<Triangle> = surface
        .triangles
        .iter()
        .map(|t| [v[t[0]], v[t[1]], v[t[2]]])
        .collect();
    let ring = &surface.ring;
    let below = |index: usize| -> Vertex {
        let p = v[index];
        match back {
            BackFace::Inverted => [p[0], p[1], 2.0 * base_z - p[2]],
            BackFace::OtherSide => [p[0], p[1], -surface.back.get(index).copied().unwrap_or(0.0)],
            _ => [p[0], p[1], base_z],
        }
    };
    match back {
        // A wrapped model is closed by wrap_rotary.
        BackFace::Open | BackFace::Sides => {}
        BackFace::Inverted | BackFace::OtherSide => {
            out.extend(
                surface
                    .triangles
                    .iter()
                    .map(|t| [below(t[0]), below(t[2]), below(t[1])]),
            );
        }
        BackFace::Flat => {
            if let (Some(lo), Some(hi)) = (
                v.iter().map(|p| p[0]).reduce(f64::min),
                v.iter().map(|p| p[0]).reduce(f64::max),
            ) {
                let (y_lo, y_hi) = v
                    .iter()
                    .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
                let middle = [(lo + hi) * 0.5, (y_lo + y_hi) * 0.5, base_z];
                for k in 0..ring.len() {
                    let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
                    out.push([middle, below(b), below(a)]);
                }
            }
        }
    }
    if !matches!(back, BackFace::Open | BackFace::Sides) {
        for k in 0..ring.len() {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            let (at, bt, ab, bb) = (v[a], v[b], below(a), below(b));
            out.push([ab, bb, bt]);
            out.push([ab, bt, at]);
        }
    }
    out
}

/// Writes the model: `field`'s surface reduced as asked, closed as `back`
/// says down to `base_z`, in `format`. An OBJ also gets `<name>.mtl` and,
/// with a texture, `<name>.jpg` beside it.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the field is smaller than two by
/// two cells, and [`CoreError::Io`] when a file cannot be written.
#[allow(clippy::too_many_arguments)]
pub fn write_model(
    field: &Field,
    backing: Backing<'_>,
    base_z: f64,
    back: BackFace,
    reduction: Reduction,
    format: MeshFormat,
    texture: Option<&Texture>,
    path: &FsPath,
) -> Result<MeshStats, CoreError> {
    let (triangles, stats) = model_triangles(field, backing, base_z, back, reduction);
    if triangles.is_empty() {
        return Err(CoreError::validation(
            "a relief needs at least two by two cells to export",
        ));
    }
    let bytes = match format {
        MeshFormat::Stl => stl_export::stl_of(&triangles),
        MeshFormat::Pov => pov_text(&IndexedMesh::from_triangles(&triangles)).into_bytes(),
        MeshFormat::Obj => {
            let mesh = IndexedMesh::from_triangles(&triangles);
            let stem = path
                .file_stem()
                .map_or_else(|| "model".to_string(), |s| s.to_string_lossy().into_owned());
            let mtl = path.with_extension("mtl");
            let jpg = path.with_extension("jpg");
            let mut material = String::from("# Zeroed model material\nnewmtl model\n");
            material.push_str("Ka 0.2 0.2 0.2\nKd 1 1 1\nKs 0 0 0\nillum 1\n");
            if let Some(texture) = texture {
                let _ = writeln!(material, "map_Kd {stem}.jpg");
                let picture =
                    image::RgbImage::from_raw(texture.width, texture.height, texture.rgb.clone())
                        .ok_or_else(|| {
                        CoreError::validation("the model texture does not fit its size")
                    })?;
                picture
                    .save_with_format(&jpg, image::ImageFormat::Jpeg)
                    .map_err(|error| CoreError::io(jpg.display(), error))?;
            }
            std::fs::write(&mtl, material).map_err(|error| CoreError::io(mtl.display(), error))?;
            textured_obj_text(&mesh, &stem, field).into_bytes()
        }
    };
    std::fs::write(path, bytes).map_err(|error| CoreError::io(path.display(), error))?;
    Ok(stats)
}

/// Mesh formats the model can be written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeshFormat {
    /// Binary STL.
    Stl,
    /// Wavefront OBJ.
    Obj,
    /// POV-Ray scene with the model as a `mesh2`.
    Pov,
}

impl MeshFormat {
    /// The usual file extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            MeshFormat::Stl => "stl",
            MeshFormat::Obj => "obj",
            MeshFormat::Pov => "pov",
        }
    }
}

/// A mesh with shared corners: positions and triangles indexing them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IndexedMesh {
    /// Corner positions.
    pub vertices: Vec<Vertex>,
    /// Triangles as indices into `vertices`, counter clockwise seen from
    /// outside.
    pub faces: Vec<[usize; 3]>,
}

impl IndexedMesh {
    /// Merges corners that sit at exactly the same position. Indices follow
    /// the order corners are first met, so the result is deterministic.
    pub fn from_triangles(triangles: &[Triangle]) -> IndexedMesh {
        let mut index: HashMap<[u64; 3], usize> = HashMap::with_capacity(triangles.len());
        let mut mesh = IndexedMesh {
            vertices: Vec::with_capacity(triangles.len() / 2 + 3),
            faces: Vec::with_capacity(triangles.len()),
        };
        for t in triangles {
            let mut face = [0usize; 3];
            for (slot, v) in face.iter_mut().zip(t) {
                let key = [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()];
                let next = mesh.vertices.len();
                let at = *index.entry(key).or_insert(next);
                if at == next {
                    mesh.vertices.push(*v);
                }
                *slot = at;
            }
            mesh.faces.push(face);
        }
        mesh
    }
}

/// Writes `field` as a closed solid down to `base_z` millimetres in
/// `format` to `path`.
///
/// # Errors
/// Returns [`CoreError::Validation`] when the field is smaller than two by
/// two cells, and [`CoreError::Io`] when the file cannot be written.
pub fn write(
    field: &Field,
    base_z: f64,
    format: MeshFormat,
    path: &FsPath,
) -> Result<(), CoreError> {
    if format == MeshFormat::Stl {
        return stl_export::write_stl(field, base_z, path);
    }
    let triangles = stl_export::solid_triangles(field, base_z);
    if triangles.is_empty() {
        return Err(CoreError::validation(
            "a relief needs at least two by two cells to export",
        ));
    }
    let mesh = IndexedMesh::from_triangles(&triangles);
    let text = match format {
        MeshFormat::Obj => obj_text(&mesh, "model"),
        _ => pov_text(&mesh),
    };
    std::fs::write(path, text).map_err(|error| CoreError::io(path.display(), error))
}

/// Six significant decimals is a micron, far finer than any model cell.
fn number(out: &mut String, v: f64) {
    let rounded = (v * 1e6).round() / 1e6;
    let _ = write!(out, "{}", if rounded == 0.0 { 0.0 } else { rounded });
}

/// Writes `label <x, z, y>`, a point in POV-Ray's axes. An empty label
/// writes the bare vector.
fn pov_point(out: &mut String, label: &str, p: [f64; 3]) {
    if !label.is_empty() {
        out.push_str(label);
        out.push(' ');
    }
    out.push('<');
    number(out, p[0]);
    out.push_str(", ");
    number(out, p[2]);
    out.push_str(", ");
    number(out, p[1]);
    out.push('>');
}

/// A Wavefront OBJ of `mesh` as one object named `name`, `v` then `f`
/// lines with 1 based indices.
pub fn obj_text(mesh: &IndexedMesh, name: &str) -> String {
    let mut out = String::with_capacity(mesh.vertices.len() * 32 + mesh.faces.len() * 24);
    out.push_str("# Zeroed model, millimeters, Z up\n");
    let _ = writeln!(out, "o {name}");
    for v in &mesh.vertices {
        out.push('v');
        for c in v {
            out.push(' ');
            number(&mut out, *c);
        }
        out.push('\n');
    }
    for f in &mesh.faces {
        let _ = writeln!(out, "f {} {} {}", f[0] + 1, f[1] + 1, f[2] + 1);
    }
    out
}

/// A Wavefront OBJ of `mesh` using material `model` from `<stem>.mtl`, each
/// corner with texture coordinates that lay the texture over `field` from
/// above, pixel centres on cell centres.
pub fn textured_obj_text(mesh: &IndexedMesh, stem: &str, field: &Field) -> String {
    let mut out = String::with_capacity(mesh.vertices.len() * 56 + mesh.faces.len() * 36);
    out.push_str(
        "# Zeroed model, millimeters, Z up
",
    );
    let _ = writeln!(out, "mtllib {stem}.mtl");
    out.push_str(
        "o model
",
    );
    for v in &mesh.vertices {
        out.push('v');
        for c in v {
            out.push(' ');
            number(&mut out, *c);
        }
        out.push('\n');
    }
    let (w, h) = (field.width.max(1) as f64, field.height.max(1) as f64);
    let cell = if field.cell > 0.0 { field.cell } else { 1.0 };
    for v in &mesh.vertices {
        let u = ((v[0] - field.origin.x) / cell + 0.5) / w;
        let t = ((v[1] - field.origin.y) / cell + 0.5) / h;
        out.push_str("vt ");
        number(&mut out, u.clamp(0.0, 1.0));
        out.push(' ');
        number(&mut out, t.clamp(0.0, 1.0));
        out.push('\n');
    }
    out.push_str("usemtl model\n");
    for f in &mesh.faces {
        let (a, b, c) = (f[0] + 1, f[1] + 1, f[2] + 1);
        let _ = writeln!(out, "f {a}/{a} {b}/{b} {c}/{c}");
    }
    out
}

/// A POV-Ray scene: the mesh as `#declare CarveModel = mesh2 {...}`, placed
/// with a wood coloured finish under a camera looking down at it and two
/// lights.
///
/// POV-Ray is left handed with Y up, so each corner is written as
/// `<x, z, y>`: that turns Z up into Y up and flips the handedness in the
/// same step, so the model is not mirrored.
pub fn pov_text(mesh: &IndexedMesh) -> String {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for v in &mesh.vertices {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    if mesh.vertices.is_empty() {
        lo = [0.0; 3];
        hi = [0.0; 3];
    }
    let centre = [
        (lo[0] + hi[0]) * 0.5,
        (lo[1] + hi[1]) * 0.5,
        (lo[2] + hi[2]) * 0.5,
    ];
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1.0);

    let mut out = String::with_capacity(mesh.vertices.len() * 36 + mesh.faces.len() * 24 + 1024);
    out.push_str("// Zeroed model, millimeters\n");
    out.push_str("#version 3.7;\n");
    out.push_str("global_settings { assumed_gamma 1.0 }\n\n");
    out.push_str("#declare CarveModel = mesh2 {\n  vertex_vectors {\n");
    let _ = write!(out, "    {}", mesh.vertices.len());
    for v in &mesh.vertices {
        out.push_str(",\n    <");
        number(&mut out, v[0]);
        out.push_str(", ");
        number(&mut out, v[2]);
        out.push_str(", ");
        number(&mut out, v[1]);
        out.push('>');
    }
    out.push_str("\n  }\n  face_indices {\n");
    let _ = write!(out, "    {}", mesh.faces.len());
    for f in &mesh.faces {
        let _ = write!(out, ",\n    <{}, {}, {}>", f[0], f[1], f[2]);
    }
    out.push_str("\n  }\n}\n\n");

    out.push_str("camera {\n  ");
    pov_point(
        &mut out,
        "location",
        [centre[0], centre[1] - span * 0.6, hi[2] + span * 1.2],
    );
    out.push_str("\n  ");
    pov_point(&mut out, "look_at", centre);
    out.push_str("\n  angle 45\n}\n");
    out.push_str("light_source { ");
    pov_point(
        &mut out,
        "",
        [centre[0] - span, centre[1] - span, hi[2] + span * 2.0],
    );
    out.push_str(" color rgb <1, 1, 1> }\n");
    out.push_str("light_source { ");
    pov_point(
        &mut out,
        "",
        [centre[0] + span, centre[1] + span, hi[2] + span],
    );
    out.push_str(" color rgb <0.4, 0.4, 0.4> }\n");
    out.push_str("background { color rgb <0.9, 0.9, 0.92> }\n\n");
    out.push_str(
        "object {\n  CarveModel\n  texture {\n    pigment { color rgb <0.82, 0.64, 0.42> }\n    finish { ambient 0.15 diffuse 0.8 specular 0.1 }\n  }\n}\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point;
    use crate::io::mesh::{obj, stl};

    fn field() -> Field {
        let mut f = Field::empty(4, 3, 2.0, Point::new(1.0, 1.0));
        for j in 0..3 {
            for i in 0..4 {
                f.z[j * 4 + i] = 1.0 + (i + j) as f32 * 0.25;
            }
        }
        f
    }

    fn sorted(mut t: Vec<Triangle>) -> Vec<Triangle> {
        t.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        t
    }

    /// Every edge of a closed mesh is used once in each direction.
    fn is_watertight(triangles: &[Triangle]) -> bool {
        let mesh = IndexedMesh::from_triangles(triangles);
        let mut edges: HashMap<(usize, usize), i32> = HashMap::new();
        for f in &mesh.faces {
            for k in 0..3 {
                let (a, b) = (f[k], f[(k + 1) % 3]);
                if a != b {
                    *edges.entry((a, b)).or_insert(0) += 1;
                }
            }
        }
        edges
            .iter()
            .all(|(&(a, b), &n)| n == 1 && edges.get(&(b, a)) == Some(&1))
    }

    /// Signed volume by the divergence theorem, positive when the normals
    /// point out.
    fn volume(triangles: &[Triangle]) -> f64 {
        triangles
            .iter()
            .map(|[a, b, c]| {
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0
            })
            .sum()
    }

    fn dome() -> Field {
        let mut f = Field::empty(25, 19, 1.0, Point::new(0.0, 0.0));
        for j in 0..19 {
            for i in 0..25 {
                let (x, y) = (i as f32 - 12.0, j as f32 - 9.0);
                f.z[j * 25 + i] = (60.0 - x * x - y * y).max(0.0) / 10.0;
            }
        }
        f
    }

    #[test]
    fn every_back_closes_a_solid_the_right_way_out() {
        let f = dome();
        for reduction in [
            Reduction::Full,
            Reduction::Tolerance(0.05),
            Reduction::MaxTriangles(120),
        ] {
            let (flat, stats) =
                model_triangles(&f, Backing::default(), -1.0, BackFace::Flat, reduction);
            assert!(is_watertight(&flat), "{reduction:?}");
            assert_eq!(stats.triangles, flat.len());
            let (inverted, _) =
                model_triangles(&f, Backing::default(), -1.0, BackFace::Inverted, reduction);
            assert!(is_watertight(&inverted), "{reduction:?}");
            assert!(volume(&flat) > 0.0 && volume(&inverted) > volume(&flat));
            let (open, _) =
                model_triangles(&f, Backing::default(), -1.0, BackFace::Open, reduction);
            assert!(!is_watertight(&open));
            assert!(open.len() < flat.len());
            let (both, _) = model_triangles(
                &f,
                Backing {
                    other: Some(&f),
                    wrap: None,
                },
                0.0,
                BackFace::OtherSide,
                reduction,
            );
            assert!(is_watertight(&both), "{reduction:?}");
            assert!(volume(&both) > volume(&flat) - 1.0 * 24.0 * 18.0);
            let (none, _) =
                model_triangles(&f, Backing::default(), 0.0, BackFace::OtherSide, reduction);
            assert!(none.is_empty());
        }
        let (full, _) = model_triangles(
            &f,
            Backing::default(),
            -1.0,
            BackFace::Flat,
            Reduction::Full,
        );
        let (reduced, stats) = model_triangles(
            &f,
            Backing::default(),
            -1.0,
            BackFace::Flat,
            Reduction::Tolerance(0.05),
        );
        assert!(reduced.len() < full.len() / 2);
        assert!(stats.deviation <= 0.05);
        // The flat solid is the same shape as the one the plain STL export builds.
        let same = stl_export::solid_triangles(&f, -1.0);
        assert!((volume(&full) - volume(&same)).abs() < 1e-6);
    }

    #[test]
    fn a_rotary_model_wraps_into_a_closed_solid() {
        // A flat model 2 pi 10 round wraps into a cylinder of radius 10; a
        // bump raises part of it.
        let round = std::f64::consts::TAU * 10.0;
        let (w, h) = (21, 64);
        let cell = round / h as f64;
        let mut f = Field::empty(w, h, cell, Point::new(0.0, cell / 2.0));
        for j in 0..h {
            for i in 0..w {
                f.z[j * w + i] = if (8..12).contains(&i) && j < 8 {
                    2.0
                } else {
                    0.0
                };
            }
        }
        let wrap = RotaryWrap {
            radius: 10.0,
            along_x: true,
            zero: 10.0,
        };
        for reduction in [Reduction::Full, Reduction::Tolerance(0.01)] {
            let (solid, _) = model_triangles(
                &f,
                Backing {
                    other: None,
                    wrap: Some(wrap),
                },
                0.0,
                BackFace::Sides,
                reduction,
            );
            assert!(is_watertight(&solid), "{reduction:?}");
            let cylinder = std::f64::consts::PI * 100.0 * (w - 1) as f64 * cell;
            let v = volume(&solid);
            assert!(v > cylinder * 0.97 && v < cylinder * 1.1, "{v} {cylinder}");
            for t in &solid {
                for p in t {
                    assert!(p[1].hypot(p[2]) <= 12.0 + 1e-9);
                }
            }
        }
        // Along Y the same model wraps just as well.
        let mut g = Field::empty(h, w, cell, Point::new(cell / 2.0, 0.0));
        for j in 0..w {
            for i in 0..h {
                g.z[j * h + i] = f.z[i * w + j];
            }
        }
        let (solid, _) = model_triangles(
            &g,
            Backing {
                other: None,
                wrap: Some(RotaryWrap {
                    along_x: false,
                    ..wrap
                }),
            },
            0.0,
            BackFace::Sides,
            Reduction::Tolerance(0.01),
        );
        assert!(is_watertight(&solid));
        assert!(volume(&solid) > 0.0);
        // No blank, no solid.
        let (none, _) = model_triangles(
            &f,
            Backing::default(),
            0.0,
            BackFace::Sides,
            Reduction::Full,
        );
        assert!(none.is_empty());
    }

    #[test]
    fn a_textured_obj_comes_with_its_material_and_picture() {
        let dir = crate::test_support::TempDir::new("obj-texture");
        let path = dir.path().join("sign.obj");
        let f = field();
        let texture = Texture {
            width: 4,
            height: 3,
            rgb: (0..36).map(|k| if k < 12 { 255 } else { 0 }).collect(),
        };
        let stats = write_model(
            &f,
            Backing::default(),
            0.0,
            BackFace::Flat,
            Reduction::Full,
            MeshFormat::Obj,
            Some(&texture),
            &path,
        )
        .expect("writes");
        let text = std::fs::read_to_string(&path).expect("obj");
        assert!(text.contains("mtllib sign.mtl") && text.contains("usemtl model"));
        assert!(text.contains("vt 0.125 0.166667"));
        assert_eq!(obj(&text).expect("reads").len(), stats.triangles);
        let material = std::fs::read_to_string(dir.path().join("sign.mtl")).expect("mtl");
        assert!(material.contains("map_Kd sign.jpg"));
        let picture = image::open(dir.path().join("sign.jpg"))
            .expect("jpg")
            .to_rgb8();
        assert_eq!(picture.dimensions(), (4, 3));
        // The first row written is the top of the model.
        assert!(picture.get_pixel(0, 0).0[0] > 200 && picture.get_pixel(0, 2).0[0] < 60);
        for format in [MeshFormat::Stl, MeshFormat::Pov] {
            let out = dir.path().join(format!("m.{}", format.extension()));
            write_model(
                &f,
                Backing::default(),
                0.0,
                BackFace::Open,
                Reduction::Full,
                format,
                None,
                &out,
            )
            .expect("writes");
            assert!(!dir.path().join("m.mtl").exists());
        }
    }

    #[test]
    fn corners_are_shared() {
        let t = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let u = [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let mesh = IndexedMesh::from_triangles(&[t, u]);
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.faces, vec![[0, 1, 2], [1, 3, 2]]);
    }

    #[test]
    fn obj_round_trips_through_the_obj_reader() {
        let f = field();
        let triangles = stl_export::solid_triangles(&f, 0.0);
        let mesh = IndexedMesh::from_triangles(&triangles);
        // Grid corners on top, the same on the bottom ring, and the bottom middle.
        assert_eq!(mesh.vertices.len(), 12 + 10 + 1);
        let back = obj(&obj_text(&mesh, "model")).expect("reads");
        assert_eq!(sorted(back), sorted(triangles));
    }

    #[test]
    fn obj_matches_the_stl_triangles() {
        let f = field();
        let from_stl = stl(&stl_export::stl_bytes(&f, -1.0)).expect("stl");
        let mesh = IndexedMesh::from_triangles(&stl_export::solid_triangles(&f, -1.0));
        let from_obj = obj(&obj_text(&mesh, "m")).expect("obj");
        assert_eq!(from_obj.len(), from_stl.len());
        for (a, b) in sorted(from_obj).iter().zip(sorted(from_stl).iter()) {
            for (p, q) in a.iter().zip(b) {
                for k in 0..3 {
                    assert!((p[k] - q[k]).abs() < 1e-5);
                }
            }
        }
    }

    /// Reads the vertex and face lists back out of a POV scene.
    fn parse_pov(text: &str) -> (Vec<Vertex>, Vec<[usize; 3]>) {
        let block = |name: &str| -> Vec<Vec<f64>> {
            let start = text.find(name).expect("block") + name.len();
            let body = &text[start..];
            let body = &body[body.find('{').expect("open") + 1..body.find('}').expect("close")];
            body.split('<')
                .skip(1)
                .map(|item| {
                    item.split('>')
                        .next()
                        .expect("item")
                        .split(',')
                        .map(|n| n.trim().parse::<f64>().expect("number"))
                        .collect()
                })
                .collect()
        };
        let vertices = block("vertex_vectors")
            .into_iter()
            .map(|v| [v[0], v[2], v[1]])
            .collect();
        let faces = block("face_indices")
            .into_iter()
            .map(|f| [f[0] as usize, f[1] as usize, f[2] as usize])
            .collect();
        (vertices, faces)
    }

    #[test]
    fn pov_holds_the_mesh_with_y_up() {
        let f = field();
        let mesh = IndexedMesh::from_triangles(&stl_export::solid_triangles(&f, 0.0));
        let text = pov_text(&mesh);
        assert!(text.contains("#declare CarveModel = mesh2"));
        assert!(text.contains("object {\n  CarveModel"));
        assert!(text.contains("camera"));
        let (vertices, faces) = parse_pov(&text);
        assert_eq!(faces, mesh.faces);
        assert_eq!(vertices.len(), mesh.vertices.len());
        for (a, b) in vertices.iter().zip(&mesh.vertices) {
            for k in 0..3 {
                assert!((a[k] - b[k]).abs() < 1e-6);
            }
        }
        // The counts in the file match the lists.
        assert!(text.contains(&format!("    {},\n", mesh.vertices.len())));
        assert!(text.contains(&format!("    {},\n", mesh.faces.len())));
    }

    #[test]
    fn files_are_written_by_format() {
        let dir = std::env::temp_dir().join("carve_mesh_export_test");
        std::fs::create_dir_all(&dir).expect("dir");
        let f = field();
        for format in [MeshFormat::Stl, MeshFormat::Obj, MeshFormat::Pov] {
            let path = dir.join(format!("model.{}", format.extension()));
            write(&f, 0.0, format, &path).expect("writes");
            let back = crate::io::mesh::read(&path);
            match format {
                MeshFormat::Pov => assert!(back.is_err()),
                _ => assert_eq!(back.expect("reads").len(), 42),
            }
        }
        let tiny = Field::empty(1, 3, 1.0, Point::new(0.0, 0.0));
        assert!(write(&tiny, 0.0, MeshFormat::Obj, &dir.join("tiny.obj")).is_err());
        assert_eq!(
            serde_json::from_str::<MeshFormat>("\"pov\"").expect("parses"),
            MeshFormat::Pov
        );
    }
}
