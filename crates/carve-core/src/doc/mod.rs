//! Document: job setup, layers, sheets, entities, selection and undo.
//!
//! The document is the kernel's single source of truth. Every mutating
//! command opens an undo step, records what it changed and commits, which
//! bumps the document revision. The frontend mirrors only ids, summaries and
//! flattened display geometry, and refreshes whenever the revision moves.
//!
//! Lengths are millimetres. Ids are u64 from per-document counters and are
//! never reused, so a stale id from the frontend fails cleanly instead of
//! landing on a different object.

mod components;
mod distortion;
pub use components::{HIGHLIGHT_HIDDEN, HIGHLIGHT_SEEN};
mod entity;
mod job;
mod layer;
mod sheets;
mod sides;
mod toolpath_groups;
mod toolpaths;
mod undo;
pub mod variables;

pub mod bitmap_cmd;
pub mod bitmap_relief_cmd;
pub mod clipboard_cmd;
pub mod display;
pub mod distort_cmd;
pub mod document_cmd;
pub mod drawtools_cmd;
pub mod entities_cmd;
pub mod images;
pub mod layers_cmd;
pub mod nest_cmd;
pub mod node_join;
pub mod ops;
pub mod ops_cmd;
pub mod picture_cmd;
pub mod plate_cmd;
pub mod probe;
pub mod rotary_cmd;
pub mod sides_cmd;
pub mod smart_select_cmd;
pub mod text_adjust;
pub mod text_cmd;
pub mod texture_cmd;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path as FsPath, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cam::toolpath::Toolpath;
use crate::error::CoreError;
use crate::geom::{BBox, Mat3, Path, Point};
use crate::model::{Component, Level};

/// The bytes of a `#rrggbb` colour, for job appearance colours.
pub fn job_colour(text: &str) -> Option<[u8; 3]> {
    job::hex_colour_bytes(text)
}

pub use distortion::{DistortMode, Distortion};
pub use entity::{
    merge, rotated_frame, tolerance_or_default, BitmapRef, DimensionEntity, DimensionSpec, Entity,
    EntityKind, TextAnchor, TextEntity, TextSpecJson, DEFAULT_SHEET, DISPLAY_TOLERANCE,
};
pub use job::{
    DoubleSidedSetup, FlipAxis, JobKind, JobSetup, MachineSetup, ModelResolution, RotaryAxis,
    RotaryBlank, RotarySetup, RotaryZZero, Side, Units, XyOrigin, ZZero,
};
pub use layer::{is_hex_colour, Layer, Sheet, LAYER_COLOURS};
pub use sheets::{SheetEditArgs, SheetMaterial};
pub use sides::{SideFile, SideState};
pub use toolpath_groups::ToolpathGroup;
pub use undo::{Change, Journal, Step, UNDO_LIMIT};

/// Name of the layer every new document starts with.
pub const FIRST_LAYER: &str = "Layer 1";
/// Name of the sheet every new document starts with.
pub const FIRST_SHEET: &str = "Sheet 1";

/// The open drawing. One document is loaded at a time.
#[derive(Debug, Clone)]
pub struct Document {
    /// How machining times are estimated, from the active machine and the
    /// preferences. Session state, not saved.
    time_estimate: crate::cam::TimeEstimate,
    /// Toolpath tolerances and options from the preferences. Session state,
    /// not saved.
    cam_settings: crate::cam::CamSettings,
    job: JobSetup,
    revision: u64,
    path: Option<PathBuf>,
    dirty: bool,
    layers: Vec<Layer>,
    sheets: Vec<Sheet>,
    entities: BTreeMap<u64, Entity>,
    order: Vec<u64>,
    selection: Vec<u64>,
    active_layer: u64,
    active_sheet: u64,
    notes: String,
    variables: BTreeMap<String, f64>,
    /// The formulas of the variables defined by one, as typed.
    variable_formulas: BTreeMap<String, String>,
    /// Distortions whose envelope can still be edited.
    distortions: Vec<Distortion>,
    /// Nest Parts marks kept with the objects: copies and filler, by
    /// entity id.
    nest_marks: BTreeMap<u64, nest_cmd::NestMark>,
    /// The copies the command running now made, original id to copy id,
    /// so links between copied objects can follow the copies. Session
    /// state, cleared before every command.
    copies_made: BTreeMap<u64, u64>,
    /// Toolpath tiling: on or off, the layout and the tile the views show.
    tiling: crate::cam::tiling::Tiling,
    /// How new dimensions look; saved with the job.
    dimension_style: crate::geom::DimensionStyle,
    /// Guide lines of the 2D view; saved with the job.
    guides: Vec<GuideLine>,
    /// Parts, instances and mates of the 3D assembly.
    assembly: crate::assembly::Assembly,
    next_entity_id: u64,
    next_layer_id: u64,
    next_sheet_id: u64,
    toolpaths: BTreeMap<u64, Toolpath>,
    toolpath_order: Vec<u64>,
    next_toolpath_id: u64,
    toolpath_groups: Vec<ToolpathGroup>,
    components: BTreeMap<u64, Component>,
    component_order: Vec<u64>,
    next_component_id: u64,
    levels: Vec<Level>,
    active_level: u64,
    next_level_id: u64,
    /// Moves whenever a component or their order changes, undo included, so
    /// the view can tell when the composite needs fetching again.
    model_stamp: u64,
    journal: Journal,
    /// The side whose content is in the fields above.
    active_side: Side,
    /// The other side's content, `None` until a double sided job first
    /// visits it.
    other_side: Option<Box<SideState>>,
    /// `job` as the active side sees it, which is what [`Document::job`]
    /// answers so that every calculation uses the active side's Z zero.
    side_job: JobSetup,
    /// Pictures the bitmap entities place, shared by both sides.
    images: images::ImageStore,
}

impl Default for Document {
    fn default() -> Self {
        Document::with_revision(0)
    }
}

impl PartialEq for Document {
    /// Two documents are equal when their content is, which is what the save
    /// and open round trip test compares. The file path, the dirty flag and
    /// the undo journal are session state, not content.
    fn eq(&self, other: &Self) -> bool {
        self.job == other.job
            && self.layers == other.layers
            && self.sheets == other.sheets
            && self.entities == other.entities
            && self.order == other.order
            && self.selection == other.selection
            && self.active_layer == other.active_layer
            && self.active_sheet == other.active_sheet
            && self.notes == other.notes
            && self.assembly == other.assembly
            && self.variables == other.variables
            && self.variable_formulas == other.variable_formulas
            && self.distortions == other.distortions
            && self.nest_marks == other.nest_marks
            && self.tiling == other.tiling
            && self.dimension_style == other.dimension_style
            && self.guides == other.guides
            && self.next_entity_id == other.next_entity_id
            && self.next_layer_id == other.next_layer_id
            && self.next_sheet_id == other.next_sheet_id
            && self.toolpaths == other.toolpaths
            && self.toolpath_order == other.toolpath_order
            && self.toolpath_groups == other.toolpath_groups
            && self.components == other.components
            && self.component_order == other.component_order
            && self.levels == other.levels
            && self.active_level == other.active_level
            && self.active_side == other.active_side
            && self.other_side == other.other_side
    }
}

/// Which way a guide line runs: `X` stands at an X position (vertical),
/// `Y` at a Y position (horizontal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GuideAxis {
    /// A vertical guide at an X position.
    X,
    /// A horizontal guide at a Y position.
    Y,
}

/// One guide line of the 2D view.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideLine {
    /// Which way it runs.
    pub axis: GuideAxis,
    /// Its X or Y position, job millimetres.
    pub position: f64,
    /// A locked guide cannot be dragged.
    #[serde(default)]
    pub locked: bool,
}

/// The document as it is written to `document.json` inside a `.carve` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentFile {
    /// Format version. Bumped when the shape changes incompatibly.
    pub format: u32,
    /// Job setup.
    pub job: JobSetup,
    /// Layers in drawing order.
    pub layers: Vec<Layer>,
    /// Sheets in tab order.
    pub sheets: Vec<Sheet>,
    /// Entities, including group members.
    pub entities: Vec<Entity>,
    /// Top level z-order, back to front.
    pub order: Vec<u64>,
    /// Active layer id.
    pub active_layer: u64,
    /// Active sheet id.
    pub active_sheet: u64,
    /// Project notes.
    #[serde(default)]
    pub notes: String,
    /// The 3D assembly of parts cut from the sheets.
    #[serde(default, skip_serializing_if = "crate::assembly::Assembly::is_empty")]
    pub assembly: crate::assembly::Assembly,
    /// Document variables: named values numeric fields can use.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, f64>,
    /// The formulas of the variables defined by one (`{w}/2+1`), worked
    /// out again when what they read changes. Older files have none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variable_formulas: BTreeMap<String, String>,
    /// Distortions not yet baked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub distortions: Vec<Distortion>,
    /// Nest Parts copies and filler marks, by entity id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nest_marks: BTreeMap<u64, nest_cmd::NestMark>,
    /// Toolpath tiling; absent when it was never set up.
    #[serde(
        default,
        skip_serializing_if = "crate::cam::tiling::Tiling::is_default"
    )]
    pub tiling: crate::cam::tiling::Tiling,
    /// Dimension settings; absent while they are the defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension_style: Option<crate::geom::DimensionStyle>,
    /// Guide lines of the 2D view.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<GuideLine>,
    /// Next entity id, so ids are never reused across a save and open.
    pub next_entity_id: u64,
    /// Next layer id.
    pub next_layer_id: u64,
    /// Next sheet id.
    pub next_sheet_id: u64,
    /// Calculated toolpaths in list order.
    #[serde(default)]
    pub toolpaths: Vec<Toolpath>,
    /// Next toolpath id.
    #[serde(default)]
    pub next_toolpath_id: u64,
    /// Folders in the toolpath list; absent in older files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub toolpath_groups: Vec<ToolpathGroup>,
    /// Relief model components, bottom first. Their heights travel beside
    /// `document.json` in the `.carve` file.
    #[serde(default)]
    pub components: Vec<Component>,
    /// Next component id.
    #[serde(default)]
    pub next_component_id: u64,
    /// Model levels, bottom first; absent in older files.
    #[serde(default)]
    pub levels: Vec<Level>,
    /// The level new components go on.
    #[serde(default)]
    pub active_level: u64,
    /// Next level id.
    #[serde(default)]
    pub next_level_id: u64,
    /// The side that was active when the file was saved; format 2.
    #[serde(default)]
    pub active_side: Side,
    /// The bottom side's content on a double sided job; the top level
    /// fields above are the top side's. Format 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<SideFile>,
}

