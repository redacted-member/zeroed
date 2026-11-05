//! 3D meshes from DXF drawings: every `3DFACE` entity (a triangle, or a
//! quad split into two) and every polyface mesh (a `POLYLINE` with its
//! polyface flag, whose face records index its position vertices). Lengths
//! come through as the file wrote them. Entities inside blocks are not
//! followed.

use dxf::entities::{EntityType, Face3D, Polyline};
use dxf::{Drawing, Point as DxfPoint};

use crate::error::CoreError;

use super::mesh::{Triangle, Vertex};

fn vertex(p: &DxfPoint) -> Vertex {
    [p.x, p.y, p.z]
}

/// Adds the triangles of a quad `a b c d`, or of a triangle when `d` is the
/// same point as `c`, fanned from `a`.
fn push_face(out: &mut Vec<Triangle>, corners: [Vertex; 4]) {
    let [a, b, c, d] = corners;
    out.push([a, b, c]);
    if d != c && d != a {
        out.push([a, c, d]);
    }
}

fn face3d(face: &Face3D, out: &mut Vec<Triangle>) {
    push_face(
        out,
        [
            vertex(&face.first_corner),
            vertex(&face.second_corner),
            vertex(&face.third_corner),
            vertex(&face.fourth_corner),
        ],
    );
}

/// The faces of a polyface mesh. Position vertices carry the 3D mesh flag
/// and no corner indices; face records name their corners by 1 based
/// index, negative for a hidden edge.
fn polyface(polyline: &Polyline, out: &mut Vec<Triangle>) -> Result<(), CoreError> {
    let mut positions: Vec<Vertex> = Vec::new();
    let mut records: Vec<[i32; 4]> = Vec::new();
    for v in polyline.vertices() {
        let record = [
            v.polyface_mesh_vertex_index1,
            v.polyface_mesh_vertex_index2,
            v.polyface_mesh_vertex_index3,
            v.polyface_mesh_vertex_index4,
        ];
        // Some writers drop the 3D mesh flag from positions, so a face
        // record is told apart by naming corners.
        if record.iter().any(|i| *i != 0) && !v.is_3d_polygon_mesh() {
            records.push(record);
        } else {
            positions.push(vertex(&v.location));
        }
    }
    for record in records {
        let mut corners: Vec<Vertex> = Vec::with_capacity(4);
        for index in record {
            if index == 0 {
                continue;
            }
            let at = usize::try_from(index.unsigned_abs())
                .ok()
                .and_then(|i| i.checked_sub(1))
                .and_then(|i| positions.get(i))
                .ok_or_else(|| {
                    CoreError::format("DXF", "a polyface mesh face names a missing vertex")
                })?;
            corners.push(*at);
        }
        match corners.len() {
            3 => out.push([corners[0], corners[1], corners[2]]),
            4 => push_face(out, [corners[0], corners[1], corners[2], corners[3]]),
            _ => {}
        }
    }
    Ok(())
}

/// Triangles of the 3D faces and polyface meshes in a loaded drawing.
///
/// # Errors
/// Returns [`CoreError::Format`] for a polyface face that names a missing
/// vertex.
pub fn triangles(drawing: &Drawing) -> Result<Vec<Triangle>, CoreError> {
    let mut out = Vec::new();
    for entity in drawing.entities() {
        match &entity.specific {
            EntityType::Face3D(face) => face3d(face, &mut out),
            EntityType::Polyline(polyline) if polyline.is_polyface_mesh() => {
                polyface(polyline, &mut out)?;
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Triangles of the 3D faces and polyface meshes of a DXF file's bytes.
///
/// # Errors
/// Returns [`CoreError::Format`] when the bytes are not a DXF drawing or a
/// face is broken.
pub fn read(bytes: &[u8]) -> Result<Vec<Triangle>, CoreError> {
    let drawing = super::dxf::load(bytes)?;
    triangles(&drawing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dxf::entities::{Entity, Vertex as DxfVertex};
    use dxf::enums::AcadVersion;

    fn point(v: Vertex) -> DxfPoint {
        DxfPoint::new(v[0], v[1], v[2])
    }

    fn save(drawing: &Drawing) -> Vec<u8> {
        let mut bytes = Vec::new();
        drawing.save(&mut bytes).expect("saves");
        bytes
    }

    #[test]
    fn faces_come_in_as_triangles() {
        let (a, b, c, d) = (
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 1.0],
            [10.0, 10.0, 2.0],
            [0.0, 10.0, 3.0],
        );
        let mut drawing = Drawing::new();
        drawing.add_entity(Entity::new(EntityType::Face3D(Face3D::new(
            point(a),
            point(b),
            point(c),
            point(d),
        ))));
        drawing.add_entity(Entity::new(EntityType::Face3D(Face3D::new(
            point(a),
            point(b),
            point(c),
            point(c),
        ))));
        // A flat line is not part of the mesh.
        drawing.add_entity(Entity::new(EntityType::Line(dxf::entities::Line::new(
            point(a),
            point(b),
        ))));
        let triangles = read(&save(&drawing)).expect("reads");
        assert_eq!(triangles, vec![[a, b, c], [a, c, d], [a, b, c]]);
    }

    #[test]
    fn polyface_meshes_come_in_as_triangles() {
        let corners = [
            [0.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [5.0, 5.0, 1.0],
            [0.0, 5.0, 1.0],
            [2.5, 8.0, 4.0],
        ];
        let mut drawing = Drawing::new();
        drawing.header.version = AcadVersion::R2000;
        let mut polyline = Polyline::default();
        polyline.set_is_polyface_mesh(true);
        for c in corners {
            let mut v = DxfVertex::new(point(c));
            v.set_is_3d_polygon_mesh(true);
            v.set_is_polyface_mesh_vertex(true);
            polyline.add_vertex(&mut drawing, v);
        }
        for face in [[1, 2, 3, -4], [-4, 3, 5, 0]] {
            let mut v = DxfVertex::default();
            v.set_is_polyface_mesh_vertex(true);
            v.polyface_mesh_vertex_index1 = face[0];
            v.polyface_mesh_vertex_index2 = face[1];
            v.polyface_mesh_vertex_index3 = face[2];
            v.polyface_mesh_vertex_index4 = face[3];
            polyline.add_vertex(&mut drawing, v);
        }
        drawing.add_entity(Entity::new(EntityType::Polyline(polyline)));
        let triangles = read(&save(&drawing)).expect("reads");
        let [p1, p2, p3, p4, p5] = corners;
        assert_eq!(triangles, vec![[p1, p2, p3], [p1, p3, p4], [p4, p3, p5]]);
    }

    #[test]
    fn garbage_is_refused() {
        assert!(read(b"\x00\x01 not a drawing").is_err());
    }
}
