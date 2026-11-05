//! Application preferences.
//!
//! Preferences belong to the user, not to a document. They live in
//! `Zeroed/prefs.json` under the platform configuration directory and are
//! read once when the kernel starts. The kernel applies the ones it uses
//! itself (undo depth, units of a new job); the rest are held for the front
//! end, which reads them with `prefs.get` and changes them with `prefs.set`.
//!
//! Every field has a default, so a file written by an older build loads with
//! the new fields filled in. A file that cannot be read as preferences is
//! renamed aside, never deleted, and the defaults are used instead.

use std::path::{Path as FsPath, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::doc::{Document, Units};
use crate::error::CoreError;
use crate::kernel::KernelReply;

/// Version written into the file.
pub const PREFS_VERSION: u32 = 1;

/// Largest undo depth the user can ask for.
pub const MAX_UNDO_LIMIT: usize = 1000;
/// Longest recent files list the user can ask for.
pub const MAX_RECENT_FILES: usize = 20;
/// Fonts the font lists keep at their top as recently used.
pub const RECENT_FONTS: usize = 5;
/// Longest autosave interval, in minutes.
pub const MAX_AUTOSAVE_MINUTES: u32 = 120;

/// Longest geometry fixing timeout, in seconds.
pub const MAX_GEOMETRY_FIX_SECONDS: u32 = 3600;
/// Narrowest a dock panel can be dragged, in CSS pixels.
pub const MIN_DOCK_WIDTH: u32 = 200;
/// Widest a dock panel can be dragged, in CSS pixels.
pub const MAX_DOCK_WIDTH: u32 = 640;
/// Tallest a dock list (toolpath list, Component Tree) can be dragged.
pub const MAX_LIST_HEIGHT: u32 = 4000;

/// Every preference, grouped the way the Preferences dialog shows them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    /// File format version.
    pub version: u32,
    /// Units, undo, autosave, tool tips, recent files.
    pub general: GeneralPrefs,
    /// Snapping, grid and rulers of the 2D view.
    pub view2d: View2dPrefs,
    /// Background, shading and camera of the 3D view.
    pub view3d: View3dPrefs,
    /// Toolpath tolerance, preview colours and recalculation.
    pub toolpaths: ToolpathPrefs,
    /// Which warnings ask before going ahead.
    pub confirmations: ConfirmationPrefs,
    /// Window layout remembered between sessions.
    pub window: WindowPrefs,
    /// The machines and which one is active.
    pub machines: crate::machines::MachinePrefs,
    /// Modelling settings: the sculpting presets.
    pub modeling: ModelingPrefs,
    /// Gadget dialogs: the values each gadget last ran with.
    pub gadgets: GadgetPrefs,
}

/// What gadget dialogs remember between runs.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GadgetPrefs {
    /// Per gadget name, the dialog values of its last successful run
    /// (lengths in millimetres), which its dialog starts from next time.
    pub last_values: std::collections::BTreeMap<String, Value>,
}

/// Modelling settings kept between sessions.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelingPrefs {
    /// Sculpting tool settings saved under a name, per tool.
    pub sculpt_presets: Vec<SculptPreset>,
}

/// One saved set of sculpting settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SculptPreset {
    /// The name it is listed under.
    pub name: String,
    /// The sculpting tool it belongs to (smooth, smudge, deposit, remove or
    /// restore).
    pub tool: String,
    /// The Sculpt form's settings as the form keeps them.
    pub settings: serde_json::Value,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            version: PREFS_VERSION,
            general: GeneralPrefs::default(),
            view2d: View2dPrefs::default(),
            view3d: View3dPrefs::default(),
            toolpaths: ToolpathPrefs::default(),
            confirmations: ConfirmationPrefs::default(),
            window: WindowPrefs::default(),
            machines: crate::machines::MachinePrefs::default(),
            modeling: ModelingPrefs::default(),
            gadgets: GadgetPrefs::default(),
        }
    }
}

/// The two smooth join methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SmoothJoinMethod {
    /// A full, rounded bridge.
    #[default]
    Smoother,
    /// A symmetrical, shallower bridge.
    Shallow,
}

/// General preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GeneralPrefs {
    /// Units a new job starts in.
    pub default_units: Units,
    /// How many steps Undo keeps, 1 to [`MAX_UNDO_LIMIT`].
    pub undo_limit: usize,
    /// Minutes between autosaves, 0 for never.
    pub autosave_minutes: u32,
    /// Whether buttons show a tip when hovered.
    pub show_tool_tips: bool,
    /// Whether the status bar shows what the pointer can do in the views.
    pub show_usage_info: bool,
    /// How many files the Open Recent list keeps, 0 to [`MAX_RECENT_FILES`].
    pub recent_files_length: usize,
    /// Recently opened or saved jobs, newest first.
    pub recent_files: Vec<String>,
    /// Font families picked lately in the text forms, newest first, at
    /// most [`RECENT_FONTS`].
    pub recent_fonts: Vec<String>,
    /// Whether the sculpting tool reads a graphics tablet's pen pressure.
    pub graphics_tablet: bool,
    /// Whether joining vectors bridges a gap with a curve tangent to both
    /// ends rather than moving the second vector's first node.
    pub smooth_join: bool,
    /// Which of the two smooth join methods the smooth Join / Close tool
    /// uses: the smoother one or the symmetrical, shallower one.
    pub smooth_join_method: SmoothJoinMethod,
    /// Whether Draw Line starts in curve mode (it remembers the last mode).
    pub line_curve_mode: bool,
    /// Whether new text starts with the anchor last used instead of the
    /// bottom left corner.
    pub last_text_anchor: bool,
    /// The anchor last used for new text.
    pub text_anchor: crate::doc::TextAnchor,
    /// Whether the Design tab shows the extra File Operations row (New,
    /// Save, Undo, Redo) above the usual one.
    pub show_file_operations: bool,
    /// Whether the Design tab shows the Align Objects group ("Show common
    /// tools on drawing tab" in the Alignment Tools form).
    pub show_align_tools: bool,
    /// Whether the getting started guide opens when the app starts (its
    /// "Don't show again" check box clears it).
    pub show_guide: bool,
}

impl Default for GeneralPrefs {
    fn default() -> Self {
        GeneralPrefs {
            default_units: Units::Mm,
            undo_limit: crate::doc::UNDO_LIMIT,
            autosave_minutes: 0,
            show_tool_tips: true,
            show_usage_info: true,
            recent_files_length: 10,
            recent_files: Vec::new(),
            recent_fonts: Vec::new(),
            graphics_tablet: false,
            smooth_join: false,
            smooth_join_method: SmoothJoinMethod::Smoother,
            line_curve_mode: false,
            last_text_anchor: false,
            text_anchor: crate::doc::TextAnchor::BottomLeft,
            show_file_operations: false,
            show_align_tools: false,
            show_guide: true,
        }
    }
}

/// Snapping options a new session starts with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SnapDefaults {
    /// Snap to the grid.
    pub grid: bool,
    /// Snap to guides.
    pub guides: bool,
    /// Snap to nodes.
    pub nodes: bool,
    /// Snap to span midpoints.
    pub midpoints: bool,
    /// Snap to centres.
    pub centres: bool,
    /// Snap to intersections.
    pub intersections: bool,
    /// Snap to the job corners and centre.
    pub job_points: bool,
    /// Smart snapping alignment lines.
    pub smart: bool,
}

