// Renders a triangle mesh (glTF, GLB or OBJ) to an orthographic depth map:
// the highest surface seen looking down -Z after an optional rotation.
// Usage: meshdepth <mesh> <out.f32> <px on longer side> [rx ry rz] [phong alpha]
// Prints a JSON line with width, height, cell and origin.

use std::io::Write;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 0.0 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

struct Mesh {
    pos: Vec<V3>,
    nrm: Vec<V3>,
    tri: Vec<[usize; 3]>,
}

fn mat_mul(a: &[[f64; 4]; 4], b: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut m = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            m[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    m
}

fn load_gltf(path: &str) -> Mesh {
    let gltf = gltf::Gltf::open(path).expect("gltf");
    let base = std::path::Path::new(path).parent().unwrap();
    let mut buffers = Vec::new();
    for b in gltf.buffers() {
        let data = match b.source() {
            gltf::buffer::Source::Bin => gltf.blob.clone().expect("blob"),
            gltf::buffer::Source::Uri(uri) => {
                let name = uri.rsplit('/').next().unwrap();
                std::fs::read(base.join(name)).expect("buffer")
            }
        };
        buffers.push(data);
    }
    let mut mesh = Mesh {
        pos: vec![],
        nrm: vec![],
        tri: vec![],
    };
    let ident = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let scene = gltf
        .default_scene()
        .or_else(|| gltf.scenes().next())
        .unwrap();
    let mut stack: Vec<(gltf::Node, [[f64; 4]; 4])> = scene.nodes().map(|n| (n, ident)).collect();
    while let Some((node, parent)) = stack.pop() {
        let local = node.transform().matrix();
        let local: [[f64; 4]; 4] = local.map(|c| c.map(f64::from));
        let world = mat_mul(&parent, &local);
        if let Some(m) = node.mesh() {
            for prim in m.primitives() {
                if prim.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let reader = prim.reader(|b| Some(&buffers[b.index()]));
                let Some(ps) = reader.read_positions() else {
                    continue;
                };
                let start = mesh.pos.len();
                for p in ps {
                    let p = p.map(f64::from);
                    let q = [0, 1, 2].map(|r| {
                        world[0][r] * p[0] + world[1][r] * p[1] + world[2][r] * p[2] + world[3][r]
                    });
                    mesh.pos.push(q);
                }
                let count = mesh.pos.len() - start;
                if let Some(ns) = reader.read_normals() {
                    for n in ns {
                        let n = n.map(f64::from);
                        let q = [0, 1, 2]
                            .map(|r| world[0][r] * n[0] + world[1][r] * n[1] + world[2][r] * n[2]);
                        mesh.nrm.push(norm(q));
                    }
                } else {
                    mesh.nrm
                        .extend(std::iter::repeat([0.0, 0.0, 0.0]).take(count));
                }
                let idx: Vec<u32> = match reader.read_indices() {
                    Some(i) => i.into_u32().collect(),
                    None => (0..count as u32).collect(),
                };
                for t in idx.chunks(3) {
                    if t.len() == 3 {
                        mesh.tri.push([
                            start + t[0] as usize,
                            start + t[1] as usize,
                            start + t[2] as usize,
                        ]);
                    }
                }
            }
        }
        for c in node.children() {
            stack.push((c, world));
        }
    }
    mesh
}

fn load_obj(path: &str) -> Mesh {
    let text = std::fs::read_to_string(path).expect("obj");
    let mut mesh = Mesh {
        pos: vec![],
        nrm: vec![],
        tri: vec![],
    };
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let v: Vec<f64> = it.take(3).map(|s| s.parse().unwrap()).collect();
                mesh.pos.push([v[0], v[1], v[2]]);
            }
            Some("f") => {
                let ids: Vec<usize> = it
                    .map(|s| {
                        let i: i64 = s.split('/').next().unwrap().parse().unwrap();
                        if i < 0 {
                            (mesh.pos.len() as i64 + i) as usize
                        } else {
                            (i - 1) as usize
                        }
                    })
                    .collect();
                for k in 1..ids.len().saturating_sub(1) {
                    mesh.tri.push([ids[0], ids[k], ids[k + 1]]);
                }
            }
            _ => {}
        }
    }
    mesh.nrm = vec![[0.0; 3]; mesh.pos.len()];
    mesh
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let out = &args[2];
    let px: usize = args[3].parse().unwrap();
    let rot: Vec<f64> = if args.len() >= 7 {
        args[4..7].iter().map(|s| s.parse().unwrap()).collect()
    } else {
        vec![0.0, 0.0, 0.0]
    };
    let alpha: f64 = args.get(7).map(|s| s.parse().unwrap()).unwrap_or(0.75);
    let mut mesh = if path.ends_with(".obj") {
        load_obj(path)
    } else {
        load_gltf(path)
    };
    // Normals missing or zero: area weighted vertex normals by position.
    if std::env::var_os("MESHDEPTH_RENORMAL").is_some()
        || mesh.nrm.iter().any(|n| dot(*n, *n) < 0.5)
    {
        use std::collections::HashMap;
        let key = |p: V3| {
            (
                (p[0] * 1e5) as i64,
                (p[1] * 1e5) as i64,
                (p[2] * 1e5) as i64,
            )
        };
        let mut acc: HashMap<(i64, i64, i64), V3> = HashMap::new();
        for t in &mesh.tri {
            let [a, b, c] = t.map(|i| mesh.pos[i]);
            let n = cross(sub(b, a), sub(c, a));
            for v in [a, b, c] {
                let e = acc.entry(key(v)).or_insert([0.0; 3]);
                for k in 0..3 {
                    e[k] += n[k];
                }
            }
        }
        for i in 0..mesh.pos.len() {
            mesh.nrm[i] = norm(*acc.get(&key(mesh.pos[i])).unwrap_or(&[0.0, 0.0, 1.0]));
        }
    }
    let (sx, cx) = rot[0].to_radians().sin_cos();
    let (sy, cy) = rot[1].to_radians().sin_cos();
    let (sz, cz) = rot[2].to_radians().sin_cos();
    let r = |v: V3| -> V3 {
        let v = [v[0], cx * v[1] - sx * v[2], sx * v[1] + cx * v[2]];
        let v = [cy * v[0] + sy * v[2], v[1], -sy * v[0] + cy * v[2]];
        [cz * v[0] - sz * v[1], sz * v[0] + cz * v[1], v[2]]
    };
    for p in mesh.pos.iter_mut() {
        *p = r(*p);
    }
    for n in mesh.nrm.iter_mut() {
        *n = r(*n);
    }
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for p in &mesh.pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let ext = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    let cell = ext / px as f64;
    let w = ((hi[0] - lo[0]) / cell).ceil() as usize + 4;
    let h = ((hi[1] - lo[1]) / cell).ceil() as usize + 4;
    let ox = lo[0] - 2.0 * cell;
    let oy = lo[1] - 2.0 * cell;
    let mut z = vec![f32::NAN; w * h];
    for t in &mesh.tri {
        let v = t.map(|i| mesh.pos[i]);
        let n = t.map(|i| mesh.nrm[i]);
        let q = v.map(|p| [(p[0] - ox) / cell - 0.5, (p[1] - oy) / cell - 0.5]);
        let area =
            (q[1][0] - q[0][0]) * (q[2][1] - q[0][1]) - (q[2][0] - q[0][0]) * (q[1][1] - q[0][1]);
        if area.abs() < 1e-12 {
            continue;
        }
        let x0 = q
            .iter()
            .map(|p| p[0])
            .fold(f64::MAX, f64::min)
            .ceil()
            .max(0.0) as usize;
        let x1 = q
            .iter()
            .map(|p| p[0])
            .fold(f64::MIN, f64::max)
            .floor()
            .min(w as f64 - 1.0);
        let y0 = q
            .iter()
            .map(|p| p[1])
            .fold(f64::MAX, f64::min)
            .ceil()
            .max(0.0) as usize;
        let y1 = q
            .iter()
            .map(|p| p[1])
            .fold(f64::MIN, f64::max)
            .floor()
            .min(h as f64 - 1.0);
        if x1 < 0.0 || y1 < 0.0 {
            continue;
        }
        for j in y0..=(y1 as usize) {
            for i in x0..=(x1 as usize) {
                let (x, y) = (i as f64, j as f64);
                let e = |a: [f64; 2], b: [f64; 2]| {
                    (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0])
                };
                let b0 = e(q[1], q[2]) / area;
                let b1 = e(q[2], q[0]) / area;
                let b2 = e(q[0], q[1]) / area;
                if b0 < -1e-9 || b1 < -1e-9 || b2 < -1e-9 {
                    continue;
                }
                let b = [b0, b1, b2];
                let p = [0, 1, 2].map(|k| b[0] * v[0][k] + b[1] * v[1][k] + b[2] * v[2][k]);
                let mut star = [0.0; 3];
                for m in 0..3 {
                    let d = dot(sub(p, v[m]), n[m]);
                    for k in 0..3 {
                        star[k] += b[m] * (p[k] - d * n[m][k]);
                    }
                }
                let zz = (1.0 - alpha) * p[2] + alpha * star[2];
                let cellz = &mut z[j * w + i];
                if cellz.is_nan() || zz as f32 > *cellz {
                    *cellz = zz as f32;
                }
            }
        }
    }
    let mut f = std::fs::File::create(out).unwrap();
    let bytes: Vec<u8> = z.iter().flat_map(|v| v.to_le_bytes()).collect();
    f.write_all(&bytes).unwrap();
    println!(
        "{{\"width\":{w},\"height\":{h},\"cell\":{cell},\"ox\":{ox},\"oy\":{oy},\"zmin\":{},\"zmax\":{},\"tris\":{}}}",
        lo[2],
        hi[2],
        mesh.tri.len()
    );
}
