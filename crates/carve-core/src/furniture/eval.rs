//! Works a template out for the values the maker typed: every parameter,
//! derived value and material, then every panel (repeats expanded, those
//! whose `when` is 0 or less left out) with its box, its normal axis and
//! its features.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::doc::variables::{evaluate, resolve, Constants, Definition};
use crate::error::CoreError;

use super::template::{Expr, GrainDef, ParamKind, Template};

/// Two lengths closer than this are the same, millimetres.
pub const TOL: f64 = 0.01;

/// Something the generator wants the maker to know, in plain words.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// What is wrong and what to do.
    pub message: String,
    /// The panel it is about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
    /// The connection it is about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    /// A joint that would work instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggest: Option<String>,
}

impl Issue {
    pub fn new(message: impl Into<String>) -> Issue {
        Issue {
            message: message.into(),
            panel: None,
            connection: None,
            suggest: None,
        }
    }

    pub fn on_panel(message: impl Into<String>, panel: &str) -> Issue {
        Issue {
            panel: Some(panel.to_string()),
            ..Issue::new(message)
        }
    }

    pub fn on_connection(message: impl Into<String>, key: &str) -> Issue {
        Issue {
            connection: Some(key.to_string()),
            ..Issue::new(message)
        }
    }
}

/// A material worked out.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Material {
    pub key: String,
    pub label: String,
    /// Millimetres.
    pub thickness: f64,
}

/// A feature worked out: numbers for its expressions, words kept.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    pub kind: String,
    pub numbers: BTreeMap<String, f64>,
    pub words: BTreeMap<String, String>,
}

impl Feature {
    /// A number field, or `default`.
    pub fn num(&self, key: &str, default: f64) -> f64 {
        self.numbers.get(key).copied().unwrap_or(default)
    }

    /// A word field, or `default`.
    pub fn word<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.words.get(key).map_or(default, String::as_str)
    }
}

/// Fields of a feature that are always words, not expressions.
const WORDS: [&str; 5] = ["edge", "face", "shape", "ends", "side"];

/// Words a feature's `at` may be instead of a distance.
const PLACES: [&str; 9] = [
    "front", "back", "top", "bottom", "left", "right", "start", "end", "middle",
];

/// A panel worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    /// `id`, or `id#i` for a repeat.
    pub key: String,
    /// The template panel id.
    pub id: String,
    /// The repeat index.
    pub index: Option<u32>,
    pub name: String,
    pub material: String,
    pub thickness: f64,
    /// Box corners, furniture space millimetres.
    pub min: [f64; 3],
    pub max: [f64; 3],
    /// The axis (0 X, 1 Y, 2 Z) the panel's thickness runs along.
    pub normal: usize,
    pub grain: GrainDef,
    pub features: Vec<Feature>,
    /// The outline in panel coordinates: (u, v, bulge).
    pub outline: Option<Vec<[f64; 3]>>,
}

impl Panel {
    /// The panel's size along `axis`.
    pub fn size(&self, axis: usize) -> f64 {
        self.max[axis] - self.min[axis]
    }

    /// The middle of the box.
    pub fn centre(&self) -> [f64; 3] {
        [0, 1, 2].map(|k| (self.min[k] + self.max[k]) / 2.0)
    }

    /// The two flat axes, in X, Y, Z order.
    pub fn flat_axes(&self) -> (usize, usize) {
        let mut flat = (0..3).filter(|k| *k != self.normal);
        let u = flat.next().unwrap_or(0);
        let v = flat.next().unwrap_or(1);
        (u, v)
    }
}

/// A hardware row worked out.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareRow {
    pub name: String,
    pub count: f64,
    pub note: String,
}

/// A template worked out for a set of values.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluated {
    /// The values used, parameter by parameter (numbers clamped to their
    /// range, choices one of their options), as a design stores them.
    pub values: BTreeMap<String, Value>,
    /// Every name expressions read.
    pub numbers: BTreeMap<String, f64>,
    pub materials: BTreeMap<String, Material>,
    pub panels: Vec<Panel>,
    pub hardware: Vec<HardwareRow>,
    pub issues: Vec<Issue>,
}

const NO_LETTERS: Constants = Constants {
    width: 0.0,
    height: 0.0,
    thickness: 0.0,
    foot: 304.8,
};

/// The value of `expr` with `numbers`.
///
/// # Errors
/// [`CoreError::Validation`] naming the expression and `what` it is.
pub fn number(expr: &str, numbers: &BTreeMap<String, f64>, what: &str) -> Result<f64, CoreError> {
    evaluate(expr, numbers, &NO_LETTERS).ok_or_else(|| {
        CoreError::validation(format!("{what}: \"{expr}\" does not work out to a number"))
    })
}

