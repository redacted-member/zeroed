//! The command entry point the shell talks to.
//!
//! Replies follow the kernel API contract: a JSON envelope
//! `{"ok": true, "result": ...}` when the command succeeded, or
//! `{"ok": false, "error": {"code", "message"}}` when it did not. Commands
//! that change the document merge the new document `revision` into the result.
//!
//! Every mutating command opens an undo step before it touches anything and
//! commits at the end. A command that fails part way rolls back, so a refused
//! command never leaves the document half changed.
//!
//! Argument fields and the segments of command names are camelCase, as are
//! the fields of every reply.

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::cam::commands::CamState;
use crate::cam::sim::Stock;
use crate::doc::display::DisplayCache;
use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::FontLibrary;
use crate::{assembly, cam, clipart, doc, gadgets, io, job_scale, model, prefs, templates};

/// What a command answers with. JSON for structured replies, bytes for large
/// buffers such as display polylines or heightfield tiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelReply {
    /// UTF-8 JSON, already wrapped in the reply envelope.
    Json(String),
    /// Raw little endian buffer with its own header.
    Bytes(Vec<u8>),
}

impl KernelReply {
    /// Wraps a serialisable value in the success envelope.
    ///
    /// # Errors
    /// Returns [`CoreError::Encode`] if the value cannot be serialised.
    pub fn ok<T: Serialize>(result: &T) -> Result<Self, CoreError> {
        let envelope = json!({ "ok": true, "result": serde_json::to_value(result)? });
        Ok(KernelReply::Json(envelope.to_string()))
    }

    /// The bytes a host sends back: a four byte ASCII tag, `JSON` or `BLOB`,
    /// followed by the payload, as the kernel API contract describes.
    pub fn into_framed(self) -> Vec<u8> {
        let (tag, payload): (&[u8; 4], Vec<u8>) = match self {
            KernelReply::Json(text) => (b"JSON", text.into_bytes()),
            KernelReply::Bytes(bytes) => (b"BLOB", bytes),
        };
        let mut framed = Vec::with_capacity(4 + payload.len());
        framed.extend_from_slice(tag);
        framed.extend_from_slice(&payload);
        framed
    }

    /// Wraps an error in the failure envelope.
    pub fn error(error: &CoreError) -> Self {
        let envelope = json!({
            "ok": false,
            "error": { "code": error.code(), "message": error.to_string() },
        });
        KernelReply::Json(envelope.to_string())
    }
}

/// Owns the open document and dispatches commands by name.
#[derive(Debug)]
pub struct Kernel {
    pub(crate) document: Document,
    pub(crate) display: DisplayCache,
    pub(crate) fonts: FontLibrary,
    pub(crate) cam: CamState,
    /// The toolpath preview, once one has been run.
    pub(crate) stock: Option<Stock>,
    /// The preview as it was before the last preview or waste removal, for
    /// Undo Last: `Some(None)` when there was none.
    pub(crate) stock_undo: Option<Option<Stock>>,
    /// How far an animated preview has got.
    pub(crate) playback: Option<crate::cam::playback::Playback>,
    /// Each sculpted component as it was before its first stroke since
    /// sculpting began, for the undo brush. Not saved.
    pub(crate) sculpt_baselines: model::sculpt::SculptSession,
    /// The sculpting brush file read last. Not saved.
    pub(crate) sculpt_brushes: model::brush_file::BrushCache,
    /// The mesh file the import flow read last. Not saved.
    pub(crate) mesh_cache: model::import::MeshCache,
    /// The Assembly View's parts of each sheet, found last. Not saved.
    pub(crate) pieces: assembly::pieces::PartsCache,
    /// The user's preferences.
    pub(crate) prefs: crate::prefs::Prefs,
    /// What Copy and Cut kept last. Not saved.
    pub(crate) clipboard: doc::clipboard_cmd::Clipboard,
    /// Where the maker's furniture templates live.
    pub(crate) furniture: crate::furniture::library::Library,
}

impl Default for Kernel {
    fn default() -> Self {
        Kernel::new()
    }
}

impl Kernel {
    /// Creates a kernel holding a new empty document, with the tool database
    /// and posts in the user's data directory.
    pub fn new() -> Self {
        let mut kernel =
            Kernel::with_cam(CamState::default()).with_prefs(crate::prefs::Prefs::user());
        kernel.furniture = crate::furniture::library::Library::user_default();
        kernel
    }

