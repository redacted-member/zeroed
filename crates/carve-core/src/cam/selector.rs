//! Vector Selector: picks the vectors on the active sheet that pass a
//! geometry filter (open, closed, only circles of a diameter) and a layer
//! filter (all visible layers or named ones).
//!
//! A toolpath can keep its filter in `params.selector` with `associate`
//! set. Its vectors are then chosen again by the filter every time it is
//! calculated, so recalculating after drawing more vectors, or loading the
//! toolpath from a template into another job, machines whatever matches.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::doc::Document;
use crate::error::CoreError;
use crate::geom::{flatten_path, path_bbox, Path, Tolerance};

use super::compute::Request;

/// Tolerance circles are flattened to when checking their roundness, in
/// millimetres.
const ROUND_TOL: f64 = 0.01;

/// A filter the Vector Selector applies. Lengths are millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VectorFilter {
    /// Pick open vectors.
    pub open: bool,
    /// Pick closed vectors.
    pub closed: bool,
    /// Of the closed vectors, pick only circles.
    pub only_circles: bool,
    /// With `only_circles`, the diameter a circle must have; any diameter
    /// when absent.
    pub diameter: Option<f64>,
    /// How far a circle's diameter may be from `diameter`.
    pub tolerance: f64,
    /// Names of the layers to pick from; every visible layer when empty.
    /// Hidden layers are never picked from.
    pub layers: Vec<String>,
    /// Keep the filter with the toolpath, so it picks its vectors each
    /// time it is calculated (automatic selection).
    pub associate: bool,
}

impl Default for VectorFilter {
    fn default() -> Self {
        VectorFilter {
            open: true,
            closed: true,
            only_circles: false,
            diameter: None,
            tolerance: 0.1,
            layers: Vec::new(),
            associate: false,
        }
    }
}

/// Counts shown while the filter is edited.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selected {
    /// Top level objects picked, in z-order.
    pub ids: Vec<u64>,
    /// Closed vectors among them (a text or group counts each part).
    pub closed: usize,
    /// Open vectors among them.
    pub open: usize,
}

/// The diameter of `path` when it is a single closed loop whose points all
/// lie on one circle, within a hundredth of a millimetre or half a percent.
pub fn circle_diameter(path: &Path) -> Option<f64> {
    if path.subpaths.len() != 1 || !path.subpaths[0].closed {
        return None;
    }
    let bbox = path_bbox(path)?;
    let (w, h) = (bbox.max.x - bbox.min.x, bbox.max.y - bbox.min.y);
    if w <= 0.0 || (w - h).abs() > (0.005_f64 * w).max(ROUND_TOL) {
        return None;
    }
    let tol = Tolerance::new(ROUND_TOL).ok()?;
    let centre = bbox.min.lerp(bbox.max, 0.5);
    let radius = (w + h) / 4.0;
    let slack = (0.005_f64 * radius).max(ROUND_TOL);
    let lines = flatten_path(path, tol);
    let round = lines
        .iter()
        .flat_map(|l| l.points.iter())
        .all(|p| (p.distance(centre) - radius).abs() <= slack);
    round.then_some(2.0 * radius)
}

impl VectorFilter {
    fn check(&self) -> Result<(), CoreError> {
        if let Some(d) = self.diameter {
            if !(d.is_finite() && d > 0.0) {
                return Err(CoreError::validation(
                    "the circle diameter must be greater than zero",
                ));
            }
        }
        if !(self.tolerance.is_finite() && self.tolerance >= 0.0) {
            return Err(CoreError::validation("the tolerance cannot be negative"));
        }
        Ok(())
    }

    /// Whether one vector passes the geometry filter.
    fn keeps(&self, path: &Path) -> bool {
        let closed = path.is_closed();
        if !closed {
            return self.open;
        }
        if self.only_circles {
            return match (circle_diameter(path), self.diameter) {
                (Some(_), None) => true,
                (Some(found), Some(wanted)) => (found - wanted).abs() <= self.tolerance + 1e-9,
                (None, _) => false,
            };
        }
        self.closed
    }

