//! Generated command sequences: a seeded driver runs long random sequences
//! of real commands (drawing, transforms, booleans, offsets, text, nesting,
//! layers, sheets, the relief model, toolpaths, joints, assembly mates,
//! variables) interleaved with Undo, Redo, Save and reopen and a units
//! switch, and checks after every step that nothing was lost:
//! - a refused command leaves the document exactly as it was;
//! - Undo puts back exactly the document before the command, Redo exactly
//!   the one after it;
//! - Save then reopen gives back exactly the saved document;
//! - undoing everything gives back the empty job (or the job as opened),
//!   and redoing everything the job again;
//! - no command panics; no vector, toolpath move or component placement
//!   holds NaN or an infinite coordinate;
//! - every toolpath that is not stale recalculates to the same moves, before
//!   and after a reopen;
//! - the selection names only objects that exist.
//!
//! A failing sequence is shrunk to a minimal one and printed as the list of
//! commands to replay. The gate runs a few fixed seeds; `long_generated_run`
//! (ignored) runs as many as `CARVE_SEQ_SEEDS` asks for.

use super::*;
use crate::geom::{circle, FontLibrary};

/// A small deterministic generator (xorshift64*), so a seed replays.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        let unit = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        // Round to a hundredth so a printed sequence replays exactly.
        ((lo + unit * (hi - lo)) * 100.0).round() / 100.0
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        if items.is_empty() {
            None
        } else {
            Some(items[self.below(items.len())])
        }
    }
}

/// Per command: how often it ran, was refused, and the last refusal.
type Stats = std::collections::BTreeMap<String, (usize, usize, String)>;

/// One step of a sequence: a kernel command, or one of the driver's own
/// steps named with a leading `@` (save and reopen, units switch).
#[derive(Debug, Clone)]
struct Op {
    name: String,
    args: Value,
}

impl Op {
    fn new(name: &str, args: Value) -> Op {
        Op {
            name: name.to_string(),
            args,
        }
    }
}

/// What went wrong, at which step.
#[derive(Debug, Clone)]
struct Failure {
    step: usize,
    kind: &'static str,
    detail: String,
}

/// The document as a comparable text: `document.json` as saved, each
/// relief component's heights and each placed image's bytes as a hash, and
/// the selection when `selection` (it is not saved).
fn canon(doc: &Document, selection: bool) -> Value {
    let file = doc.to_file();
    let mut fields = Vec::new();
    let bottom = file.bottom.iter().flat_map(|s| s.components.iter());
    for component in file.components.iter().chain(bottom) {
        fields.push(json!([component.id, hash(&component.field.z_bytes())]));
    }
    let images: Vec<Value> = crate::doc::images::referenced(&file)
        .into_iter()
        .map(|id| json!([id, doc.images().get(id).map(|i| hash(&i.bytes))]))
        .collect();
    let mut value = json!({
        "file": serde_json::to_value(&file).expect("the document serialises"),
        "heights": fields,
        "images": images,
    });
    if selection {
        value["selection"] = json!(doc.selection());
    }
    value
}

fn hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Where two canonical documents first differ, as a JSON path with both
/// values, short enough to read.
fn first_difference(a: &Value, b: &Value, at: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter().find_map(|k| {
                let (p, q) = (
                    x.get(k).unwrap_or(&Value::Null),
                    y.get(k).unwrap_or(&Value::Null),
                );
                first_difference(p, q, &format!("{at}.{k}"))
            })
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (p, q))| first_difference(p, q, &format!("{at}[{i}]"))),
        _ if a == b => None,
        _ => {
            let short = |v: &Value| {
                let text = v.to_string();
                if text.len() > 300 {
                    format!("{}...", &text[..300])
                } else {
                    text
                }
            };
            Some(format!("{at}: {} vs {}", short(a), short(b)))
        }
    }
}

/// Takes out the id counters and the side of a double sided job shown.
/// Counters only move forward, so Undo leaves them where they are by design
/// (an undone create does not hand its id out again); showing a side is a
/// view change, not an edit, and Undo and Redo turn to the side of the step
/// they take back.
fn without_counters(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| {
                    !(k.starts_with("next") && k.ends_with("Id")) && *k != "activeSide"
                })
                .map(|(k, v)| (k.clone(), without_counters(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_counters).collect()),
        other => other.clone(),
    }
}

/// Takes out what is not an undo step by design, for comparing the job
/// with everything undone against the job as it began: the selection, the
/// guides, the dimension style, the side shown, and an other side that has
/// been shown but holds nothing.
fn view_free(value: &Value) -> Value {
    let mut value = side_free(value);
    if let Some(map) = value.as_object_mut() {
        map.remove("selection");
    }
    if let Some(file) = value.get_mut("file").and_then(Value::as_object_mut) {
        file.remove("guides");
        file.remove("dimensionStyle");
    }
    value
}

/// Takes out the side shown and an other side that has been shown but
/// holds nothing (showing it the first time makes its empty layer and
/// sheet): neither is an edit.
fn side_free(value: &Value) -> Value {
    let mut value = value.clone();
    let file = match value.get_mut("file") {
        Some(file) => file,
        None => &mut value,
    };
    if let Some(file) = file.as_object_mut() {
        file.remove("activeSide");
        let empty = |side: &Value| {
            ["entities", "components"]
                .iter()
                .all(|k| side[*k].as_array().is_none_or(Vec::is_empty))
        };
        if file.get("bottom").is_some_and(empty) {
            file.remove("bottom");
        }
    }
    value
}

/// Answers a failure unless the two documents are the same, id counters
/// aside.
fn same_undone(kind: &'static str, step: usize, a: &Value, b: &Value) -> Result<(), Failure> {
    same(kind, step, &without_counters(a), &without_counters(b))
}

/// Answers a failure unless the two documents are the same.
fn same(kind: &'static str, step: usize, a: &Value, b: &Value) -> Result<(), Failure> {
    match first_difference(a, b, "") {
        None => Ok(()),
        Some(detail) => Err(Failure { step, kind, detail }),
    }
}

/// Every coordinate a document holds that must be a finite number.
fn non_finite(doc: &Document) -> Option<String> {
    let bad = |v: f64| !v.is_finite();
    for id in doc.entity_ids() {
        for (_, path) in doc.paths_of(id) {
            for sub in &path.subpaths {
                let points = std::iter::once(sub.start).chain(sub.segments.iter().map(|s| s.end()));
                for p in points {
                    if bad(p.x) || bad(p.y) {
                        return Some(format!("entity {id} has a point ({}, {})", p.x, p.y));
                    }
                }
            }
        }
        if let Some(b) = doc.bbox_of(id) {
            if bad(b.min.x) || bad(b.min.y) || bad(b.max.x) || bad(b.max.y) {
                return Some(format!("entity {id} has the box {b:?}"));
            }
        }
    }
    for t in doc.toolpaths() {
        for (i, m) in t.moves.iter().enumerate() {
            if bad(m.to.x) || bad(m.to.y) || bad(m.to.z) {
                return Some(format!("toolpath {} move {i} is {m:?}", t.id));
            }
        }
    }
    for c in doc.all_components() {
        let f = &c.field;
        if bad(f.cell) || bad(f.origin.x) || bad(f.origin.y) {
            return Some(format!("component {} has its grid at {:?}", c.id, f.origin));
        }
        if f.z.iter().any(|z| z.is_infinite()) {
            return Some(format!("component {} has an infinite height", c.id));
        }
    }
    None
}

