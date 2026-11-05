//! The document's image store: the pictures that bitmap entities place.
//!
//! An image is kept as the encoded file it was imported from (PNG, JPEG, BMP,
//! WebP or GIF), so the document stays small and the 2D view can hand the
//! bytes straight to the browser's decoder. A TIFF, which browsers cannot
//! show, is kept as a PNG of the same pixels. Images are never edited; a bitmap
//! entity refers to one by id and places it with its corner quad. The store
//! is shared by both sides of a double sided job and outside the undo
//! journal: undoing an import drops the entity, and an image no entity
//! refers to is simply not written when the document is saved.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{Document, DocumentFile, EntityKind};
use crate::error::CoreError;
use crate::io::bitmap::{self, Bitmap};

/// One stored image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// File extension of the encoding: `png`, `jpg`, `bmp`, `webp` or `gif`.
    pub format: String,
    /// The encoded file.
    pub bytes: Arc<Vec<u8>>,
}

impl StoredImage {
    /// Decodes the image to greyscale for tracing.
    ///
    /// # Errors
    /// The errors of [`bitmap::decode`].
    pub fn decode(&self) -> Result<Bitmap, CoreError> {
        bitmap::decode(&self.bytes, Some(&self.format))
    }

    /// MIME type of the encoding, for the view.
    pub fn mime(&self) -> &'static str {
        match self.format.as_str() {
            "jpg" => "image/jpeg",
            "bmp" => "image/bmp",
            "webp" => "image/webp",
            "gif" => "image/gif",
            _ => "image/png",
        }
    }
}

/// Images by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageStore {
    images: BTreeMap<u64, StoredImage>,
    next_id: u64,
}

impl ImageStore {
    /// Adds an image and answers its id.
    pub fn insert(&mut self, image: StoredImage) -> u64 {
        let id = self.next_id.max(1);
        self.images.insert(id, image);
        self.next_id = id + 1;
        id
    }

    /// Puts an image back under a known id, as opening a file does.
    pub fn restore(&mut self, id: u64, image: StoredImage) {
        self.images.insert(id, image);
        self.next_id = self.next_id.max(id + 1);
    }

    /// The image with this id.
    pub fn get(&self, id: u64) -> Option<&StoredImage> {
        self.images.get(&id)
    }

    /// How many images are held.
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// True when no image is held.
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

/// The encoding a file name's extension names, normalised, if it is one the
/// store keeps.
pub fn format_of(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "bmp" => Some("bmp"),
        "webp" => Some("webp"),
        "gif" => Some("gif"),
        "tif" | "tiff" => Some("tif"),
        _ => None,
    }
}

/// Recognises the encoding of image bytes: PNG, JPEG, BMP, WebP, GIF or
/// TIFF (`tif`, which the store converts to PNG).
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"BM") {
        Some("bmp")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        Some("tif")
    } else {
        None
    }
}

/// Ids of the images the bitmap entities of a saved document refer to, on
/// both sides.
pub fn referenced(file: &DocumentFile) -> BTreeSet<u64> {
    let bottom = file.bottom.iter().flat_map(|side| side.entities.iter());
    file.entities
        .iter()
        .chain(bottom)
        .filter_map(|entity| match &entity.kind {
            EntityKind::Bitmap(bitmap) => Some(bitmap.image_id),
            _ => None,
        })
        .collect()
}

/// Name of the archive entry that holds an image.
pub fn entry_name(id: u64, image: &StoredImage) -> String {
    format!("{}{id}.{}", crate::io::native::IMAGES_FOLDER, image.format)
}

impl Document {
    /// The images bitmap entities place.
    pub fn images(&self) -> &ImageStore {
        &self.images
    }

    /// The image store, for importing and opening.
    pub fn images_mut(&mut self) -> &mut ImageStore {
        &mut self.images
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(format: &str) -> StoredImage {
        StoredImage {
            width: 1,
            height: 1,
            format: format.to_string(),
            bytes: Arc::new(vec![1, 2, 3]),
        }
    }

    #[test]
    fn ids_are_never_reused() {
        let mut store = ImageStore::default();
        let a = store.insert(image("png"));
        store.restore(7, image("jpg"));
        let b = store.insert(image("png"));
        assert_eq!((a, b), (1, 8));
        assert_eq!(store.get(7).map(StoredImage::mime), Some("image/jpeg"));
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn formats_are_known_by_extension_and_by_bytes() {
        assert_eq!(format_of("JPEG"), Some("jpg"));
        assert_eq!(format_of("tif"), Some("tif"));
        assert_eq!(format_of("GIF"), Some("gif"));
        assert_eq!(format_of("psd"), None);
        assert_eq!(sniff(b"GIF89a\x01\0"), Some("gif"));
        assert_eq!(sniff(b"II*\0\x08\0"), Some("tif"));
        assert_eq!(sniff(b"MM\0*\0\0"), Some("tif"));
        assert_eq!(sniff(b"\x89PNG\r\n"), Some("png"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
    }
}
