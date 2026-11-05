//! Flattened display geometry and the binary reply that carries it.
//!
//! The 2D view never draws from a JSON copy of the document. It asks for
//! `entities.display`, which answers with one buffer of f32 x, y pairs plus a
//! small JSON header saying which run of that buffer belongs to which entity.
//! Flattened polylines are cached per entity, keyed by the entity revision
//! and the tolerance that was asked for, so panning and zooming at a steady
//! tolerance costs nothing.
//!
//! Wire format after the four byte `BLOB` tag: a little endian u32 header
//! length, the header as UTF-8 JSON, then the payload. Section offsets are
//! byte offsets from the start of the payload and are aligned to four bytes.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{Document, EntityKind};
use crate::geom::{flatten_path, Polyline, Tolerance};

/// One run of points inside the shared `xy` buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// Index of the first point, in points not floats.
    pub offset: u32,
    /// How many points the run has.
    pub count: u32,
}

/// What the header says about one entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayEntity {
    /// Entity id.
    pub id: u64,
    /// Entity revision, so the client can drop what it already has.
    pub revision: u64,
    /// Layer, which the renderer takes its colour from.
    pub layer: u64,
    /// `path`, `text`, `bitmap`, `dimension` or `group`.
    pub kind: String,
    /// True when every run is a closed loop.
    pub closed: bool,
    /// True when the entity, or the group holding it, is selected.
    pub selected: bool,
    /// Runs of this entity inside the shared buffer.
    pub runs: Vec<Run>,
    /// Image the bitmap quad belongs to, for bitmap entities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_id: Option<u64>,
    /// Opacity from 0 to 1, for bitmap entities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
}

/// One section of the payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    /// Name the client looks the section up by.
    pub name: String,
    /// `f32`, `u32`, `u16` or `u8`.
    pub dtype: String,
    /// Byte offset from the start of the payload.
    pub offset: usize,
    /// How many values the section holds.
    pub count: usize,
}

/// The JSON header of an `entities.display` reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayHeader {
    /// Sections of the payload.
    pub sections: Vec<Section>,
    /// Document revision this reply describes.
    pub revision: u64,
    /// Entities in z-order, back to front.
    pub entities: Vec<DisplayEntity>,
    /// Ids that no longer exist, when the client asked with `sinceRevision`.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub removed: Vec<u64>,
}

/// What a client asked `entities.display` for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DisplayArgs {
    /// Only these entities, when given.
    pub ids: Option<Vec<u64>>,
    /// Skip entities whose revision is no newer than this.
    pub since_revision: Option<u64>,
    /// Flattening tolerance in millimetres.
    pub tolerance: Option<f64>,
}

/// One entity's flattened geometry at one tolerance.
#[derive(Debug, Clone)]
struct Cached {
    revision: u64,
    tolerance: u64,
    polylines: Vec<Polyline>,
}

/// Flattened geometry, kept between calls so that a steady tolerance costs
/// nothing to redraw.
#[derive(Debug, Clone, Default)]
pub struct DisplayCache {
    entries: BTreeMap<u64, Cached>,
    /// Ids the last reply sent as selected, so that an incremental reply can
    /// resend the ones that have since been deselected.
    sent_selected: BTreeSet<u64>,
}

/// Tolerances are cached by their bit pattern so that a client asking for the
/// same value twice hits the cache.
fn tolerance_key(tol: Tolerance) -> u64 {
    tol.get().to_bits()
}

impl DisplayCache {
    /// An empty cache.
    pub fn new() -> Self {
        DisplayCache::default()
    }