/// The driver: a kernel on a scratch folder, and what the checks compare
/// against.
struct World {
    temp: crate::test_support::TempDir,
    kernel: Kernel,
    /// The document as opened or created, which undoing everything must
    /// give back.
    base: Value,
    saves: usize,
    font: String,
    /// Per command: how often it ran, was refused, and the last refusal.
    stats: Stats,
}

impl World {
    fn new(label: &str) -> World {
        let (temp, mut kernel) = temp_kernel(label);
        kernel.fonts = FontLibrary::bundled();
        let font = kernel
            .fonts
            .families()
            .into_iter()
            .next()
            .unwrap_or_else(|| "Single Line Sans".to_string());
        // A new job as the app starts one: the preferences applied.
        kernel.call("doc.new", "{}");
        kernel.document.set_undo_limit(usize::MAX);
        let base = canon(&kernel.document, true);
        World {
            temp,
            kernel,
            base,
            saves: 0,
            font,
            stats: std::collections::BTreeMap::new(),
        }
    }

    fn call(&mut self, name: &str, args: &Value) -> Result<Value, (String, String)> {
        match self.kernel.call(name, &args.to_string()) {
            KernelReply::Json(text) => {
                let value: Value = serde_json::from_str(&text).expect("a JSON reply");
                if value["ok"] == Value::Bool(true) {
                    Ok(value["result"].clone())
                } else {
                    Err((
                        value["error"]["code"].as_str().unwrap_or("").to_string(),
                        value["error"]["message"].as_str().unwrap_or("").to_string(),
                    ))
                }
            }
            KernelReply::Bytes(_) => Ok(Value::Null),
        }
    }

    /// Runs one step and checks every invariant after it.
    fn step(&mut self, index: usize, op: &Op) -> Result<(), Failure> {
        let fail = |kind: &'static str, detail: String| Failure {
            step: index,
            kind,
            detail,
        };
        match op.name.as_str() {
            "@saveReopen" => return self.save_reopen(index),
            "@undoAll" => return self.undo_all(index),
            _ => {}
        }
        let before = canon(&self.kernel.document, true);
        let revision = self.kernel.document.revision();
        let side_before = self.kernel.document.active_side();
        let steps = self.kernel.document.steps_committed();
        let answer = self.call(&op.name, &op.args);
        let after = canon(&self.kernel.document, true);
        // A change to what is saved must mark the job changed (else closing
        // the app does not offer to save it) and move the revision (the
        // views and lists refresh on it).
        if without_counters(&side_free(&before["file"]))
            != without_counters(&side_free(&after["file"]))
            || before["heights"] != after["heights"]
        {
            let detail = || first_difference(&before, &after, "").unwrap_or_default();
            if !self.kernel.document.is_dirty() {
                return Err(fail(
                    "a change leaves the job marked unchanged",
                    format!("{}: {}", op.name, detail()),
                ));
            }
            // Guides, dimension style and the side shown are the view's own
            // settings, which it holds already.
            let changed =
                without_counters(&view_free(&before)) != without_counters(&view_free(&after));
            if changed && self.kernel.document.revision() == revision {
                return Err(fail(
                    "a change does not move the revision",
                    format!("{}: {}", op.name, detail()),
                ));
            }
        }
        let entry = self.stats.entry(op.name.clone()).or_default();
        entry.0 += 1;
        if let Err((_, message)) = &answer {
            entry.1 += 1;
            entry.2.clone_from(message);
        }
        match answer {
            Err((code, message)) => {
                if code == "internal" {
                    return Err(fail("panic", format!("{}: {message}", op.name)));
                }
                same_undone(
                    "refused command changed the document",
                    index,
                    &before,
                    &after,
                )
                .map_err(|f| Failure {
                    detail: format!("{} ({message}): {}", op.name, f.detail),
                    ..f
                })?;
            }
            Ok(_) => {
                let stepped = self.kernel.document.steps_committed() != steps;
                let history = op.name == "undo" || op.name == "redo";
                if stepped && !history {
                    // Undo and Redo turn to the side the step was made on,
                    // whose selection is its own: the selection is compared
                    // only when they end on the side it was taken on.
                    let on = |doc: &Document, side, value: &Value| {
                        let mut value = value.clone();
                        if doc.active_side() != side {
                            if let Some(map) = value.as_object_mut() {
                                map.remove("selection");
                            }
                        }
                        value
                    };
                    let side_after = self.kernel.document.active_side();
                    self.call("undo", &json!({}))
                        .map_err(|e| fail("undo refused", format!("{e:?}")))?;
                    let undone = on(
                        &self.kernel.document,
                        side_before,
                        &canon(&self.kernel.document, true),
                    );
                    let before = on(&self.kernel.document, side_before, &before);
                    same_undone("undo is not the document before", index, &before, &undone)
                        .map_err(|f| Failure {
                            detail: format!("{}: {}", op.name, f.detail),
                            ..f
                        })?;
                    self.call("redo", &json!({}))
                        .map_err(|e| fail("redo refused", format!("{e:?}")))?;
                    let redone = on(
                        &self.kernel.document,
                        side_after,
                        &canon(&self.kernel.document, true),
                    );
                    let after = on(&self.kernel.document, side_after, &after);
                    same_undone("redo is not the document after", index, &after, &redone).map_err(
                        |f| Failure {
                            detail: format!("{}: {}", op.name, f.detail),
                            ..f
                        },
                    )?;
                }
            }
        }
        self.check_state(index)
    }

    fn check_state(&self, index: usize) -> Result<(), Failure> {
        let doc = &self.kernel.document;
        if let Some(detail) = non_finite(doc) {
            return Err(Failure {
                step: index,
                kind: "non-finite coordinate",
                detail,
            });
        }
        for &id in doc.selection() {
            if doc.entity(id).is_err() {
                return Err(Failure {
                    step: index,
                    kind: "selection names a missing object",
                    detail: format!("{id} in {:?}", doc.selection()),
                });
            }
        }
        Ok(())
    }

    /// Undoes everything back to the base, checking the state after each
    /// undo, then redoes as many steps. The selection is left out of the
    /// comparison with the base: choosing objects is not an undo step.
    fn undo_all(&mut self, index: usize) -> Result<(), Failure> {
        let end = canon(&self.kernel.document, false);
        let mut undone = 0;
        while self.kernel.document.journal().can_undo() {
            if let Err(e) = self.call("undo", &json!({})) {
                return Err(Failure {
                    step: index,
                    kind: "undo refused",
                    detail: format!("{e:?}"),
                });
            }
            undone += 1;
            self.check_state(index)?;
        }
        let start = view_free(&canon(&self.kernel.document, false));
        same_undone(
            "undo all is not the start",
            index,
            &view_free(&self.base),
            &start,
        )?;
        for _ in 0..undone {
            if let Err(e) = self.call("redo", &json!({})) {
                return Err(Failure {
                    step: index,
                    kind: "redo refused",
                    detail: format!("{e:?}"),
                });
            }
            self.check_state(index)?;
        }
        // The selection made outside the steps is not brought back.
        let back = canon(&self.kernel.document, false);
        same_undone("redo all is not the end", index, &end, &back)
    }

    /// Each toolpath that is not stale, recalculated from its stored
    /// request, must give the moves it holds; the recalculations are undone.
    fn recalculate_all(&mut self, index: usize, when: &str) -> Result<(), Failure> {
        let fresh: Vec<(u64, Vec<crate::cam::toolpath::Move>)> = self
            .kernel
            .document
            .toolpaths()
            .into_iter()
            .filter(|t| !self.kernel.document.toolpath_is_stale(t))
            .map(|t| (t.id, t.moves.clone()))
            .collect();
        for (id, moves) in fresh {
            let steps = self.kernel.document.steps_committed();
            match self.call("cam.recalculate", &json!({ "id": id })) {
                Ok(_) => {}
                Err((code, message)) => {
                    return Err(Failure {
                        step: index,
                        kind: "a fresh toolpath does not recalculate",
                        detail: format!("{when}: toolpath {id}: {code} {message}"),
                    })
                }
            }
            let now = self
                .kernel
                .document
                .toolpath(id)
                .map(|t| t.moves.clone())
                .unwrap_or_default();
            if self.kernel.document.steps_committed() != steps {
                self.call("undo", &json!({})).ok();
            }
            if now != moves {
                let first = now
                    .iter()
                    .zip(&moves)
                    .position(|(a, b)| a != b)
                    .unwrap_or(now.len().min(moves.len()));
                return Err(Failure {
                    step: index,
                    kind: "a fresh toolpath recalculates to other moves",
                    detail: format!(
                        "{when}: toolpath {id}: {} moves, was {}; first differs at {first}: {:?} vs {:?}",
                        now.len(),
                        moves.len(),
                        now.get(first),
                        moves.get(first)
                    ),
                });
            }
        }
        Ok(())
    }

    fn save_reopen(&mut self, index: usize) -> Result<(), Failure> {
        self.recalculate_all(index, "before saving")?;
        self.undo_all(index)?;
        self.saves += 1;
        let path = self.temp.path().join(format!("job{}.carve", self.saves));
        let saved = canon(&self.kernel.document, false);
        let stale: Vec<(u64, bool)> = self
            .kernel
            .document
            .toolpaths()
            .into_iter()
            .map(|t| (t.id, self.kernel.document.toolpath_is_stale(t)))
            .collect();
        let path_text = path.to_string_lossy().to_string();
        if let Err(e) = self.call("doc.save", &json!({ "path": path_text })) {
            return Err(Failure {
                step: index,
                kind: "save refused",
                detail: format!("{e:?}"),
            });
        }
        if let Err(e) = self.call("doc.open", &json!({ "path": path_text })) {
            return Err(Failure {
                step: index,
                kind: "the saved job does not open",
                detail: format!("{e:?}"),
            });
        }
        self.kernel.document.set_undo_limit(usize::MAX);
        let opened = canon(&self.kernel.document, false);
        same("reopen is not the saved document", index, &saved, &opened)?;
        let stale_now: Vec<(u64, bool)> = self
            .kernel
            .document
            .toolpaths()
            .into_iter()
            .map(|t| (t.id, self.kernel.document.toolpath_is_stale(t)))
            .collect();
        if stale != stale_now {
            return Err(Failure {
                step: index,
                kind: "reopen changes which toolpaths are stale",
                detail: format!("{stale:?} vs {stale_now:?}"),
            });
        }
        self.base = canon(&self.kernel.document, true);
        self.recalculate_all(index, "after reopening")?;
        self.check_state(index)
    }
}

