//! A distortion kept on the objects it bent, so the envelope can be edited
//! again until it is baked (Distort Object's Bake Distortion).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::geom::distort::EdgeLines;
use crate::geom::{Path, Point};

/// How the envelope was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DistortMode {
    /// A box round the objects, its nodes dragged.
    #[default]
    Box,
    /// The last selected open vector is the bottom edge.
    Curve,
    /// The last two selected open vectors are the bottom and top edges.
    TwoCurves,
}

/// One distortion: what was bent, its envelope and the geometry before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Distortion {
    /// The objects bent, top level ids, sorted.
    pub ids: Vec<u64>,
    /// How the envelope was made.
    #[serde(default)]
    pub mode: DistortMode,
    /// The curves of the curve modes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curves: Vec<u64>,
    /// Frame angle in degrees (Use Rotated Bounds): the box and the edges
    /// are in the frame turned by it about the job origin.
    #[serde(default)]
    pub angle: f64,
    /// The original box `[minX, minY, maxX, maxY]`, millimetres.
    pub bbox: [f64; 4],
    /// Bottom edge nodes, left to right.
    pub bottom: Vec<Point>,
    /// Top edge nodes, left to right.
    pub top: Vec<Point>,
    /// Left side nodes, bottom to top; straight when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Vec<Point>>,
    /// Right side nodes, bottom to top; straight when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<Vec<Point>>,
    /// Which spans of the edges are straight lines (node editing's line
    /// and curve spans); every span a curve when empty.
    #[serde(default, skip_serializing_if = "EdgeLines::is_empty")]
    pub lines: EdgeLines,
    /// Each bent vector's geometry before bending, by id.
    pub sources: BTreeMap<u64, Path>,
    /// Each bent vector's revision once bent: an edit made elsewhere moves
    /// it, which leaves the object baked.
    pub revisions: BTreeMap<u64, u64>,
}