    /// The objects on the active sheet that pass the filter.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for a bad diameter or tolerance.
    pub fn select(&self, doc: &Document) -> Result<Selected, CoreError> {
        self.check()?;
        let layers: Vec<u64> = doc
            .layers()
            .iter()
            .filter(|l| l.visible && (self.layers.is_empty() || self.layers.contains(&l.name)))
            .map(|l| l.id)
            .collect();
        let mut out = Selected {
            ids: Vec::new(),
            closed: 0,
            open: 0,
        };
        for summary in doc.list(None) {
            if !layers.contains(&summary.layer)
                || matches!(summary.kind.as_str(), "bitmap" | "dimension")
            {
                continue;
            }
            let parts: Vec<Path> = doc
                .paths_of(summary.id)
                .into_iter()
                .map(|(_, p)| p)
                .collect();
            // A text or group is one object, picked when any part passes.
            let kept: Vec<&Path> = parts.iter().filter(|p| self.keeps(p)).collect();
            if kept.is_empty() {
                continue;
            }
            out.ids.push(summary.id);
            for path in parts {
                if path.is_closed() {
                    out.closed += 1;
                } else {
                    out.open += 1;
                }
            }
        }
        Ok(out)
    }
}

/// The filter a request keeps for automatic selection, if any.
pub fn associated(params: &Value) -> Option<VectorFilter> {
    associated_at(params, "selector")
}

/// The associated filter a request keeps under `key`, if any.
fn associated_at(params: &Value, key: &str) -> Option<VectorFilter> {
    let value = params.get(key)?;
    let filter: VectorFilter = serde_json::from_value(value.clone()).ok()?;
    filter.associate.then_some(filter)
}

/// The request with its vectors picked by its associated filter, or as it
/// is when it has none. A moulding may keep a second filter,
/// `profileSelector`, for its profile: the profile is then the first
/// vector the second filter picks, placed last as the moulding reads it,
/// and the rails every other vector the first filter picks.
///
/// # Errors
/// [`CoreError::Validation`] for a bad filter.
pub fn resolve(doc: &Document, request: &Request) -> Result<Request, CoreError> {
    let mut resolved = request.clone();
    if let Some(filter) = associated(&request.params) {
        resolved.ids = filter.select(doc)?.ids;
        if let Some(profile) = associated_at(&request.params, "profileSelector") {
            if let Some(&found) = profile.select(doc)?.ids.first() {
                resolved.ids.retain(|id| *id != found);
                resolved.ids.push(found);
            }
        }
    }
    Ok(resolved)
}