    /// Creates a kernel whose tool database and posts live where `cam` says,
    /// which is how tests keep away from the user's files.
    pub fn with_cam(cam: CamState) -> Self {
        Kernel {
            document: Document::new(),
            display: DisplayCache::new(),
            fonts: FontLibrary::system(),
            cam,
            stock: None,
            stock_undo: None,
            playback: None,
            sculpt_baselines: model::sculpt::SculptSession::default(),
            sculpt_brushes: model::brush_file::BrushCache::default(),
            mesh_cache: model::import::MeshCache::default(),
            pieces: assembly::pieces::PartsCache::default(),
            prefs: crate::prefs::Prefs::in_memory(),
            clipboard: doc::clipboard_cmd::Clipboard::default(),
            furniture: crate::furniture::library::Library::default(),
        }
    }

    /// Uses `prefs` and applies them to the open document.
    pub fn with_prefs(mut self, prefs: crate::prefs::Prefs) -> Self {
        prefs.values().apply_to_new_document(&mut self.document);
        self.prefs = prefs;
        self
    }

    /// The user's preferences.
    pub fn prefs(&self) -> &crate::prefs::Prefs {
        &self.prefs
    }

    /// The tool database, post library and calculated toolpaths.
    pub fn cam(&self) -> &CamState {
        &self.cam
    }

    /// The CAM state, for hosts and toolpath commands that add toolpaths.
    pub fn cam_mut(&mut self) -> &mut CamState {
        &mut self.cam
    }

    /// The open document.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The open document, for hosts that drive the kernel directly.
    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    /// The fonts text commands lay out with.
    pub fn fonts(&self) -> &FontLibrary {
        &self.fonts
    }