impl Default for SnapDefaults {
    fn default() -> Self {
        SnapDefaults {
            grid: false,
            guides: true,
            nodes: true,
            midpoints: true,
            centres: true,
            intersections: true,
            job_points: true,
            smart: true,
        }
    }
}

/// 2D view preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct View2dPrefs {
    /// Snapping a session starts with.
    pub snap: SnapDefaults,
    /// Snap radius in screen pixels, 1 to 50.
    pub snap_radius_px: u32,
    /// Whether the grid shows.
    pub show_grid: bool,
    /// Whether the rulers show.
    pub show_rulers: bool,
    /// Grid spacing in millimetres.
    pub grid_spacing: f64,
    /// Drags move in round steps that follow the zoom (Snap Distances).
    pub snap_distances: bool,
    /// Distance Ctrl+Shift+arrow nudges the selection, in millimetres.
    pub nudge_distance: f64,
    /// How the grid draws: points at the grid spacing, or lines.
    pub grid_style: GridStyle,
}

/// How the 2D view draws its grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GridStyle {
    /// A point at every grid spacing.
    Points,
    /// Lines that adapt to the zoom.
    Lines,
}

impl Default for View2dPrefs {
    fn default() -> Self {
        View2dPrefs {
            snap: SnapDefaults::default(),
            snap_radius_px: 10,
            show_grid: true,
            show_rulers: true,
            grid_spacing: 10.0,
            snap_distances: false,
            nudge_distance: 5.0,
            grid_style: GridStyle::Points,
        }
    }
}

/// How the 3D view background is painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Background3d {
    /// The colours of the light or dark theme.
    Theme,
    /// A gradient from the top colour to the bottom colour.
    Gradient,
    /// The top colour only.
    Solid,
}

/// How model surfaces are shaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Shading {
    /// Smooth normals.
    Smooth,
    /// One normal per facet.
    Flat,
}

/// How the 3D view projects the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Projection {
    /// Things further away look smaller.
    Perspective,
    /// Parallel projection, so 2D content lines up over the 3D view.
    Orthographic,
}

/// Which mouse scheme moves the 3D camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Navigation3d {
    /// Left drag rotates, right or middle drag pans, the wheel zooms.
    Modern,
    /// Left drag rotates, right drag zooms, middle or Shift drag pans.
    Legacy,
}

/// How surfaces are drawn in the 3D view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RenderMode {
    /// Lit, filled surfaces.
    Shaded,
    /// Triangle edges only.
    Wireframe,
    /// Filled surfaces with their edges drawn over them.
    ShadedWire,
}

/// The look of the material in the 3D view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Material3d {
    /// The theme's material colour.
    Theme,
    /// Pale softwood.
    Pine,
    /// Mid brown hardwood.
    Oak,
    /// Dark hardwood.
    Walnut,
    /// Very pale hardwood.
    Maple,
    /// Medium density fibreboard.
    Mdf,
    /// Glossy plastic sheet.
    Acrylic,
    /// Brushed metal.
    Aluminium,
    /// The solid colour below.
    Solid,
}

/// How the 2D and 3D views share the window when both show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TileDirection {
    /// Side by side.
    Vertical,
    /// One above the other.
    Horizontal,
}

/// 3D view preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct View3dPrefs {
    /// Background style.
    pub background: Background3d,
    /// Top background colour, `#rrggbb`.
    pub top_colour: String,
    /// Bottom background colour, `#rrggbb`.
    pub bottom_colour: String,
    /// Surface shading.
    pub shading: Shading,
    /// Whether view changes glide instead of jumping.
    pub animate_camera: bool,
    /// Perspective or orthographic camera.
    pub projection: Projection,
    /// Mouse scheme for moving the camera.
    pub navigation: Navigation3d,
    /// Filled, wireframe or both.
    pub render_mode: RenderMode,
    /// The look of the material and model.
    pub material: Material3d,
    /// Colour used when `material` is `solid`, `#rrggbb`.
    pub solid_colour: String,
    /// Whether the model (or a toolpath preview) is drawn.
    pub show_model: bool,
    /// Whether the material block is drawn.
    pub show_block: bool,
    /// Whether toolpaths are drawn in 3D.
    pub show_toolpaths: bool,
    /// Whether 2D vectors are drawn on the material top.
    pub show_vectors: bool,
    /// Whether the orientation cube shows in a corner.
    pub show_cube: bool,
    /// Whether arrows mark the job origin (X0, Y0, Z0).
    pub draw_origin: bool,
    /// Whether a see through plane marks the base plane of the model.
    pub draw_modeling_plane: bool,
    /// Whether a double sided job shows both sides at once.
    pub multi_sided: bool,
    /// Whether a rotary job is drawn wrapped round its axis.
    pub auto_wrap: bool,
    /// Whether the model is drawn in its material and component colours
    /// rather than plain grey.
    pub color_shaded: bool,
    /// Whether the main light shines from the camera instead of the top left.
    pub light_follows: bool,
    /// Whether the lighting is set for deeper shadows that bring out detail.
    pub shadow_shading: bool,
    /// The user's own material textures, pictures the 3D view can lay over
    /// the material, by category.
    pub textures: Vec<MaterialTexture>,
}

/// A picture added to the material list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialTexture {
    /// Identity a job's appearance refers to it by, above zero.
    pub id: u32,
    /// Name shown in the list.
    pub name: String,
    /// Group it is listed under.
    pub category: String,
    /// The picture file.
    pub path: String,
}

impl Default for View3dPrefs {
    fn default() -> Self {
        View3dPrefs {
            background: Background3d::Theme,
            top_colour: "#4c525a".to_string(),
            bottom_colour: "#23262b".to_string(),
            shading: Shading::Smooth,
            animate_camera: true,
            projection: Projection::Perspective,
            navigation: Navigation3d::Modern,
            render_mode: RenderMode::Shaded,
            material: Material3d::Theme,
            solid_colour: "#c9b28f".to_string(),
            show_model: true,
            show_block: true,
            show_toolpaths: true,
            show_vectors: false,
            show_cube: true,
            draw_origin: false,
            draw_modeling_plane: false,
            multi_sided: false,
            auto_wrap: true,
            color_shaded: true,
            light_follows: false,
            shadow_shading: false,
            textures: Vec::new(),
        }
    }
}

