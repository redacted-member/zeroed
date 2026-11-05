//! Export Assembly: the placed pieces as one STL or 3MF, in assembly
//! space, millimetres. A piece with an exact solid is written from it (its
//! walls, floors, top and underside); any other from its simulation relief,
//! a column per grid cell.

use std::io::Write;

use serde::Deserialize;
use serde_json::json;

use crate::cam::sim::{Quality, Stock, THROUGH};
use crate::doc::Document;
use crate::error::CoreError;
use crate::io::mesh::{Triangle, Vertex};
use crate::io::stl_export::stl_of;
use crate::kernel::KernelReply;

use super::exact::ExactSolid;
use super::fit::{self, Face};
use super::pieces::PartsCache;
use super::pose::{self, Pose};
use super::triangulate::triangulate;

/// The file an export writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeshFormat {
    /// Binary STL, every piece in one solid.
    Stl,
    /// 3MF, each piece its own object.
    #[serde(rename = "3mf")]
    ThreeMf,
}

fn corner(face: &Face, p: [f64; 2]) -> Vertex {
    face.at(p)
}

/// The triangles of one planar face, wound counter clockwise seen from
/// outside (along its normal).
fn face_triangles(face: &Face, out: &mut Vec<Triangle>) {
    for shape in &face.shapes {
        let Some(outer) = shape.first() else {
            continue;
        };
        let holes: Vec<Vec<[f64; 2]>> = shape[1..].to_vec();
        let flat: Vec<[f64; 2]> = shape.iter().flatten().copied().collect();
        for [a, b, c] in triangulate(outer, &holes) {
            let (Some(pa), Some(pb), Some(pc)) = (flat.get(a), flat.get(b), flat.get(c)) else {
                continue;
            };
            let t = [corner(face, *pa), corner(face, *pb), corner(face, *pc)];
            let n = pose::cross(pose::sub(t[1], t[0]), pose::sub(t[2], t[0]));
            out.push(if pose::dot(n, face.n) >= 0.0 {
                t
            } else {
                [t[0], t[2], t[1]]
            });
        }
    }
}

/// An exact solid's surface in its own frame.
pub fn exact_triangles(solid: &ExactSolid) -> Vec<Triangle> {
    let mut out = Vec::new();
    for face in fit::faces_of(solid) {
        face_triangles(&face, &mut out);
    }
    out
}

/// A relief's surface in its piece's frame: each cell of material a column
/// from its underside to its top, walled where its neighbour's column does
/// not cover it.
pub fn relief_triangles(stock: &Stock) -> Vec<Triangle> {
    let (w, h, cell) = (stock.width, stock.height, stock.cell);
    let column = |x: i64, y: i64| -> Option<(f64, f64)> {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            return None;
        }
        let i = y as usize * w + x as usize;
        (stock.floor[i] < stock.z[i] - THROUGH).then(|| {
            (
                f64::from(stock.floor[i]) - stock.bottom,
                f64::from(stock.z[i]) - stock.bottom,
            )
        })
    };
    let mut out = Vec::new();
    let quad = |out: &mut Vec<Triangle>, a: Vertex, b: Vertex, c: Vertex, d: Vertex| {
        out.push([a, b, c]);
        out.push([a, c, d]);
    };
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let Some((lo, hi)) = column(x, y) else {
                continue;
            };
            let (x0, y0) = (x as f64 * cell, y as f64 * cell);
            let (x1, y1) = (x0 + cell, y0 + cell);
            quad(
                &mut out,
                [x0, y0, hi],
                [x1, y0, hi],
                [x1, y1, hi],
                [x0, y1, hi],
            );
            quad(
                &mut out,
                [x0, y0, lo],
                [x0, y1, lo],
                [x1, y1, lo],
                [x1, y0, lo],
            );
            // Each side: its two corners (counter clockwise seen from
            // outside) and the neighbour beyond it.
            let sides = [
                ([x1, y0], [x1, y1], (x + 1, y)),
                ([x1, y1], [x0, y1], (x, y + 1)),
                ([x0, y1], [x0, y0], (x - 1, y)),
                ([x0, y0], [x1, y0], (x, y - 1)),
            ];
            for (p, q, (nx, ny)) in sides {
                let spans: Vec<(f64, f64)> = match column(nx, ny) {
                    None => vec![(lo, hi)],
                    Some((nlo, nhi)) => {
                        let mut s = Vec::new();
                        if hi > nhi {
                            s.push((lo.max(nhi), hi));
                        }
                        if nlo > lo {
                            s.push((lo, hi.min(nlo)));
                        }
                        s
                    }
                };
                for (a, b) in spans {
                    if b > a {
                        quad(
                            &mut out,
                            [p[0], p[1], a],
                            [q[0], q[1], a],
                            [q[0], q[1], b],
                            [p[0], p[1], b],
                        );
                    }
                }
            }
        }
    }
    out
}

