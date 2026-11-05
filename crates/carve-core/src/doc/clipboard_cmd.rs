//! The in-app clipboard: `clipboard.copy` keeps copies of the chosen
//! objects (vectors, text, bitmaps, groups with everything under them, and
//! relief components) outside the document, and `clipboard.paste` puts new
//! copies of them back, so a cut and paste works after the originals are
//! gone. Pasted components keep their settings: a shape pasted with the
//! vectors it was made from is made from the pasted vectors. One pasted
//! without them keeps its link to the originals only while they are in
//! the same job and as they were copied: pasted into another job, or after
//! they were deleted or changed, it is a plain component that stays where
//! it is, so editing it again cannot build it on unrelated vectors or move
//! it. Pasted into another job, a component also leaves its group and
//! texture seed behind (their ids mean something else there) and goes on
//! the active level.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::json;

use crate::doc::{Document, Entity, EntityKind};
use crate::error::CoreError;
use crate::geom::{Mat3, Vec2};
use crate::kernel::{edit, parse_args, KernelReply};
use crate::model::component_cmd::copy_of;
use crate::model::Component;

/// What was copied last. Not saved with the document.
#[derive(Debug, Default, Clone)]
pub struct Clipboard {
    /// Every copied entity, groups and everything under them, by its id
    /// when it was copied.
    entities: BTreeMap<u64, Entity>,
    /// The copied top level entities, in the order they were chosen.
    roots: Vec<u64>,
    /// The copied relief components.
    components: Vec<Component>,
    /// The vectors (leaves) the copied components are made from, as they
    /// were when copied.
    linked: BTreeMap<u64, EntityKind>,
    /// Another job was opened or started since the copy, so ids the copies
    /// refer to outside the clipboard mean nothing there.
    foreign: bool,
}

impl Clipboard {
    /// How many objects a paste would add: top level entities and
    /// components.
    pub fn count(&self) -> usize {
        self.roots.len() + self.components.len()
    }

    /// Records that the document was replaced (New, Open, a template): a
    /// later paste goes into another job.
    pub fn left_job(&mut self) {
        self.foreign = true;
    }

    /// Drops the Create Shape link of a pasted component `copy` when it
    /// points at vectors that were not pasted with it and are gone or were
    /// in another job (`pasted` holds the ids of the copies just made).
    fn unlink_strays(&self, doc: &Document, copy: &mut Component, pasted: &BTreeMap<u64, u64>) {
        let changed = |id: u64| {
            doc.entity(id).is_err()
                || doc
                    .leaves(id)
                    .iter()
                    .any(|leaf| doc.entity(*leaf).ok().map(|e| &e.kind) != self.linked.get(leaf))
        };
        let stray = copy.extra.shape.as_deref().is_some_and(|shape| {
            shape.all_vectors().iter().any(|id| {
                let ours = pasted.values().any(|v| v == id);
                !ours && (self.foreign || changed(*id))
            })
        });
        if stray {
            copy.extra.shape = None;
        }
    }

    fn keep(&mut self, document: &Document, id: u64) -> Result<(), CoreError> {
        let entity = document.entity(id)?.clone();
        if let EntityKind::Group { children } = &entity.kind {
            for child in children {
                self.keep(document, *child)?;
            }
        }
        self.entities.insert(id, entity);
        Ok(())
    }