/// Toolpath preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolpathPrefs {
    /// Filter tolerance of 2D toolpaths, in millimetres: straight moves
    /// within it of a line are merged once a toolpath is calculated.
    pub tolerance: f64,
    /// Filter tolerance of 3D and projected toolpaths, in millimetres.
    #[serde(rename = "tolerance3d")]
    pub tolerance_3d: f64,
    /// Filter tolerance of V-groove toolpaths, in millimetres.
    pub vgroove_tolerance: f64,
    /// Projecting onto the model drops the whole tool onto the surface.
    pub drop_tool: bool,
    /// Raster clearing cuts each separate strip in turn.
    pub new_raster: bool,
    /// Seconds a calculation spends repairing vectors that cross themselves
    /// (Toolpath Geometry Fixing Timeout); zero repairs none.
    pub geometry_fix_seconds: u32,
    /// After a job size change every toolpath's depth is checked, not only
    /// the visible toolpaths'.
    pub check_all_depths: bool,
    /// A duplicated toolpath goes to the end of the list rather than
    /// straight after the original.
    pub append_duplicated: bool,
    /// New toolpaths go to the end of the selected toolpath group rather
    /// than the end of the list.
    pub append_to_selected_group: bool,
    /// Which toolpaths the list shows when a job opens: the active sheet's
    /// or every sheet's.
    pub default_filter: ToolpathFilter,
    /// Editing a toolpath of another sheet makes that sheet active.
    pub select_sheet_on_edit: bool,
    /// Whether changing the job asks before recalculating toolpaths.
    pub ask_before_recalculating: bool,
    /// Preview colour of cutting moves, `#rrggbb`.
    pub cut_colour: String,
    /// Preview colour of rapid moves, `#rrggbb`.
    pub rapid_colour: String,
    /// Colour a solid 2D preview fills the cut width with, `#rrggbb`.
    pub solid_preview_colour: String,
    /// The toolpath operations stay in the Toolpaths panel above the
    /// Preview Toolpaths form while it is open.
    pub show_operations_with_preview: bool,
    /// The Material Setup summary on the Toolpaths panel shows every
    /// setting (Show detailed summary on toolpath tab) instead of the
    /// simple view.
    pub detailed_material_summary: bool,
    /// The output choice Save Toolpaths opens with: the one it last saved
    /// with.
    pub save_output: crate::cam::save::SaveOutput,
    /// Save Toolpaths' "M6 macro applies tool length": on, a probing post
    /// leaves the length to the M6 macro; off, it writes G43 H after each
    /// change.
    pub m6_applies_length: bool,
    /// Whether toolpaths are drawn in the 2D view.
    pub draw_2d_previews: bool,
    /// Whether the 2D view fills the area a toolpath cuts instead of drawing
    /// its centre lines.
    pub solid_previews: bool,
    /// Whether the 3D view opens after a new toolpath is calculated.
    pub auto_open_3d: bool,
    /// Whether rapid moves are drawn.
    pub draw_rapids: bool,
    /// Whether plunge moves are drawn.
    pub draw_plunges: bool,
    /// Whether retract moves are drawn.
    pub draw_retracts: bool,
    /// Whether join moves between contours are drawn.
    pub draw_joins: bool,
    /// Scale factor the machining time estimates are multiplied by.
    pub estimate_scale: f64,
}

impl Default for ToolpathPrefs {
    fn default() -> Self {
        ToolpathPrefs {
            tolerance: 0.025,
            tolerance_3d: 0.01,
            vgroove_tolerance: 0.025,
            drop_tool: false,
            new_raster: true,
            geometry_fix_seconds: crate::cam::filter::DEFAULT_GEOMETRY_FIX_SECONDS,
            check_all_depths: false,
            append_duplicated: false,
            append_to_selected_group: true,
            default_filter: ToolpathFilter::ActiveSheet,
            select_sheet_on_edit: true,
            ask_before_recalculating: true,
            cut_colour: "#1d4ed8".to_string(),
            rapid_colour: "#d62828".to_string(),
            solid_preview_colour: "#5b8def".to_string(),
            show_operations_with_preview: false,
            detailed_material_summary: false,
            save_output: crate::cam::save::SaveOutput::Selected,
            m6_applies_length: true,
            draw_2d_previews: true,
            solid_previews: false,
            auto_open_3d: true,
            draw_rapids: true,
            draw_plunges: true,
            draw_retracts: true,
            draw_joins: true,
            estimate_scale: 1.0,
        }
    }
}

/// Warnings the user can turn off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConfirmationPrefs {
    /// Ask before deleting a layer that holds objects.
    pub delete_layer: bool,
    /// Ask before deleting a sheet that holds objects.
    pub delete_sheet: bool,
    /// Ask before deleting a tool or tool group.
    pub delete_tool: bool,
    /// What V-Groove and Pocket do when their vectors cross.
    pub toolpath_intersections: IntersectionCheck,
    /// What a radiused offset does when its vectors cross.
    pub radiused_offset_intersections: IntersectionCheck,
    /// What a sharp offset does when its vectors cross.
    pub sharp_offset_intersections: IntersectionCheck,
    /// Warn before embossing a large component.
    pub emboss_warning: bool,
}

impl Default for ConfirmationPrefs {
    fn default() -> Self {
        ConfirmationPrefs {
            delete_layer: true,
            delete_sheet: true,
            delete_tool: true,
            toolpath_intersections: IntersectionCheck::Ask,
            radiused_offset_intersections: IntersectionCheck::Ask,
            sharp_offset_intersections: IntersectionCheck::Ask,
            emboss_warning: true,
        }
    }
}

/// The sheet filter the toolpath list starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolpathFilter {
    /// Only the active sheet's toolpaths.
    ActiveSheet,
    /// Every sheet's toolpaths, the inactive ones faded.
    AllSheets,
}

/// What happens when the vectors a calculation uses cross each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntersectionCheck {
    /// Ask whether to go on, open the Vector Validator or stop.
    Ask,
    /// Open the Vector Validator on the crossing vectors.
    Validator,
    /// Do not check.
    Skip,
    /// Go on without asking (sharp offsets only).
    Proceed,
}

/// Which tab is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowTab {
    /// Design tab (drawing and modeling tools).
    Design,
    /// Components tab (the component tree).
    Components,
    /// Drawing tab. Older layouts only: the front end shows the Design tab.
    Drawing,
    /// Modeling tab. Older layouts only: the front end shows the Design tab.
    Modeling,
    /// Toolpaths tab. Older layouts only: the toolpaths now live in their
    /// own panel, so the front end shows the Design tab for it.
    Toolpaths,
    /// Clipart tab.
    Clipart,
    /// Layers tab.
    Layers,
    /// Sheets tab.
    Sheets,
    /// Assembly tab.
    Assembly,
}

/// Which views are showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowViewMode {
    /// The 2D view only.
    #[serde(rename = "2d")]
    TwoD,
    /// The 3D view only.
    #[serde(rename = "3d")]
    ThreeD,
    /// Both side by side.
    #[serde(rename = "split")]
    Split,
    /// The assembly view.
    #[serde(rename = "assembly")]
    Assembly,
}

/// Window layout remembered between sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowPrefs {
    /// Whether the layout below is restored at start.
    pub remember: bool,
    /// Tab showing when the app last closed.
    pub active_tab: WindowTab,
    /// Views showing when the app last closed.
    pub view_mode: WindowViewMode,
    /// Whether Job Setup opens when the app starts.
    pub job_setup_at_start: bool,
    /// How the views tile when both show.
    pub tile: TileDirection,
    /// Whether the left tool panel stays open (pinned) or hides to its tabs.
    pub left_pinned: bool,
    /// Whether the Toolpaths panel stays open (pinned) or hides to its tab.
    pub toolpaths_pinned: bool,
    /// Width of the left tool panel in CSS pixels.
    pub left_width: u32,
    /// Width of the Toolpaths panel in CSS pixels.
    pub right_width: u32,
    /// Height of the toolpath list under the Toolpaths operations, CSS
    /// pixels; 0 lets the panel share the space.
    pub toolpath_list_height: u32,
    /// Height of the Component Tree under the Modeling tools, CSS pixels;
    /// 0 lets the tab share the space.
    pub component_tree_height: u32,
    /// Size the font drop-down list was dragged to, CSS pixels; 0 keeps
    /// the usual size.
    pub font_list_width: u32,
    /// Height of the font drop-down list, CSS pixels; 0 keeps the usual size.
    pub font_list_height: u32,
    /// Ruler tint while the top side of a double sided job shows, `#rrggbb`.
    pub top_ruler_colour: String,
    /// Ruler tint while the bottom side shows, `#rrggbb`.
    pub bottom_ruler_colour: String,
}

