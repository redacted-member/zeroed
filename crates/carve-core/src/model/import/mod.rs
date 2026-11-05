//! 3D model import as a three step flow: Orientate Model ([`orient`]),
//! Position Model ([`place`]) and, instead of positioning, 3D Segmenting
//! ([`segment`]). The settings each import used are kept on its components
//! ([`ImportSource`]) so that the model can be imported again from them.

pub mod commands;
pub mod orient;
pub mod place;
pub mod raster;
pub mod rotary;
pub mod segment;
pub mod vec3;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Combine;

pub use commands::{dispatch, handles, MeshCache};

/// How a component was imported, for Reimport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSource {
    /// The mesh file.
    pub path: PathBuf,
    /// The Orientate Model settings.
    pub orient: orient::Orientation,
    /// The Position Model settings; absent for a segmented import.
    #[serde(default)]
    pub place: Option<place::Placement>,
    /// The segmenting state; present for a segmented import.
    #[serde(default)]
    pub plan: Option<segment::SegmentPlan>,
    /// Shared by every component one import made.
    pub session: u64,
    /// How the components met the model.
    #[serde(default)]
    pub combine: Combine,
    /// A full 3D model on a rotary job: how it sits on the axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotary: Option<rotary::RotaryFit>,
}