    /// Runs one command and always answers with an envelope, so that hosts do
    /// not have to encode errors themselves. A panic inside the command is
    /// caught and answered as an `internal` error, so a damaged file or a
    /// kernel fault never takes the host down. While it runs,
    /// [`crate::progress`] says so, and a host may ask it to stop.
    pub fn call(&mut self, name: &str, args: &str) -> KernelReply {
        crate::progress::begin();
        let applied =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.apply(name, args)));
        crate::progress::end();
        match applied {
            Ok(Ok(reply)) => reply,
            Ok(Err(error)) => KernelReply::error(&error),
            Err(payload) => {
                // Whatever the command changed before it failed goes back,
                // so the next command does not commit it under its name.
                self.document.rollback();
                let message = if let Some(text) = payload.downcast_ref::<&str>() {
                    (*text).to_string()
                } else if let Some(text) = payload.downcast_ref::<String>() {
                    text.clone()
                } else {
                    "a panic without a message".to_string()
                };
                KernelReply::error(&CoreError::Internal(format!("{name} failed: {message}")))
            }
        }
    }

    /// Runs one command. `args` is a JSON string, empty or `null` when the
    /// command takes no arguments.
    ///
    /// The command tables of `docs/KERNEL_API.md` list every name and
    /// its arguments. Commands that change the document merge the new
    /// `revision` into their result.
    ///
    /// # Errors
    /// [`CoreError::UnknownCommand`] for an unknown name,
    /// [`CoreError::InvalidArgs`] if `args` does not parse,
    /// [`CoreError::Validation`] if the arguments are not legal values, and
    /// whatever the geometry or file layers report for the rest.
    pub fn apply(&mut self, name: &str, args: &str) -> Result<KernelReply, CoreError> {
        let before = self.document.revision();
        let (side, job) = (self.document.active_side(), *self.document.job());
        self.document.take_copies();
        let steps = self.document.steps_committed();
        let thicknesses = crate::joints::commands::joint_thicknesses(&self.document);
        let reply = self.run(name, args)?;
        let stepped = self.document.steps_committed() != steps;
        self.turn_preview(side, &job);
        let copies = self.document.take_copies();
        if self.document.revision() != before && name != "undo" && name != "redo" {
            doc::text_cmd::relink_copies(&mut self.document, &copies);
            doc::nest_cmd::carry_marks(&mut self.document, name, &copies);
            doc::text_cmd::follow_curves(&mut self.document, &self.fonts);
            if stepped && !name.starts_with("joints.") && !name.starts_with("furniture.") {
                crate::joints::commands::follow_moved_parts(&mut self.document);
                crate::joints::commands::follow_thickness(&mut self.document, &thicknesses);
            }
            if stepped && name.starts_with("cam.") {
                crate::joints::commands::seat_after_toolpaths(&mut self.document, &mut self.pieces);
            }
        }
        Ok(reply)
    }

    /// Turns the toolpath preview over when a command showed the other side
    /// of the same double sided job, so the preview is seen from the side
    /// shown; `side` and `job` are as they were before the command.
    fn turn_preview(&mut self, side: crate::doc::Side, job: &crate::doc::JobSetup) {
        if self.document.active_side() == side
            || !job.is_double_sided()
            || self.document.job() != job
        {
            return;
        }
        if let Some(stock) = self.stock.as_mut() {
            *stock = stock.turned(job, side);
        }
        if let Some(Some(stock)) = self.stock_undo.as_mut() {
            *stock = stock.turned(job, side);
        }
    }

    fn run(&mut self, name: &str, args: &str) -> Result<KernelReply, CoreError> {
        #[cfg(test)]
        if name == "test.panic" {
            panic!("the test asked for a panic");
        }
        #[cfg(test)]
        if name == "test.panicMidway" {
            // A step opened and a change made, then the panic.
            self.document.begin("Midway");
            if let Some(first) = self.document.order().first().copied() {
                let entity = self.document.entity(first)?.clone();
                self.document.insert_entity(entity);
            }
            panic!("the test asked for a panic part way");
        }
        // Each command module answers for its own names; the first module
        // that claims a name runs it.
        if cam::library_cmd::handles(name) {
            return cam::library_cmd::dispatch(
                &mut self.cam,
                &self.document,
                &self.prefs,
                name,
                args,
            );
        }
        if prefs::handles(name) {
            return prefs::dispatch(&mut self.prefs, &mut self.document, name, args);
        }
        if templates::handles(name) {
            return templates::dispatch(self, name, args);
        }
        if job_scale::handles(name) {
            return job_scale::dispatch(&mut self.document, name, args);
        }
        if model::import::handles(name) {
            return model::import::dispatch(&mut self.document, &mut self.mesh_cache, name, args);
        }
        if model::tree_cmd::handles(name) {
            return model::tree_cmd::dispatch(&mut self.document, name, args);
        }
        if io::commands::handles(name) {
            return io::commands::dispatch(&self.document, name, args);
        }
        if cam::list_cmd::handles(name) {
            return cam::list_cmd::dispatch(&mut self.document, name, args);
        }
        if cam::playback::handles(name) {
            return cam::playback::dispatch(
                &self.document,
                &mut self.stock,
                &mut self.stock_undo,
                &mut self.playback,
                name,
                args,
            );
        }
        if cam::job_sheet::handles(name) {
            return cam::job_sheet::dispatch(&self.document, name, args);
        }
        if assembly::pieces_cmd::handles(name) {
            return assembly::pieces_cmd::dispatch(
                &mut self.document,
                &mut self.pieces,
                name,
                args,
            );
        }
        if crate::furniture::handles(name) {
            return crate::furniture::dispatch(&mut self.document, &mut self.furniture, name, args);
        }
        if crate::joints::commands::handles(name) {
            return crate::joints::commands::dispatch(
                &mut self.document,
                &mut self.pieces,
                name,
                args,
            );
        }
        if assembly::commands::handles(name) {
            return assembly::commands::dispatch(&mut self.document, name, args);
        }
        if doc::bitmap_cmd::handles(name) {
            return doc::bitmap_cmd::dispatch(&mut self.document, name, args);
        }
        if doc::texture_cmd::handles(name) {
            return doc::texture_cmd::dispatch(&mut self.document, name, args);
        }
        if doc::picture_cmd::handles(name) {
            return doc::picture_cmd::dispatch(&mut self.document, name, args);
        }
        if doc::smart_select_cmd::handles(name) {
            return doc::smart_select_cmd::dispatch(&mut self.document, name, args);
        }
        if doc::drawtools_cmd::handles(name) {
            return doc::drawtools_cmd::dispatch(&mut self.document, name, args);
        }
        if doc::rotary_cmd::handles(name) {
            return doc::rotary_cmd::dispatch(&mut self.document, name, args);
        }
        if doc::distort_cmd::handles(name) {
            return doc::distort_cmd::dispatch(&mut self.document, name, args);
        }
        if doc::bitmap_relief_cmd::handles(name) {
            return doc::bitmap_relief_cmd::dispatch(&mut self.document, name, args);
        }
        if model::tools_cmd::handles(name) {
            return model::tools_cmd::dispatch(&mut self.document, self.stock.as_ref(), name, args);
        }
        if model::commands::handles(name) {
            return model::commands::dispatch(&mut self.document, name, args);
        }
        if gadgets::handles(name) {
            return gadgets::dispatch(self, name, args);
        }
        if clipart::handles(name) {
            return clipart::dispatch(&mut self.document, name, args);
        }
        if doc::document_cmd::handles(name) {
            return doc::document_cmd::dispatch(self, name, args);
        }
        if doc::layers_cmd::handles(name) {
            return doc::layers_cmd::dispatch(&mut self.document, name, args);
        }
        if doc::entities_cmd::handles(name) {
            return doc::entities_cmd::dispatch(&mut self.document, &mut self.display, name, args);
        }
        if doc::clipboard_cmd::handles(name) {
            return doc::clipboard_cmd::dispatch(
                &mut self.document,
                &mut self.clipboard,
                name,
                args,
            );
        }
        if doc::ops_cmd::handles(name) {
            return doc::ops_cmd::dispatch(&mut self.document, name, args);
        }
        if doc::text_cmd::handles(name) {
            return doc::text_cmd::dispatch(&mut self.document, &self.fonts, name, args);
        }
        if doc::sides_cmd::handles(name) {
            return doc::sides_cmd::dispatch(&self.document, name, args);
        }
        if cam::import_cmd::handles(name) {
            return cam::import_cmd::dispatch(&mut self.document, name, args);
        }
        if cam::tiling_cmd::handles(name) {
            return cam::tiling_cmd::dispatch(&mut self.document, name, args);
        }
        if cam::toolpath_cmd::handles(name) {
            return cam::toolpath_cmd::dispatch(&mut self.document, name, args);
        }
        if cam::preview_cmd::handles(name) {
            return cam::preview_cmd::dispatch(
                &self.document,
                &mut self.stock,
                &mut self.stock_undo,
                name,
                args,
            );
        }
        if model::create_cmd::handles(name) {
            return model::create_cmd::dispatch(&mut self.document, name, args);
        }
        if model::component_cmd::handles(name) {
            return model::component_cmd::dispatch(
                &mut self.document,
                &mut self.sculpt_baselines,
                &mut self.sculpt_brushes,
                name,
                args,
            );
        }
        if io::vectors_cmd::handles(name) {
            return io::vectors_cmd::dispatch(&mut self.document, Some(&self.fonts), name, args);
        }
        if name == "ping" {
            return KernelReply::ok(&json!({ "pong": true, "version": crate::VERSION }));
        }
        Err(CoreError::UnknownCommand(name.to_string()))
    }
}