impl Default for WindowPrefs {
    fn default() -> Self {
        WindowPrefs {
            remember: true,
            active_tab: WindowTab::Design,
            view_mode: WindowViewMode::TwoD,
            job_setup_at_start: true,
            tile: TileDirection::Vertical,
            left_pinned: true,
            toolpaths_pinned: true,
            left_width: 268,
            right_width: 300,
            toolpath_list_height: 0,
            component_tree_height: 0,
            font_list_width: 0,
            font_list_height: 0,
            top_ruler_colour: "#5b9bd5".to_string(),
            bottom_ruler_colour: "#e0a040".to_string(),
        }
    }
}

impl Preferences {
    /// Checks every value is in range.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] naming the first field that fails.
    pub fn validate(&self) -> Result<(), CoreError> {
        let general = &self.general;
        if !(1..=MAX_UNDO_LIMIT).contains(&general.undo_limit) {
            return Err(CoreError::validation(format!(
                "general.undoLimit must be 1 to {MAX_UNDO_LIMIT}"
            )));
        }
        if general.autosave_minutes > MAX_AUTOSAVE_MINUTES {
            return Err(CoreError::validation(format!(
                "general.autosaveMinutes must be 0 to {MAX_AUTOSAVE_MINUTES}"
            )));
        }
        if general.recent_files_length > MAX_RECENT_FILES {
            return Err(CoreError::validation(format!(
                "general.recentFilesLength must be 0 to {MAX_RECENT_FILES}"
            )));
        }
        if general.recent_files.iter().any(|p| p.trim().is_empty()) {
            return Err(CoreError::validation(
                "general.recentFiles holds an empty path",
            ));
        }
        let view2d = &self.view2d;
        if !(1..=50).contains(&view2d.snap_radius_px) {
            return Err(CoreError::validation("view2d.snapRadiusPx must be 1 to 50"));
        }
        if !(view2d.grid_spacing.is_finite()
            && view2d.grid_spacing > 0.0
            && view2d.grid_spacing <= 10_000.0)
        {
            return Err(CoreError::validation(
                "view2d.gridSpacing must be above 0 and at most 10000 mm",
            ));
        }
        if !(view2d.nudge_distance.is_finite()
            && view2d.nudge_distance > 0.0
            && view2d.nudge_distance <= 1_000.0)
        {
            return Err(CoreError::validation(
                "view2d.nudgeDistance must be above 0 and at most 1000 mm",
            ));
        }
        colour("view3d.topColour", &self.view3d.top_colour)?;
        colour("view3d.bottomColour", &self.view3d.bottom_colour)?;
        colour("view3d.solidColour", &self.view3d.solid_colour)?;
        let scale = self.toolpaths.estimate_scale;
        if !(scale.is_finite() && (0.1..=10.0).contains(&scale)) {
            return Err(CoreError::validation(
                "toolpaths.estimateScale must be 0.1 to 10",
            ));
        }
        for (index, texture) in self.view3d.textures.iter().enumerate() {
            if texture.id == 0
                || self.view3d.textures[..index]
                    .iter()
                    .any(|t| t.id == texture.id)
            {
                return Err(CoreError::validation(
                    "view3d.textures need ids above zero, each used once",
                ));
            }
            if texture.name.trim().is_empty() || texture.path.trim().is_empty() {
                return Err(CoreError::validation(
                    "view3d.textures need a name and a picture file",
                ));
            }
        }
        let tolerance = self.toolpaths.tolerance;
        if !(tolerance.is_finite() && (0.0001..=1.0).contains(&tolerance)) {
            return Err(CoreError::validation(
                "toolpaths.tolerance must be 0.0001 to 1 mm",
            ));
        }
        for (field, value) in [
            ("toolpaths.tolerance3d", self.toolpaths.tolerance_3d),
            (
                "toolpaths.vgrooveTolerance",
                self.toolpaths.vgroove_tolerance,
            ),
        ] {
            if !(value.is_finite() && (0.0001..=1.0).contains(&value)) {
                return Err(CoreError::validation(format!(
                    "{field} must be 0.0001 to 1 mm"
                )));
            }
        }
        for (field, value) in [
            (
                "confirmations.toolpathIntersections",
                self.confirmations.toolpath_intersections,
            ),
            (
                "confirmations.radiusedOffsetIntersections",
                self.confirmations.radiused_offset_intersections,
            ),
        ] {
            if value == IntersectionCheck::Proceed {
                return Err(CoreError::validation(format!(
                    "{field} is ask, validator or skip"
                )));
            }
        }
        if self.toolpaths.geometry_fix_seconds > MAX_GEOMETRY_FIX_SECONDS {
            return Err(CoreError::validation(format!(
                "toolpaths.geometryFixSeconds must be 0 to {MAX_GEOMETRY_FIX_SECONDS}"
            )));
        }
        colour("toolpaths.cutColour", &self.toolpaths.cut_colour)?;
        colour("toolpaths.rapidColour", &self.toolpaths.rapid_colour)?;
        colour(
            "toolpaths.solidPreviewColour",
            &self.toolpaths.solid_preview_colour,
        )?;
        colour("window.topRulerColour", &self.window.top_ruler_colour)?;
        colour("window.bottomRulerColour", &self.window.bottom_ruler_colour)?;
        let docks = MIN_DOCK_WIDTH..=MAX_DOCK_WIDTH;
        if !docks.contains(&self.window.left_width) || !docks.contains(&self.window.right_width) {
            return Err(CoreError::validation(format!(
                "window dock widths must be {MIN_DOCK_WIDTH} to {MAX_DOCK_WIDTH} px"
            )));
        }
        if self.window.toolpath_list_height > MAX_LIST_HEIGHT
            || self.window.component_tree_height > MAX_LIST_HEIGHT
            || self.window.font_list_width > MAX_LIST_HEIGHT
            || self.window.font_list_height > MAX_LIST_HEIGHT
        {
            return Err(CoreError::validation(format!(
                "window list heights must be 0 to {MAX_LIST_HEIGHT} px"
            )));
        }
        self.machines.validate()?;
        Ok(())
    }

    /// Tidies values that have an obvious fix: drops duplicate recent files
    /// and fonts and cuts the lists to their lengths.
    pub fn normalise(&mut self) {
        let general = &mut self.general;
        let mut seen: Vec<String> = Vec::with_capacity(general.recent_files.len());
        for path in general.recent_files.drain(..) {
            if !seen.contains(&path) {
                seen.push(path);
            }
        }
        seen.truncate(general.recent_files_length);
        general.recent_files = seen;
        let mut fonts: Vec<String> = Vec::with_capacity(RECENT_FONTS);
        for family in general.recent_fonts.drain(..) {
            if !family.trim().is_empty() && !fonts.contains(&family) {
                fonts.push(family);
            }
        }
        fonts.truncate(RECENT_FONTS);
        general.recent_fonts = fonts;
        self.machines.normalise();
        self.version = PREFS_VERSION;
    }

