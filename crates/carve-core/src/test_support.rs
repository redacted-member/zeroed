//! Helpers the unit tests share. Not compiled into the shipped library.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A directory under the system temporary directory that deletes itself when
/// it goes out of scope. Tests use it so that nothing ever reads or writes the
/// real user data directory.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a new empty directory whose name starts with `label`.
    ///
    /// # Panics
    /// Panics when the directory cannot be created, which a test cannot
    /// recover from anyway.
    pub fn new(label: &str) -> TempDir {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!("carve-{label}-{}-{nanos}-{count}", std::process::id());
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir_all(&path).expect("temporary directory is created");
        TempDir { path }
    }

    /// The directory itself.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