    /// Forgets everything, which `doc.new` and `doc.open` do.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.sent_selected.clear();
    }

    /// Flattened polylines for one entity, reflattening only when the entity
    /// or the tolerance has changed.
    fn polylines(&mut self, doc: &Document, id: u64, tol: Tolerance) -> &[Polyline] {
        let (revision, path) = match doc.entities().get(&id) {
            Some(entity) => (entity.revision, entity.path()),
            None => (0, None),
        };
        let key = tolerance_key(tol);
        let stale = self
            .entries
            .get(&id)
            .is_none_or(|c| c.revision != revision || c.tolerance != key);
        if stale {
            let polylines = path
                .as_ref()
                .map(|p| flatten_path(p, tol))
                .unwrap_or_default();
            self.entries.insert(
                id,
                Cached {
                    revision,
                    tolerance: key,
                    polylines,
                },
            );
        }
        self.entries
            .get(&id)
            .map(|c| c.polylines.as_slice())
            .unwrap_or(&[])
    }

    /// Drops cache entries for entities that no longer exist.
    pub fn prune(&mut self, doc: &Document) {
        self.entries.retain(|id, _| doc.entities().contains_key(id));
    }

    /// Builds the binary reply for `entities.display`.
    pub fn build(&mut self, doc: &Document, args: &DisplayArgs) -> Vec<u8> {
        let tol = super::tolerance_or_default(args.tolerance);
        let since = args.since_revision;
        let wanted = args.ids.as_ref();
        let mut entities: Vec<DisplayEntity> = Vec::new();
        let mut xy: Vec<f32> = Vec::new();
        for id in doc.order() {
            if wanted.is_some_and(|ids| !ids.contains(id)) {
                continue;
            }
            let Some(entity) = doc.entities().get(id) else {
                continue;
            };
            if entity.sheet != doc.active_sheet() {
                continue;
            }
            let selected = doc.is_selected(*id);
            let leaves = doc.leaves(*id);
            let newest = leaves
                .iter()
                .filter_map(|leaf| doc.entities().get(leaf).map(|e| e.revision))
                .max()
                .unwrap_or(entity.revision)
                .max(entity.revision);
            // Selection is not part of an entity's revision, so a selected
            // entity is always resent, and so is one that was sent selected
            // last time: its outline has to change colour.
            let was_selected = self.sent_selected.contains(id);
            if since.is_some_and(|since| newest <= since) && !selected && !was_selected {
                continue;
            }
            let mut runs = Vec::new();
            let mut closed = true;
            let mut any = false;
            for leaf in &leaves {
                for polyline in self.polylines(doc, *leaf, tol).to_vec() {
                    if polyline.points.is_empty() {
                        continue;
                    }
                    any = true;
                    closed &= polyline.closed;
                    runs.push(Run {
                        offset: (xy.len() / 2) as u32,
                        count: polyline.points.len() as u32,
                    });
                    for point in &polyline.points {
                        xy.push(point.x as f32);
                        xy.push(point.y as f32);
                    }
                }
            }
            let (image_id, opacity) = match &entity.kind {
                EntityKind::Bitmap(bitmap) => (Some(bitmap.image_id), Some(bitmap.opacity)),
                _ => (None, None),
            };
            entities.push(DisplayEntity {
                id: *id,
                revision: newest,
                layer: entity.layer,
                kind: entity.kind.tag().to_string(),
                closed: any && closed,
                selected,
                runs,
                image_id,
                opacity,
            });
        }
        let removed = match since {
            Some(_) => self
                .entries
                .keys()
                .copied()
                .filter(|id| !doc.entities().contains_key(id))
                .collect(),
            None => Vec::new(),
        };
        self.prune(doc);
        for sent in &entities {
            if sent.selected {
                self.sent_selected.insert(sent.id);
            } else {
                self.sent_selected.remove(&sent.id);
            }
        }
        self.sent_selected
            .retain(|id| doc.entities().contains_key(id));
        let header = DisplayHeader {
            sections: vec![Section {
                name: "xy".to_string(),
                dtype: "f32".to_string(),
                offset: 0,
                count: xy.len(),
            }],
            revision: doc.revision(),
            entities,
            removed,
        };
        encode(&header, &xy)
    }
}