    /// Answers these preferences with `patch` merged over them. Objects in
    /// the patch merge field by field; any other value replaces the old one.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for a key that is not a preference or a
    /// value out of range, [`CoreError::InvalidArgs`] for a value of the wrong
    /// type.
    pub fn merged(&self, patch: &Value) -> Result<Preferences, CoreError> {
        let Value::Object(patch) = patch else {
            return Err(CoreError::invalid_args(
                "prefs.set",
                "expected an object of preferences",
            ));
        };
        let mut current = serde_json::to_value(self)?;
        merge_into(&mut current, patch, "")?;
        let mut merged: Preferences = serde_json::from_value(current)
            .map_err(|error| CoreError::invalid_args("prefs.set", error))?;
        merged.normalise();
        merged.validate()?;
        Ok(merged)
    }

    /// Makes `document` follow the preferences that apply to any document.
    pub fn apply_to_document(&self, document: &mut Document) {
        document.set_undo_limit(self.general.undo_limit);
        let estimate = self
            .machines
            .active_machine()
            .map_or_else(crate::cam::TimeEstimate::default, |machine| {
                machine.time_estimate(self.toolpaths.estimate_scale)
            });
        document.set_time_estimate(estimate.scaled(self.toolpaths.estimate_scale));
        document.set_cam_settings(crate::cam::CamSettings {
            tolerance_2d: self.toolpaths.tolerance,
            tolerance_3d: self.toolpaths.tolerance_3d,
            tolerance_vgroove: self.toolpaths.vgroove_tolerance,
            drop_tool: self.toolpaths.drop_tool,
            new_raster: self.toolpaths.new_raster,
            geometry_fix_seconds: self.toolpaths.geometry_fix_seconds,
            check_all_depths: self.toolpaths.check_all_depths,
            append_duplicated: self.toolpaths.append_duplicated,
            append_to_selected_group: self.toolpaths.append_to_selected_group,
        });
    }

    /// Makes a freshly created `document` follow the preferences, including
    /// the units a new job starts in.
    pub fn apply_to_new_document(&self, document: &mut Document) {
        self.apply_to_document(document);
        let mut job = *document.job();
        // New jobs start with the preferred material look.
        job.appearance.material = self.view3d.material;
        if let Some(rgb) = crate::doc::job_colour(&self.view3d.solid_colour) {
            job.appearance.colour = rgb;
        }
        if job.units == self.general.default_units {
            let _ = document.set_job(job);
            return;
        }
        job.units = self.general.default_units;
        if job.units == Units::Inch {
            // A 12 x 8 x 0.75 inch board rather than an odd metric size.
            job.width = 12.0 * 25.4;
            job.height = 8.0 * 25.4;
            job.thickness = 0.75 * 25.4;
        }
        // Outside an undo step, so the new job starts with an empty history.
        // The sizes above are always valid, so there is no error to report.
        let _ = document.set_job(job);
    }
}

fn colour(field: &str, value: &str) -> Result<(), CoreError> {
    let bytes = value.as_bytes();
    let ok =
        bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(|b| b.is_ascii_hexdigit());
    if ok {
        Ok(())
    } else {
        Err(CoreError::validation(format!(
            "{field} must be a color like #1a2b3c"
        )))
    }
}

fn merge_into(target: &mut Value, patch: &Map<String, Value>, at: &str) -> Result<(), CoreError> {
    let Value::Object(fields) = target else {
        return Err(CoreError::validation(format!("{at} is not a group")));
    };
    for (key, value) in patch {
        let path = if at.is_empty() {
            key.clone()
        } else {
            format!("{at}.{key}")
        };
        if key == "version" && at.is_empty() {
            continue;
        }
        let Some(slot) = fields.get_mut(key) else {
            return Err(CoreError::validation(format!("{path} is not a preference")));
        };
        match (slot.is_object(), value) {
            (true, Value::Object(inner)) => merge_into(slot, inner, &path)?,
            (true, _) => {
                return Err(CoreError::invalid_args(
                    "prefs.set",
                    format!("{path} expects an object"),
                ))
            }
            (false, _) => *slot = value.clone(),
        }
    }
    Ok(())
}

/// Where the preferences file lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefsStore {
    path: PathBuf,
}

impl PrefsStore {
    /// A store that reads and writes the given file. Tests use this to stay
    /// away from the user's own preferences.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        PrefsStore { path: path.into() }
    }

    /// The store in the user configuration directory,
    /// `Zeroed/prefs.json`. Falls back to the temporary directory on a
    /// platform that has no configuration directory.
    pub fn user_default() -> Self {
        let base = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
        PrefsStore::at(base.join("Zeroed").join("prefs.json"))
    }

    /// The file the store reads and writes.
    pub fn path(&self) -> &FsPath {
        &self.path
    }

    /// Reads the preferences. A missing file gives the defaults. A file that
    /// is not valid preferences is renamed aside (see [`Loaded::set_aside`])
    /// and the defaults are used.
    pub fn load(&self) -> Loaded {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(_) => return Loaded::defaults(),
        };
        let parsed = serde_json::from_str::<Preferences>(&text)
            .map_err(CoreError::from)
            .and_then(|mut prefs| {
                prefs.normalise();
                prefs.validate().map(|()| prefs)
            });
        match parsed {
            Ok(prefs) => Loaded {
                prefs,
                set_aside: None,
            },
            Err(_) => Loaded {
                prefs: Preferences::default(),
                set_aside: self.set_aside(),
            },
        }
    }

    /// Writes the preferences, creating the directory if it is missing.
    ///
    /// # Errors
    /// Returns [`CoreError::Io`] when the file cannot be written.
    pub fn save(&self, prefs: &Preferences) -> Result<(), CoreError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| CoreError::io(parent.display(), error))?;
        }
        let text = serde_json::to_string_pretty(prefs)?;
        std::fs::write(&self.path, text).map_err(|error| CoreError::io(self.path.display(), error))
    }

    /// Renames the file to the first free `prefs.json.broken`,
    /// `prefs.json.broken2`, ... and answers the new path.
    fn set_aside(&self) -> Option<PathBuf> {
        let name = self.path.file_name()?.to_string_lossy().into_owned();
        (1..1000)
            .map(|n| {
                let suffix = if n == 1 { String::new() } else { n.to_string() };
                self.path.with_file_name(format!("{name}.broken{suffix}"))
            })
            .find(|candidate| !candidate.exists())
            .and_then(|target| std::fs::rename(&self.path, &target).ok().map(|()| target))
    }
}

/// What [`PrefsStore::load`] found.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// The preferences to use.
    pub prefs: Preferences,
    /// Where an unreadable file was moved to, if one was.
    pub set_aside: Option<PathBuf>,
}

impl Loaded {
    fn defaults() -> Self {
        Loaded {
            prefs: Preferences::default(),
            set_aside: None,
        }
    }
}

/// The preferences the kernel holds, and where they are saved.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Prefs {
    store: Option<PrefsStore>,
    values: Preferences,
    set_aside: Option<PathBuf>,
}

impl Prefs {
    /// Defaults that are never written anywhere.
    pub fn in_memory() -> Self {
        Prefs::default()
    }

    /// Preferences read from `store`, saved back there on every change.
    pub fn load(store: PrefsStore) -> Self {
        let loaded = store.load();
        Prefs {
            store: Some(store),
            values: loaded.prefs,
            set_aside: loaded.set_aside,
        }
    }

    /// The user's preferences file. Unit tests of this crate get in-memory
    /// defaults instead so that they never touch the user's files.
    pub fn user() -> Self {
        if cfg!(test) {
            Prefs::in_memory()
        } else {
            Prefs::load(PrefsStore::user_default())
        }
    }

    /// The current values.
    pub fn values(&self) -> &Preferences {
        &self.values
    }

