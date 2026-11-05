//! The clipart library: finding clips on disk, drawing their thumbnails,
//! inserting them into the job, and saving a component as a clip.
//!
//! The library is the clipart that ships with the app (see [`library`]),
//! listed by category, plus the user's folders. Each subfolder of a user
//! folder is a category. It lists our own `.carveclip` relief clips (see
//! [`clip_file`]), STL and OBJ meshes (inserted as relief components) and
//! SVG, DXF, EPS and AI drawings (inserted as vectors).
//!
//! Commands: `clipart.scan`, `clipart.thumbnail`, `clipart.insert` and
//! `clipart.save`; see `docs/KERNEL_API.md`.

pub mod clip_file;
pub mod library;
pub mod samples;
pub mod scan;
pub mod thumbnail;

use std::path::{Path as FsPath, PathBuf};

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::doc::{Document, Entity, EntityKind};
use crate::error::CoreError;
use crate::geom::{flatten_path, transform_path, BBox, Mat3, Path, Point, Tolerance, Vec2};
use crate::io;
use crate::kernel::KernelReply;
use crate::model::field::Field;
use crate::model::mesh::{MeshParams, Up};
use crate::model::shape::Grid;
use crate::model::transform::{transform, TransformParams};
use crate::model::{self, Combine, Component};

use clip_file::Clip;
use scan::{ClipFormat, ClipItem};

/// Largest insert scale, as a factor of the clip's own size.
pub const MAX_INSERT_SCALE: f64 = 100.0;

/// Where the library lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipartDirs {
    /// The user's own clipart folder, scanned when a scan names no folders.
    pub user: PathBuf,
    /// Where the shipped library's items are written as they are used.
    pub library: PathBuf,
}

impl ClipartDirs {
    /// `CarveStudio/clipart` under the platform data directory and
    /// `CarveStudio/clipart-library-<version>` under the cache directory. Unit tests
    /// get folders under the temporary directory instead, so they never
    /// touch the user's files.
    pub fn user_default() -> Self {
        if cfg!(test) {
            let base = std::env::temp_dir().join(format!("carve-clipart-{}", std::process::id()));
            return ClipartDirs {
                user: base.join("clipart"),
                library: base.join(library::dir_name()),
            };
        }
        let data = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
        let cache = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
        ClipartDirs {
            user: data.join("CarveStudio").join("clipart"),
            library: cache.join("CarveStudio").join(library::dir_name()),
        }
    }
}

/// Whether `name` is a clipart command.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "clipart.scan" | "clipart.thumbnail" | "clipart.insert" | "clipart.save"
    )
}

/// Runs a clipart command against the user's library folders.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses, and the
/// errors of [`dispatch_in`].
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    dispatch_in(&ClipartDirs::user_default(), document, name, args)
}