/// `name` with each `{...}` worked out: names inside the braces read
/// parameters and derived values (`Shelf {i+1}` gives `Shelf 3`).
pub fn name_text(name: &str, numbers: &BTreeMap<String, f64>) -> String {
    let mut out = String::new();
    let mut rest = name;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}').map(|c| c + open) else {
            break;
        };
        out.push_str(&rest[..open]);
        let inner = &rest[open + 1..close];
        let mut expr = String::new();
        let chars: Vec<char> = inner.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                expr.push('{');
                expr.extend(&chars[start..i]);
                expr.push('}');
            } else {
                expr.push(chars[i]);
                i += 1;
            }
        }
        match evaluate(&expr, numbers, &NO_LETTERS) {
            Some(v) if (v - v.round()).abs() < 1e-9 => out.push_str(&format!("{}", v.round())),
            Some(v) => out.push_str(&format!("{}", (v * 1000.0).round() / 1000.0)),
            None => out.push_str(&rest[open..=close]),
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// The values to use: each parameter's given value made legal (clamped,
/// whole for counts, 0 or 1 for toggles, an option for choices), else its
/// default; and the numbers expressions read for them (a choice `back`
/// gives `{back}` its index and `{back_<option>}` 1 for the one picked).
pub fn param_values(
    template: &Template,
    given: &BTreeMap<String, Value>,
) -> (BTreeMap<String, Value>, BTreeMap<String, f64>) {
    let mut values = BTreeMap::new();
    let mut numbers = BTreeMap::new();
    for param in &template.params {
        let asked = given.get(&param.key);
        if param.kind == ParamKind::Choice {
            let default = param.default.as_str().unwrap_or_default();
            let picked = asked
                .and_then(Value::as_str)
                .filter(|s| param.options.iter().any(|o| o == s))
                .unwrap_or(default)
                .to_string();
            let index = param.options.iter().position(|o| *o == picked).unwrap_or(0);
            numbers.insert(param.key.clone(), index as f64);
            for option in &param.options {
                numbers.insert(
                    format!("{}_{option}", param.key),
                    if *option == picked { 1.0 } else { 0.0 },
                );
            }
            values.insert(param.key.clone(), Value::String(picked));
            continue;
        }
        let default = as_number(&param.default).unwrap_or(0.0);
        let mut v = asked
            .and_then(as_number)
            .filter(|v| v.is_finite())
            .unwrap_or(default);
        if let Some(min) = param.min {
            v = v.max(min);
        }
        if let Some(max) = param.max {
            v = v.min(max);
        }
        match param.kind {
            ParamKind::Count => v = v.round(),
            ParamKind::Toggle => v = if v > 0.5 { 1.0 } else { 0.0 },
            _ => {}
        }
        numbers.insert(param.key.clone(), v);
        values.insert(param.key.clone(), serde_json::json!(v));
    }
    (values, numbers)
}

/// Works `template` out for `given` values.
///
/// # Errors
/// [`CoreError::Validation`] when a derived value or a material does not
/// work out. A panel whose box does not work out, or has no axis of its
/// material's thickness, is left out and reported as an issue.
pub fn evaluate_template(
    template: &Template,
    given: &BTreeMap<String, Value>,
) -> Result<Evaluated, CoreError> {
    let (values, params) = param_values(template, given);
    let mut defs: BTreeMap<String, Definition> = params
        .iter()
        .map(|(k, v)| (k.clone(), Definition::Number(*v)))
        .collect();
    for (name, expr) in &template.derived {
        defs.insert(name.clone(), Definition::Formula(expr.0.clone()));
    }
    let numbers = resolve(&defs, &NO_LETTERS)?.values;
    let mut materials = BTreeMap::new();
    for (key, m) in &template.materials {
        let thickness = number(m.thickness.text(), &numbers, &format!("material {key}"))?;
        if thickness.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return Err(CoreError::validation(format!(
                "the {} material must be thicker than 0",
                m.label
            )));
        }
        materials.insert(
            key.clone(),
            Material {
                key: key.clone(),
                label: m.label.clone(),
                thickness,
            },
        );
    }
    let mut panels = Vec::new();
    let mut issues = Vec::new();
    for def in &template.panels {
        let count = match &def.repeat {
            None => None,
            Some(expr) => {
                let n = number(expr.text(), &numbers, &format!("repeat of {}", def.id))?;
                Some(n.round().clamp(0.0, 500.0) as u32)
            }
        };
        let indices: Vec<Option<u32>> = match count {
            None => vec![None],
            Some(n) => (0..n).map(Some).collect(),
        };
        let Some(material) = materials.get(&def.material).cloned() else {
            continue;
        };
        for index in indices {
            let mut vars = numbers.clone();
            vars.insert("i".into(), f64::from(index.unwrap_or(0)));
            let key = match index {
                Some(i) => format!("{}#{i}", def.id),
                None => def.id.clone(),
            };
            if let Some(when) = &def.when {
                if number(when.text(), &vars, &format!("when of {key}"))? <= 0.0 {
                    continue;
                }
            }
            let what = format!("the box of {key}");
            let read = |e: &Expr| number(e.text(), &vars, &what);
            let b = &def.bounds;
            let (x, y, z, w, d, h) = (
                read(&b.x)?,
                read(&b.y)?,
                read(&b.z)?,
                read(&b.w)?,
                read(&b.d)?,
                read(&b.h)?,
            );
            let size = [w, d, h];
            if size.iter().any(|s| *s <= TOL) {
                issues.push(Issue::on_panel(
                    format!(
                        "{} has no size at these values ({} x {} x {} mm), so it is left out",
                        name_text(&def.name, &vars),
                        round(w),
                        round(d),
                        round(h)
                    ),
                    &key,
                ));
                continue;
            }
            let Some(normal) = (0..3).find(|k| (size[*k] - material.thickness).abs() <= TOL) else {
                issues.push(Issue::on_panel(
                    format!(
                        "{} is {} x {} x {} mm and none of its sizes is its material's thickness ({} mm), so it is left out",
                        name_text(&def.name, &vars),
                        round(w),
                        round(d),
                        round(h),
                        round(material.thickness)
                    ),
                    &key,
                ));
                continue;
            };
            let mut features = Vec::new();
            for f in &def.features {
                if let Some(when) = &f.when {
                    if number(when.text(), &vars, &format!("a feature of {key}"))? <= 0.0 {
                        continue;
                    }
                }
                let mut feature = Feature {
                    kind: f.kind.clone(),
                    numbers: BTreeMap::new(),
                    words: BTreeMap::new(),
                };
                for (field, value) in &f.fields {
                    match value {
                        Value::String(s)
                            if WORDS.contains(&field.as_str())
                                || (field == "at" && PLACES.contains(&s.trim())) =>
                        {
                            feature.words.insert(field.clone(), s.clone());
                        }
                        Value::String(s) => {
                            let v = number(s, &vars, &format!("{} of {key}", f.kind))?;
                            feature.numbers.insert(field.clone(), v);
                        }
                        Value::Number(n) => {
                            feature
                                .numbers
                                .insert(field.clone(), n.as_f64().unwrap_or(0.0));
                        }
                        Value::Bool(b) => {
                            feature
                                .numbers
                                .insert(field.clone(), if *b { 1.0 } else { 0.0 });
                        }
                        _ => {}
                    }
                }
                features.push(feature);
            }
            let outline = match &def.outline {
                None => None,
                Some(points) => {
                    let mut out = Vec::with_capacity(points.len());
                    for p in points {
                        let what = format!("the outline of {key}");
                        let bulge = match &p.bulge {
                            Some(b) => number(b.text(), &vars, &what)?,
                            None => 0.0,
                        };
                        out.push([
                            number(p.u.text(), &vars, &what)?,
                            number(p.v.text(), &vars, &what)?,
                            bulge,
                        ]);
                    }
                    Some(out)
                }
            };
            panels.push(Panel {
                key,
                id: def.id.clone(),
                index,
                name: name_text(&def.name, &vars),
                material: def.material.clone(),
                thickness: material.thickness,
                min: [x, y, z],
                max: [x + w, y + d, z + h],
                normal,
                grain: def.grain,
                features,
                outline,
            });
        }
    }
    let mut hardware = Vec::new();
    for row in &template.hardware {
        let count = number(
            row.count.text(),
            &numbers,
            &format!("hardware {}", row.name),
        )?;
        if count > 0.0 {
            hardware.push(HardwareRow {
                name: row.name.clone(),
                count: count.round(),
                note: row.note.clone(),
            });
        }
    }
    Ok(Evaluated {
        values,
        numbers,
        materials,
        panels,
        hardware,
        issues,
    })
}