    /// Merges `patch` over the current values, saves them and makes them
    /// current. Nothing changes if the patch is invalid or the save fails.
    ///
    /// # Errors
    /// See [`Preferences::merged`] and [`PrefsStore::save`].
    pub fn set(&mut self, patch: &Value) -> Result<&Preferences, CoreError> {
        let merged = self.values.merged(patch)?;
        if let Some(store) = &self.store {
            store.save(&merged)?;
        }
        self.values = merged;
        Ok(&self.values)
    }

    /// Remembers the values gadget `name` just ran with and saves them.
    ///
    /// # Errors
    /// Returns [`CoreError::Io`] when the file cannot be written; the values
    /// are then not remembered.
    pub fn remember_gadget(&mut self, name: &str, values: &Value) -> Result<(), CoreError> {
        let mut next = self.values.clone();
        next.gadgets
            .last_values
            .insert(name.to_string(), values.clone());
        if let Some(store) = &self.store {
            store.save(&next)?;
        }
        self.values = next;
        Ok(())
    }

    /// Puts every preference back to its default and saves.
    ///
    /// # Errors
    /// Returns [`CoreError::Io`] when the file cannot be written.
    pub fn reset(&mut self) -> Result<&Preferences, CoreError> {
        let defaults = Preferences::default();
        if let Some(store) = &self.store {
            store.save(&defaults)?;
        }
        self.values = defaults;
        Ok(&self.values)
    }

    /// The reply of `prefs.get` and `prefs.set`: the preferences plus where
    /// they live and whether a broken file was set aside at start.
    fn reply(&self) -> Result<KernelReply, CoreError> {
        let mut value = serde_json::to_value(&self.values)?;
        if let Value::Object(fields) = &mut value {
            fields.insert(
                "path".to_string(),
                json!(self.store.as_ref().map(|s| s.path().display().to_string())),
            );
            fields.insert(
                "setAside".to_string(),
                json!(self.set_aside.as_ref().map(|p| p.display().to_string())),
            );
        }
        KernelReply::ok(&value)
    }
}

/// Whether `name` is a preferences command.
pub fn handles(name: &str) -> bool {
    matches!(name, "prefs.get" | "prefs.set" | "prefs.reset")
}