fn moved(triangles: Vec<Triangle>, pose: &Pose) -> Vec<Triangle> {
    triangles
        .into_iter()
        .map(|t| t.map(|v| pose.apply(v)))
        .collect()
}

/// A 3MF package holding each named mesh as an object, all built in place.
///
/// # Errors
/// [`CoreError::Format`] when the package cannot be written.
pub fn three_mf(objects: &[(String, Vec<Triangle>)]) -> Result<Vec<u8>, CoreError> {
    let mut model = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n<resources>\n",
    );
    let escape = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    for (index, (name, triangles)) in objects.iter().enumerate() {
        model.push_str(&format!(
            "<object id=\"{}\" type=\"model\" name=\"{}\">\n<mesh>\n<vertices>\n",
            index + 1,
            escape(name)
        ));
        for t in triangles {
            for v in t {
                model.push_str(&format!(
                    "<vertex x=\"{:.6}\" y=\"{:.6}\" z=\"{:.6}\"/>\n",
                    v[0], v[1], v[2]
                ));
            }
        }
        model.push_str("</vertices>\n<triangles>\n");
        for k in 0..triangles.len() {
            model.push_str(&format!(
                "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>\n",
                3 * k,
                3 * k + 1,
                3 * k + 2
            ));
        }
        model.push_str("</triangles>\n</mesh>\n</object>\n");
    }
    model.push_str("</resources>\n<build>\n");
    for index in 0..objects.len() {
        model.push_str(&format!("<item objectid=\"{}\"/>\n", index + 1));
    }
    model.push_str("</build>\n</model>\n");
    let types = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\"/></Types>\n";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Target=\"/3D/3dmodel.model\" Id=\"rel0\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/></Relationships>\n";
    let encode = |e: std::io::Error| CoreError::format("3MF", e);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, body) in [
        ("[Content_Types].xml", types),
        ("_rels/.rels", rels),
        ("3D/3dmodel.model", model.as_str()),
    ] {
        zip.start_file(name, options)
            .map_err(|e| CoreError::format("3MF", e))?;
        zip.write_all(body.as_bytes()).map_err(encode)?;
    }
    let cursor = zip.finish().map_err(|e| CoreError::format("3MF", e))?;
    Ok(cursor.into_inner())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportArgs {
    path: String,
    format: MeshFormat,
    #[serde(default)]
    quality: Quality,
}

/// `assembly.exportAssembly`: writes the placed pieces (as assembled) to
/// `path` as one STL or 3MF. Reads the document only.
///
/// # Errors
/// [`CoreError::InvalidArgs`] for arguments that do not parse,
/// [`CoreError::Validation`] with no pieces, [`CoreError::Io`] when the
/// file cannot be written.
pub fn export(
    document: &Document,
    cache: &mut PartsCache,
    args: &str,
) -> Result<KernelReply, CoreError> {
    let args: ExportArgs = serde_json::from_str(args)
        .map_err(|error| CoreError::invalid_args("assembly.exportAssembly", error))?;
    let solids = fit::solids(document, cache, args.quality)?;
    if solids.is_empty() {
        return Err(CoreError::validation("there are no pieces to export"));
    }
    let mut objects = Vec::new();
    let mut exact = 0usize;
    for held in &solids {
        let local = match &held.solid {
            Some(solid) => {
                exact += 1;
                exact_triangles(solid)
            }
            None => relief_triangles(&held.stock),
        };
        objects.push((held.name.clone(), moved(local, &held.pose)));
    }
    let bytes = match args.format {
        MeshFormat::Stl => {
            let all: Vec<Triangle> = objects
                .iter()
                .flat_map(|(_, t)| t.iter().copied())
                .collect();
            stl_of(&all)
        }
        MeshFormat::ThreeMf => three_mf(&objects)?,
    };
    let path = std::path::Path::new(&args.path);
    std::fs::write(path, &bytes).map_err(|error| CoreError::io(path.display(), error))?;
    let triangles: usize = objects.iter().map(|(_, t)| t.len()).sum();
    KernelReply::ok(&json!({
        "path": args.path,
        "pieces": objects.len(),
        "exact": exact,
        "triangles": triangles,
    }))
}