/// Picks the next step from what the document holds now.
fn generate(rng: &mut Rng, world: &World) -> Op {
    let doc = &world.kernel.document;
    let top: Vec<u64> = doc.order().to_vec();
    let paths: Vec<u64> = top
        .iter()
        .copied()
        .filter(|&id| {
            matches!(
                doc.entity(id).map(|e| &e.kind),
                Ok(crate::doc::EntityKind::Path { .. })
            )
        })
        .collect();
    let closed: Vec<u64> = paths
        .iter()
        .copied()
        .filter(|&id| {
            doc.paths_of(id)
                .iter()
                .all(|(_, p)| !p.subpaths.is_empty() && p.subpaths.iter().all(|s| s.closed))
        })
        .collect();
    let texts: Vec<u64> = top
        .iter()
        .copied()
        .filter(|&id| {
            matches!(
                doc.entity(id).map(|e| &e.kind),
                Ok(crate::doc::EntityKind::Text { .. })
            )
        })
        .collect();
    let layers: Vec<u64> = doc.layers().iter().map(|l| l.id).collect();
    let sheets: Vec<u64> = doc.sheets().iter().map(|s| s.id).collect();
    let components: Vec<u64> = doc.all_components().iter().map(|c| c.id).collect();
    let toolpaths: Vec<u64> = doc.toolpath_order().to_vec();
    let levels: Vec<u64> = doc.levels().iter().map(|l| l.id).collect();
    let instances: Vec<u64> = doc.assembly().instances.iter().map(|i| i.id).collect();
    let joints: Vec<u64> = doc
        .assembly()
        .joint_set
        .joints
        .iter()
        .map(|j| j.id)
        .collect();
    let (w, h) = (doc.job().width, doc.job().height);
    let x = |rng: &mut Rng| rng.range(0.0, w);
    let y = |rng: &mut Rng| rng.range(0.0, h);

    let mut roll = rng.below(130);
    // Commands on components while there are none draw a shape instead.
    let on_components = [41, 42, 95, 96, 101, 102, 111, 112, 122, 123];
    if components.is_empty() && on_components.contains(&roll) {
        roll = 38;
    }
    let ids = |rng: &mut Rng, from: &[u64], most: usize| -> Value {
        let mut chosen: Vec<u64> = Vec::new();
        for _ in 0..(1 + rng.below(most)) {
            if let Some(id) = rng.pick(from) {
                if !chosen.contains(&id) {
                    chosen.push(id);
                }
            }
        }
        json!(chosen)
    };
    match roll {
        0..=13 => {
            // Draw: a rectangle, circle, line, arc polyline or curve.
            let (x0, y0) = (x(rng), y(rng));
            let (dx, dy) = (rng.range(5.0, 80.0), rng.range(5.0, 60.0));
            let path = match rng.below(5) {
                0 | 1 => serde_json::to_value(
                    rect(Point::new(x0, y0), Point::new(x0 + dx, y0 + dy)).expect("rect"),
                )
                .expect("path"),
                2 => serde_json::to_value(circle(Point::new(x0, y0), dx / 2.0).expect("circle"))
                    .expect("path"),
                3 => json!({ "subpaths": [{ "start": { "x": x0, "y": y0 }, "closed": false,
                    "segments": [{ "kind": "line", "to": { "x": x0 + dx, "y": y0 + dy } },
                                 { "kind": "arc", "to": { "x": x0 + dx, "y": y0 }, "bulge": 0.5 }] }] }),
                _ => {
                    json!({ "subpaths": [{ "start": { "x": x0, "y": y0 }, "closed": rng.chance(50),
                    "segments": [{ "kind": "cubic", "c1": { "x": x0 + dx, "y": y0 + dy },
                                   "c2": { "x": x0 - dx, "y": y0 + dy }, "to": { "x": x0, "y": y0 + dy / 3.0 } },
                                 { "kind": "line", "to": { "x": x0 + dx / 2.0, "y": y0 - dy / 4.0 } }] }] })
                }
            };
            let layer = if rng.chance(20) {
                rng.pick(&layers)
            } else {
                None
            };
            Op::new(
                "entities.create",
                json!({ "entity": { "kind": "path", "path": path }, "layer": layer }),
            )
        }
        14..=18 => {
            let (dx, dy) = (rng.range(-40.0, 40.0), rng.range(-40.0, 40.0));
            let (a, s) = (rng.range(-3.2, 3.2), rng.range(0.5, 1.5));
            let matrix = if rng.chance(50) {
                [1.0, 0.0, 0.0, 1.0, dx, dy]
            } else {
                [s * a.cos(), s * a.sin(), -s * a.sin(), s * a.cos(), dx, dy]
            };
            Op::new(
                "entities.transform",
                json!({ "ids": ids(rng, &top, 3), "matrix": matrix }),
            )
        }
        19..=21 => {
            let name = ["ops.weld", "ops.subtract", "ops.intersect", "ops.xor"][rng.below(4)];
            let mut chosen = ids(rng, &closed, 3);
            if let (Some(list), Some(extra)) = (chosen.as_array_mut(), rng.pick(&closed)) {
                if !list.contains(&json!(extra)) {
                    list.push(json!(extra));
                }
            }
            Op::new(name, json!({ "ids": chosen }))
        }
        22..=24 => Op::new(
            "ops.offset",
            json!({ "ids": ids(rng, &paths, 2), "distance": rng.range(0.5, 10.0),
                    "side": if rng.chance(50) { "outward" } else { "inward" },
                    "corner": if rng.chance(50) { "round" } else { "sharp" },
                    "deleteOriginal": rng.chance(30) }),
        ),
        25..=26 => Op::new(
            "text.create",
            json!({ "spec": { "text": (["A", "Hi", "Oak 1", "W"][rng.below(4)]),
                              "fontFamily": if rng.chance(50) { world.font.clone() } else { "Single Line Sans".to_string() },
                              "sizeMm": rng.range(5.0, 30.0) },
                    "x": x(rng), "y": y(rng) }),
        ),
        27 => Op::new(
            "ops.nest",
            json!({ "ids": ids(rng, &closed, 3),
                    "params": { "toolDiameter": 3.0, "gap": 1.0, "border": 2.0, "rotationStepDeg": 90.0 },
                    "addSheets": rng.chance(50) }),
        ),
        28..=29 => Op::new("entities.delete", json!({ "ids": ids(rng, &top, 2) })),
        30 => Op::new("entities.group", json!({ "ids": ids(rng, &top, 3) })),
        31 => Op::new("entities.ungroup", json!({ "ids": ids(rng, &top, 2) })),
        32..=33 => match rng.below(6) {
            0 => Op::new(
                "layers.add",
                json!({ "name": "L", "active": rng.chance(50) }),
            ),
            1 => Op::new(
                "layers.update",
                json!({ "id": rng.pick(&layers), "visible": rng.chance(70), "locked": rng.chance(20) }),
            ),
            2 => Op::new(
                "layers.delete",
                json!({ "id": rng.pick(&layers), "moveTo": if rng.chance(50) { rng.pick(&layers) } else { None } }),
            ),
            3 => Op::new("layers.setActive", json!({ "id": rng.pick(&layers) })),
            4 => Op::new("layers.duplicate", json!({ "id": rng.pick(&layers) })),
            _ => Op::new(
                "entities.setLayer",
                json!({ "ids": ids(rng, &top, 2), "layer": rng.pick(&layers) }),
            ),
        },
        34..=35 => match rng.below(5) {
            0 => Op::new(
                "sheets.add",
                json!({ "name": "S", "activate": rng.chance(50) }),
            ),
            1 => Op::new("sheets.delete", json!({ "id": rng.pick(&sheets) })),
            2 => Op::new("sheets.setActive", json!({ "id": rng.pick(&sheets) })),
            3 => Op::new("sheets.duplicate", json!({ "id": rng.pick(&sheets) })),
            _ => Op::new(
                "entities.setSheet",
                json!({ "ids": ids(rng, &top, 2), "sheet": rng.pick(&sheets) }),
            ),
        },
        36..=37 => {
            if rng.chance(50) {
                Op::new(
                    "clipboard.copy",
                    json!({ "ids": ids(rng, &top, 2), "components": if rng.chance(30) { ids(rng, &components, 1) } else { json!([]) } }),
                )
            } else {
                Op::new(
                    "clipboard.paste",
                    json!({ "dx": rng.range(-20.0, 20.0), "dy": rng.range(-20.0, 20.0) }),
                )
            }
        }
        38..=40 => Op::new(
            "model.createShape",
            json!({ "ids": ids(rng, &closed, 2),
                    "params": { "profile": (["round", "angular", "flat"][rng.below(3)]),
                                "height": rng.range(1.0, 8.0), "angle": 45.0, "baseHeight": rng.range(0.0, 2.0) },
                    "combine": (["add", "subtract", "mergeHigh", "mergeLow"][rng.below(4)]) }),
        ),
        41..=42 => match rng.below(6) {
            0 => Op::new(
                "model.update",
                json!({ "id": rng.pick(&components), "visible": rng.chance(80), "heightScale": rng.range(0.5, 2.0) }),
            ),
            1 => Op::new("model.delete", json!({ "ids": ids(rng, &components, 1) })),
            2 => Op::new("model.levels.add", json!({ "name": "Level" })),
            3 => Op::new(
                "model.transform",
                json!({ "id": rng.pick(&components), "params": { "offset": { "x": rng.range(-20.0, 20.0), "y": rng.range(-20.0, 20.0) },
                        "scaleX": rng.range(0.5, 1.5), "scaleY": rng.range(0.5, 1.5), "rotateDeg": rng.range(-90.0, 90.0) } }),
            ),
            4 => Op::new(
                "model.update",
                json!({ "id": rng.pick(&components), "level": rng.pick(&levels) }),
            ),
            _ => Op::new(
                "model.transformMatrix",
                json!({ "ids": ids(rng, &components, 2), "matrix": [1.0, 0.0, 0.0, 1.0, rng.range(-20.0, 20.0), rng.range(-20.0, 20.0)] }),
            ),
        },
        43..=47 => {
            let strategy = ["profile", "pocket", "drill", "profile", "quickEngrave"][rng.below(5)];
            let params = match strategy {
                "profile" => json!({ "cutDepth": rng.range(1.0, 12.0),
                    "side": (["outside", "inside", "on"][rng.below(3)]),
                    "allowance": rng.range(0.0, 1.0),
                    "tabs": { "enabled": rng.chance(30), "count": 4, "length": 8.0, "thickness": 2.0 } }),
                "pocket" => json!({ "cutDepth": rng.range(1.0, 6.0),
                    "strategy": if rng.chance(50) { "offset" } else { "raster" } }),
                "drill" => json!({ "cutDepth": rng.range(1.0, 10.0), "peckDepth": 2.0 }),
                _ => json!({ "depth": 1.0, "passes": 1, "mode": "outline" }),
            };
            Op::new(
                "cam.compute",
                json!({ "strategy": strategy, "tool": end_mill(), "params": params,
                        "ids": ids(rng, if strategy == "pocket" { &closed } else { &paths }, 3) }),
            )
        }
        48..=50 => match rng.below(5) {
            0 | 1 => Op::new("cam.recalculate", json!({ "id": rng.pick(&toolpaths) })),
            2 => Op::new("cam.delete", json!({ "ids": ids(rng, &toolpaths, 1) })),
            3 => Op::new(
                "cam.rename",
                json!({ "id": rng.pick(&toolpaths), "name": "Renamed" }),
            ),
            _ => Op::new(
                "cam.duplicate",
                json!({ "id": rng.pick(&toolpaths), "name": "" }),
            ),
        },
        51..=53 => {
            // A joint between the facing edges of two rectangles.
            let a = rng.pick(&closed);
            // B: another closed vector as wide as A, so the edges match.
            let width = |id: u64| doc.bbox_of(id).map(|b| b.width());
            let partners: Vec<u64> = closed
                .iter()
                .copied()
                .filter(|&b| {
                    Some(b) != a && a.is_some_and(|a| {
                    matches!((width(a), width(b)), (Some(p), Some(q)) if (p - q).abs() < 1e-6)
                })
                })
                .collect();
            let b = rng.pick(&partners);
            let (Some(a), Some(b)) = (a, b) else {
                return Op::new("joints.coupon", json!({ "kind": "finger", "params": {} }));
            };
            let (ba, bb) = (doc.bbox_of(a), doc.bbox_of(b));
            let (Some(ba), Some(bb)) = (ba, bb) else {
                return Op::new("joints.list", json!({}));
            };
            if !joints.is_empty() && rng.chance(30) {
                return Op::new("joints.remove", json!({ "id": rng.pick(&joints) }));
            }
            if rng.chance(40) {
                return Op::new(
                    "joints.coupon",
                    json!({ "kind": (["finger", "dovetail", "halfLap", "dado", "tabSlot"][rng.below(5)]), "params": {} }),
                );
            }
            let kind = ["finger", "dovetail", "rabbet", "halfLap"][rng.below(4)];
            Op::new(
                "joints.add",
                json!({ "kind": kind,
                        "a": { "kind": "edge", "vector": a, "from": [ba.max.x, ba.max.y], "to": [ba.min.x, ba.max.y] },
                        "b": { "kind": "edge", "vector": b, "from": [bb.min.x, bb.min.y], "to": [bb.max.x, bb.min.y] },
                        "params": {} }),
            )
        }
        54..=56 => {
            if instances.len() >= 2 && rng.chance(60) {
                let p = rng.pick(&instances);
                let others: Vec<u64> = instances
                    .iter()
                    .copied()
                    .filter(|i| Some(*i) != p)
                    .collect();
                let c = rng.pick(&others);
                Op::new(
                    "assembly.mate",
                    json!({ "kind": (["flush", "hinge", "slide"][rng.below(3)]),
                            "parent": p, "parentFace": { "kind": "edge", "index": rng.below(4) },
                            "child": c, "childFace": if rng.chance(50) { json!({ "kind": "bottom" }) } else { json!({ "kind": "edge", "index": rng.below(4) }) } }),
                )
            } else {
                Op::new(
                    "assembly.makePart",
                    json!({ "ids": ids(rng, &closed, 1), "name": "Part" }),
                )
            }
        }
        57..=58 => {
            let mut variables = serde_json::Map::new();
            variables.insert("a".to_string(), json!(rng.range(0.0, 100.0)));
            if rng.chance(50) {
                variables.insert("half".to_string(), json!("{a}/2"));
            }
            Op::new("doc.variables.set", json!({ "variables": variables }))
        }
        59..=61 => Op::new("selection.set", json!({ "ids": ids(rng, &top, 3) })),
        62..=63 => Op::new(
            "ops.mirror",
            json!({ "ids": ids(rng, &top, 2), "axis": (["h", "v"][rng.below(2)]), "copy": rng.chance(50) }),
        ),
        64..=65 => Op::new(
            "ops.arrayCopy",
            json!({ "ids": ids(rng, &top, 2), "rows": 1 + rng.below(3), "cols": 1 + rng.below(3),
                    "gapX": 5.0, "gapY": 5.0, "mode": "gap", "group": rng.chance(30) }),
        ),
        66 => Op::new(
            "ops.rotate",
            json!({ "ids": ids(rng, &top, 2), "angleDeg": rng.range(-180.0, 180.0) }),
        ),
        67 => Op::new(
            "ops.join",
            json!({ "ids": ids(rng, &paths, 3), "tolerance": 1.0 }),
        ),
        68..=69 => Op::new(
            "ops.fillet",
            json!({ "id": rng.pick(&closed), "x": x(rng), "y": y(rng), "radius": rng.range(1.0, 5.0) }),
        ),
        70..=71 => {
            let mut job = serde_json::to_value(world.kernel.document.job_setup()).expect("job");
            job["units"] = json!(if job["units"] == "mm" { "inch" } else { "mm" });
            Op::new("job.set", job)
        }
        72..=74 => Op::new("@saveReopen", json!({})),
        75..=82 => Op::new("undo", json!({})),
        83..=87 => Op::new("redo", json!({})),
        88 => Op::new("@undoAll", json!({})),
        89..=90 => {
            // A double sided job, and turning it over.
            let mut job = serde_json::to_value(world.kernel.document.job_setup()).expect("job");
            match rng.below(4) {
                0 => {
                    job["kind"] = json!(if job["kind"] == "doubleSided" {
                        "singleSided"
                    } else {
                        "doubleSided"
                    });
                    Op::new("job.set", job)
                }
                1 => Op::new("job.swapSides", json!({})),
                _ => Op::new(
                    "job.setSide",
                    json!({ "side": if doc.active_side() == crate::doc::Side::Top { "bottom" } else { "top" } }),
                ),
            }
        }
        91..=92 => Op::new(
            "ops.copyAlong",
            json!({ "ids": ids(rng, &closed, 1), "pathIds": ids(rng, &paths, 1), "distributeMode": "count",
                    "count": 1 + rng.below(4), "alignToCurve": rng.chance(50), "deleteOriginal": rng.chance(30) }),
        ),
        93..=94 => Op::new(
            "ops.circularArray",
            json!({ "ids": ids(rng, &top, 2), "components": if rng.chance(30) { ids(rng, &components, 1) } else { json!([]) },
                    "cx": x(rng), "cy": y(rng), "count": 2 + rng.below(4), "angle": 360.0, "rotateCopies": rng.chance(50), "group": rng.chance(30) }),
        ),
        95..=96 => match rng.below(4) {
            0 => Op::new(
                "model.groups.create",
                json!({ "ids": ids(rng, &components, 3) }),
            ),
            1 => Op::new(
                "model.copyMatrix",
                json!({ "ids": ids(rng, &components, 2), "matrix": [1.0, 0.0, 0.0, 1.0, rng.range(-30.0, 30.0), rng.range(-30.0, 30.0)],
                        "vectors": if rng.chance(50) { ids(rng, &closed, 2) } else { json!([]) } }),
            ),
            2 => Op::new(
                "model.props",
                json!({ "ids": ids(rng, &components, 2), "baseHeight": rng.range(0.0, 3.0), "combine": (["add", "subtract", "mergeHigh"][rng.below(3)]) }),
            ),
            _ => Op::new(
                "model.levels.setClip",
                json!({ "id": rng.pick(&levels), "vectorIds": ids(rng, &closed, 1) }),
            ),
        },
        97..=98 => Op::new(
            "ops.distort",
            json!({ "ids": ids(rng, &top, 2), "componentIds": ids(rng, &components, 1),
                    "envelope": { "bottom": [{ "x": 0.0, "y": 0.0 }, { "x": 50.0, "y": rng.range(-10.0, 10.0) }, { "x": 100.0, "y": 0.0 }],
                                  "top": [{ "x": 0.0, "y": 100.0 }, { "x": 50.0, "y": rng.range(90.0, 110.0) }, { "x": 100.0, "y": 100.0 }] },
                    "bbox": [0.0, 0.0, 100.0, 100.0] }),
        ),
        99..=100 => Op::new(
            "text.update",
            json!({ "id": rng.pick(&texts), "spec": { "text": "Edited", "fontFamily": world.font.clone(), "sizeMm": rng.range(5.0, 20.0) } }),
        ),
        101..=102 => Op::new(
            "model.sculpt",
            json!({ "id": rng.pick(&components), "points": [{ "x": x(rng), "y": y(rng) }, { "x": x(rng), "y": y(rng) }],
                    "params": { "brush": (["smooth", "deposit", "remove", "erase"][rng.below(4)]), "diameter": rng.range(5.0, 40.0),
                                "strength": 0.5, "falloff": 0.5, "grow": false } }),
        ),
        103 => Op::new(
            "cam.tiling.set",
            json!({ "tiling": { "enabled": rng.chance(50), "spec": { "method": "individual", "tileWidth": rng.range(50.0, 300.0),
                    "tileHeight": rng.range(50.0, 200.0), "overlap": 5.0, "smallestFirst": false }, "active": 0, "originalPosition": false } }),
        ),
        104..=105 => Op::new(
            "ops.trimObjects",
            json!({ "ids": ids(rng, &top, 3), "componentIds": ids(rng, &components, 1), "side": (["inside", "outside"][rng.below(2)]) }),
        ),
        106 => Op::new(
            "text.onCurve",
            json!({ "textId": rng.pick(&texts), "curve": { "pathId": rng.pick(&paths), "offset": rng.range(-3.0, 3.0),
                    "otherSide": rng.chance(50), "keepVertical": rng.chance(30) } }),
        ),
        107..=108 => Op::new("joints.makeToolpaths", json!({ "tool": end_mill() })),
        109 => Op::new(
            "ops.close",
            json!({ "ids": ids(rng, &paths, 2), "mode": (["straight", "smooth", "moveEnd"][rng.below(3)]) }),
        ),
        110 => Op::new(
            "doc.guides.set",
            json!({ "guides": [{ "axis": "x", "position": x(rng) }] }),
        ),
        111..=112 => Op::new(
            "entities.delete",
            json!({ "ids": [], "components": ids(rng, &components, 1) }),
        ),
        113..=114 => Op::new("job.setScaled", {
            let mut job = serde_json::to_value(world.kernel.document.job_setup()).expect("job");
            job["width"] = json!(rng.range(150.0, 600.0));
            job["height"] = json!(rng.range(100.0, 400.0));
            json!({ "job": job })
        }),
        115..=116 => Op::new("sheets.add", json!({ "name": "Extra", "activate": true })),
        117..=118 => Op::new(
            "ops.nestMarks",
            json!({ "ids": ids(rng, &closed, 2), "copies": 1 + rng.below(3), "filler": rng.chance(20) }),
        ),
        119..=121 => Op::new(
            "assembly.makePart",
            json!({ "ids": ids(rng, &closed, 1), "name": "Panel" }),
        ),
        122..=123 => Op::new(
            "model.edit",
            match rng.below(3) {
                0 => json!({ "id": rng.pick(&components), "op": "flip" }),
                1 => json!({ "id": rng.pick(&components), "op": "mirror", "params": {} }),
                _ => {
                    json!({ "id": rng.pick(&components), "op": "scale", "params": { "scale": rng.range(0.3, 2.0) } })
                }
            },
        ),
        _ => Op::new(
            "doc.notes.set",
            json!({ "notes": format!("note {}", rng.below(10)) }),
        ),
    }
}