/// Runs a clipart command with the library in `dirs`.
///
/// # Errors
/// [`CoreError::InvalidArgs`] for arguments that do not parse,
/// [`CoreError::Validation`] for bad values or files that hold nothing to
/// insert, and the file layers' errors for files that cannot be read or
/// written.
pub fn dispatch_in(
    dirs: &ClipartDirs,
    document: &mut Document,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "clipart.scan" => {
            let args: ScanArgs = parse_args(name, args)?;
            KernelReply::ok(&scan_library(dirs, &args))
        }
        "clipart.thumbnail" => {
            let args: ThumbnailArgs = parse_args(name, args)?;
            let size = args.size.unwrap_or(thumbnail::THUMBNAIL_SIZE);
            if !(16..=thumbnail::MAX_THUMBNAIL_SIZE).contains(&size) {
                return Err(CoreError::validation(format!(
                    "a thumbnail is 16 to {} pixels",
                    thumbnail::MAX_THUMBNAIL_SIZE
                )));
            }
            let png = thumbnail_png(&args.path, size)?;
            Ok(KernelReply::Bytes(png_blob(&png, size)))
        }
        "clipart.insert" => {
            let args: InsertArgs = parse_args(name, args)?;
            insert(document, &args)
        }
        "clipart.save" => {
            let args: SaveArgs = parse_args(name, args)?;
            let path = save(document, &args)?;
            KernelReply::ok(&json!({ "path": path.to_string_lossy() }))
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

fn parse_args<T: DeserializeOwned>(command: &str, args: &str) -> Result<T, CoreError> {
    let trimmed = args.trim();
    let text = if trimmed.is_empty() || trimmed == "null" {
        "{}"
    } else {
        args
    };
    serde_json::from_str(text).map_err(|error| CoreError::invalid_args(command, error))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ScanArgs {
    /// Folders to scan instead of the user's clipart folder.
    dirs: Option<Vec<PathBuf>>,
    /// Whether the shipped library is listed; true when absent.
    #[serde(alias = "samples")]
    library: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThumbnailArgs {
    path: PathBuf,
    size: Option<u32>,
}

/// Arguments of `clipart.insert`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertArgs {
    /// The file to insert.
    pub path: PathBuf,
    /// Where the clip's centre goes, job millimetres.
    pub x: f64,
    /// Where the clip's centre goes, job millimetres.
    pub y: f64,
    /// Size as a factor of the clip's own size; heights scale with it.
    #[serde(default = "one")]
    pub scale: f64,
    /// How an inserted relief meets the model; Add when absent.
    #[serde(default)]
    pub combine: Combine,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveArgs {
    component_id: u64,
    path: PathBuf,
    name: Option<String>,
}

/// Lists the library: the shipped items first when asked for, then every
/// folder.
fn scan_library(dirs: &ClipartDirs, args: &ScanArgs) -> Value {
    let roots: Vec<PathBuf> = match &args.dirs {
        Some(list) => list.clone(),
        None => {
            // The user's folder is made on first use so there is somewhere
            // to put clips; failing to make it only means it lists nothing.
            let _ = std::fs::create_dir_all(&dirs.user);
            vec![dirs.user.clone()]
        }
    };
    let mut items: Vec<ClipItem> = Vec::new();
    let warnings: Vec<String> = Vec::new();
    if args.library.unwrap_or(true) {
        items.extend(library::items(&dirs.library));
    }
    let mut found = Vec::new();
    for root in &roots {
        scan::scan_root(root, &mut found);
    }
    found.sort_by(|a, b| (&a.category, &a.name, &a.path).cmp(&(&b.category, &b.name, &b.path)));
    items.extend(found);
    json!({
        "categories": scan::categories(&items),
        "items": items,
        "userDir": dirs.user.to_string_lossy(),
        "roots": roots.iter().map(|r| r.to_string_lossy()).collect::<Vec<_>>(),
        "warnings": warnings,
    })
}

/// The thumbnail of the file at `path` as PNG bytes.
///
/// # Errors
/// [`CoreError::Unsupported`] for a file the library does not list, and
/// the readers' errors.
pub fn thumbnail_png(path: &FsPath, size: u32) -> Result<Vec<u8>, CoreError> {
    library::materialize(path)?;
    let image = match format_of(path)? {
        ClipFormat::Clip => thumbnail::relief(&clip_file::read(path)?.field, size),
        ClipFormat::Mesh => thumbnail::relief(&mesh_preview(path, size as usize)?, size),
        ClipFormat::Vectors => thumbnail::vectors(&read_vectors(path)?, size),
    };
    thumbnail::png(&image)
}

/// A binary reply holding one PNG: `{width, height, sections: [png u8]}`.
fn png_blob(png: &[u8], size: u32) -> Vec<u8> {
    let header = json!({
        "width": size,
        "height": size,
        "mime": "image/png",
        "sections": [{ "name": "png", "dtype": "u8", "offset": 0, "count": png.len() }],
    })
    .to_string()
    .into_bytes();
    let mut out = Vec::with_capacity(4 + header.len() + png.len());
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(png);
    out
}

fn format_of(path: &FsPath) -> Result<ClipFormat, CoreError> {
    ClipFormat::of_path(path).ok_or_else(|| {
        CoreError::unsupported(format!(
            "{} is not a clipart file this build can use",
            path.display()
        ))
    })
}

/// The mesh at `path` projected onto a grid `cells` across, for its
/// thumbnail.
fn mesh_preview(path: &FsPath, cells: usize) -> Result<Field, CoreError> {
    let triangles = io::mesh::read(path)?;
    let (lo, hi) = mesh_extent(&triangles);
    let (span, depth) = (hi[0] - lo[0], hi[1] - lo[1]);
    if !(span.is_finite() && depth.is_finite() && span > 0.0 && depth > 0.0) {
        return Err(CoreError::validation(
            "the model has no size seen from above",
        ));
    }
    let cell = span.max(depth) / cells.max(1) as f64;
    let grid = Grid {
        width: ((span / cell).ceil() as usize).max(1),
        height: ((depth / cell).ceil() as usize).max(1),
        cell,
        origin: Point::new(lo[0] + cell / 2.0, lo[1] + cell / 2.0),
    };
    let centre = Point::new((lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0);
    let params = MeshParams {
        up: Up::Z,
        width: None,
        centre: Some(centre),
    };
    let full = model::mesh::project(&triangles, &grid, &params, centre)?;
    Ok(full.cropped().unwrap_or(full))
}

/// The lowest and highest corner of a mesh, the file's own axes.
fn mesh_extent(triangles: &[io::mesh::Triangle]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for v in triangles.iter().flatten() {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(v[axis]);
            hi[axis] = hi[axis].max(v[axis]);
        }
    }
    (lo, hi)
}

fn read_vectors(path: &FsPath) -> Result<Vec<Path>, CoreError> {
    let read = io::import_vectors(path, &io::ImportOptions::default())?;
    Ok(read.paths.into_iter().map(|p| p.path).collect())
}

/// Inserts the clip at `args.path` centred on `(x, y)` as one undo step.
///
/// # Errors
/// [`CoreError::Validation`] for a bad position or scale or a clip that
/// lands on no cell of the model, and the readers' errors.
pub fn insert(document: &mut Document, args: &InsertArgs) -> Result<KernelReply, CoreError> {
    if !(args.x.is_finite() && args.y.is_finite()) {
        return Err(CoreError::validation("the position must be a number"));
    }
    if !(args.scale.is_finite() && args.scale > 0.0 && args.scale <= MAX_INSERT_SCALE) {
        return Err(CoreError::validation(format!(
            "the scale must be above 0 and at most {MAX_INSERT_SCALE}"
        )));
    }
    library::materialize(&args.path)?;
    let at = Point::new(args.x, args.y);
    let grid = model::grid_for(document.job());
    let stem = args
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Clipart")
        .to_string();
    match format_of(&args.path)? {
        ClipFormat::Clip => {
            let clip = clip_file::read(&args.path)?;
            let field = place_field(&clip.field, &grid, at, args.scale)?;
            let name = if clip.name.trim().is_empty() {
                stem
            } else {
                clip.name
            };
            add_relief(document, name, args.combine, field)
        }
        ClipFormat::Mesh => {
            let triangles = io::mesh::read(&args.path)?;
            let (lo, hi) = mesh_extent(&triangles);
            let params = MeshParams {
                up: Up::Z,
                width: Some((hi[0] - lo[0]) * args.scale),
                centre: Some(at),
            };
            let full = model::mesh::project(&triangles, &grid, &params, at)?;
            let field = full
                .cropped()
                .ok_or_else(|| CoreError::validation("the clip lands outside the material"))?;
            add_relief(document, stem, args.combine, field)
        }
        ClipFormat::Vectors => {
            let paths = read_vectors(&args.path)?;
            let placed = place_paths(&paths, at, args.scale)?;
            let layer = document.active_layer();
            let sheet = document.active_sheet();
            document.begin("Insert Clipart");
            let ids: Vec<u64> = placed
                .into_iter()
                .map(|path| {
                    document.insert_entity(Entity {
                        rotation: 0.0,
                        id: 0,
                        layer,
                        sheet,
                        revision: 0,
                        name: None,
                        parent: None,
                        kind: EntityKind::Path { path },
                    })
                })
                .collect();
            document.set_selection(&ids);
            let revision = document.commit();
            KernelReply::ok(&json!({ "kind": "vector", "ids": ids, "revision": revision }))
        }
    }
}

fn add_relief(
    document: &mut Document,
    name: String,
    combine: Combine,
    field: Field,
) -> Result<KernelReply, CoreError> {
    document.begin("Insert Clipart");
    let id = document.add_component(Component {
        id: 0,
        name,
        visible: true,
        combine,
        height_scale: 1.0,
        level: 0,
        extra: Default::default(),
        field,
    });
    let revision = document.commit();
    KernelReply::ok(&json!({ "kind": "relief", "id": id, "revision": revision }))
}

/// A clip's heights resampled onto the model grid, scaled by `scale` in
/// every direction and centred on `at`.
///
/// # Errors
/// [`CoreError::Validation`] when the result holds no cell.
pub fn place_field(clip: &Field, grid: &Grid, at: Point, scale: f64) -> Result<Field, CoreError> {
    // Seen on the model grid's lattice first, then stretched to size, so
    // the result shares the lattice and bakes cell for cell.
    let mut source = clip.clone();
    source.cell = grid.cell;
    source.origin = grid.origin;
    let (w, h) = (source.width as f64, source.height as f64);
    let centre = Point::new(
        source.origin.x + (w - 1.0) * grid.cell / 2.0,
        source.origin.y + (h - 1.0) * grid.cell / 2.0,
    );
    let k = scale * clip.cell / grid.cell;
    let params = TransformParams {
        offset: Point::new(at.x - centre.x, at.y - centre.y),
        scale_x: k,
        scale_y: k,
        rotate_deg: 0.0,
        centre: None,
    };
    let mut placed = transform(&source, &params)?;
    let lift = scale as f32;
    for z in &mut placed.z {
        *z *= lift;
    }
    placed
        .cropped()
        .ok_or_else(|| CoreError::validation("the clip is too small to cover a cell of the model"))
}

/// `paths` scaled by `scale` about the centre of their box, then moved so
/// that centre lands on `at`.
///
/// # Errors
/// [`CoreError::Validation`] when the paths have no extent.
pub fn place_paths(paths: &[Path], at: Point, scale: f64) -> Result<Vec<Path>, CoreError> {
    let tol = Tolerance::new(0.01)?;
    let bbox = paths
        .iter()
        .flat_map(|p| flatten_path(p, tol))
        .filter_map(|p| BBox::of_points(p.points))
        .reduce(BBox::union)
        .ok_or_else(|| CoreError::validation("that file has no vectors in it to insert"))?;
    let c = bbox.centre();
    let m = Mat3::translate(Vec2::new(-c.x, -c.y))
        .then(&Mat3::scale(scale, scale))
        .then(&Mat3::translate(Vec2::new(at.x, at.y)));
    Ok(paths.iter().map(|p| transform_path(p, &m)).collect())
}

/// Writes component `args.component_id` to a clip file, adding the
/// `.carveclip` extension when the path has none. Answers the path written.
fn save(document: &Document, args: &SaveArgs) -> Result<PathBuf, CoreError> {
    let component = document.component(args.component_id)?;
    let mut field = component
        .field
        .cropped()
        .ok_or_else(|| CoreError::validation("the component has no heights to save"))?;
    let lift = component.height_scale as f32;
    for z in &mut field.z {
        *z *= lift;
    }
    let name = args
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| component.name.clone());
    let mut path = args.path.clone();
    if path.extension().is_none() {
        path.set_extension(clip_file::EXTENSION);
    }
    clip_file::write(&Clip { name, field }, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    fn dirs_in(temp: &TempDir) -> ClipartDirs {
        ClipartDirs {
            user: temp.path().join("clipart"),
            library: temp.path().join(library::dir_name()),
        }
    }

    fn json_of(reply: KernelReply) -> Value {
        match reply {
            KernelReply::Json(text) => {
                let value: Value = serde_json::from_str(&text).expect("json");
                assert_eq!(value["ok"], true, "{value}");
                value["result"].clone()
            }
            KernelReply::Bytes(_) => panic!("expected JSON"),
        }
    }

    fn square_clip(cells: usize, cell: f64, height: f32) -> Clip {
        let mut field = Field::empty(cells, cells, cell, Point::ORIGIN);
        field.z.fill(height);
        Clip {
            name: "Block".to_string(),
            field,
        }
    }

    #[test]
    fn a_scan_lists_the_library_first_and_the_user_folder() {
        let temp = TempDir::new("clipart-dispatch-scan");
        let dirs = dirs_in(&temp);
        std::fs::create_dir_all(dirs.user.join("Flowers")).expect("dirs");
        std::fs::write(dirs.user.join("Flowers/Tulip.svg"), b"<svg/>").expect("writes");
        let mut doc = Document::new();
        let result = json_of(dispatch_in(&dirs, &mut doc, "clipart.scan", "{}").expect("scans"));
        let items = result["items"].as_array().expect("items");
        let shipped = library::catalogue().len();
        assert_eq!(items.len(), shipped + 1);
        assert_eq!(items[0]["name"], library::catalogue()[0].name.as_str());
        assert!(items[0]["tags"].is_array());
        assert_eq!(items[shipped]["name"], "Tulip");
        assert_eq!(items[shipped]["kind"], "vector");
        let categories = result["categories"].as_array().expect("categories");
        assert!(categories.contains(&json!("Flowers")));
        assert!(categories.contains(&json!("Shapes/Domes")));
        // Listing writes nothing: items are made when first used.
        assert!(!dirs.library.exists());
        let without = json_of(
            dispatch_in(&dirs, &mut doc, "clipart.scan", r#"{"library":false}"#).expect("scans"),
        );
        assert_eq!(without["categories"], json!(["Flowers"]));
    }

    #[test]
    fn a_thumbnail_is_a_256_pixel_png() {
        let temp = TempDir::new("clipart-thumb");
        let dirs = dirs_in(&temp);
        let first = &library::items(&dirs.library)[0];
        let mut doc = Document::new();
        let args = json!({ "path": first.path }).to_string();
        let reply = dispatch_in(&dirs, &mut doc, "clipart.thumbnail", &args).expect("draws");
        let KernelReply::Bytes(bytes) = reply else {
            panic!("expected bytes");
        };
        assert!(PathBuf::from(&first.path).is_file(), "made on first use");
        let header_len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let header: Value = serde_json::from_slice(&bytes[4..4 + header_len]).expect("header");
        assert_eq!(header["width"], 256);
        let png = &bytes[4 + header_len..];
        assert_eq!(header["sections"][0]["count"], png.len());
        let image = image::load_from_memory(png).expect("decodes");
        assert_eq!((image.width(), image.height()), (256, 256));
        let missing = json!({ "path": temp.path().join("x.txt") }).to_string();
        assert!(dispatch_in(&dirs, &mut doc, "clipart.thumbnail", &missing).is_err());
    }

    #[test]
    fn a_mesh_thumbnail_is_drawn_from_its_projection() {
        let temp = TempDir::new("clipart-mesh-thumb");
        let path = temp.path().join("tent.obj");
        std::fs::write(
            &path,
            "v 0 0 0\nv 10 0 0\nv 10 10 0\nv 0 10 0\nv 5 5 4\nf 1 2 5\nf 2 3 5\nf 3 4 5\nf 4 1 5\n",
        )
        .expect("writes");
        let png = thumbnail_png(&path, 64).expect("draws");
        let image = image::load_from_memory(&png).expect("decodes").to_rgba8();
        assert_eq!(image.get_pixel(32, 32).0[3], 255);
    }

    #[test]
    fn an_inserted_clip_lands_centred_at_the_point_and_scaled() {
        let temp = TempDir::new("clipart-insert");
        let path = temp.path().join("block.carveclip");
        // 20 cells of 0.5 mm: a 10 mm square, 2 mm high.
        clip_file::write(&square_clip(20, 0.5, 2.0), &path).expect("writes");
        let mut doc = Document::new();
        let dirs = dirs_in(&temp);
        let args = json!({ "path": path, "x": 100.0, "y": 60.0, "scale": 2.0 }).to_string();
        let result = json_of(dispatch_in(&dirs, &mut doc, "clipart.insert", &args).expect("ok"));
        assert_eq!(result["kind"], "relief");
        let id = result["id"].as_u64().expect("id");
        let component = doc.component(id).expect("added");
        assert_eq!(component.name, "Block");
        let field = &component.field;
        let width = field.width as f64 * field.cell;
        let height = field.height as f64 * field.cell;
        assert!((width - 20.0).abs() <= 2.0 * field.cell, "width {width}");
        assert!((height - 20.0).abs() <= 2.0 * field.cell, "height {height}");
        let cx = field.origin.x + (field.width as f64 - 1.0) * field.cell / 2.0;
        let cy = field.origin.y + (field.height as f64 - 1.0) * field.cell / 2.0;
        assert!((cx - 100.0).abs() <= field.cell, "centre x {cx}");
        assert!((cy - 60.0).abs() <= field.cell, "centre y {cy}");
        assert_eq!(field.max(), Some(4.0));
        // One undo step takes it away again.
        assert!(doc.undo().is_some());
        assert!(doc.components().is_empty());
    }

    #[test]
    fn inserted_vectors_are_centred_and_scaled() {
        let temp = TempDir::new("clipart-insert-svg");
        let path = temp.path().join("box.svg");
        std::fs::write(
            &path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="4mm" viewBox="0 0 10 4"><rect x="0" y="0" width="10" height="4"/></svg>"#,
        )
        .expect("writes");
        let mut doc = Document::new();
        let dirs = dirs_in(&temp);
        let args = json!({ "path": path, "x": 50.0, "y": 50.0, "scale": 3.0 }).to_string();
        let result = json_of(dispatch_in(&dirs, &mut doc, "clipart.insert", &args).expect("ok"));
        assert_eq!(result["kind"], "vector");
        let ids = result["ids"].as_array().expect("ids");
        assert_eq!(ids.len(), 1);
        let id = ids[0].as_u64().expect("id");
        let paths: Vec<Path> = doc.paths_of(id).into_iter().map(|(_, p)| p).collect();
        let tol = Tolerance::new(0.01).expect("tol");
        let bbox = paths
            .iter()
            .flat_map(|p| flatten_path(p, tol))
            .filter_map(|p| BBox::of_points(p.points))
            .reduce(BBox::union)
            .expect("bbox");
        assert!((bbox.width() - 30.0).abs() < 1e-6, "{bbox:?}");
        assert!((bbox.height() - 12.0).abs() < 1e-6, "{bbox:?}");
        assert!((bbox.centre().x - 50.0).abs() < 1e-6);
        assert!((bbox.centre().y - 50.0).abs() < 1e-6);
    }

    #[test]
    fn bad_insert_arguments_are_refused_without_a_change() {
        let temp = TempDir::new("clipart-insert-bad");
        let path = temp.path().join("block.carveclip");
        clip_file::write(&square_clip(4, 1.0, 1.0), &path).expect("writes");
        let mut doc = Document::new();
        let dirs = dirs_in(&temp);
        let before = doc.revision();
        for scale in [0.0, -1.0, 1000.0] {
            let args = json!({ "path": path, "x": 1.0, "y": 1.0, "scale": scale }).to_string();
            assert!(dispatch_in(&dirs, &mut doc, "clipart.insert", &args).is_err());
        }
        let args = json!({ "path": temp.path().join("a.txt"), "x": 1.0, "y": 1.0 }).to_string();
        assert!(dispatch_in(&dirs, &mut doc, "clipart.insert", &args).is_err());
        assert_eq!(doc.revision(), before);
    }

    #[test]
    fn a_saved_component_inserts_back_the_same() {
        let temp = TempDir::new("clipart-save");
        let dirs = dirs_in(&temp);
        let mut doc = Document::new();
        let grid = model::grid_for(doc.job());
        let clip = samples::sample("Star Dome").expect("sample");
        let field = place_field(&clip.field, &grid, Point::new(80.0, 80.0), 1.0).expect("placed");
        doc.begin("Add");
        let id = doc.add_component(Component {
            id: 0,
            name: "Star".to_string(),
            visible: true,
            combine: Combine::Add,
            height_scale: 0.5,
            level: 0,
            extra: Default::default(),
            field: field.clone(),
        });
        doc.commit();
        let target = temp.path().join("star");
        let args = json!({ "componentId": id, "path": target }).to_string();
        let result = json_of(dispatch_in(&dirs, &mut doc, "clipart.save", &args).expect("saves"));
        let written = PathBuf::from(result["path"].as_str().expect("path"));
        assert_eq!(
            written.extension().and_then(|e| e.to_str()),
            Some("carveclip")
        );
        let back = clip_file::read(&written).expect("reads");
        assert_eq!(back.name, "Star");
        assert_eq!(
            (back.field.width, back.field.height),
            (field.width, field.height)
        );
        let peak = field.max().expect("peak") * 0.5;
        assert_eq!(back.field.max(), Some(peak));
        // Inserting at scale 1 on the same grid gives the same cells.
        let again = place_field(&back.field, &grid, Point::new(80.0, 80.0), 1.0).expect("placed");
        assert_eq!((again.width, again.height), (field.width, field.height));
    }

    #[test]
    fn the_command_names_are_claimed() {
        for name in [
            "clipart.scan",
            "clipart.thumbnail",
            "clipart.insert",
            "clipart.save",
        ] {
            assert!(handles(name));
        }
        assert!(!handles("clipart.other"));
        let mut doc = Document::new();
        let temp = TempDir::new("clipart-unknown");
        assert!(dispatch_in(&dirs_in(&temp), &mut doc, "clipart.other", "{}").is_err());
    }
}
