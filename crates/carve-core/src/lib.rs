//! CarveStudio kernel.
//!
//! The kernel owns the open [`Document`] and answers named commands with JSON
//! or raw bytes. It is platform agnostic: it has no knowledge of Tauri, of the
//! web view or of any UI framework.
//!
//! Units: all lengths are millimetres, all angles are degrees at the API
//! boundary. Y is up in job space, Z is up out of the material.

pub mod assembly;
pub mod blob;
pub mod cam;
pub mod clipart;
pub mod doc;
pub mod error;
pub mod furniture;
pub mod gadgets;
pub mod geom;
pub mod io;
pub mod job_scale;
pub mod joints;
pub mod kernel;
pub mod machines;
pub mod model;
pub mod post;
pub mod prefs;
pub mod progress;
pub mod templates;
pub mod warning;

#[cfg(test)]
mod test_support;

pub use doc::{
    Document, DoubleSidedSetup, FlipAxis, JobKind, JobSetup, RotaryAxis, RotarySetup, Side, Units,
    XyOrigin, ZZero,
};
pub use error::CoreError;
pub use kernel::{Kernel, KernelReply};

/// Version of the kernel crate, reported by the `ping` command.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
