//! CAM: tools, the tool database, toolpaths and machining time estimates.
//!
//! Lengths are millimetres and feed rates are millimetres per minute unless
//! a field says otherwise. Toolpath strategies arrive with their own phase,
//! this module holds what they all share.

pub mod array_copy;
pub mod chain;
pub mod chamfer;
pub mod collide;
pub mod commands;
pub mod common;
pub mod compute;
pub mod depth;
pub mod deviation;
pub mod drill;
pub mod drill_bank;
pub mod engrave;
pub mod estimate;
pub mod filter;
pub mod fix;
pub mod fluting;
pub mod gcode_import;
pub mod import_cmd;
pub mod inlay;
pub mod job_sheet;
pub mod keepout;
pub mod keyhole;
pub mod laser;
pub mod library_cmd;
pub mod list_cmd;
pub mod merge;
pub mod moulding;
pub mod move_class;
pub mod naming;
pub mod photo_carve;
pub mod playback;
pub mod pocket;
pub mod preview_cmd;
pub mod prism;
pub mod profile;
pub mod project;
pub mod rounding;
pub mod save;
pub mod selector;
pub mod sim;
pub mod sketch;
pub mod straight_inlay;
pub mod surface;
pub mod tabs;
pub mod taper;
pub mod template;
pub mod texture;
pub mod thread;
pub mod tiling;
pub mod tiling_cmd;
pub mod tool;
pub mod tooldb;
pub mod toolpath;
pub mod toolpath_cmd;
pub mod trial_cut;
pub mod vgroove;

pub use commands::{CamState, COMMANDS};
pub use estimate::{format_duration, TimeEstimate};
pub use filter::CamSettings;
pub use tool::{CuttingData, FeedUnits, Tool, ToolGeometry};
pub use tooldb::{ToolDatabase, ToolGroup, ToolStore, ROOT_GROUP};
pub use toolpath::{Bounds3, Move, MoveKind, Point3, StrategyKind, Toolpath};