/// Version written into `document.json`. Format 2 added the two sides of a
/// double sided job; a format 1 file reads as a job whose content is all on
/// the top side.
pub const FORMAT_VERSION: u32 = 2;

/// How an entity is summarised for the object list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntitySummary {
    /// Entity id.
    pub id: u64,
    /// `path`, `text`, `bitmap`, `dimension` or `group`.
    pub kind: String,
    /// Layer the entity draws on.
    pub layer: u64,
    /// `[minX, minY, maxX, maxY]` in millimetres.
    pub bbox: [f64; 4],
    /// True when every subpath is closed.
    pub closed: bool,
    /// Entity revision.
    pub revision: u64,
    /// Name, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Members, when the entity is a group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<u64>>,
    /// The vector a Text On A Curve text follows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_curve: Option<u64>,
    /// Its Nest Parts copies and filler mark, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nest: Option<nest_cmd::NestMark>,
}

/// What `selection.*` answers with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionState {
    /// Selected ids, in selection order.
    pub ids: Vec<u64>,
    /// Box round the selection, or `null` when nothing is selected.
    pub bbox: Option<[f64; 4]>,
    /// The rotated frame the selection's bounds follow, absent when the
    /// selection is not rotated as a whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<SelectionFrame>,
}

/// A rotated selection's bounds: the box lies in the frame turned by
/// `angle` degrees about the job origin, so a job point p sits at
/// `rotate(-angle) * p` in frame space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionFrame {
    /// Frame rotation in degrees, counter clockwise.
    pub angle: f64,
    /// `[minX, minY, maxX, maxY]` of the selection in frame space, mm.
    pub extent: [f64; 4],
}

/// Turns a box into the `[minX, minY, maxX, maxY]` array the API uses.
pub fn bbox_array(bbox: BBox) -> [f64; 4] {
    [bbox.min.x, bbox.min.y, bbox.max.x, bbox.max.y]
}

/// The empty box used for entities that have no geometry of their own.
pub const EMPTY_BBOX: [f64; 4] = [0.0, 0.0, 0.0, 0.0];

impl Document {
    /// Creates an empty document with the default job setup, at revision 0.
    pub fn new() -> Self {
        Document::with_revision(0)
    }

    /// Creates an empty document that carries on from an earlier revision, so
    /// that revisions stay monotonic across `doc.new`.
    pub fn with_revision(revision: u64) -> Self {
        Document {
            time_estimate: crate::cam::TimeEstimate::default(),
            cam_settings: crate::cam::CamSettings::default(),
            job: JobSetup::default(),
            revision,
            path: None,
            dirty: false,
            layers: vec![Layer::new(1, FIRST_LAYER, Layer::colour_for(0))],
            sheets: vec![Sheet::new(1, FIRST_SHEET)],
            entities: BTreeMap::new(),
            order: Vec::new(),
            selection: Vec::new(),
            active_layer: 1,
            active_sheet: 1,
            notes: String::new(),
            assembly: crate::assembly::Assembly::default(),
            variables: BTreeMap::new(),
            variable_formulas: BTreeMap::new(),
            distortions: Vec::new(),
            nest_marks: BTreeMap::new(),
            copies_made: BTreeMap::new(),
            tiling: crate::cam::tiling::Tiling::default(),
            dimension_style: crate::geom::DimensionStyle::default(),
            guides: Vec::new(),
            next_entity_id: 1,
            next_layer_id: 2,
            next_sheet_id: 2,
            toolpaths: BTreeMap::new(),
            toolpath_order: Vec::new(),
            toolpath_groups: Vec::new(),
            next_toolpath_id: 1,
            components: BTreeMap::new(),
            component_order: Vec::new(),
            next_component_id: 1,
            levels: vec![Level::first()],
            active_level: 1,
            next_level_id: 2,
            model_stamp: 0,
            journal: Journal::new(),
            active_side: Side::Top,
            other_side: None,
            side_job: JobSetup::default(),
            images: images::ImageStore::default(),
        }
    }

    // ---------------------------------------------------------------- state

    /// Revision of the document. Bumped by every committed change.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Bumps the revision without recording a change, which read-modify
    /// commands such as `selection.set` use.
    pub fn bump(&mut self) -> u64 {
        self.revision += 1;
        self.revision
    }

    /// The job as the active side sees it: the stored setup, with the
    /// bottom side's Z zero while a double sided job's bottom side is
    /// active. Toolpaths, the model and the preview all use this.
    #[allow(clippy::misnamed_getters)]
    pub fn job(&self) -> &JobSetup {
        &self.side_job
    }

    /// The job setup as the user entered it, for Job Setup and saving.
    pub fn job_setup(&self) -> &JobSetup {
        &self.job
    }

    /// The file the document was last saved to or opened from.
    pub fn file_path(&self) -> Option<&FsPath> {
        self.path.as_deref()
    }

    /// Records the file the document now lives at and marks it clean.
    pub fn mark_saved(&mut self, path: PathBuf) {
        self.path = Some(path);
        self.dirty = false;
    }

    /// Records the file a restored backup stands for (none for a job never
    /// saved) and marks the document changed, since its content is not in
    /// that file.
    pub fn mark_restored(&mut self, path: Option<PathBuf>) {
        self.path = path;
        self.dirty = true;
    }

    /// Whether the document has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The project notes.
    pub fn notes(&self) -> &str {
        &self.notes
    }

    /// The 3D assembly.
    pub fn assembly(&self) -> &crate::assembly::Assembly {
        &self.assembly
    }

    /// Replaces the 3D assembly as part of the open undo step.
    pub fn set_assembly(&mut self, assembly: crate::assembly::Assembly) {
        let before = std::mem::replace(&mut self.assembly, assembly);
        self.journal.record(Change::Assembly(Box::new(before)));
    }

    /// Replaces the 3D assembly without an undo step, for state that moves
    /// continuously (a door dragged open). Marks the document changed.
    pub fn set_assembly_untracked(&mut self, assembly: crate::assembly::Assembly) {
        self.assembly = assembly;
        self.dirty = true;
    }

    /// Replaces the project notes.
    pub fn set_notes(&mut self, notes: String) {
        self.journal.record(Change::Notes(self.notes.clone()));
        self.notes = notes;
    }

    /// How new dimensions look (Draw Dimensions settings), saved with the job.
    pub fn dimension_style(&self) -> &crate::geom::DimensionStyle {
        &self.dimension_style
    }

    /// Replaces the dimension settings. Not an undo step, like the other
    /// form settings, but the job counts as changed.
    pub fn set_dimension_style(&mut self, style: crate::geom::DimensionStyle) {
        if style != self.dimension_style {
            self.dimension_style = style;
            self.dirty = true;
        }
    }

    /// The guide lines of the 2D view, saved with the job.
    pub fn guides(&self) -> &[GuideLine] {
        &self.guides
    }

    /// Replaces the guide lines. Not an undo step (dragging a guide is not
    /// one either), but the job counts as changed.
    pub fn set_guides(&mut self, guides: Vec<GuideLine>) {
        if guides != self.guides {
            self.guides = guides;
            self.dirty = true;
        }
    }

    /// The toolpath tiling settings.
    pub fn tiling(&self) -> &crate::cam::tiling::Tiling {
        &self.tiling
    }

    /// Replaces the toolpath tiling settings.
    pub fn set_tiling(&mut self, tiling: crate::cam::tiling::Tiling) {
        if self.journal.is_recording() {
            self.journal.record(Change::Tiling(Box::new(self.tiling)));
        }
        self.tiling = tiling;
    }

    /// Notes that `copy` was made as a copy of `original` by the command
    /// running now.
    pub fn note_copy(&mut self, original: u64, copy: u64) {
        self.copies_made.insert(original, copy);
    }

    /// Takes the copies noted since the last call.
    pub fn take_copies(&mut self) -> BTreeMap<u64, u64> {
        std::mem::take(&mut self.copies_made)
    }

    /// The distortions whose envelopes can still be edited.
    pub fn distortions(&self) -> &[Distortion] {
        &self.distortions
    }

    /// Replaces the distortion list as part of the open undo step.
    pub fn set_distortions(&mut self, distortions: Vec<Distortion>) {
        if self.journal.is_recording() {
            self.journal
                .record(Change::Distortions(self.distortions.clone()));
        }
        self.distortions = distortions;
    }

    /// The Nest Parts marks of the objects: copies and filler, by entity
    /// id. An object left out nests once and is no filler.
    pub fn nest_marks(&self) -> &BTreeMap<u64, nest_cmd::NestMark> {
        &self.nest_marks
    }

    /// Replaces the Nest Parts marks as part of the open undo step.
    pub fn set_nest_marks(&mut self, marks: BTreeMap<u64, nest_cmd::NestMark>) {
        if self.journal.is_recording() {
            self.journal
                .record(Change::NestMarks(self.nest_marks.clone()));
        }
        self.nest_marks = marks;
    }

    /// The document variables, by name.
    pub fn variables(&self) -> &BTreeMap<String, f64> {
        &self.variables
    }

    /// The formulas of the variables defined by one, by name.
    pub fn variable_formulas(&self) -> &BTreeMap<String, String> {
        &self.variable_formulas
    }

    /// Every variable's definition: its formula, or its value.
    pub fn variable_definitions(&self) -> BTreeMap<String, variables::Definition> {
        self.variables
            .iter()
            .map(|(name, value)| {
                let definition = match self.variable_formulas.get(name) {
                    Some(formula) => variables::Definition::Formula(formula.clone()),
                    None => variables::Definition::Number(*value),
                };
                (name.clone(), definition)
            })
            .collect()
    }

    /// Replaces the document variables with plain values. Names are
    /// letters, digits and underscores starting with a letter; values must
    /// be finite.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for a bad name or value; nothing changes.
    pub fn set_variables(&mut self, variables: BTreeMap<String, f64>) -> Result<(), CoreError> {
        self.set_variable_definitions(
            variables
                .into_iter()
                .map(|(name, value)| (name, variables::Definition::Number(value)))
                .collect(),
        )
    }