/// Runs one mutating command inside an undo step, rolling back and leaving
/// the document untouched when it fails. The reply carries the new revision.
pub(crate) fn edit<F>(
    document: &mut Document,
    label: &str,
    action: F,
) -> Result<KernelReply, CoreError>
where
    F: FnOnce(&mut Document) -> Result<Value, CoreError>,
{
    document.begin(label);
    match action(document) {
        Ok(result) => {
            let revision = document.commit();
            KernelReply::ok(&with_revision(result, revision))
        }
        Err(error) => {
            document.rollback();
            Err(error)
        }
    }
}

/// A reply holding only the document revision.
pub(crate) fn revision_reply(document: &Document) -> Result<KernelReply, CoreError> {
    KernelReply::ok(&json!({ "revision": document.revision() }))
}

/// Merges the document revision into a result object.
fn with_revision(mut value: Value, revision: u64) -> Value {
    match value.as_object_mut() {
        Some(object) => {
            object.insert("revision".to_string(), json!(revision));
        }
        None => {
            let mut object = Map::new();
            object.insert("result".to_string(), value);
            object.insert("revision".to_string(), json!(revision));
            value = Value::Object(object);
        }
    }
    value
}

/// Reads optional settings, where `null` means the defaults.
pub(crate) fn parse_value<T: DeserializeOwned + Default>(value: &Value) -> Result<T, CoreError> {
    if value.is_null() {
        return Ok(T::default());
    }
    serde_json::from_value(value.clone())
        .map_err(|error| CoreError::validation(format!("the settings do not read: {error}")))
}

/// Reads a command's JSON arguments; no arguments reads as `{}`.
pub(crate) fn parse_args<T: DeserializeOwned>(command: &str, args: &str) -> Result<T, CoreError> {
    // No arguments reads as an empty object, so commands whose fields all
    // have defaults accept an empty string, `null` or `{}` alike.
    let trimmed = args.trim();
    let text = if trimmed.is_empty() || trimmed == "null" {
        "{}"
    } else {
        args
    };
    serde_json::from_str(text).map_err(|error| CoreError::invalid_args(command, error))
}

// ------------------------------------- argument types several modules share

/// `{ path }`: a file to read or write.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PathArgs {
    pub(crate) path: PathBuf,
}

/// `{ ids }`: the objects to act on, often the selection when left out.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct IdsArgs {
    pub(crate) ids: Option<Vec<u64>>,
}

/// `{ id }`: one object.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdArgs {
    pub(crate) id: u64,
}

/// `{ id, name }`: a new name for one object.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RenameArgs {
    pub(crate) id: u64,
    pub(crate) name: String,
}

#[cfg(test)]
mod tests;
