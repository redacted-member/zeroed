//! Finding clipart on disk: every file the library can insert under a set
//! of folders, each filed under a category named after its subfolder.

use std::path::{Path as FsPath, PathBuf};

use serde::Serialize;

use super::clip_file::EXTENSION;

/// Deepest subfolder level a scan descends to.
pub const MAX_DEPTH: usize = 6;
/// Most items one scan lists, so a scan of a huge folder stays quick.
pub const MAX_ITEMS: usize = 5000;

/// What an item inserts as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipKind {
    /// A relief component (clip files and meshes).
    Relief,
    /// Vectors on the active layer.
    Vector,
}

/// The kind of file an item is, by its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipFormat {
    /// Our own `.carveclip`.
    Clip,
    /// A triangle mesh (STL or OBJ).
    Mesh,
    /// A vector drawing (SVG, DXF, EPS or AI).
    Vectors,
}

impl ClipFormat {
    /// The format a file name suggests, or `None` for a file the library
    /// does not list.
    pub fn of_path(path: &FsPath) -> Option<ClipFormat> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            e if e == EXTENSION => Some(ClipFormat::Clip),
            "stl" | "obj" => Some(ClipFormat::Mesh),
            "svg" | "dxf" | "eps" | "ai" => Some(ClipFormat::Vectors),
            _ => None,
        }
    }

    /// What the format inserts as.
    pub fn kind(self) -> ClipKind {
        match self {
            ClipFormat::Clip | ClipFormat::Mesh => ClipKind::Relief,
            ClipFormat::Vectors => ClipKind::Vector,
        }
    }
}

/// One file in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    /// Name shown under the thumbnail: the file name without extension.
    pub name: String,
    /// Full path, as `clipart.thumbnail` and `clipart.insert` take it.
    pub path: String,
    /// Category: the subfolder under the scanned folder, `/` between
    /// levels, or the scanned folder's own name for files at its top.
    pub category: String,
    /// What it inserts as.
    pub kind: ClipKind,
    /// The kind of file.
    pub format: ClipFormat,
    /// Words a search matches besides the name and category.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Lists the files under `root`, sorted by category then name. A missing
/// folder lists nothing. Hidden files and folders are skipped.
pub fn scan_root(root: &FsPath, out: &mut Vec<ClipItem>) {
    let top = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Clipart")
        .to_string();
    let mut found = Vec::new();
    walk(root, &[], 0, &mut found);
    for (parts, path) in found {
        if out.len() >= MAX_ITEMS {
            break;
        }
        let Some(format) = ClipFormat::of_path(&path) else {
            continue;
        };
        let category = if parts.is_empty() {
            top.clone()
        } else {
            parts.join("/")
        };
        out.push(item(&path, category, format));
    }
}

/// An item for `path` in `category`.
pub fn item(path: &FsPath, category: String, format: ClipFormat) -> ClipItem {
    ClipItem {
        name: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Clip")
            .to_string(),
        path: path.to_string_lossy().into_owned(),
        category,
        kind: format.kind(),
        format,
        tags: Vec::new(),
    }
}

fn walk(dir: &FsPath, parts: &[String], depth: usize, out: &mut Vec<(Vec<String>, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        if out.len() >= MAX_ITEMS {
            return;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if depth < MAX_DEPTH {
                let mut next = parts.to_vec();
                next.push(name.to_string());
                walk(&path, &next, depth + 1, out);
            }
        } else if ClipFormat::of_path(&path).is_some() {
            out.push((parts.to_vec(), path));
        }
    }
}

/// The categories of `items`, sorted, each once.
pub fn categories(items: &[ClipItem]) -> Vec<String> {
    let mut names: Vec<String> = items.iter().map(|i| i.category.clone()).collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn a_scan_files_items_under_their_subfolder() {
        let temp = TempDir::new("clipart-scan");
        let root = temp.path().join("library");
        std::fs::create_dir_all(root.join("Animals/Birds")).expect("dirs");
        std::fs::create_dir_all(root.join("Borders")).expect("dirs");
        std::fs::create_dir_all(root.join(".hidden")).expect("dirs");
        for file in [
            "Animals/Cat.stl",
            "Animals/Birds/Owl.carveclip",
            "Borders/Rope.svg",
            "Loose.dxf",
            "notes.txt",
            ".hidden/Secret.stl",
        ] {
            std::fs::write(root.join(file), b"x").expect("writes");
        }
        let mut items = Vec::new();
        scan_root(&root, &mut items);
        let summary: Vec<(&str, &str, ClipKind)> = items
            .iter()
            .map(|i| (i.category.as_str(), i.name.as_str(), i.kind))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("Animals/Birds", "Owl", ClipKind::Relief),
                ("Animals", "Cat", ClipKind::Relief),
                ("Borders", "Rope", ClipKind::Vector),
                ("library", "Loose", ClipKind::Vector),
            ]
        );
        assert_eq!(
            categories(&items),
            vec!["Animals", "Animals/Birds", "Borders", "library"]
        );
    }

    #[test]
    fn a_missing_folder_lists_nothing() {
        let mut items = Vec::new();
        scan_root(FsPath::new("/no/such/clipart/folder"), &mut items);
        assert!(items.is_empty());
    }
}