/// Runs a preferences command. `prefs.set` and `prefs.reset` apply the new
/// values to the open document straight away.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses, and the
/// errors of [`Prefs::set`] and [`Prefs::reset`].
pub fn dispatch(
    prefs: &mut Prefs,
    document: &mut Document,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "prefs.get" => {}
        "prefs.set" => {
            let trimmed = args.trim();
            let patch: Value = if trimmed.is_empty() || trimmed == "null" {
                Value::Object(Map::new())
            } else {
                serde_json::from_str(trimmed)
                    .map_err(|error| CoreError::invalid_args(name, error))?
            };
            prefs.set(&patch)?.apply_to_document(document);
        }
        "prefs.reset" => prefs.reset()?.apply_to_document(document),
        _ => return Err(CoreError::UnknownCommand(name.to_string())),
    }
    prefs.reply()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    fn result(reply: KernelReply) -> Value {
        let KernelReply::Json(text) = reply else {
            panic!("expected JSON");
        };
        let envelope: Value = serde_json::from_str(&text).expect("envelope");
        envelope["result"].clone()
    }

    #[test]
    fn defaults_validate_and_round_trip_through_json() {
        let prefs = Preferences::default();
        prefs.validate().expect("defaults are valid");
        let text = serde_json::to_string(&prefs).expect("encode");
        let back: Preferences = serde_json::from_str(&text).expect("decode");
        assert_eq!(back, prefs);
        assert!(text.contains("\"defaultUnits\":\"mm\""));
        assert!(text.contains("\"viewMode\":\"2d\""));
    }

    #[test]
    fn a_partial_file_fills_in_the_missing_fields() {
        let prefs: Preferences =
            serde_json::from_str(r#"{"general":{"undoLimit":50}}"#).expect("decode");
        assert_eq!(prefs.general.undo_limit, 50);
        assert!(prefs.general.show_tool_tips);
        assert!(!prefs.general.show_file_operations);
        assert!(!prefs.general.show_align_tools);
        assert!(prefs.general.show_guide);
        assert_eq!(prefs.view2d, View2dPrefs::default());
    }

    #[test]
    fn a_patch_merges_field_by_field() {
        let prefs = Preferences::default();
        let merged = prefs
            .merged(
                &json!({ "view2d": { "snap": { "grid": true } }, "general": { "undoLimit": 30 } }),
            )
            .expect("merge");
        assert!(merged.view2d.snap.grid);
        assert!(merged.view2d.snap.nodes);
        assert_eq!(merged.general.undo_limit, 30);
        assert_eq!(merged.view3d, prefs.view3d);
        let design = prefs
            .merged(&json!({ "general": { "showFileOperations": true, "showAlignTools": true } }))
            .expect("merge");
        assert!(design.general.show_file_operations && design.general.show_align_tools);
        let quiet = prefs
            .merged(&json!({ "general": { "showGuide": false } }))
            .expect("merge");
        assert!(!quiet.general.show_guide);
    }

    #[test]
    fn a_patch_with_an_unknown_key_is_refused() {
        let error = Preferences::default()
            .merged(&json!({ "general": { "undoLimt": 3 } }))
            .expect_err("typo");
        assert!(error.to_string().contains("general.undoLimt"));
        assert!(Preferences::default()
            .merged(&json!({ "nonsense": {} }))
            .is_err());
    }

    #[test]
    fn a_patch_with_the_wrong_type_or_range_is_refused() {
        let prefs = Preferences::default();
        assert!(prefs
            .merged(&json!({ "general": { "undoLimit": "ten" } }))
            .is_err());
        assert!(prefs
            .merged(&json!({ "general": { "undoLimit": 0 } }))
            .is_err());
        assert!(prefs
            .merged(&json!({ "general": { "undoLimit": MAX_UNDO_LIMIT + 1 } }))
            .is_err());
        assert!(prefs
            .merged(&json!({ "view3d": { "topColour": "red" } }))
            .is_err());
        assert!(prefs
            .merged(&json!({ "view3d": { "background": "stars" } }))
            .is_err());
        assert!(prefs
            .merged(&json!({ "toolpaths": { "tolerance": 0.0 } }))
            .is_err());
        assert!(prefs
            .merged(&json!({ "view2d": { "gridSpacing": -1.0 } }))
            .is_err());
        assert!(prefs.merged(&json!({ "general": 3 })).is_err());
        assert!(prefs.merged(&json!([1, 2])).is_err());
    }

    #[test]
    fn drawing_toggles_default_on_and_take_camel_case_names() {
        let old: Preferences =
            serde_json::from_str(r#"{"toolpaths":{"tolerance":0.05}}"#).expect("decode");
        assert!(old.toolpaths.draw_2d_previews && old.toolpaths.draw_joins);
        assert!(old.view3d.auto_wrap && old.view3d.color_shaded);
        assert!(!old.view3d.draw_origin && !old.view3d.shadow_shading);
        let merged = old
            .merged(&json!({
                "toolpaths": { "draw2dPreviews": false, "autoOpen3d": false, "drawRetracts": false },
                "view3d": { "lightFollows": true, "drawModelingPlane": true },
            }))
            .expect("merge");
        assert!(!merged.toolpaths.draw_2d_previews && !merged.toolpaths.auto_open_3d);
        assert!(!merged.toolpaths.draw_retracts);
        assert!(merged.view3d.light_follows && merged.view3d.draw_modeling_plane);
    }

    #[test]
    fn view_settings_default_for_an_older_file_and_merge_from_a_patch() {
        let old: Preferences =
            serde_json::from_str(r#"{"view3d":{"shading":"flat"},"window":{"viewMode":"3d"}}"#)
                .expect("decode");
        assert_eq!(old.view3d.shading, Shading::Flat);
        assert_eq!(old.view3d.projection, Projection::Perspective);
        assert_eq!(old.view3d.navigation, Navigation3d::Modern);
        assert_eq!(old.view3d.render_mode, RenderMode::Shaded);
        assert_eq!(old.view3d.material, Material3d::Theme);
        assert!(old.view3d.show_model && old.view3d.show_block && old.view3d.show_cube);
        assert!(!old.view3d.show_vectors);
        assert_eq!(old.window.tile, TileDirection::Vertical);

        let merged = old
            .merged(&json!({
                "view3d": {
                    "projection": "orthographic",
                    "navigation": "legacy",
                    "renderMode": "shadedWire",
                    "material": "walnut",
                    "showVectors": true,
                    "showToolpaths": false
                },
                "window": { "tile": "horizontal" }
            }))
            .expect("merge");
        assert_eq!(merged.view3d.projection, Projection::Orthographic);
        assert_eq!(merged.view3d.navigation, Navigation3d::Legacy);
        assert_eq!(merged.view3d.render_mode, RenderMode::ShadedWire);
        assert_eq!(merged.view3d.material, Material3d::Walnut);
        assert!(merged.view3d.show_vectors);
        assert!(!merged.view3d.show_toolpaths);
        assert_eq!(merged.view3d.shading, Shading::Flat);
        assert_eq!(merged.window.tile, TileDirection::Horizontal);
        assert!(old.window.left_pinned && old.window.toolpaths_pinned);
        assert!(!old.view2d.snap_distances);
        assert_eq!(old.view2d.grid_style, GridStyle::Points);
        assert_eq!(old.window.toolpath_list_height, 0);
        assert!(old
            .merged(&json!({ "window": { "componentTreeHeight": 5000 } }))
            .is_err());
        let tall = old
            .merged(&json!({ "window": { "toolpathListHeight": 420 } }))
            .expect("list height merge");
        assert_eq!(tall.window.toolpath_list_height, 420);
        assert!(old.general.show_usage_info);
        assert_eq!(old.window.top_ruler_colour, "#5b9bd5");
        assert!(old
            .merged(&json!({ "window": { "bottomRulerColour": "orange" } }))
            .is_err());
        let styled = old
            .merged(&json!({ "view2d": { "gridStyle": "lines" }, "general": { "showUsageInfo": false } }))
            .expect("grid style merge");
        assert_eq!(styled.view2d.grid_style, GridStyle::Lines);
        assert!(!styled.general.show_usage_info);
        assert_eq!(old.view2d.nudge_distance, 5.0);
        assert!(old
            .merged(&json!({ "view2d": { "nudgeDistance": 0 } }))
            .is_err());
        let nudged = old
            .merged(&json!({ "view2d": { "nudgeDistance": 2.5, "snapDistances": true } }))
            .expect("nudge merge");
        assert!(nudged.view2d.snap_distances);
        assert_eq!(nudged.view2d.nudge_distance, 2.5);
        assert_eq!((old.window.left_width, old.window.right_width), (268, 300));
        let docked = old
            .merged(&json!({ "window": { "toolpathsPinned": false, "rightWidth": 360 } }))
            .expect("dock merge");
        assert!(!docked.window.toolpaths_pinned);
        assert_eq!(docked.window.right_width, 360);
        assert!(old
            .merged(&json!({ "window": { "leftWidth": 90 } }))
            .is_err());

        assert!(old
            .merged(&json!({ "view3d": { "projection": "fisheye" } }))
            .is_err());
        assert!(old
            .merged(&json!({ "view3d": { "solidColour": "red" } }))
            .is_err());
        assert!(old
            .merged(&json!({ "window": { "tile": "diagonal" } }))
            .is_err());
        let text = serde_json::to_string(&merged).expect("encode");
        assert!(text.contains("\"renderMode\":\"shadedWire\""));
        assert!(text.contains("\"solidColour\":\"#c9b28f\""));
    }

    #[test]
    fn recent_fonts_lose_duplicates_and_keep_five() {
        let merged = Preferences::default()
            .merged(&json!({
                "general": { "recentFonts": ["A", "B", "A", "", "C", "D", "E", "F"] },
                "window": { "fontListWidth": 320, "fontListHeight": 600 }
            }))
            .expect("merges");
        assert_eq!(merged.general.recent_fonts, vec!["A", "B", "C", "D", "E"]);
        assert_eq!(merged.window.font_list_height, 600);
        assert!(Preferences::default()
            .merged(&json!({ "window": { "fontListHeight": 5000 } }))
            .is_err());
    }

    #[test]
    fn recent_files_lose_duplicates_and_keep_their_length() {
        let merged = Preferences::default()
            .merged(&json!({
                "general": {
                    "recentFilesLength": 2,
                    "recentFiles": ["/a.carve", "/b.carve", "/a.carve", "/c.carve"]
                }
            }))
            .expect("merge");
        assert_eq!(merged.general.recent_files, vec!["/a.carve", "/b.carve"]);
    }

    #[test]
    fn a_missing_file_gives_the_defaults_without_writing() {
        let temp = TempDir::new("prefs-missing");
        let store = PrefsStore::at(temp.path().join("prefs.json"));
        let loaded = store.load();
        assert_eq!(loaded.prefs, Preferences::default());
        assert_eq!(loaded.set_aside, None);
        assert!(!store.path().exists());
    }

    #[test]
    fn set_saves_and_a_new_load_reads_it_back() {
        let temp = TempDir::new("prefs-save");
        let store = PrefsStore::at(temp.path().join("nested").join("prefs.json"));
        let mut prefs = Prefs::load(store.clone());
        prefs
            .set(&json!({ "general": { "defaultUnits": "inch" }, "view3d": { "shading": "flat" } }))
            .expect("set");
        let again = Prefs::load(store);
        assert_eq!(again.values().general.default_units, Units::Inch);
        assert_eq!(again.values().view3d.shading, Shading::Flat);
    }

    #[test]
    fn a_broken_file_is_set_aside_and_defaults_are_used() {
        let temp = TempDir::new("prefs-broken");
        let path = temp.path().join("prefs.json");
        std::fs::write(&path, "{ not json").expect("write");
        let first = PrefsStore::at(&path).load();
        assert_eq!(first.prefs, Preferences::default());
        let aside = first.set_aside.expect("renamed");
        assert_eq!(aside, temp.path().join("prefs.json.broken"));
        assert_eq!(std::fs::read_to_string(&aside).expect("kept"), "{ not json");
        assert!(!path.exists());

        // A second broken file does not overwrite the first one.
        std::fs::write(&path, r#"{"general":{"undoLimit":-4}}"#).expect("write");
        let second = PrefsStore::at(&path).load();
        assert_eq!(
            second.set_aside,
            Some(temp.path().join("prefs.json.broken2"))
        );
        assert!(aside.exists());
    }

    #[test]
    fn a_file_with_values_out_of_range_is_set_aside() {
        let temp = TempDir::new("prefs-range");
        let path = temp.path().join("prefs.json");
        std::fs::write(&path, r#"{"view3d":{"topColour":"blue"}}"#).expect("write");
        let loaded = PrefsStore::at(&path).load();
        assert_eq!(loaded.prefs, Preferences::default());
        assert!(loaded.set_aside.is_some());
    }

    #[test]
    fn unknown_fields_in_a_file_are_ignored() {
        let temp = TempDir::new("prefs-unknown");
        let path = temp.path().join("prefs.json");
        std::fs::write(&path, r#"{"future":{"x":1},"general":{"undoLimit":12}}"#).expect("write");
        let loaded = PrefsStore::at(&path).load();
        assert_eq!(loaded.prefs.general.undo_limit, 12);
        assert_eq!(loaded.set_aside, None);
    }

    #[test]
    fn a_failed_set_changes_nothing() {
        let mut prefs = Prefs::in_memory();
        assert!(prefs
            .set(&json!({ "general": { "undoLimit": 0 } }))
            .is_err());
        assert_eq!(prefs.values(), &Preferences::default());
    }

    #[test]
    fn dispatch_applies_the_undo_limit_to_the_document() {
        let mut prefs = Prefs::in_memory();
        let mut document = Document::new();
        let reply = dispatch(
            &mut prefs,
            &mut document,
            "prefs.set",
            r#"{"general":{"undoLimit":3}}"#,
        )
        .expect("set");
        assert_eq!(result(reply)["general"]["undoLimit"], 3);
        assert_eq!(document.journal().limit(), 3);

        let reply = dispatch(&mut prefs, &mut document, "prefs.reset", "").expect("reset");
        assert_eq!(
            result(reply)["general"]["undoLimit"],
            crate::doc::UNDO_LIMIT
        );
        assert_eq!(document.journal().limit(), crate::doc::UNDO_LIMIT);

        let got = result(dispatch(&mut prefs, &mut document, "prefs.get", "").expect("get"));
        assert!(got["path"].is_null());
        assert!(got["setAside"].is_null());
        assert!(dispatch(&mut prefs, &mut document, "prefs.nope", "").is_err());
    }

    #[test]
    fn a_new_document_takes_the_default_units() {
        let mut prefs = Preferences::default();
        prefs.general.default_units = Units::Inch;
        let mut document = Document::new();
        prefs.apply_to_new_document(&mut document);
        assert_eq!(document.job().units, Units::Inch);
        assert!((document.job().width - 304.8).abs() < 1e-9);
        assert!(!document.journal().can_undo());
    }

    #[test]
    fn the_kernel_answers_prefs_commands_and_applies_them() {
        let mut kernel = crate::Kernel::new();
        let set = |kernel: &mut crate::Kernel, args: &str| match kernel.call("prefs.set", args) {
            KernelReply::Json(text) => serde_json::from_str::<Value>(&text).expect("json"),
            KernelReply::Bytes(_) => panic!("expected JSON"),
        };
        let reply = set(
            &mut kernel,
            r#"{"general":{"undoLimit":2,"defaultUnits":"inch"}}"#,
        );
        assert_eq!(reply["ok"], true);
        assert_eq!(kernel.document().journal().limit(), 2);

        kernel.call("doc.new", "");
        assert_eq!(kernel.document().job().units, Units::Inch);
        assert_eq!(kernel.document().journal().limit(), 2);

        let refused = set(&mut kernel, r#"{"general":{"undoLimit":0}}"#);
        assert_eq!(refused["ok"], false);
        assert_eq!(refused["error"]["code"], "validation");
        assert_eq!(kernel.prefs().values().general.undo_limit, 2);
    }

    #[test]
    fn window_tab_accepts_the_layers_tab_and_older_toolpaths_layouts() {
        let prefs = Preferences::default();
        let layers = prefs
            .merged(&json!({ "window": { "activeTab": "layers" } }))
            .expect("layers tab");
        assert_eq!(layers.window.active_tab, WindowTab::Layers);
        let old = prefs
            .merged(&json!({ "window": { "activeTab": "toolpaths" } }))
            .expect("older layout");
        assert_eq!(old.window.active_tab, WindowTab::Toolpaths);
        let components = prefs
            .merged(&json!({ "window": { "activeTab": "components" } }))
            .expect("components tab");
        assert_eq!(components.window.active_tab, WindowTab::Components);
        assert_eq!(Preferences::default().window.active_tab, WindowTab::Design);
    }

    #[test]
    fn toolpath_preferences_reach_the_calculation() {
        let mut prefs = Preferences::default();
        prefs.toolpaths.tolerance = 0.05;
        prefs.toolpaths.tolerance_3d = 0.02;
        prefs.toolpaths.drop_tool = true;
        prefs.toolpaths.new_raster = false;
        prefs.toolpaths.geometry_fix_seconds = 30;
        prefs.toolpaths.check_all_depths = true;
        prefs.toolpaths.append_duplicated = true;
        prefs.toolpaths.append_to_selected_group = false;
        let mut document = Document::new();
        prefs.apply_to_document(&mut document);
        let settings = document.cam_settings();
        assert_eq!(settings.tolerance_2d, 0.05);
        assert_eq!(settings.tolerance_3d, 0.02);
        assert!(settings.drop_tool);
        assert!(!settings.new_raster);
        assert_eq!(settings.geometry_fix_seconds, 30);
        assert!(settings.check_all_depths);
        assert!(settings.append_duplicated);
        assert!(!settings.append_to_selected_group);
        assert!(prefs
            .merged(&json!({ "toolpaths": { "geometryFixSeconds": MAX_GEOMETRY_FIX_SECONDS + 1 } }))
            .is_err());
        assert!(prefs
            .merged(&json!({ "toolpaths": { "solidPreviewColour": "blue" } }))
            .is_err());
        let shown = prefs
            .merged(&json!({ "toolpaths": {
                "solidPreviewColour": "#112233", "showOperationsWithPreview": true } }))
            .expect("merge");
        assert_eq!(shown.toolpaths.solid_preview_colour, "#112233");
        assert!(shown.toolpaths.show_operations_with_preview);
        prefs.confirmations.toolpath_intersections = IntersectionCheck::Proceed;
        assert!(
            prefs.validate().is_err(),
            "proceed is for sharp offsets only"
        );
        let old: Preferences =
            serde_json::from_str(r#"{"confirmations":{"intersectionChecks":false}}"#).expect("old");
        assert_eq!(
            old.confirmations.toolpath_intersections,
            IntersectionCheck::Ask
        );
    }

    #[test]
    fn sculpt_presets_are_kept_and_replaced_whole() {
        let prefs = Preferences::default();
        assert!(prefs.modeling.sculpt_presets.is_empty());
        let merged = prefs
            .merged(&json!({ "modeling": { "sculptPresets": [
                { "name": "Fine", "tool": "smooth", "settings": { "diameter": 2.0 } }
            ] } }))
            .expect("merge");
        assert_eq!(merged.modeling.sculpt_presets.len(), 1);
        assert_eq!(merged.modeling.sculpt_presets[0].tool, "smooth");
        let cleared = merged
            .merged(&json!({ "modeling": { "sculptPresets": [] } }))
            .expect("merge");
        assert!(cleared.modeling.sculpt_presets.is_empty());
        let old: Preferences = serde_json::from_str(r#"{"version":1}"#).expect("an older file");
        assert!(old.modeling.sculpt_presets.is_empty());
    }
}