    /// Inserts a copy of `id` and everything under it on `layer` and
    /// `sheet`, recording each copy in `copies`.
    fn put(
        &self,
        doc: &mut Document,
        id: u64,
        layer: u64,
        sheet: u64,
        copies: &mut BTreeMap<u64, u64>,
    ) -> Result<u64, CoreError> {
        let source = self
            .entities
            .get(&id)
            .ok_or_else(|| CoreError::validation("the clipboard lost an object"))?;
        let copy = match &source.kind {
            EntityKind::Group { children } => {
                let mut members = Vec::with_capacity(children.len());
                for child in children {
                    members.push(self.put(doc, *child, layer, sheet, copies)?);
                }
                let group = doc.group_copies(&members)?;
                let mut entity = doc.entity(group)?.clone();
                entity.name.clone_from(&source.name);
                entity.rotation = source.rotation;
                doc.replace_entity(group, entity)?;
                group
            }
            _ => {
                let mut entity = source.clone();
                entity.id = 0;
                entity.parent = None;
                entity.layer = layer;
                entity.sheet = sheet;
                doc.insert_entity(entity)
            }
        };
        copies.insert(id, copy);
        doc.note_copy(id, copy);
        Ok(copy)
    }
}

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "clipboard.copy" | "clipboard.paste" | "clipboard.state"
    )
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and
/// [`CoreError::Validation`] for unknown objects or an empty clipboard.
pub fn dispatch(
    document: &mut Document,
    clipboard: &mut Clipboard,
    name: &str,
    args: &str,
) -> Result<KernelReply, CoreError> {
    match name {
        "clipboard.copy" => {
            let args: CopyArgs = parse_args(name, args)?;
            if args.ids.is_empty() && args.components.is_empty() {
                return Err(CoreError::validation("select the objects to copy"));
            }
            let mut next = Clipboard::default();
            for id in &args.ids {
                if !next.roots.contains(id) {
                    next.keep(document, *id)?;
                    next.roots.push(*id);
                }
            }
            for id in &args.components {
                let component = document.component(*id)?.clone();
                if let Some(shape) = component.extra.shape.as_deref() {
                    for vector in shape.all_vectors() {
                        for leaf in document.leaves(vector) {
                            if let Ok(entity) = document.entity(leaf) {
                                next.linked.insert(leaf, entity.kind.clone());
                            }
                        }
                    }
                }
                next.components.push(component);
            }
            *clipboard = next;
            KernelReply::ok(&json!({ "count": clipboard.count() }))
        }
        "clipboard.paste" => {
            let args: PasteArgs = parse_args(name, args)?;
            if clipboard.count() == 0 {
                return Err(CoreError::validation("there is nothing to paste"));
            }
            if !args.dx.is_finite() || !args.dy.is_finite() {
                return Err(CoreError::validation("the paste offset must be a number"));
            }
            let m = Mat3::translate(Vec2::new(args.dx, args.dy));
            let (layer, sheet) = (document.active_layer(), document.active_sheet());
            let level = document.active_level();
            let board = &*clipboard;
            edit(document, "Paste", |doc| {
                let mut map = BTreeMap::new();
                let mut ids = Vec::with_capacity(board.roots.len());
                for id in &board.roots {
                    ids.push(board.put(doc, *id, layer, sheet, &mut map)?);
                }
                doc.transform_entities(&ids, &m);
                let mut copies = Vec::with_capacity(board.components.len());
                for component in &board.components {
                    let mut source = component.clone();
                    if board.foreign {
                        source.extra.texture = None;
                        source.extra.group = 0;
                        source.level = level;
                    }
                    let mut copy = copy_of(doc, &source, &m, &map)?;
                    board.unlink_strays(doc, &mut copy, &map);
                    copy.extra.sheet = sheet;
                    copies.push(copy);
                }
                let components: Vec<u64> =
                    copies.into_iter().map(|c| doc.add_component(c)).collect();
                doc.set_selection(&ids);
                Ok(json!({ "ids": ids, "components": components }))
            })
        }
        "clipboard.state" => KernelReply::ok(&json!({ "count": clipboard.count() })),
        _ => Err(CoreError::UnknownCommand(name.to_string())),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CopyArgs {
    /// Top level entities.
    #[serde(default)]
    ids: Vec<u64>,
    /// Relief components.
    #[serde(default)]
    components: Vec<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasteArgs {
    /// Move of the copies from where the originals were, in millimetres.
    #[serde(default)]
    dx: f64,
    #[serde(default)]
    dy: f64,
}