    /// Replaces the document variables: values, or formulas that read the
    /// other variables and the material (see [`variables::resolve`]); the
    /// formulas are kept and worked out again when what they read changes.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for a bad name or value, a formula that
    /// does not read, uses a name no variable has or loops; nothing changes.
    pub fn set_variable_definitions(
        &mut self,
        definitions: BTreeMap<String, variables::Definition>,
    ) -> Result<(), CoreError> {
        let resolved = variables::resolve(&definitions, &variables::Constants::of(&self.job))?;
        self.journal.record(Change::Variables {
            values: self.variables.clone(),
            formulas: self.variable_formulas.clone(),
        });
        self.variables = resolved.values;
        self.variable_formulas = resolved.formulas;
        Ok(())
    }

    /// Works the formulas out again after the job changed, as part of the
    /// open undo step; a formula that no longer works out keeps its value.
    fn refresh_variables(&mut self) {
        if self.variable_formulas.is_empty() {
            return;
        }
        let Ok(resolved) = variables::resolve(
            &self.variable_definitions(),
            &variables::Constants::of(&self.job),
        ) else {
            return;
        };
        if resolved.values != self.variables {
            self.journal.record(Change::Variables {
                values: self.variables.clone(),
                formulas: self.variable_formulas.clone(),
            });
            self.variables = resolved.values;
        }
    }