/// Runs a generated sequence; answers the steps run and the failure,
/// adding up what each command did in `stats`.
fn run_generated_with(seed: u64, length: usize, stats: &mut Stats) -> (Vec<Op>, Option<Failure>) {
    let mut rng = Rng::new(seed);
    let mut world = World::new(&format!("seq-{seed}"));
    let mut ops = Vec::new();
    for index in 0..length {
        let op = generate(&mut rng, &world);
        ops.push(op.clone());
        if let Err(failure) = world.step(index, &op) {
            return (ops, Some(failure));
        }
    }
    let end = Op::new("@undoAll", json!({}));
    ops.push(end.clone());
    let failure = world.step(length, &end).err();
    for (name, (runs, refused, last)) in std::mem::take(&mut world.stats) {
        let entry = stats.entry(name).or_default();
        entry.0 += runs;
        entry.1 += refused;
        if !last.is_empty() {
            entry.2 = last;
        }
    }
    (ops, failure)
}

/// Replays recorded steps; answers the first failure.
fn replay(ops: &[Op]) -> Option<Failure> {
    let mut world = World::new("seq-replay");
    for (index, op) in ops.iter().enumerate() {
        if let Err(failure) = world.step(index, op) {
            return Some(failure);
        }
    }
    None
}

/// Takes steps out while the same kind of failure still happens.
fn shrink(ops: Vec<Op>, kind: &str) -> (Vec<Op>, Failure) {
    let still = |candidate: &[Op]| replay(candidate).filter(|f| f.kind == kind);
    let mut best = ops;
    let mut failure = still(&best).expect("the failure replays");
    best.truncate(failure.step + 1);
    let mut chunk = best.len() / 2;
    while chunk >= 1 {
        let mut start = 0;
        while start < best.len() {
            let end = (start + chunk).min(best.len());
            let mut candidate = best.clone();
            candidate.drain(start..end);
            if let Some(f) = still(&candidate) {
                best = candidate;
                best.truncate(f.step + 1);
                failure = f;
            } else {
                start += chunk;
            }
        }
        chunk /= 2;
    }
    (best, failure)
}

