//! Geometry: points, transforms, paths, offsets, booleans, text and
//! dimensions.
//!
//! Lengths are millimetres and areas are square millimetres. Angles are
//! radians inside this module; anything that takes or returns degrees says so
//! in its own documentation. Closed outlines are wound counter clockwise and
//! holes clockwise, which is what the boolean and offset operations expect
//! and what they hand back.

pub mod bbox;
pub mod boolean;
pub mod construct;
pub mod contain;
pub mod dimension;
pub mod distort;
pub mod duplicates;
pub mod edit;
pub mod flatten;
pub mod freehand;
pub mod intersect;
pub mod metrics;
pub mod nest;
pub mod offset;
pub mod path;
pub mod point;
pub mod region;
pub mod simplify;
pub mod text;
pub mod trace;
pub mod transform;
pub mod vector_texture;

#[cfg(test)]
mod review033_tests;
#[cfg(test)]
mod review_tests;

pub use bbox::BBox;
pub use boolean::{clip_to_boundary, crop, intersect_paths, subtract, weld, xor};
pub use construct::{
    arc_by_3_points, arc_centre_start_end, bezier_through_points, circle, ellipse,
    ellipse_with_tolerance, line, polyline_path, rect, regular_polygon, rounded_rect, star,
    EllipseForm, RadiusKind, RectCorner, StarInner,
};
pub use contain::{
    encloses_area, path_in_path, point_in_path, validate, validate_set, FillRule, ValidationIssue,
    ValidationKind,
};
pub use dimension::{
    angular_dimension, arc_length_dimension, horizontal_dimension, linear_dimension,
    note_dimension, radius_dimension, vertical_dimension, Dimension, DimensionStyle,
    DimensionUnits,
};
pub use edit::{
    chamfer_corner, close_subpath, close_with, convert_segment, delete_node, extend_to_nearest,
    fillet_corner, fillet_corner_on, gap_length, insert_node, join_paths, join_paths_with,
    join_two, nearest_node, parameter_near, remove_fillet, reverse, reverse_chain, reverse_subpath,
    set_start_node, smooth_node, split_at, split_segment, subpath_span, tbone_side_near,
    trim_to_intersection, CloseStyle, FilletKind, JoinHow, TBoneSide,
};
pub use flatten::{
    cubic_derivative_at, cubic_point_at, cubic_second_derivative_at, fit_arcs, fit_cubics,
    flatten_path, flatten_segment_into, flatten_subpath, path_from_polyline,
};
pub use intersect::{
    path_intersections, path_self_intersects, segment_intersections, subpath_self_crossings,
    subpath_self_intersects, subpath_self_intersects_within, Intersection, PathIntersection,
};
pub use metrics::{
    nearest_on_segment, nearest_point_on_path, path_area, path_bbox, path_centroid, path_is_closed,
    path_length, path_point_at, path_tangent_at, path_winding, segment_bbox, segment_length,
    segment_point_at, segment_tangent_at, subpath_bbox, subpath_length, subpath_signed_area,
    NearestPoint,
};
pub use offset::{
    offset_path, outline_open, parallel_offset_open, sharpen_corners, OffsetCorner, OffsetSide,
};
pub use path::{
    bulge_from_sweep, sweep_from_bulge, ArcGeom, Path, Polygon, Polyline, Segment, SegmentKind,
    SubPath, Winding,
};
pub use point::{Mat3, Point, Tolerance, Vec2};
pub use region::Region;
pub use simplify::{reduce_nodes, reduce_subpath, simplify_polyline};
pub use text::{
    arc_text, cap_height, layout_glyphs, layout_text, layout_text_block, line_height, line_width,
    outlines_bbox, text_on_curve, text_on_curve_with, wrap_text, CurveLayout, FontLibrary,
    LoadedFont, PlacedGlyph, TextAlign, TextBlockSpec, TextSide, TextSpec,
};
pub use transform::{
    arc_to_cubic_segments, arcs_to_cubics, transform_path, transform_polyline, transform_segment,
    transform_subpath,
};