/// Frames a header and one f32 section into the body that follows the tag.
fn encode(header: &DisplayHeader, xy: &[f32]) -> Vec<u8> {
    let header_bytes = serde_json::to_vec(header).unwrap_or_else(|_| b"{\"sections\":[]}".to_vec());
    let mut out = Vec::with_capacity(4 + header_bytes.len() + xy.len() * 4);
    out.extend_from_slice(&(header_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&header_bytes);
    for value in xy {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// Reads a binary display reply back, which the tests and the shell use.
///
/// # Errors
/// Returns [`crate::CoreError::Validation`] when the buffer is truncated or
/// the header is not the JSON a display reply carries.
pub fn decode(body: &[u8]) -> Result<(DisplayHeader, Vec<f32>), crate::CoreError> {
    if body.len() < 4 {
        return Err(crate::CoreError::validation(
            "a binary reply needs at least a header length",
        ));
    }
    let mut length = [0u8; 4];
    length.copy_from_slice(&body[..4]);
    let header_len = u32::from_le_bytes(length) as usize;
    let end = 4 + header_len;
    if end > body.len() {
        return Err(crate::CoreError::validation(
            "the binary reply header runs past the end of the reply",
        ));
    }
    let header: DisplayHeader = serde_json::from_slice(&body[4..end])?;
    let payload = &body[end..];
    let section = header
        .sections
        .iter()
        .find(|s| s.name == "xy")
        .ok_or_else(|| crate::CoreError::validation("the reply has no xy section"))?;
    let bytes = section.count * 4;
    if section.offset + bytes > payload.len() {
        return Err(crate::CoreError::validation(
            "the xy section runs past the end of the reply",
        ));
    }
    let mut values = Vec::with_capacity(section.count);
    for chunk in payload[section.offset..section.offset + bytes].chunks_exact(4) {
        let mut raw = [0u8; 4];
        raw.copy_from_slice(chunk);
        values.push(f32::from_le_bytes(raw));
    }
    Ok((header, values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Entity, EntityKind};
    use crate::geom::{circle, rect, Point};

    fn document_with_a_rectangle() -> (Document, u64) {
        let mut doc = Document::new();
        doc.begin("Create");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: rect(Point::new(0.0, 0.0), Point::new(10.0, 20.0)).expect("rect"),
            },
        });
        doc.commit();
        (doc, id)
    }

    #[test]
    fn a_display_reply_decodes_to_the_points_that_were_flattened() {
        let (doc, id) = document_with_a_rectangle();
        let mut cache = DisplayCache::new();
        let body = cache.build(&doc, &DisplayArgs::default());
        let (header, xy) = decode(&body).expect("the reply decodes");
        assert_eq!(header.revision, doc.revision());
        assert_eq!(header.entities.len(), 1);
        let entity = &header.entities[0];
        assert_eq!(entity.id, id);
        assert_eq!(entity.kind, "path");
        assert!(entity.closed);
        assert!(!entity.selected);
        assert_eq!(entity.runs.len(), 1);
        assert_eq!(xy.len(), entity.runs[0].count as usize * 2);
        assert_eq!(xy.len() % 2, 0);
    }

    #[test]
    fn a_rectangle_flattens_to_its_four_corners() {
        let (doc, _) = document_with_a_rectangle();
        let mut cache = DisplayCache::new();
        let body = cache.build(&doc, &DisplayArgs::default());
        let (header, xy) = decode(&body).expect("decodes");
        assert_eq!(header.entities[0].runs[0].count, 4);
        assert!((xy[0] - 0.0).abs() < 1e-6, "{xy:?}");
    }

    #[test]
    fn a_circle_is_finer_at_a_finer_tolerance() {
        let mut doc = Document::new();
        doc.begin("Create");
        doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: circle(Point::new(0.0, 0.0), 50.0).expect("circle"),
            },
        });
        doc.commit();
        let mut cache = DisplayCache::new();
        let coarse = decode(&cache.build(
            &doc,
            &DisplayArgs {
                tolerance: Some(1.0),
                ..DisplayArgs::default()
            },
        ))
        .expect("decodes")
        .1
        .len();
        let fine = decode(&cache.build(
            &doc,
            &DisplayArgs {
                tolerance: Some(0.005),
                ..DisplayArgs::default()
            },
        ))
        .expect("decodes")
        .1
        .len();
        assert!(fine > coarse, "fine {fine} coarse {coarse}");
    }

    #[test]
    fn since_revision_skips_entities_that_have_not_changed() {
        let (doc, _) = document_with_a_rectangle();
        let mut cache = DisplayCache::new();
        let body = cache.build(
            &doc,
            &DisplayArgs {
                since_revision: Some(doc.revision()),
                ..DisplayArgs::default()
            },
        );
        let (header, xy) = decode(&body).expect("decodes");
        assert!(header.entities.is_empty());
        assert!(xy.is_empty());
    }

    #[test]
    fn a_selected_entity_is_always_resent() {
        let (mut doc, id) = document_with_a_rectangle();
        doc.set_selection(&[id]);
        let mut cache = DisplayCache::new();
        let body = cache.build(
            &doc,
            &DisplayArgs {
                since_revision: Some(doc.revision()),
                ..DisplayArgs::default()
            },
        );
        let (header, _) = decode(&body).expect("decodes");
        assert_eq!(header.entities.len(), 1);
        assert!(header.entities[0].selected);
    }

    #[test]
    fn a_deselected_entity_is_resent_once_as_unselected() {
        let (mut doc, id) = document_with_a_rectangle();
        doc.set_selection(&[id]);
        let mut cache = DisplayCache::new();
        let incremental = DisplayArgs {
            since_revision: Some(doc.revision()),
            ..DisplayArgs::default()
        };
        cache.build(&doc, &incremental);
        doc.clear_selection();
        let (header, _) = decode(&cache.build(&doc, &incremental)).expect("decodes");
        assert_eq!(header.entities.len(), 1);
        assert!(!header.entities[0].selected);
        let (header, _) = decode(&cache.build(&doc, &incremental)).expect("decodes");
        assert!(header.entities.is_empty());
    }

    #[test]
    fn asking_for_ids_leaves_the_rest_out() {
        let (mut doc, id) = document_with_a_rectangle();
        doc.begin("Create");
        let other = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: circle(Point::new(40.0, 40.0), 5.0).expect("circle"),
            },
        });
        doc.commit();
        let mut cache = DisplayCache::new();
        let body = cache.build(
            &doc,
            &DisplayArgs {
                ids: Some(vec![other]),
                ..DisplayArgs::default()
            },
        );
        let (header, _) = decode(&body).expect("decodes");
        assert_eq!(header.entities.len(), 1);
        assert_eq!(header.entities[0].id, other);
        assert_ne!(header.entities[0].id, id);
    }

    #[test]
    fn deleted_entities_are_reported_as_removed() {
        let (mut doc, id) = document_with_a_rectangle();
        let mut cache = DisplayCache::new();
        cache.build(&doc, &DisplayArgs::default());
        let before = doc.revision();
        doc.begin("Delete");
        doc.delete_entities(&[id]);
        doc.commit();
        let body = cache.build(
            &doc,
            &DisplayArgs {
                since_revision: Some(before),
                ..DisplayArgs::default()
            },
        );
        let (header, _) = decode(&body).expect("decodes");
        assert_eq!(header.removed, vec![id]);
    }

    #[test]
    fn a_group_reports_its_members_runs_under_one_id() {
        let mut doc = Document::new();
        doc.begin("Create");
        let a = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: rect(Point::new(0.0, 0.0), Point::new(5.0, 5.0)).expect("rect"),
            },
        });
        let b = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: rect(Point::new(10.0, 0.0), Point::new(15.0, 5.0)).expect("rect"),
            },
        });
        let group = doc.group(&[a, b]).expect("group");
        doc.commit();
        let mut cache = DisplayCache::new();
        let (header, _) = decode(&cache.build(&doc, &DisplayArgs::default())).expect("decodes");
        assert_eq!(header.entities.len(), 1);
        assert_eq!(header.entities[0].id, group);
        assert_eq!(header.entities[0].kind, "group");
        assert_eq!(header.entities[0].runs.len(), 2);
    }

    #[test]
    fn a_truncated_reply_is_reported() {
        assert!(decode(&[1, 2]).is_err());
        assert!(decode(&[255, 255, 0, 0]).is_err());
    }
}