fn report(seed: u64, ops: Vec<Op>, failure: &Failure) -> String {
    let (minimal, shrunk) = shrink(ops, failure.kind);
    let mut text = format!(
        "seed {seed}: {} at step {}: {}\nminimal sequence ({} steps), failing at step {}: {}\n",
        failure.kind,
        failure.step,
        failure.detail,
        minimal.len(),
        shrunk.step,
        shrunk.detail
    );
    for op in &minimal {
        text.push_str(&format!("  {} {}\n", op.name, op.args));
    }
    text
}

fn run_seeds(seeds: std::ops::Range<u64>, length: usize) {
    use rayon::prelude::*;
    let results: Vec<(Option<String>, Stats)> = seeds
        .into_par_iter()
        .map(|seed| {
            let mut stats = std::collections::BTreeMap::new();
            let (ops, failure) = run_generated_with(seed, length, &mut stats);
            let failure = failure.map(|f| report(seed, ops, &f));
            if let Some(text) = &failure {
                eprintln!("{text}");
            }
            (failure, stats)
        })
        .collect();
    let mut failures = Vec::new();
    let mut stats: Stats = std::collections::BTreeMap::new();
    for (failure, part) in results {
        failures.extend(failure);
        for (name, (runs, refused, last)) in part {
            let entry = stats.entry(name).or_default();
            entry.0 += runs;
            entry.1 += refused;
            if !last.is_empty() {
                entry.2 = last;
            }
        }
    }
    if std::env::var("CARVE_SEQ_STATS").is_ok() {
        for (name, (runs, refused, last)) in &stats {
            eprintln!("{name}: {runs} run, {refused} refused; last: {last}");
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The gate's share: a few fixed seeds, each its own test so they run side
/// by side.
#[test]
fn generated_sequences_keep_every_invariant_seed_1() {
    run_seeds(1..2, 100);
}

#[test]
fn generated_sequences_keep_every_invariant_seed_2() {
    run_seeds(2..3, 100);
}

#[test]
fn generated_sequences_keep_every_invariant_seed_3() {
    run_seeds(3..4, 100);
}

/// The long run: `CARVE_SEQ_SEEDS=2000 CARVE_SEQ_LEN=300 cargo test -p
/// carve-core long_generated_run -- --ignored`.
#[test]
#[ignore = "long; run by hand"]
fn long_generated_run() {
    let count: u64 = std::env::var("CARVE_SEQ_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    let first: u64 = std::env::var("CARVE_SEQ_FIRST")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
    let length: usize = std::env::var("CARVE_SEQ_LEN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    run_seeds(first..first + count, length);
}

// ------------------------------------------ regressions the driver found

/// Found by the driver (seed 4): draw a vector, select it, Undo. The
/// drawing was gone but the selection still named it, so the next command
/// on the selection answered "no entity" and the Assembly and forms showed
/// a selected object that did not exist.
#[test]
fn undoing_a_drawing_takes_it_out_of_the_selection() {
    let mut kernel = Kernel::new();
    let id = make_rect(&mut kernel, 10.0, 10.0, 50.0, 40.0);
    run(
        &mut kernel,
        "selection.set",
        &json!({ "ids": [id] }).to_string(),
    );
    assert_eq!(kernel.document().selection(), [id]);
    run(&mut kernel, "undo", "");
    assert!(kernel.document().entity(id).is_err());
    assert_eq!(kernel.document().selection(), [] as [u64; 0]);
    // A command on the selection now finds nothing selected, rather than
    // an object that is not there.
    let refused = kernel
        .apply("ops.offset", r#"{"distance": 2}"#)
        .expect_err("nothing is selected");
    assert!(!refused.to_string().contains("no entity"), "{refused}");
    // Redo brings the rectangle back; the selection made outside the step
    // is not an undo step, so it stays empty.
    run(&mut kernel, "redo", "");
    assert!(kernel.document().entity(id).is_ok());
    assert_eq!(kernel.document().selection(), [] as [u64; 0]);
}

/// Found by the driver (seeds 2 and 3): nesting and text leave coordinates
/// such as 22.139999999999986 that the JSON reader read back a last digit
/// off (22.139999999999983), so every save and reopen moved the drawing.
#[test]
fn a_saved_coordinate_reads_back_as_the_exact_number_written() {
    let (temp, mut kernel) = temp_kernel("seq-exact-float");
    let odd: [f64; 3] = [22.139999999999986, 98.59196214702291, 125.90238095238095];
    let path = json!({ "subpaths": [{ "start": { "x": odd[0], "y": odd[1] }, "closed": false,
        "segments": [{ "kind": "line", "to": { "x": odd[2], "y": odd[0] } }] }] });
    let id = run(
        &mut kernel,
        "entities.create",
        &json!({ "entity": { "kind": "path", "path": path } }).to_string(),
    )["id"]
        .as_u64()
        .expect("an id");
    let file = temp.path().join("exact.carve");
    let file = file.to_string_lossy();
    run(
        &mut kernel,
        "doc.save",
        &json!({ "path": file }).to_string(),
    );
    run(
        &mut kernel,
        "doc.open",
        &json!({ "path": file }).to_string(),
    );
    let (_, read) = kernel.document().paths_of(id).remove(0);
    let sub = &read.subpaths[0];
    assert_eq!(sub.start.x.to_bits(), odd[0].to_bits());
    assert_eq!(sub.start.y.to_bits(), odd[1].to_bits());
    assert_eq!(sub.segments[0].end().x.to_bits(), odd[2].to_bits());
}

/// The same read, where it shows: a 3D toolpath records the fingerprint of
/// the model it was cut from (the grid origin and cell to the last bit), so
/// a component whose origin read back a digit off made every 3D toolpath
/// need recalculating each time the job was opened.
#[test]
fn a_3d_toolpath_is_still_up_to_date_after_the_job_is_reopened() {
    let (temp, mut kernel) = temp_kernel("seq-3d-fresh");
    let id = make_rect(&mut kernel, 31.7, 23.3, 97.1, 81.9);
    let shape = run(
        &mut kernel,
        "model.createShape",
        &json!({ "ids": [id], "params": { "profile": "flat", "height": 3.0 } }).to_string(),
    )["id"]
        .as_u64()
        .expect("a component");
    // Nudged six times: the grid origin ends at 34.660279860381976, which
    // the reader without exact floats read back a last digit off.
    for k in 1..=6 {
        let dx = f64::from(k) / 7.0;
        run(
            &mut kernel,
            "model.transform",
            &json!({ "id": shape, "params": { "offset": { "x": dx, "y": dx / 3.0 } } }).to_string(),
        );
    }
    run(
        &mut kernel,
        "cam.compute",
        &json!({ "strategy": "finish3d", "tool": end_mill(), "ids": [],
                 "params": { "stepover": 4.0 } })
        .to_string(),
    );
    assert_eq!(run(&mut kernel, "cam.list", "")[0]["stale"], false);
    let file = temp.path().join("fresh.carve");
    let file = file.to_string_lossy();
    run(
        &mut kernel,
        "doc.save",
        &json!({ "path": file }).to_string(),
    );
    run(
        &mut kernel,
        "doc.open",
        &json!({ "path": file }).to_string(),
    );
    assert_eq!(run(&mut kernel, "cam.list", "")[0]["stale"], false);
}

/// Found by the driver (seeds 1133 and 1195): a toolpath made on the
/// bottom of a double sided job, then the job made single sided. The
/// toolpath stayed in the list marked up to date, Recalculate answered
/// "no entity", and Save Toolpaths wrote its moves, which are in the turned
/// over material's job space, so on a single sided job they cut mirrored.
/// Now it shows as needing recalculation, recalculating and saving it say
/// why they cannot, and making the job double sided again brings it back
/// with its vectors.
#[test]
fn a_bottom_side_toolpath_waits_while_the_job_is_single_sided() {
    let (temp, mut kernel) = temp_kernel("seq-bottom-single");
    let mut job = run(&mut kernel, "job.get", "");
    job["kind"] = json!("doubleSided");
    run(&mut kernel, "job.set", &job.to_string());
    run(&mut kernel, "job.setSide", r#"{"side":"bottom"}"#);
    let path = rect(Point::new(10.0, 10.0), Point::new(50.0, 40.0)).expect("rect");
    let vector = run(
        &mut kernel,
        "entities.create",
        &json!({ "entity": { "kind": "path", "path": path } }).to_string(),
    )["id"]
        .as_u64()
        .expect("an id");
    let request = json!({ "strategy": "profile", "tool": end_mill(),
                          "params": { "cutDepth": 3.0 }, "ids": [vector] });
    let toolpath = run(&mut kernel, "cam.compute", &request.to_string())["id"]
        .as_u64()
        .expect("a toolpath");
    let moves = kernel
        .document()
        .toolpath(toolpath)
        .expect("kept")
        .moves
        .clone();

    job["kind"] = json!("singleSided");
    run(&mut kernel, "job.set", &job.to_string());
    assert_eq!(run(&mut kernel, "cam.list", "")[0]["stale"], true);
    let refused = kernel
        .apply("cam.recalculate", &json!({ "id": toolpath }).to_string())
        .expect_err("the job has no bottom side");
    assert!(refused.to_string().contains("double sided"), "{refused}");
    let file = temp.path().join("bottom.nc");
    for save in [
        json!({ "toolpathIds": [toolpath], "post": "grbl_mm", "path": file }),
        json!({ "outputAllVisible": true, "post": "grbl_mm", "path": file }),
    ] {
        let refused = kernel
            .apply("cam.save", &save.to_string())
            .expect_err("a bottom toolpath is not posted on a single sided job");
        assert!(refused.to_string().contains("bottom side"), "{refused}");
    }
    assert!(!file.exists());

    job["kind"] = json!("doubleSided");
    run(&mut kernel, "job.set", &job.to_string());
    assert_eq!(run(&mut kernel, "cam.list", "")[0]["stale"], false);
    run(
        &mut kernel,
        "cam.recalculate",
        &json!({ "id": toolpath }).to_string(),
    );
    assert_eq!(
        kernel.document().toolpath(toolpath).expect("kept").moves,
        moves
    );
}

/// Found by the driver (Duplicate Layer and Duplicate Sheet refused in
/// the long run): a group left holding one object, its other member
/// deleted, could not be duplicated with its layer or sheet, nor copied
/// and pasted: every copy asked for "at least two objects to group".
#[test]
fn a_group_left_with_one_object_is_copied_with_its_layer_and_sheet() {
    let mut kernel = Kernel::new();
    let a = make_rect(&mut kernel, 10.0, 10.0, 50.0, 40.0);
    let b = make_rect(&mut kernel, 60.0, 10.0, 90.0, 40.0);
    let group = run(
        &mut kernel,
        "entities.group",
        &json!({ "ids": [a, b] }).to_string(),
    )["id"]
        .as_u64()
        .expect("a group");
    run(
        &mut kernel,
        "entities.delete",
        &json!({ "ids": [a] }).to_string(),
    );
    let children =
        |kernel: &Kernel, id: u64| match &kernel.document().entity(id).expect("a group").kind {
            crate::doc::EntityKind::Group { children } => children.len(),
            _ => 0,
        };
    assert_eq!(children(&kernel, group), 1);
    let groups = |kernel: &Kernel| {
        kernel
            .document()
            .entity_ids()
            .into_iter()
            .filter(|&id| children(kernel, id) == 1)
            .count()
    };
    run(&mut kernel, "layers.duplicate", r#"{"id":1}"#);
    assert_eq!(groups(&kernel), 2);
    run(&mut kernel, "sheets.duplicate", r#"{"id":1}"#);
    assert_eq!(groups(&kernel), 4);
    run(
        &mut kernel,
        "clipboard.copy",
        &json!({ "ids": [group] }).to_string(),
    );
    run(&mut kernel, "clipboard.paste", r#"{"dx":5,"dy":5}"#);
    assert_eq!(groups(&kernel), 5);
}

/// Found by the driver (seeds 2217 and 2358): select a group and another
/// object, Ungroup the group, Undo. The group came back but not in the
/// selection: Ungroup took it out of the selection without recording it.
#[test]
fn undoing_an_ungroup_selects_the_group_again() {
    let mut kernel = Kernel::new();
    let a = make_rect(&mut kernel, 10.0, 10.0, 50.0, 40.0);
    let b = make_rect(&mut kernel, 60.0, 10.0, 90.0, 40.0);
    let c = make_rect(&mut kernel, 100.0, 10.0, 130.0, 40.0);
    let group = run(
        &mut kernel,
        "entities.group",
        &json!({ "ids": [a, b] }).to_string(),
    )["id"]
        .as_u64()
        .expect("a group");
    run(
        &mut kernel,
        "selection.set",
        &json!({ "ids": [group, c] }).to_string(),
    );
    run(
        &mut kernel,
        "entities.ungroup",
        &json!({ "ids": [group] }).to_string(),
    );
    assert!(!kernel.document().selection().contains(&group));
    run(&mut kernel, "undo", "");
    assert_eq!(kernel.document().selection(), [group, c]);
}