    /// Replaces the job setup after validating it. A rotary job takes its
    /// flat size, thickness and Z zero from its blank (see
    /// [`JobSetup::normalised`]).
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] if the job is not machinable, in
    /// which case the document is left untouched.
    pub fn set_job(&mut self, job: JobSetup) -> Result<(), CoreError> {
        let job = job.normalised();
        job.validate()?;
        self.journal.record(Change::Job(Box::new(self.job)));
        self.job = job;
        // Formulas reading the material size follow it.
        self.refresh_variables();
        // The model grid follows the job, so the model reads differently.
        self.model_stamp += 1;
        if !job.is_double_sided() && self.active_side == Side::Bottom {
            self.set_active_side(Side::Top)?;
        }
        self.sync_job();
        Ok(())
    }

    /// Where the job datum sits inside the material, in millimetres from the
    /// lower left corner of the sheet.
    pub fn job_datum(&self) -> Point {
        let (x, y) = self.job.datum();
        Point::new(x, y)
    }

    /// The material rectangle in job space, in millimetres.
    pub fn job_bounds(&self) -> BBox {
        self.job.material_bounds()
    }

    // -------------------------------------------------------------- journal

    /// Opens an undo step. Pair with [`Document::commit`].
    pub fn begin(&mut self, label: &str) {
        self.journal.begin(label);
    }

    /// Closes the open step and, when it changed anything, bumps the revision
    /// and marks the document dirty. Answers the new revision.
    pub fn commit(&mut self) -> u64 {
        self.journal.set_side(self.active_side);
        if self.journal.commit() {
            self.revision += 1;
            self.dirty = true;
        }
        self.revision
    }

    /// Opens the newest undo step again so that changes which follow from it
    /// (text laid out again along a curve that moved) undo with it. Close it
    /// with [`Document::close_reopened`]. Answers false when there is no step.
    pub fn reopen_newest_step(&mut self) -> bool {
        self.journal.reopen()
    }

    /// Closes a step opened by [`Document::reopen_newest_step`] without
    /// moving the revision: the command that made the step already did.
    pub fn close_reopened(&mut self) {
        self.journal.set_side(self.active_side);
        self.journal.commit();
    }

    /// How many undo steps have been committed so far: a command made one
    /// when this moved.
    pub fn steps_committed(&self) -> u64 {
        self.journal.committed()
    }

    /// Entity `id` as it was before the newest undo step changed it (see
    /// [`Journal::newest_before`]).
    pub fn newest_step_before(&self, id: u64) -> Option<Option<&Entity>> {
        self.journal.newest_before(id)
    }

    /// True when the newest undo step changed entity `id`.
    pub fn newest_step_touches(&self, id: u64) -> bool {
        self.journal.newest_touches(id)
    }

    /// Puts the document back the way it was before the open step and throws
    /// the step away, which is what a command that fails half way does.
    pub fn rollback(&mut self) {
        if let Some(step) = self.journal.abort() {
            for change in step.changes.into_iter().rev() {
                self.revert(change);
            }
        }
    }

    /// The journal, for `undo.info`.
    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Sets how many steps Undo keeps, dropping the oldest beyond it.
    pub fn set_undo_limit(&mut self, limit: usize) {
        self.journal.set_limit(limit);
    }

    /// Opens an undo group: the commands run until
    /// [`Document::end_undo_group`] undo as one step, called `label` or
    /// after the first of them.
    pub fn begin_undo_group(&mut self, label: Option<String>) {
        self.journal.begin_group(label);
    }

    /// Closes the open undo group. Answers whether its steps were merged.
    pub fn end_undo_group(&mut self) -> bool {
        self.journal.end_group()
    }

    /// Merges every step pushed since the undo stack was `depth` deep into
    /// one step called `label`, so that a scripted run undoes in one go.
    pub fn squash_steps_since(&mut self, depth: usize, label: &str) {
        let mut steps = Vec::new();
        while self.journal.undo_depth() > depth {
            match self.journal.pop_undo() {
                Some(step) => steps.push(step),
                None => break,
            }
        }
        if steps.is_empty() {
            return;
        }
        let mut merged = Step::new(label);
        for step in steps.into_iter().rev() {
            merged.changes.extend(step.changes);
            merged.side = step.side.or(merged.side);
        }
        self.journal.push_undo(merged);
    }

    /// Reverts and drops every step pushed since the undo stack was `depth`
    /// deep, which is how a scripted run that fails half way is taken back.
    pub fn revert_steps_since(&mut self, depth: usize) {
        let mut reverted = false;
        while self.journal.undo_depth() > depth {
            let Some(step) = self.journal.pop_undo() else {
                break;
            };
            self.turn_to(step.side);
            for change in step.changes.into_iter().rev() {
                self.revert(change);
            }
            reverted = true;
        }
        if reverted {
            self.revision += 1;
        }
    }

    /// Undoes the newest step. Answers its label, or `None` when there was
    /// nothing to undo.
    pub fn undo(&mut self) -> Option<String> {
        let step = self.journal.pop_undo()?;
        let label = step.label.clone();
        // The step's changes are on the side it ended on: show that side.
        self.turn_to(step.side);
        let inverse: Vec<Change> = step
            .changes
            .into_iter()
            .rev()
            .map(|change| self.revert(change))
            .collect();
        self.journal.push_redo(Step {
            label: label.clone(),
            changes: inverse.into_iter().rev().collect(),
            side: Some(self.active_side),
        });
        self.drop_missing_from_selection();
        self.revision += 1;
        self.dirty = true;
        Some(label)
    }

    /// Undoes the newest step and forgets it, so Redo cannot bring it back:
    /// what a form does when it takes back a preview it never applied.
    /// Answers its label, or `None` when there was nothing to undo.
    pub fn discard_last(&mut self) -> Option<String> {
        let label = self.undo()?;
        self.journal.pop_redo();
        Some(label)
    }

    /// Redoes the newest undone step. Answers its label.
    pub fn redo(&mut self) -> Option<String> {
        let step = self.journal.pop_redo()?;
        let label = step.label.clone();
        self.turn_to(step.side);
        // The redo step holds the state after each change in the order the
        // changes were made, so it replays front to back (a step that
        // changed one entity twice must end on the second state), and the
        // states it replaces are the new undo step in that same order.
        let inverse: Vec<Change> = step
            .changes
            .into_iter()
            .map(|change| self.revert(change))
            .collect();
        self.journal.push_undo(Step {
            label: label.clone(),
            changes: inverse,
            side: Some(self.active_side),
        });
        self.drop_missing_from_selection();
        self.revision += 1;
        self.dirty = true;
        Some(label)
    }

    /// Takes out of the selection the objects an undo or redo took away
    /// (a selection made after drawing a vector names it; undoing the
    /// drawing must not leave the selection naming nothing). Outside the
    /// journal: the step's own selection changes are already reverted.
    fn drop_missing_from_selection(&mut self) {
        let entities = &self.entities;
        self.selection.retain(|id| entities.contains_key(id));
    }

    /// Applies one change and answers the change that would undo it.
    fn revert(&mut self, change: Change) -> Change {
        match change {
            Change::Entity { id, before } => {
                let after = self.entities.get(&id).cloned().map(Box::new);
                match before {
                    Some(entity) => {
                        self.entities.insert(id, *entity);
                    }
                    None => {
                        self.entities.remove(&id);
                    }
                }
                Change::Entity { id, before: after }
            }
            Change::Order(order) => {
                let after = std::mem::replace(&mut self.order, order);
                Change::Order(after)
            }
            Change::Layers(layers, active) => {
                let after = std::mem::replace(&mut self.layers, layers);
                let active_after = std::mem::replace(&mut self.active_layer, active);
                Change::Layers(after, active_after)
            }
            Change::Sheets(sheets, active) => {
                let after = std::mem::replace(&mut self.sheets, sheets);
                let active_after = std::mem::replace(&mut self.active_sheet, active);
                // Components belong to a sheet, so the model shows another.
                self.model_stamp += 1;
                Change::Sheets(after, active_after)
            }
            Change::Selection(selection) => {
                let after = std::mem::replace(&mut self.selection, selection);
                Change::Selection(after)
            }
            Change::Job(job) => {
                let after = std::mem::replace(&mut self.job, *job);
                self.model_stamp += 1;
                self.sync_job();
                Change::Job(Box::new(after))
            }
            Change::Assembly(assembly) => {
                let after = std::mem::replace(&mut self.assembly, *assembly);
                Change::Assembly(Box::new(after))
            }
            Change::Notes(notes) => {
                let after = std::mem::replace(&mut self.notes, notes);
                Change::Notes(after)
            }
            Change::Tiling(tiling) => {
                let after = std::mem::replace(&mut self.tiling, *tiling);
                Change::Tiling(Box::new(after))
            }
            Change::Distortions(distortions) => {
                let after = std::mem::replace(&mut self.distortions, distortions);
                Change::Distortions(after)
            }
            Change::NestMarks(marks) => {
                let after = std::mem::replace(&mut self.nest_marks, marks);
                Change::NestMarks(after)
            }
            Change::Variables { values, formulas } => Change::Variables {
                values: std::mem::replace(&mut self.variables, values),
                formulas: std::mem::replace(&mut self.variable_formulas, formulas),
            },
            Change::Counters {
                entity,
                layer,
                sheet,
                toolpath,
                component,
            } => {
                let after = Change::Counters {
                    entity: self.next_entity_id,
                    layer: self.next_layer_id,
                    sheet: self.next_sheet_id,
                    toolpath: self.next_toolpath_id,
                    component: self.next_component_id,
                };
                self.next_component_id = self.next_component_id.max(component);
                // Counters never go back, so an id is never handed out twice,
                // not even after its object was undone away.
                self.next_entity_id = self.next_entity_id.max(entity);
                self.next_layer_id = self.next_layer_id.max(layer);
                self.next_sheet_id = self.next_sheet_id.max(sheet);
                self.next_toolpath_id = self.next_toolpath_id.max(toolpath);
                after
            }
            Change::Toolpath { id, before } => {
                let after = self.toolpaths.get(&id).cloned().map(Box::new);
                match before {
                    Some(toolpath) => {
                        self.toolpaths.insert(id, *toolpath);
                    }
                    None => {
                        self.toolpaths.remove(&id);
                    }
                }
                Change::Toolpath { id, before: after }
            }
            Change::ToolpathOrder(order) => {
                let after = std::mem::replace(&mut self.toolpath_order, order);
                Change::ToolpathOrder(after)
            }
            Change::ToolpathGroups(groups) => {
                let after = std::mem::replace(&mut self.toolpath_groups, groups);
                Change::ToolpathGroups(after)
            }
            Change::Component { id, before } => {
                self.model_stamp += 1;
                let after = self.components.get(&id).cloned().map(Box::new);
                match before {
                    Some(component) => {
                        self.components.insert(id, *component);
                    }
                    None => {
                        self.components.remove(&id);
                    }
                }
                Change::Component { id, before: after }
            }
            Change::ComponentOrder(order) => {
                self.model_stamp += 1;
                let after = std::mem::replace(&mut self.component_order, order);
                Change::ComponentOrder(after)
            }
            Change::Levels {
                levels,
                active,
                next,
            } => {
                self.model_stamp += 1;
                let after = Change::Levels {
                    levels: std::mem::replace(&mut self.levels, levels),
                    active: std::mem::replace(&mut self.active_level, active),
                    next: self.next_level_id,
                };
                self.next_level_id = self.next_level_id.max(next);
                after
            }
            Change::SwapSides {
                toggle,
                install,
                remove,
            } => self.revert_swap(toggle, install, remove),
        }
    }

    /// Records the state of one entity so the open step can put it back.
    pub fn capture(&mut self, id: u64) {
        if !self.journal.is_recording() {
            return;
        }
        let before = self.entities.get(&id).cloned().map(Box::new);
        self.journal.record(Change::Entity { id, before });
    }

    /// Records the z-order so the open step can put it back.
    pub fn capture_order(&mut self) {
        if self.journal.is_recording() {
            let order = self.order.clone();
            self.journal.record(Change::Order(order));
        }
    }

    /// Records the layer list and the active layer.
    pub fn capture_layers(&mut self) {
        if self.journal.is_recording() {
            let layers = self.layers.clone();
            let active = self.active_layer;
            self.journal.record(Change::Layers(layers, active));
        }
    }

    /// Records the sheet list and the active sheet.
    pub fn capture_sheets(&mut self) {
        if self.journal.is_recording() {
            let sheets = self.sheets.clone();
            let active = self.active_sheet;
            self.journal.record(Change::Sheets(sheets, active));
        }
    }

    /// Records the selection.
    pub fn capture_selection(&mut self) {
        if self.journal.is_recording() {
            let selection = self.selection.clone();
            self.journal.record(Change::Selection(selection));
        }
    }

    /// Records the id counters.
    pub fn capture_counters(&mut self) {
        if self.journal.is_recording() {
            self.journal.record(Change::Counters {
                entity: self.next_entity_id,
                layer: self.next_layer_id,
                sheet: self.next_sheet_id,
                toolpath: self.next_toolpath_id,
                component: self.next_component_id,
            });
        }
    }

    // --------------------------------------------------------------- layers

    /// Layers in drawing order.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// The active layer id.
    pub fn active_layer(&self) -> u64 {
        self.active_layer
    }

    /// The active sheet id.
    pub fn active_sheet(&self) -> u64 {
        self.active_sheet
    }

    /// Sheets in tab order.
    pub fn sheets(&self) -> &[Sheet] {
        &self.sheets
    }

    /// Looks a layer up.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when there is no such layer.
    pub fn layer(&self, id: u64) -> Result<&Layer, CoreError> {
        self.layers
            .iter()
            .find(|l| l.id == id)
            .ok_or_else(|| CoreError::validation(format!("no layer {id}")))
    }

    /// How many top level entities sit on a layer.
    pub fn layer_count(&self, id: u64) -> usize {
        self.order
            .iter()
            .filter_map(|e| self.entities.get(e))
            .filter(|e| e.layer == id)
            .count()
    }

    /// Adds a layer and answers its id.
    pub fn add_layer(&mut self, name: Option<String>, colour: Option<String>) -> u64 {
        self.capture_layers();
        self.capture_counters();
        let id = self.next_layer_id;
        self.next_layer_id += 1;
        let index = self.layers.len();
        let name = name.unwrap_or_else(|| format!("Layer {}", index + 1));
        let colour = colour.unwrap_or_else(|| Layer::colour_for(index).to_string());
        self.layers.push(Layer::new(id, name, colour));
        id
    }

    /// Edits a layer in place. Fields left as `None` keep their value.
    ///
    /// Hiding or locking a layer drops its entities from the selection, which
    /// is what stops a hidden object from being dragged by mistake.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown layer or a colour
    /// that is not `#rrggbb`.
    /// Marks a layer as a keep-out zone layer, or clears the mark.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for an unknown layer.
    pub fn set_layer_keep_out(&mut self, id: u64, keep_out: bool) -> Result<(), CoreError> {
        self.layer(id)?;
        self.capture_layers();
        if let Some(layer) = self.layers.iter_mut().find(|l| l.id == id) {
            layer.keep_out = keep_out;
        }
        Ok(())
    }

    pub fn update_layer(
        &mut self,
        id: u64,
        name: Option<String>,
        colour: Option<String>,
        visible: Option<bool>,
        locked: Option<bool>,
    ) -> Result<(), CoreError> {
        self.layer(id)?;
        if let Some(colour) = colour.as_deref() {
            if !is_hex_colour(colour) {
                return Err(CoreError::validation(format!(
                    "a layer color must look like #rrggbb, got {colour}"
                )));
            }
        }
        self.capture_layers();
        let Some(layer) = self.layers.iter_mut().find(|l| l.id == id) else {
            return Err(CoreError::validation(format!("no layer {id}")));
        };
        if let Some(name) = name {
            layer.name = name;
        }
        if let Some(colour) = colour {
            layer.colour = colour;
        }
        if let Some(visible) = visible {
            layer.visible = visible;
        }
        if let Some(locked) = locked {
            layer.locked = locked;
        }
        let hidden = !layer.visible || layer.locked;
        if hidden {
            let drop: Vec<u64> = self
                .selection
                .iter()
                .copied()
                .filter(|s| self.entities.get(s).is_some_and(|e| e.layer == id))
                .collect();
            if !drop.is_empty() {
                self.capture_selection();
                self.selection.retain(|s| !drop.contains(s));
            }
        }
        Ok(())
    }

    /// Deletes a layer. Its entities move to `move_to` or are deleted.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown layer, for an unknown
    /// destination or when the layer is the only one left.
    pub fn delete_layer(&mut self, id: u64, move_to: Option<u64>) -> Result<(), CoreError> {
        self.layer(id)?;
        if self.layers.len() == 1 {
            return Err(CoreError::validation(
                "a document needs at least one layer, so the last one cannot be deleted",
            ));
        }
        if let Some(target) = move_to {
            if target == id {
                return Err(CoreError::validation(
                    "a layer's entities cannot be moved to the layer being deleted",
                ));
            }
            self.layer(target)?;
        }
        let members: Vec<u64> = self
            .entities
            .iter()
            .filter(|(_, e)| e.layer == id)
            .map(|(eid, _)| *eid)
            .collect();
        match move_to {
            Some(target) => {
                for member in members {
                    self.capture(member);
                    if let Some(entity) = self.entities.get_mut(&member) {
                        entity.layer = target;
                    }
                }
            }
            None => self.delete_entities(&members),
        }
        self.capture_layers();
        self.layers.retain(|l| l.id != id);
        if self.active_layer == id {
            self.active_layer = self.layers.first().map(|l| l.id).unwrap_or(0);
        }
        Ok(())
    }

    /// Makes a layer the active one.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when there is no such layer.
    pub fn set_active_layer(&mut self, id: u64) -> Result<(), CoreError> {
        self.layer(id)?;
        self.capture_layers();
        self.active_layer = id;
        Ok(())
    }

    /// Reorders the layer list. Ids not mentioned keep their relative order
    /// after the ones that are.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when an id is not a layer.
    pub fn reorder_layers(&mut self, ids: &[u64]) -> Result<(), CoreError> {
        for id in ids {
            self.layer(*id)?;
        }
        self.capture_layers();
        let mut ordered: Vec<Layer> = Vec::with_capacity(self.layers.len());
        for id in ids {
            if let Some(index) = self.layers.iter().position(|l| l.id == *id) {
                ordered.push(self.layers.remove(index));
            }
        }
        ordered.append(&mut self.layers);
        self.layers = ordered;
        Ok(())
    }

    // --------------------------------------------------------------- sheets

    /// Looks a sheet up.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when there is no such sheet.
    pub fn sheet(&self, id: u64) -> Result<&Sheet, CoreError> {
        self.sheets
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| CoreError::validation(format!("no sheet {id}")))
    }

    /// Adds a sheet and answers its id.
    pub fn add_sheet(&mut self, name: Option<String>) -> u64 {
        self.capture_sheets();
        self.capture_counters();
        let id = self.next_sheet_id;
        self.next_sheet_id += 1;
        let name = name.unwrap_or_else(|| format!("Sheet {}", self.sheets.len() + 1));
        self.sheets.push(Sheet::new(id, name));
        self.remember_sheet_material(id);
        id
    }

    /// Deletes a sheet and everything on it.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown sheet or when it is
    /// the only one left.
    pub fn delete_sheet(&mut self, id: u64) -> Result<(), CoreError> {
        self.sheet(id)?;
        if self.sheets.len() == 1 {
            return Err(CoreError::validation(
                "a document needs at least one sheet, so the last one cannot be deleted",
            ));
        }
        let members: Vec<u64> = self
            .entities
            .iter()
            .filter(|(_, e)| e.sheet == id)
            .map(|(eid, _)| *eid)
            .collect();
        self.delete_entities(&members);
        let toolpaths: Vec<u64> = self
            .toolpaths
            .values()
            .filter(|t| t.sheet == id)
            .map(|t| t.id)
            .collect();
        if !toolpaths.is_empty() {
            self.delete_toolpaths(&toolpaths)?;
        }
        self.delete_sheet_components(id)?;
        self.capture_sheets();
        self.sheets.retain(|s| s.id != id);
        if self.active_sheet == id {
            self.active_sheet = self.sheets.first().map(|s| s.id).unwrap_or(0);
        }
        Ok(())
    }

    /// Makes a sheet the active one.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when there is no such sheet.
    pub fn set_active_sheet(&mut self, id: u64) -> Result<(), CoreError> {
        self.sheet(id)?;
        self.capture_sheets();
        self.swap_sheet_material(id)?;
        self.active_sheet = id;
        Ok(())
    }

    /// Renames a sheet.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when there is no such sheet.
    pub fn rename_sheet(&mut self, id: u64, name: String) -> Result<(), CoreError> {
        self.sheet(id)?;
        self.capture_sheets();
        if let Some(sheet) = self.sheets.iter_mut().find(|s| s.id == id) {
            sheet.name = name;
        }
        Ok(())
    }

    // ------------------------------------------------------------- entities

    /// Every entity, including group members, keyed by id.
    pub fn entities(&self) -> &BTreeMap<u64, Entity> {
        &self.entities
    }

    /// Every entity, for an operation that has already recorded the change it
    /// is about to make with [`Document::capture`].
    pub fn entities_mut(&mut self) -> &mut BTreeMap<u64, Entity> {
        &mut self.entities
    }

    /// Top level z-order, back to front.
    pub fn order(&self) -> &[u64] {
        &self.order
    }

    /// Looks an entity up.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when there is no such entity.
    pub fn entity(&self, id: u64) -> Result<&Entity, CoreError> {
        self.entities
            .get(&id)
            .ok_or_else(|| CoreError::validation(format!("no entity {id}")))
    }

    /// The ids of every entity, groups and their members included.
    pub fn entity_ids(&self) -> BTreeSet<u64> {
        self.entities.keys().copied().collect()
    }

    /// Adds an entity, giving it the next id, and answers that id.
    ///
    /// The entity lands on top of the z-order, which is where a newly drawn
    /// vector goes.
    pub fn insert_entity(&mut self, mut entity: Entity) -> u64 {
        self.capture_counters();
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        entity.id = id;
        entity.revision = self.revision + 1;
        self.capture(id);
        self.capture_order();
        self.entities.insert(id, entity);
        self.order.push(id);
        id
    }

    /// Replaces an entity's content, keeping its id and bumping its revision.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when there is no such entity.
    pub fn replace_entity(&mut self, id: u64, mut entity: Entity) -> Result<(), CoreError> {
        let existing = self.entity(id)?;
        entity.id = id;
        entity.parent = existing.parent;
        entity.revision = self.revision + 1;
        self.capture(id);
        self.entities.insert(id, entity);
        Ok(())
    }

    /// Bumps an entity's revision so the display cache reflattens it.
    pub fn touch(&mut self, id: u64) {
        let revision = self.revision + 1;
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.revision = revision;
        }
    }

    /// Deletes entities, their group members and any reference to them.
    pub fn delete_entities(&mut self, ids: &[u64]) {
        let mut wanted: Vec<u64> = Vec::new();
        for id in ids {
            self.collect_tree(*id, &mut wanted);
        }
        if wanted.is_empty() {
            return;
        }
        self.capture_order();
        self.capture_selection();
        for id in &wanted {
            self.capture(*id);
        }
        // A member of a group that survives must leave that group's list.
        let parents: BTreeSet<u64> = wanted
            .iter()
            .filter_map(|id| self.entities.get(id).and_then(|e| e.parent))
            .filter(|p| !wanted.contains(p))
            .collect();
        for parent in parents {
            self.capture(parent);
            if let Some(Entity {
                kind: EntityKind::Group { children },
                ..
            }) = self.entities.get_mut(&parent)
            {
                children.retain(|c| !wanted.contains(c));
            }
        }
        for id in &wanted {
            self.entities.remove(id);
        }
        // Text left on a deleted curve stays where it is, as plain text.
        let orphans: Vec<u64> = self
            .entities
            .iter()
            .filter(|(_, e)| {
                matches!(&e.kind, EntityKind::Text(t)
                    if t.spec.on_curve.as_ref().is_some_and(|on| wanted.contains(&on.path_id)))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in orphans {
            self.capture(id);
            if let Some(Entity {
                kind: EntityKind::Text(text),
                ..
            }) = self.entities.get_mut(&id)
            {
                text.spec.on_curve = None;
            }
        }
        self.order.retain(|id| !wanted.contains(id));
        self.selection.retain(|id| !wanted.contains(id));
    }

    /// Collects an entity and, for a group, everything under it.
    fn collect_tree(&self, id: u64, out: &mut Vec<u64>) {
        if out.contains(&id) {
            return;
        }
        let Some(entity) = self.entities.get(&id) else {
            return;
        };
        out.push(id);
        if let EntityKind::Group { children } = &entity.kind {
            for child in children.clone() {
                self.collect_tree(child, out);
            }
        }
    }

    /// The entities that actually carry geometry under an id: the id itself
    /// for a plain entity, every leaf under it for a group.
    pub fn leaves(&self, id: u64) -> Vec<u64> {
        let mut out = Vec::new();
        self.collect_leaves(id, &mut out);
        out
    }

    fn collect_leaves(&self, id: u64, out: &mut Vec<u64>) {
        match self.entities.get(&id) {
            Some(Entity {
                kind: EntityKind::Group { children },
                ..
            }) => {
                for child in children {
                    self.collect_leaves(*child, out);
                }
            }
            Some(_) => out.push(id),
            None => {}
        }
    }

    /// Box round an entity and everything under it.
    pub fn bbox_of(&self, id: u64) -> Option<BBox> {
        self.leaves(id)
            .into_iter()
            .filter_map(|leaf| self.entities.get(&leaf).and_then(|e| e.bbox()))
            .reduce(|a, b| a.union(b))
    }

    /// Box round a set of entities.
    pub fn bbox_of_many(&self, ids: &[u64]) -> Option<BBox> {
        ids.iter()
            .filter_map(|id| self.bbox_of(*id))
            .reduce(|a, b| a.union(b))
    }

    /// The geometry of an entity and everything under it, as one path per
    /// leaf, in z-order.
    pub fn paths_of(&self, id: u64) -> Vec<(u64, Path)> {
        self.leaves(id)
            .into_iter()
            .filter_map(|leaf| {
                self.entities
                    .get(&leaf)
                    .and_then(|e| e.path())
                    .map(|p| (leaf, p))
            })
            .collect()
    }

    /// Applies an affine transform to entities and everything under them.
    /// A shape component whose vectors all move stays where it lies (see
    /// `Document::shapes_left_by`).
    pub fn transform_entities(&mut self, ids: &[u64], m: &Mat3) {
        let revision = self.revision + 1;
        let mut moved = BTreeSet::new();
        for id in ids {
            let leaves = self.leaves(*id);
            moved.extend(leaves.iter().copied());
            if !leaves.contains(id) {
                self.capture(*id);
                if let Some(group) = self.entities.get_mut(id) {
                    group.rotation = entity::rotated_frame(group.rotation, m);
                }
            }
            for leaf in leaves {
                self.capture(leaf);
                if let Some(entity) = self.entities.get_mut(&leaf) {
                    entity.transform_with_frame(m);
                    entity.revision = revision;
                }
            }
        }
        self.shapes_left_by(&moved, m);
    }

    /// The rotation frame the selection shares: `Some` when every item is
    /// rotated by the same non zero angle, with the box in that frame.
    pub fn selection_frame(&self, ids: &[u64]) -> Option<SelectionFrame> {
        let first = self.entities.get(ids.first()?)?.rotation;
        if first == 0.0 {
            return None;
        }
        for id in ids {
            let rotation = self.entities.get(id)?.rotation;
            if (rotation - first).abs() > 1e-6 {
                return None;
            }
        }
        let unrotate = Mat3::rotate(-first.to_radians());
        let extent = ids
            .iter()
            .flat_map(|id| self.paths_of(*id))
            .filter_map(|(_, path)| {
                crate::geom::path_bbox(&crate::geom::transform_path(&path, &unrotate))
            })
            .reduce(|a, b| a.union(b))?;
        Some(SelectionFrame {
            angle: first,
            extent: bbox_array(extent),
        })
    }

    /// Moves entities onto another layer.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown layer or entity.
    pub fn set_entity_layer(&mut self, ids: &[u64], layer: u64) -> Result<(), CoreError> {
        self.layer(layer)?;
        for id in ids {
            self.entity(*id)?;
        }
        for id in ids {
            for member in self.leaves(*id).into_iter().chain(std::iter::once(*id)) {
                self.capture(member);
                if let Some(entity) = self.entities.get_mut(&member) {
                    entity.layer = layer;
                }
            }
        }
        Ok(())
    }

    /// Moves entities, and everything under them, onto another sheet.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown sheet or entity.
    pub fn set_entity_sheet(&mut self, ids: &[u64], sheet: u64) -> Result<(), CoreError> {
        self.sheet(sheet)?;
        for id in ids {
            self.entity(*id)?;
        }
        for id in ids {
            for member in self.leaves(*id).into_iter().chain(std::iter::once(*id)) {
                self.capture(member);
                if let Some(entity) = self.entities.get_mut(&member) {
                    entity.sheet = sheet;
                }
            }
        }
        Ok(())
    }

    /// Groups entities into one object and answers the new group's id.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when fewer than two top level
    /// entities were named.
    pub fn group(&mut self, ids: &[u64]) -> Result<u64, CoreError> {
        let members = self.top_level_members(ids)?;
        if members.len() < 2 {
            return Err(CoreError::validation(
                "select at least two objects to group them",
            ));
        }
        self.make_group(members)
    }

    /// Groups the copies of an existing group's members, however many it
    /// holds: a group left with one member (the others deleted) is copied,
    /// pasted and duplicated with its layer or sheet like any other.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when no top level entity was named.
    pub fn group_copies(&mut self, ids: &[u64]) -> Result<u64, CoreError> {
        let members = self.top_level_members(ids)?;
        if members.is_empty() {
            return Err(CoreError::validation("the group to copy is empty"));
        }
        self.make_group(members)
    }

    /// The named entities that are not inside a group, each once.
    fn top_level_members(&self, ids: &[u64]) -> Result<Vec<u64>, CoreError> {
        let mut members: Vec<u64> = Vec::new();
        for id in ids {
            let entity = self.entity(*id)?;
            if entity.parent.is_none() && !members.contains(id) {
                members.push(*id);
            }
        }
        Ok(members)
    }

    /// Groups `members` (top level, at least one) into a new group.
    fn make_group(&mut self, mut members: Vec<u64>) -> Result<u64, CoreError> {
        // Keep the group's members in z-order, which is what ungrouping and
        // drawing both expect.
        members.sort_by_key(|id| {
            self.order
                .iter()
                .position(|o| o == id)
                .unwrap_or(usize::MAX)
        });
        let first = self.entity(members[0])?;
        let (layer, sheet) = (first.layer, first.sheet);
        self.capture_order();
        for member in &members {
            self.capture(*member);
        }
        let group = Entity {
            rotation: 0.0,
            id: 0,
            layer,
            sheet,
            revision: self.revision + 1,
            name: None,
            parent: None,
            kind: EntityKind::Group {
                children: members.clone(),
            },
        };
        let id = self.insert_entity(group);
        for member in &members {
            if let Some(entity) = self.entities.get_mut(member) {
                entity.parent = Some(id);
            }
        }
        self.order.retain(|o| !members.contains(o) || *o == id);
        Ok(id)
    }

    /// Ungroups entities and answers what came out: the members of every
    /// group named, and any non group entity unchanged.
    pub fn ungroup(&mut self, ids: &[u64]) -> Vec<u64> {
        let mut released = Vec::new();
        let mut captured_order = false;
        for id in ids {
            let children = match self.entities.get(id) {
                Some(Entity {
                    kind: EntityKind::Group { children },
                    ..
                }) => children.clone(),
                Some(_) => {
                    released.push(*id);
                    continue;
                }
                None => continue,
            };
            if !captured_order {
                self.capture_order();
                // The group leaves the selection below: Undo puts it back.
                self.capture_selection();
                captured_order = true;
            }
            self.capture(*id);
            let at = self.order.iter().position(|o| o == id);
            for child in &children {
                self.capture(*child);
                if let Some(entity) = self.entities.get_mut(child) {
                    entity.parent = None;
                }
                released.push(*child);
            }
            self.entities.remove(id);
            match at {
                Some(index) => {
                    self.order
                        .splice(index..index + 1, children.iter().copied());
                }
                None => self.order.extend(children.iter().copied()),
            }
            self.selection.retain(|s| s != id);
        }
        released
    }

    // ------------------------------------------------------------ selection

    /// The selection, in selection order.
    pub fn selection(&self) -> &[u64] {
        &self.selection
    }

    /// The selection with its bounding box, which is what `selection.*`
    /// answers with.
    pub fn selection_state(&self) -> SelectionState {
        SelectionState {
            ids: self.selection.clone(),
            bbox: self.bbox_of_many(&self.selection).map(bbox_array),
            frame: self.selection_frame(&self.selection),
        }
    }

    /// The ids a command works on: the ones it was given, or the selection
    /// when it was given none.
    pub fn target_ids(&self, ids: Option<&[u64]>) -> Vec<u64> {
        match ids {
            Some(ids) if !ids.is_empty() => ids.to_vec(),
            _ => self.selection.clone(),
        }
    }

    /// Replaces the selection, keeping only ids that exist and are pickable.
    pub fn set_selection(&mut self, ids: &[u64]) {
        self.capture_selection();
        let mut next = Vec::new();
        for id in ids {
            if self.is_pickable(*id) && !next.contains(id) {
                next.push(*id);
            }
        }
        self.selection = next;
    }

    /// Adds to the selection, keeping selection order.
    pub fn add_selection(&mut self, ids: &[u64]) {
        self.capture_selection();
        for id in ids {
            if self.is_pickable(*id) && !self.selection.contains(id) {
                self.selection.push(*id);
            }
        }
    }

    /// Removes ids from the selection.
    pub fn remove_selection(&mut self, ids: &[u64]) {
        self.capture_selection();
        self.selection.retain(|s| !ids.contains(s));
    }

    /// Toggles ids in the selection.
    pub fn toggle_selection(&mut self, ids: &[u64]) {
        self.capture_selection();
        for id in ids {
            if let Some(index) = self.selection.iter().position(|s| s == id) {
                self.selection.remove(index);
            } else if self.is_pickable(*id) {
                self.selection.push(*id);
            }
        }
    }

    /// Clears the selection.
    pub fn clear_selection(&mut self) {
        self.capture_selection();
        self.selection.clear();
    }

    /// Selects every pickable entity on the active sheet.
    pub fn select_all(&mut self) {
        self.capture_selection();
        self.selection = self.selectable();
    }

    /// True when the entity, or the group holding it, is selected.
    pub fn is_selected(&self, id: u64) -> bool {
        if self.selection.contains(&id) {
            return true;
        }
        match self.entities.get(&id).and_then(|e| e.parent) {
            Some(parent) => self.is_selected(parent),
            None => false,
        }
    }

    /// Top level entities on the active sheet whose layer is visible and
    /// unlocked, in z-order. These are the ones a click can land on.
    pub fn selectable(&self) -> Vec<u64> {
        self.order
            .iter()
            .copied()
            .filter(|id| self.is_pickable(*id))
            .collect()
    }

    /// Whether an entity can be picked: it exists, is top level, sits on the
    /// active sheet and its layer is visible and unlocked.
    pub fn is_pickable(&self, id: u64) -> bool {
        let Some(entity) = self.entities.get(&id) else {
            return false;
        };
        if entity.parent.is_some() || entity.sheet != self.active_sheet {
            return false;
        }
        self.layers
            .iter()
            .find(|l| l.id == entity.layer)
            .is_some_and(|l| l.visible && !l.locked)
    }

    /// Summary of one entity for the object list.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when there is no such entity.
    pub fn summary(&self, id: u64) -> Result<EntitySummary, CoreError> {
        let entity = self.entity(id)?;
        let children = match &entity.kind {
            EntityKind::Group { children } => Some(children.clone()),
            _ => None,
        };
        let closed = match &entity.kind {
            EntityKind::Group { children } => {
                !children.is_empty()
                    && children
                        .iter()
                        .all(|c| self.summary(*c).map(|s| s.closed).unwrap_or(false))
            }
            _ => entity.is_closed(),
        };
        Ok(EntitySummary {
            id,
            kind: entity.kind.tag().to_string(),
            layer: entity.layer,
            bbox: self.bbox_of(id).map(bbox_array).unwrap_or(EMPTY_BBOX),
            closed,
            revision: entity.revision,
            name: entity.name.clone(),
            children,
            on_curve: match &entity.kind {
                EntityKind::Text(text) => text.spec.on_curve.as_ref().map(|c| c.path_id),
                _ => None,
            },
            nest: self.nest_marks.get(&id).copied(),
        })
    }

    /// Summaries of the top level entities on a sheet, in z-order.
    pub fn list(&self, sheet: Option<u64>) -> Vec<EntitySummary> {
        let sheet = sheet.unwrap_or(self.active_sheet);
        self.order
            .iter()
            .filter(|id| {
                self.entities
                    .get(id)
                    .is_some_and(|e| e.sheet == sheet && e.parent.is_none())
            })
            .filter_map(|id| self.summary(*id).ok())
            .collect()
    }

    // ------------------------------------------------------- save and open

    /// The document in the shape written to `document.json`.
    pub fn to_file(&self) -> DocumentFile {
        let live = self.live_side_file();
        let other = self.other_side.as_ref().map(|side| side.to_file());
        let (top, bottom) = match (self.active_side, other) {
            (Side::Bottom, Some(other)) => (other, Some(live)),
            (_, other) => (live, other),
        };
        DocumentFile {
            format: FORMAT_VERSION,
            job: self.job,
            layers: top.layers,
            sheets: top.sheets,
            entities: top.entities,
            order: top.order,
            active_layer: top.active_layer,
            active_sheet: top.active_sheet,
            notes: self.notes.clone(),
            assembly: self.assembly.clone(),
            variables: self.variables.clone(),
            variable_formulas: self.variable_formulas.clone(),
            distortions: self.distortions.clone(),
            nest_marks: self.nest_marks.clone(),
            tiling: self.tiling,
            dimension_style: (self.dimension_style != crate::geom::DimensionStyle::default())
                .then(|| self.dimension_style.clone()),
            guides: self.guides.clone(),
            next_entity_id: self.next_entity_id,
            next_layer_id: self.next_layer_id,
            next_sheet_id: self.next_sheet_id,
            toolpaths: self
                .toolpath_order
                .iter()
                .filter_map(|id| self.toolpaths.get(id).cloned())
                .collect(),
            next_toolpath_id: self.next_toolpath_id,
            toolpath_groups: self.toolpath_groups.clone(),
            components: top.components,
            next_component_id: self.next_component_id,
            levels: top.levels,
            active_level: top.active_level,
            next_level_id: self.next_level_id,
            active_side: self.active_side,
            bottom,
        }
    }

    /// The active side's content in its file shape.
    fn live_side_file(&self) -> SideFile {
        SideFile {
            layers: self.layers.clone(),
            sheets: self.sheets.clone(),
            entities: self.entities.values().cloned().collect(),
            order: self.order.clone(),
            active_layer: self.active_layer,
            active_sheet: self.active_sheet,
            components: self
                .component_order
                .iter()
                .filter_map(|id| self.components.get(id).cloned())
                .collect(),
            levels: self.levels.clone(),
            active_level: self.active_level,
        }
    }

    /// Rebuilds a document from a parsed `document.json`.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when the format version is newer
    /// than this build understands or the file has no layers or sheets.
    pub fn from_file(mut file: DocumentFile, revision: u64) -> Result<Self, CoreError> {
        let bottom = file.bottom.take().map(SideState::from_file).transpose()?;
        let active_side = file.active_side;
        let mut doc = Document::from_top_side(file, revision)?;
        if let Some(bottom) = bottom {
            let (entity, layer, sheet, component, level) = bottom.max_ids();
            doc.next_entity_id = doc.next_entity_id.max(entity + 1);
            doc.next_layer_id = doc.next_layer_id.max(layer + 1);
            doc.next_sheet_id = doc.next_sheet_id.max(sheet + 1);
            doc.next_component_id = doc.next_component_id.max(component + 1);
            doc.next_level_id = doc.next_level_id.max(level + 1);
            doc.other_side = Some(Box::new(bottom));
            if active_side == Side::Bottom {
                doc.swap_live(true);
            }
        }
        doc.sync_job();
        Ok(doc)
    }

    /// Rebuilds the document from the file's top level fields, which hold
    /// the top side's content.
    fn from_top_side(file: DocumentFile, revision: u64) -> Result<Self, CoreError> {
        if file.format > FORMAT_VERSION {
            return Err(CoreError::validation(format!(
                "this file was written by a newer version of Zeroed (format {})",
                file.format
            )));
        }
        if file.layers.is_empty() {
            return Err(CoreError::validation("the file has no layers"));
        }
        if file.sheets.is_empty() {
            return Err(CoreError::validation("the file has no sheets"));
        }
        let entities: BTreeMap<u64, Entity> =
            file.entities.into_iter().map(|e| (e.id, e)).collect();
        // Files from before levels put every component on one level.
        let levels = if file.levels.is_empty() {
            vec![Level::first()]
        } else {
            file.levels
        };
        let first_level = levels.first().map_or(1, |l| l.id);
        let active_level = if levels.iter().any(|l| l.id == file.active_level) {
            file.active_level
        } else {
            first_level
        };
        let next_level_id = levels
            .iter()
            .map(|l| l.id + 1)
            .max()
            .unwrap_or(2)
            .max(file.next_level_id);
        let mut components = file.components;
        for component in &mut components {
            if !levels.iter().any(|l| l.id == component.level) {
                component.level = first_level;
            }
        }
        let order: Vec<u64> = file
            .order
            .into_iter()
            .filter(|id| entities.contains_key(id))
            .collect();
        let mut document = Document {
            time_estimate: crate::cam::TimeEstimate::default(),
            cam_settings: crate::cam::CamSettings::default(),
            job: file.job,
            revision,
            path: None,
            dirty: false,
            layers: file.layers,
            sheets: file.sheets,
            entities,
            order,
            selection: Vec::new(),
            active_layer: file.active_layer,
            active_sheet: file.active_sheet,
            notes: file.notes,
            assembly: file.assembly,
            variables: file.variables,
            variable_formulas: file.variable_formulas,
            distortions: file.distortions,
            nest_marks: file.nest_marks,
            copies_made: BTreeMap::new(),
            tiling: file.tiling,
            dimension_style: file.dimension_style.unwrap_or_default(),
            guides: file.guides,
            next_entity_id: file.next_entity_id.max(1),
            next_layer_id: file.next_layer_id.max(1),
            next_sheet_id: file.next_sheet_id.max(1),
            next_toolpath_id: file
                .toolpaths
                .iter()
                .map(|t| t.id + 1)
                .max()
                .unwrap_or(1)
                .max(file.next_toolpath_id),
            toolpath_order: file.toolpaths.iter().map(|t| t.id).collect(),
            toolpath_groups: file.toolpath_groups,
            toolpaths: file.toolpaths.into_iter().map(|t| (t.id, t)).collect(),
            next_component_id: components
                .iter()
                .map(|c| c.id + 1)
                .max()
                .unwrap_or(1)
                .max(file.next_component_id),
            component_order: components.iter().map(|c| c.id).collect(),
            components: components.into_iter().map(|c| (c.id, c)).collect(),
            levels,
            active_level,
            next_level_id,
            model_stamp: 0,
            journal: Journal::new(),
            active_side: Side::Top,
            other_side: None,
            side_job: file.job,
            images: images::ImageStore::default(),
        };
        document.fill_through_cuts();
        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{circle, rect, Vec2};

    fn path_entity(doc: &Document, path: Path, name: &str) -> Entity {
        Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: Some(name.to_string()),
            parent: None,
            kind: EntityKind::Path { path },
        }
    }

    fn with_two_rects() -> (Document, u64, u64) {
        let mut doc = Document::new();
        doc.begin("Create");
        let a = path_entity(
            &doc,
            rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect"),
            "A",
        );
        let a = doc.insert_entity(a);
        let b = path_entity(
            &doc,
            rect(Point::new(20.0, 0.0), Point::new(30.0, 10.0)).expect("rect"),
            "B",
        );
        let b = doc.insert_entity(b);
        doc.commit();
        (doc, a, b)
    }

    #[test]
    fn a_new_document_has_one_layer_and_one_sheet() {
        let doc = Document::new();
        assert_eq!(doc.layers().len(), 1);
        assert_eq!(doc.layers()[0].name, FIRST_LAYER);
        assert_eq!(doc.layers()[0].colour, "#000000");
        assert_eq!(doc.sheets().len(), 1);
        assert_eq!(doc.active_layer(), 1);
        assert_eq!(doc.active_sheet(), 1);
    }

    #[test]
    fn new_document_has_default_job() {
        assert_eq!(*Document::new().job(), JobSetup::default());
    }

    #[test]
    fn rejected_job_leaves_document_untouched() {
        let mut doc = Document::new();
        let job = JobSetup {
            thickness: 0.0,
            ..JobSetup::default()
        };
        assert!(doc.set_job(job).is_err());
        assert_eq!(*doc.job(), JobSetup::default());
    }

    #[test]
    fn revision_only_moves_on_committed_changes() {
        let mut doc = Document::new();
        assert_eq!(doc.revision(), 0);
        doc.begin("Job Setup");
        doc.set_job(JobSetup {
            width: 600.0,
            ..JobSetup::default()
        })
        .expect("valid job");
        assert_eq!(doc.commit(), 1);
        doc.begin("Nothing");
        assert_eq!(doc.commit(), 1);
    }

    #[test]
    fn inserting_an_entity_gives_it_the_next_id_and_puts_it_on_top() {
        let (doc, a, b) = with_two_rects();
        assert_eq!(a, 1);
        assert_eq!(b, 2);
        assert_eq!(doc.order(), &[a, b]);
    }

    #[test]
    fn undo_puts_a_deleted_entity_back_with_its_z_order() {
        let (mut doc, a, b) = with_two_rects();
        doc.begin("Delete");
        doc.delete_entities(&[a]);
        doc.commit();
        assert_eq!(doc.order(), &[b]);
        assert_eq!(doc.undo().as_deref(), Some("Delete"));
        assert_eq!(doc.order(), &[a, b]);
        assert!(doc.entity(a).is_ok());
    }

    #[test]
    fn redo_deletes_the_entity_again() {
        let (mut doc, a, _) = with_two_rects();
        doc.begin("Delete");
        doc.delete_entities(&[a]);
        doc.commit();
        doc.undo().expect("undo");
        assert_eq!(doc.redo().as_deref(), Some("Delete"));
        assert!(doc.entity(a).is_err());
    }

    #[test]
    fn undo_restores_the_selection() {
        let (mut doc, a, b) = with_two_rects();
        doc.set_selection(&[a]);
        doc.begin("Select");
        doc.set_selection(&[a, b]);
        doc.commit();
        doc.undo().expect("undo");
        assert_eq!(doc.selection(), &[a]);
    }

    #[test]
    fn transforming_moves_the_geometry_and_bumps_the_entity_revision() {
        let (mut doc, a, _) = with_two_rects();
        let before = doc.entity(a).expect("entity").revision;
        doc.begin("Move");
        doc.transform_entities(&[a], &Mat3::translate(Vec2::new(5.0, 0.0)));
        doc.commit();
        let entity = doc.entity(a).expect("entity");
        assert!(entity.revision > before);
        let bbox = doc.bbox_of(a).expect("bbox");
        assert!((bbox.min.x - 5.0).abs() < 1e-9, "{bbox:?}");
    }

    #[test]
    fn a_rotated_object_keeps_its_frame_through_a_scale_along_it() {
        let (mut doc, a, _) = with_two_rects();
        let size = doc.bbox_of(a).expect("bbox");
        let (w, h) = (size.max.x - size.min.x, size.max.y - size.min.y);
        doc.begin("Rotate");
        doc.transform_entities(&[a], &Mat3::rotate_about(30f64.to_radians(), Point::ORIGIN));
        doc.commit();
        assert!((doc.entity(a).expect("entity").rotation - 30.0).abs() < 1e-9);
        let frame = doc.selection_frame(&[a]).expect("rotated frame");
        assert!(
            (frame.extent[2] - frame.extent[0] - w).abs() < 1e-6,
            "{frame:?}"
        );
        assert!(
            (frame.extent[3] - frame.extent[1] - h).abs() < 1e-6,
            "{frame:?}"
        );
        // Doubling the width along the frame keeps the rotation.
        let turn = Mat3::rotate(30f64.to_radians());
        let back = Mat3::rotate(-30f64.to_radians());
        let stretch = back.then(&Mat3::scale(2.0, 1.0)).then(&turn);
        doc.begin("Scale");
        doc.transform_entities(&[a], &stretch);
        doc.commit();
        assert!((doc.entity(a).expect("entity").rotation - 30.0).abs() < 1e-9);
        let frame = doc.selection_frame(&[a]).expect("still rotated");
        assert!((frame.extent[2] - frame.extent[0] - 2.0 * w).abs() < 1e-6);
        assert!((frame.extent[3] - frame.extent[1] - h).abs() < 1e-6);
        // A world axis stretch skews the frame, which resets it.
        doc.begin("Skew");
        doc.transform_entities(&[a], &Mat3::scale(2.0, 1.0));
        doc.commit();
        assert_eq!(doc.entity(a).expect("entity").rotation, 0.0);
        assert!(doc.selection_frame(&[a]).is_none());
        doc.undo().expect("undo");
        assert!((doc.entity(a).expect("entity").rotation - 30.0).abs() < 1e-9);
    }

    #[test]
    fn the_selection_state_reports_a_shared_frame_only() {
        let (mut doc, a, b) = with_two_rects();
        doc.begin("Rotate");
        doc.transform_entities(&[a], &Mat3::rotate(45f64.to_radians()));
        doc.commit();
        doc.set_selection(&[a]);
        let state = doc.selection_state();
        assert!((state.frame.expect("frame").angle - 45.0).abs() < 1e-9);
        doc.set_selection(&[a, b]);
        assert!(doc.selection_state().frame.is_none());
        let group = doc.group(&[a, b]).expect("group");
        doc.transform_entities(&[group], &Mat3::rotate(10f64.to_radians()));
        assert!((doc.entity(group).expect("group").rotation - 10.0).abs() < 1e-9);
        assert!((doc.entity(a).expect("member").rotation - 55.0).abs() < 1e-9);
    }

    #[test]
    fn grouping_takes_the_members_out_of_the_top_level_order() {
        let (mut doc, a, b) = with_two_rects();
        doc.begin("Group");
        let group = doc.group(&[a, b]).expect("two objects group");
        doc.commit();
        assert_eq!(doc.order(), &[group]);
        assert_eq!(doc.leaves(group), vec![a, b]);
        assert_eq!(doc.entity(a).expect("member").parent, Some(group));
    }

    #[test]
    fn grouping_one_object_is_refused() {
        let (mut doc, a, _) = with_two_rects();
        doc.begin("Group");
        let error = doc.group(&[a]).expect_err("one object is not a group");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn ungrouping_puts_the_members_back_where_the_group_was() {
        let (mut doc, a, b) = with_two_rects();
        doc.begin("Group");
        let group = doc.group(&[a, b]).expect("group");
        doc.commit();
        doc.begin("Ungroup");
        let released = doc.ungroup(&[group]);
        doc.commit();
        assert_eq!(released, vec![a, b]);
        assert_eq!(doc.order(), &[a, b]);
        assert!(doc.entity(group).is_err());
    }

    #[test]
    fn deleting_a_group_deletes_its_members() {
        let (mut doc, a, b) = with_two_rects();
        doc.begin("Group");
        let group = doc.group(&[a, b]).expect("group");
        doc.commit();
        doc.begin("Delete");
        doc.delete_entities(&[group]);
        doc.commit();
        assert!(doc.entity(a).is_err());
        assert!(doc.entity(b).is_err());
        assert!(doc.order().is_empty());
    }

    #[test]
    fn hiding_a_layer_takes_its_entities_out_of_the_selection() {
        let (mut doc, a, b) = with_two_rects();
        doc.set_selection(&[a, b]);
        doc.begin("Edit Layer");
        doc.update_layer(1, None, None, Some(false), None)
            .expect("layer updates");
        doc.commit();
        assert!(doc.selection().is_empty());
        assert!(doc.selectable().is_empty());
    }

    #[test]
    fn a_locked_layer_cannot_be_picked() {
        let (mut doc, _, _) = with_two_rects();
        doc.begin("Edit Layer");
        doc.update_layer(1, None, None, None, Some(true))
            .expect("layer updates");
        doc.commit();
        assert!(doc.selectable().is_empty());
    }

    #[test]
    fn a_layer_colour_must_look_like_a_hex_colour() {
        let mut doc = Document::new();
        doc.begin("Edit Layer");
        let error = doc
            .update_layer(1, None, Some("red".to_string()), None, None)
            .expect_err("a name is not a colour");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn deleting_a_layer_can_move_its_entities() {
        let (mut doc, a, _) = with_two_rects();
        doc.begin("Add Layer");
        let second = doc.add_layer(Some("Cut".to_string()), None);
        doc.commit();
        doc.begin("Delete Layer");
        doc.delete_layer(1, Some(second)).expect("layer deletes");
        doc.commit();
        assert_eq!(doc.entity(a).expect("entity").layer, second);
        assert_eq!(doc.layers().len(), 1);
        assert_eq!(doc.active_layer(), second);
    }

    #[test]
    fn deleting_a_layer_without_a_destination_deletes_its_entities() {
        let (mut doc, a, _) = with_two_rects();
        doc.begin("Add Layer");
        doc.add_layer(None, None);
        doc.commit();
        doc.begin("Delete Layer");
        doc.delete_layer(1, None).expect("layer deletes");
        doc.commit();
        assert!(doc.entity(a).is_err());
    }

    #[test]
    fn the_last_layer_cannot_be_deleted() {
        let mut doc = Document::new();
        doc.begin("Delete Layer");
        let error = doc.delete_layer(1, None).expect_err("the last layer stays");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn reordering_layers_puts_the_named_ones_first() {
        let mut doc = Document::new();
        doc.begin("Add Layer");
        let second = doc.add_layer(None, None);
        let third = doc.add_layer(None, None);
        doc.commit();
        doc.begin("Reorder Layers");
        doc.reorder_layers(&[third, second]).expect("reorders");
        doc.commit();
        let ids: Vec<u64> = doc.layers().iter().map(|l| l.id).collect();
        assert_eq!(ids, vec![third, second, 1]);
    }

    #[test]
    fn sheets_can_be_added_renamed_and_deleted() {
        let mut doc = Document::new();
        doc.begin("Add Sheet");
        let second = doc.add_sheet(None);
        doc.commit();
        assert_eq!(doc.sheets().len(), 2);
        doc.begin("Rename Sheet");
        doc.rename_sheet(second, "Offcut".to_string())
            .expect("renames");
        doc.commit();
        assert_eq!(doc.sheet(second).expect("sheet").name, "Offcut");
        doc.begin("Delete Sheet");
        doc.delete_sheet(second).expect("deletes");
        doc.commit();
        assert_eq!(doc.sheets().len(), 1);
    }

    #[test]
    fn entities_list_filters_by_the_active_sheet() {
        let (mut doc, _, _) = with_two_rects();
        doc.begin("Add Sheet");
        let second = doc.add_sheet(None);
        doc.commit();
        assert_eq!(doc.list(None).len(), 2);
        assert_eq!(doc.list(Some(second)).len(), 0);
    }

    #[test]
    fn selection_bbox_covers_everything_selected() {
        let (mut doc, a, b) = with_two_rects();
        doc.set_selection(&[a, b]);
        let state = doc.selection_state();
        assert_eq!(state.ids, vec![a, b]);
        let bbox = state.bbox.expect("two rectangles have a box");
        assert!((bbox[0] - 0.0).abs() < 1e-9);
        assert!((bbox[2] - 30.0).abs() < 1e-9);
    }

    #[test]
    fn an_empty_selection_has_no_bbox() {
        let doc = Document::new();
        assert!(doc.selection_state().bbox.is_none());
    }

    #[test]
    fn selection_keeps_the_order_ids_were_added_in() {
        let (mut doc, a, b) = with_two_rects();
        doc.set_selection(&[b]);
        doc.add_selection(&[a]);
        assert_eq!(doc.selection(), &[b, a]);
    }

    #[test]
    fn toggling_removes_what_is_already_selected() {
        let (mut doc, a, b) = with_two_rects();
        doc.set_selection(&[a]);
        doc.toggle_selection(&[a, b]);
        assert_eq!(doc.selection(), &[b]);
    }

    #[test]
    fn the_job_datum_follows_the_xy_origin() {
        let mut doc = Document::new();
        doc.begin("Job Setup");
        doc.set_job(JobSetup {
            xy_origin: XyOrigin::Centre,
            ..JobSetup::default()
        })
        .expect("valid job");
        doc.commit();
        let datum = doc.job_datum();
        assert!((datum.x - 150.0).abs() < 1e-9, "{datum:?}");
        let bounds = doc.job_bounds();
        assert!((bounds.min.x + 150.0).abs() < 1e-9, "{bounds:?}");
    }

    #[test]
    fn a_document_round_trips_through_its_file_shape() {
        let (doc, _, _) = with_two_rects();
        let file = doc.to_file();
        let text = serde_json::to_string(&file).expect("serialises");
        let parsed: DocumentFile = serde_json::from_str(&text).expect("parses");
        let back = Document::from_file(parsed, doc.revision()).expect("rebuilds");
        assert_eq!(back, doc);
    }

    #[test]
    fn a_newer_format_is_refused() {
        let mut file = Document::new().to_file();
        file.format = FORMAT_VERSION + 1;
        let error = Document::from_file(file, 0).expect_err("a newer format is refused");
        assert!(matches!(error, CoreError::Validation(_)), "{error}");
    }

    #[test]
    fn rollback_puts_a_half_finished_command_back() {
        let (mut doc, a, _) = with_two_rects();
        let before = doc.revision();
        let depth = doc.journal().undo_depth();
        doc.begin("Offset");
        doc.delete_entities(&[a]);
        doc.rollback();
        assert!(doc.entity(a).is_ok());
        assert_eq!(doc.revision(), before);
        assert_eq!(doc.journal().undo_depth(), depth);
        assert_eq!(doc.journal().undo_label(), Some("Create"));
    }

    #[test]
    fn replacing_an_entity_keeps_its_id_and_group() {
        let (mut doc, a, b) = with_two_rects();
        doc.begin("Group");
        let group = doc.group(&[a, b]).expect("group");
        doc.commit();
        doc.begin("Edit");
        let replacement = path_entity(
            &doc,
            circle(Point::new(0.0, 0.0), 4.0).expect("circle"),
            "C",
        );
        doc.replace_entity(a, replacement).expect("replaces");
        doc.commit();
        let entity = doc.entity(a).expect("entity");
        assert_eq!(entity.id, a);
        assert_eq!(entity.parent, Some(group));
    }

    #[test]
    fn deleting_a_group_member_leaves_the_group_consistent() {
        let (mut doc, a, b) = with_two_rects();
        doc.begin("Group");
        let group = doc.group(&[a, b]).expect("group");
        doc.commit();
        doc.begin("Delete");
        doc.delete_entities(&[a]);
        doc.commit();
        match &doc.entity(group).expect("group").kind {
            EntityKind::Group { children } => assert_eq!(children, &vec![b]),
            other => panic!("expected a group, got {other:?}"),
        }
    }
}
