//! Furniture Designer: parametric furniture templates turned into flat
//! panels on sheets, Joint Creator joints between them, the toolpaths that
//! cut them and Assembly View pieces placed where the panels go.
//!
//! A design is a template (a full copy, so a job opens without the
//! library), the values the maker typed and the joints they picked. Every
//! change regenerates everything the design made in one undo step,
//! reusing the ids of the vectors, joints, toolpaths and pieces of panels
//! that still exist.
//!
//! Furniture space: X width (left to right), Y depth (front at 0, back
//! +Y), Z height (floor at 0), millimetres.

pub mod attach;
pub mod commands;
pub mod connect;
pub mod design;
pub mod eval;
pub mod features;
pub mod generate;
pub mod layout;
pub mod library;
pub mod mesh;
pub mod models;
pub mod report;
pub mod template;
#[cfg(test)]
mod tests;

pub use commands::{dispatch, handles};
pub use design::{FurnitureDesign, Made};