/// `v` to two decimals, for messages.
pub fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::furniture::template::builtin_template;
    use serde_json::json;

    fn given(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn choices_define_their_index_and_a_flag_per_option() {
        let t = builtin_template("bookcase").expect("bookcase");
        let (values, numbers) = param_values(&t, &given(&[("back", json!("overlay"))]));
        assert_eq!(values["back"], json!("overlay"));
        assert_eq!(numbers["back"], 2.0);
        assert_eq!(numbers["back_overlay"], 1.0);
        assert_eq!(numbers["back_inset"], 0.0);
        assert_eq!(numbers["back_none"], 0.0);
        // Not an option: the default.
        let (values, numbers) = param_values(&t, &given(&[("back", json!("glass"))]));
        assert_eq!(values["back"], json!("inset"));
        assert_eq!(numbers["back_inset"], 1.0);
    }

    #[test]
    fn numbers_are_clamped_counts_whole_and_toggles_zero_or_one() {
        let t = builtin_template("bookcase").expect("bookcase");
        let (_, n) = param_values(
            &t,
            &given(&[
                ("width", json!(5000)),
                ("depth", json!(149.99)),
                ("shelves", json!(2.6)),
                ("kick", json!(0.4)),
            ]),
        );
        assert_eq!(n["width"], 1200.0);
        assert_eq!(n["depth"], 150.0);
        assert_eq!(n["shelves"], 3.0);
        assert_eq!(n["kick"], 0.0);
        let (_, n) = param_values(&t, &given(&[("kick", json!(0.6)), ("width", json!("900"))]));
        assert_eq!(n["kick"], 1.0);
        assert_eq!(n["width"], 900.0);
    }

    #[test]
    fn repeats_expand_with_their_index_and_names_work_out() {
        let t = builtin_template("bookcase").expect("bookcase");
        let e = evaluate_template(&t, &given(&[("shelves", json!(3))])).expect("works out");
        let shelves: Vec<&Panel> = e.panels.iter().filter(|p| p.id == "shelf").collect();
        assert_eq!(shelves.len(), 3);
        assert_eq!(shelves[2].key, "shelf#2");
        assert_eq!(shelves[2].name, "Shelf 3");
        // Height 1800, base 80, t 18: pitch (1800 - 80 - 36) / 4 = 421.
        let pitch = (1800.0 - 80.0 - 36.0) / 4.0;
        let z = 80.0 + 18.0 + 3.0 * pitch - 9.0;
        assert!(
            (shelves[2].min[2] - z).abs() < 1e-9,
            "{}",
            shelves[2].min[2]
        );
        assert_eq!(shelves[2].normal, 2);
        // The sides stand on X.
        let left = e.panels.iter().find(|p| p.key == "left").expect("left");
        assert_eq!(left.normal, 0);
        assert_eq!(left.max, [18.0, 300.0, 1800.0]);
    }

    #[test]
    fn when_drops_panels_and_features() {
        let t = builtin_template("bookcase").expect("bookcase");
        let e = evaluate_template(&t, &given(&[("back", json!("none")), ("kick", json!(0))]))
            .expect("works out");
        assert!(e.panels.iter().all(|p| p.id != "back" && p.id != "kick"));
        let left = e.panels.iter().find(|p| p.key == "left").expect("left");
        assert!(left.features.is_empty());
        let e = evaluate_template(&t, &BTreeMap::new()).expect("works out");
        let left = e.panels.iter().find(|p| p.key == "left").expect("left");
        assert_eq!(left.features.len(), 1);
        assert_eq!(left.features[0].word("edge", ""), "bottom");
        assert_eq!(left.features[0].num("h", 0.0), 80.0);
    }

    #[test]
    fn names_work_out_their_braces() {
        let n: BTreeMap<String, f64> = [("i".to_string(), 2.0), ("w".to_string(), 1.5)]
            .into_iter()
            .collect();
        assert_eq!(name_text("Shelf {i+1}", &n), "Shelf 3");
        assert_eq!(name_text("Door {i}", &n), "Door 2");
        assert_eq!(name_text("Part {w*2}x", &n), "Part 3x");
        assert_eq!(name_text("Plain", &n), "Plain");
        assert_eq!(name_text("Odd {nothing}", &n), "Odd {nothing}");
    }

    #[test]
    fn a_panel_without_its_thickness_is_reported_and_left_out() {
        let mut t = builtin_template("bookcase").expect("bookcase");
        t.panels[0].bounds.w = Expr::from("20");
        let e = evaluate_template(&t, &BTreeMap::new()).expect("works out");
        assert!(e.panels.iter().all(|p| p.key != "left"));
        assert_eq!(e.issues.len(), 1);
        assert_eq!(e.issues[0].panel.as_deref(), Some("left"));
        assert!(e.issues[0].message.contains("thickness"), "{:?}", e.issues);
    }
}