/// `cam.selectVectors`: `{ filter }` answers `{ ids, closed, open }`.
///
/// # Errors
/// [`CoreError::InvalidArgs`] when the arguments do not parse, and the
/// errors of [`VectorFilter::select`].
pub fn select_command(doc: &Document, args: &str) -> Result<Value, CoreError> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(default)]
        filter: VectorFilter,
    }
    let args: Args = if args.trim().is_empty() || args.trim() == "null" {
        Args {
            filter: VectorFilter::default(),
        }
    } else {
        serde_json::from_str(args).map_err(|e| CoreError::invalid_args("cam.selectVectors", e))?
    };
    let picked = args.filter.select(doc)?;
    Ok(json!(picked))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Entity, EntityKind};
    use crate::geom::{circle, rect, Point};

    fn add(doc: &mut Document, layer: u64, path: Path) -> u64 {
        doc.begin("Create");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer,
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path },
        });
        doc.commit();
        id
    }

    fn open_line() -> Path {
        let mut path = rect(Point::new(0.0, 0.0), Point::new(5.0, 5.0)).expect("rect");
        path.subpaths[0].closed = false;
        path
    }

    #[test]
    fn circles_are_told_from_squares_and_measured() {
        let c = circle(Point::new(10.0, 10.0), 3.0).expect("circle");
        let d = circle_diameter(&c).expect("a circle");
        assert!((d - 6.0).abs() < 0.02, "{d}");
        let square = rect(Point::new(0.0, 0.0), Point::new(6.0, 6.0)).expect("rect");
        assert!(circle_diameter(&square).is_none());
    }

    #[test]
    fn the_geometry_filter_picks_open_closed_or_circles_of_a_size() {
        let mut doc = Document::new();
        let layer = doc.active_layer();
        let square = add(
            &mut doc,
            layer,
            rect(Point::new(0.0, 0.0), Point::new(6.0, 6.0)).expect("rect"),
        );
        let small = add(
            &mut doc,
            layer,
            circle(Point::new(20.0, 20.0), 2.0).expect("c"),
        );
        let big = add(
            &mut doc,
            layer,
            circle(Point::new(40.0, 20.0), 5.0).expect("c"),
        );
        let line = add(&mut doc, layer, open_line());

        let all = VectorFilter::default().select(&doc).expect("select");
        assert_eq!(all.ids, vec![square, small, big, line]);
        assert_eq!((all.closed, all.open), (3, 1));

        let open = VectorFilter {
            closed: false,
            ..VectorFilter::default()
        };
        assert_eq!(open.select(&doc).expect("select").ids, vec![line]);

        let circles = VectorFilter {
            open: false,
            only_circles: true,
            ..VectorFilter::default()
        };
        assert_eq!(circles.select(&doc).expect("select").ids, vec![small, big]);

        let four = VectorFilter {
            diameter: Some(4.0),
            tolerance: 0.1,
            ..circles
        };
        assert_eq!(four.select(&doc).expect("select").ids, vec![small]);
        let bad = VectorFilter {
            diameter: Some(-1.0),
            ..four
        };
        assert!(bad.select(&doc).is_err());
    }

    #[test]
    fn the_layer_filter_picks_named_visible_layers_only() {
        let mut doc = Document::new();
        let first = doc.active_layer();
        let a = add(
            &mut doc,
            first,
            rect(Point::new(0.0, 0.0), Point::new(6.0, 6.0)).expect("rect"),
        );
        doc.begin("Layer");
        let cuts = doc.add_layer(Some("Cuts".to_string()), None);
        doc.commit();
        let b = add(
            &mut doc,
            cuts,
            rect(Point::new(10.0, 0.0), Point::new(16.0, 6.0)).expect("rect"),
        );
        let named = VectorFilter {
            layers: vec!["Cuts".to_string()],
            ..VectorFilter::default()
        };
        assert_eq!(named.select(&doc).expect("select").ids, vec![b]);
        assert_eq!(
            VectorFilter::default().select(&doc).expect("select").ids,
            vec![a, b]
        );
        doc.begin("Hide");
        doc.update_layer(cuts, None, None, Some(false), None)
            .expect("hide");
        doc.commit();
        assert!(named.select(&doc).expect("select").ids.is_empty());
    }

    #[test]
    fn an_associated_filter_replaces_the_request_ids() {
        let mut doc = Document::new();
        let layer = doc.active_layer();
        let a = add(
            &mut doc,
            layer,
            rect(Point::new(0.0, 0.0), Point::new(6.0, 6.0)).expect("rect"),
        );
        let request = Request {
            strategy: super::super::compute::Strategy::Profile,
            name: None,
            tool: super::super::tool::Tool::default(),
            params: json!({ "selector": { "associate": true, "open": false } }),
            ids: vec![],
        };
        assert_eq!(resolve(&doc, &request).expect("resolve").ids, vec![a]);
        let manual = Request {
            params: json!({ "selector": { "associate": false } }),
            ids: vec![99],
            ..request
        };
        assert_eq!(resolve(&doc, &manual).expect("resolve").ids, vec![99]);
    }

    #[test]
    fn a_moulding_profile_filter_puts_its_vector_last() {
        let mut doc = Document::new();
        let rails = doc.active_layer();
        let rail = add(
            &mut doc,
            rails,
            rect(Point::new(0.0, 0.0), Point::new(60.0, 40.0)).expect("rect"),
        );
        let profile = add(&mut doc, rails, open_line());
        let request = Request {
            strategy: super::super::compute::Strategy::Moulding,
            name: None,
            tool: super::super::tool::Tool::default(),
            params: json!({
                "selector": { "associate": true },
                "profileSelector": { "associate": true, "closed": false },
            }),
            ids: vec![],
        };
        // The rail filter picks both; the profile filter's open vector goes last.
        assert_eq!(
            resolve(&doc, &request).expect("resolve").ids,
            vec![rail, profile]
        );
        let rails_only = Request {
            params: json!({
                "selector": { "associate": true, "open": false },
                "profileSelector": { "associate": true, "closed": false },
            }),
            ..request.clone()
        };
        assert_eq!(
            resolve(&doc, &rails_only).expect("resolve").ids,
            vec![rail, profile]
        );
        let manual_profile = Request {
            params: json!({
                "selector": { "associate": true },
                "profileSelector": { "associate": false, "closed": false },
            }),
            ..request
        };
        assert_eq!(
            resolve(&doc, &manual_profile).expect("resolve").ids,
            vec![rail, profile]
        );
    }

    #[test]
    fn the_command_answers_ids_and_counts() {
        let mut doc = Document::new();
        let layer = doc.active_layer();
        add(&mut doc, layer, open_line());
        let reply = select_command(&doc, r#"{"filter":{"closed":false}}"#).expect("reply");
        assert_eq!(reply["open"], 1);
        assert!(select_command(&doc, "{").is_err());
    }
}
