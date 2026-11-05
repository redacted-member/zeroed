//! Furniture templates: the JSON format (`format: 1`) the library is
//! written in, read and written as it is, and the built-in library bundled
//! into the kernel.
//!
//! Every length is in millimetres. Sizes are expressions read by the
//! document's formula evaluator (`{name}` reads a parameter or a derived
//! value; `+ - * / ^` and brackets), kept as typed so a template saved
//! back is the template read.

use std::collections::BTreeMap;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use crate::error::CoreError;

/// The template format this kernel reads and writes.
pub const FORMAT: u32 = 1;

/// An expression kept as typed: a JSON string, or a number read as one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Expr(pub String);

impl Expr {
    /// The expression's text.
    pub fn text(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Expr {
    fn from(text: &str) -> Self {
        Expr(text.to_string())
    }
}

impl Serialize for Expr {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Expr {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::String(text) => Ok(Expr(text)),
            Value::Number(n) => Ok(Expr(n.to_string())),
            Value::Bool(b) => Ok(Expr(if b { "1".into() } else { "0".into() })),
            other => Err(de::Error::custom(format!(
                "an expression must be text or a number, not {other}"
            ))),
        }
    }
}

/// What a parameter holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParamKind {
    /// A length, millimetres.
    Length,
    /// A material thickness, millimetres (offered from the material list).
    Thickness,
    /// A whole number.
    Count,
    /// Any number.
    Number,
    /// An angle, degrees.
    Angle,
    /// 0 or 1.
    Toggle,
    /// One of `options`.
    Choice,
}

/// A value the maker can change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    /// Name used in expressions, `{key}`.
    pub key: String,
    /// Label shown in the designer.
    pub label: String,
    /// What it holds.
    pub kind: ParamKind,
    /// Its default: a number, or for a choice the option's text.
    pub default: Value,
    /// Least value allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Most value allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// A choice's options.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// Group heading in the designer.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
}

/// A material the panels are cut from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialDef {
    /// Label used in sheet names and the cut list.
    pub label: String,
    /// Thickness, an expression in millimetres.
    pub thickness: Expr,
}

/// A panel's axis aligned box in furniture space (X width, Y depth with the
/// front at 0, Z height from the floor), millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxDef {
    pub x: Expr,
    pub y: Expr,
    pub z: Expr,
    pub w: Expr,
    pub d: Expr,
    pub h: Expr,
}

/// Which way the grain runs on a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GrainDef {
    /// Along the panel's longest side.
    #[default]
    Long,
    /// Along its shortest side.
    Short,
    /// No grain: any turn on the sheet.
    None,
}

/// A point of a panel outline in panel coordinates, `["u", "v"]`,
/// `["u", "v", bulge]` or `{ "u", "v", "bulge" }`.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlinePoint {
    pub u: Expr,
    pub v: Expr,
    /// Bulge of the segment from this point to the next (0 straight).
    pub bulge: Option<Expr>,
}

impl Serialize for OutlinePoint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.bulge {
            None => (&self.u, &self.v).serialize(serializer),
            Some(b) => {
                let mut map = serde_json::Map::new();
                map.insert("u".into(), Value::String(self.u.0.clone()));
                map.insert("v".into(), Value::String(self.v.0.clone()));
                map.insert("bulge".into(), Value::String(b.0.clone()));
                Value::Object(map).serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for OutlinePoint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Object {
            u: Expr,
            v: Expr,
            #[serde(default)]
            bulge: Option<Expr>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Form {
            Pair(Expr, Expr),
            Triple(Expr, Expr, Expr),
            Object(Object),
        }
        Ok(match Form::deserialize(deserializer)? {
            Form::Pair(u, v) => OutlinePoint { u, v, bulge: None },
            Form::Triple(u, v, b) => OutlinePoint {
                u,
                v,
                bulge: Some(b),
            },
            Form::Object(o) => OutlinePoint {
                u: o.u,
                v: o.v,
                bulge: o.bulge,
            },
        })
    }
}

/// A feature cut into a panel (notch, hole, shelf pins...): its kind, an
/// optional `when`, and the kind's own fields, kept as written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDef {
    /// notch, cutout, hole, radius, arch, handle, pocket, shelfPins,
    /// hingeCups or slideHoles.
    pub kind: String,
    /// Left out when this works out to 0 or less.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Expr>,
    /// The kind's own fields.
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,
}

/// A panel of the template, or `repeat` copies of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelDef {
    /// Unique among the template's panels.
    pub id: String,
    /// Name, which may hold expressions in braces ("Shelf {i+1}").
    pub name: String,
    /// Key of its material.
    pub material: String,
    /// Copies, `{i}` = 0 up to the count less one in every expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<Expr>,
    /// Left out when this works out to 0 or less.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Expr>,
    /// Its box in furniture space.
    #[serde(rename = "box")]
    pub bounds: BoxDef,
    /// Grain.
    #[serde(default)]
    pub grain: GrainDef,
    /// An outline replacing the rectangle, in panel coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Vec<OutlinePoint>>,
    /// Features cut into it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<FeatureDef>,
}

/// Names the class (and so the default joint) of the connections between
/// panels whose ids match `a` and `b` (`|` between ids, `*` for any).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub a: String,
    pub b: String,
    pub class: String,
}

/// A class of connections and its default joint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassDef {
    /// Label in the designer ("Shelves to sides").
    pub label: String,
    /// The joint used unless the maker picks another.
    pub default: String,
}

/// A hardware row of the template (shelf pins, feet, hinges...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareDef {
    pub name: String,
    pub count: Expr,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// A parametric furniture template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub format: u32,
    /// Unique, kebab case.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub description: String,
    /// SPDX id: CC0-1.0, CC-BY-4.0, CC-BY-SA-4.0 or PD.
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub author: String,
    /// URL of the open design it follows.
    #[serde(default)]
    pub source: Option<String>,
    /// Attribution line CC BY and BY-SA need.
    #[serde(default)]
    pub credit: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub params: Vec<Param>,
    /// Values worked out from the parameters, by name.
    #[serde(default)]
    pub derived: BTreeMap<String, Expr>,
    /// Materials by key.
    #[serde(default)]
    pub materials: BTreeMap<String, MaterialDef>,
    pub panels: Vec<PanelDef>,
    #[serde(default)]
    pub connections: Vec<Rule>,
    #[serde(default)]
    pub joint_classes: BTreeMap<String, ClassDef>,
    #[serde(default)]
    pub hardware: Vec<HardwareDef>,
}

/// Whether `id` is kebab case: lower case letters and digits in words
/// joined by single hyphens.
pub fn is_kebab(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--")
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl Template {
    /// Reads a template from its JSON text and checks it.
    ///
    /// # Errors
    /// [`CoreError::Validation`] in plain words when the text does not read
    /// or the template is not a usable one.
    pub fn from_json(text: &str) -> Result<Template, CoreError> {
        let template: Template = serde_json::from_str(text)
            .map_err(|e| CoreError::validation(format!("the template does not read: {e}")))?;
        template.validate()?;
        Ok(template)
    }

    /// Checks what reading alone cannot: the format, ids, names and that
    /// every panel's material exists.
    ///
    /// # Errors
    /// [`CoreError::Validation`] naming the first problem.
    pub fn validate(&self) -> Result<(), CoreError> {
        let bad = |m: String| Err(CoreError::validation(m));
        if self.format != FORMAT {
            return bad(format!(
                "the template is format {}; this version reads format {FORMAT}",
                self.format
            ));
        }
        if !is_kebab(&self.id) {
            return bad(format!(
                "the template id {:?} must be lower case words joined by hyphens",
                self.id
            ));
        }
        if self.panels.is_empty() {
            return bad("the template has no panels".into());
        }
        let mut keys: Vec<&str> = Vec::new();
        for param in &self.params {
            if !crate::doc::variables::is_name(&param.key) || param.key == "i" {
                return bad(format!("{:?} is not a usable parameter name", param.key));
            }
            if keys.contains(&param.key.as_str()) {
                return bad(format!("the parameter {} is there twice", param.key));
            }
            keys.push(&param.key);
            match param.kind {
                ParamKind::Choice => {
                    if param.options.is_empty() {
                        return bad(format!("the choice {} has no options", param.key));
                    }
                    let fine = param
                        .default
                        .as_str()
                        .is_some_and(|d| param.options.iter().any(|o| o == d));
                    if !fine {
                        return bad(format!(
                            "the default of {} must be one of its options",
                            param.key
                        ));
                    }
                    if param
                        .options
                        .iter()
                        .any(|o| !crate::doc::variables::is_name(o))
                    {
                        return bad(format!(
                            "the options of {} must be names (letters, digits, underscores)",
                            param.key
                        ));
                    }
                }
                _ => {
                    if !param.default.is_number() {
                        return bad(format!("the default of {} must be a number", param.key));
                    }
                }
            }
        }
        for name in self.derived.keys() {
            if !crate::doc::variables::is_name(name) || keys.contains(&name.as_str()) {
                return bad(format!("{name:?} is not a usable derived value name"));
            }
        }
        let mut ids: Vec<&str> = Vec::new();
        for panel in &self.panels {
            if panel.id.is_empty() || panel.id.contains(['~', '#', '|', '*']) {
                return bad(format!("{:?} is not a usable panel id", panel.id));
            }
            if ids.contains(&panel.id.as_str()) {
                return bad(format!("the panel id {} is there twice", panel.id));
            }
            ids.push(&panel.id);
            if !self.materials.contains_key(&panel.material) {
                return bad(format!(
                    "the panel {} uses the material {}, which the template does not list",
                    panel.id, panel.material
                ));
            }
        }
        Ok(())
    }

    /// The template as JSON.
    ///
    /// # Errors
    /// [`CoreError::Encode`] when it cannot be written out.
    pub fn to_value(&self) -> Result<Value, CoreError> {
        Ok(serde_json::to_value(self)?)
    }
}

/// The built-in library: (file name, JSON text) of every template in
/// `furniture/library`, bundled when the kernel is built.
pub fn builtin_sources() -> &'static [(&'static str, &'static str)] {
    include!(concat!(env!("OUT_DIR"), "/furniture_library.rs"))
}

/// The built-in templates that read, sorted by id.
pub fn builtin() -> Vec<Template> {
    let mut out: Vec<Template> = builtin_sources()
        .iter()
        .filter_map(|(_, text)| Template::from_json(text).ok())
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// The built-in template `id`.
pub fn builtin_template(id: &str) -> Option<Template> {
    builtin_sources()
        .iter()
        .filter_map(|(_, text)| Template::from_json(text).ok())
        .find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_template_reads_and_ids_are_unique() {
        let mut ids = Vec::new();
        for (file, text) in builtin_sources() {
            let template = Template::from_json(text).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert!(!ids.contains(&template.id), "{} twice", template.id);
            ids.push(template.id);
        }
        assert!(ids.contains(&"bookcase".to_string()), "{ids:?}");
        assert!(ids.contains(&"base-cabinet".to_string()), "{ids:?}");
    }

    #[test]
    fn a_template_saved_reads_back_the_same() {
        let template = builtin_template("bookcase").expect("bookcase");
        let text = serde_json::to_string(&template).expect("json");
        assert_eq!(Template::from_json(&text).expect("reads"), template);
    }

    #[test]
    fn bad_templates_are_refused_in_plain_words() {
        let good = builtin_template("bookcase").expect("bookcase");
        let mut t = good.clone();
        t.format = 2;
        assert!(t.validate().is_err());
        let mut t = good.clone();
        t.id = "Book Case".into();
        assert!(t.validate().is_err());
        let mut t = good.clone();
        t.panels[0].material = "nothing".into();
        let e = t.validate().expect_err("no material");
        assert!(e.to_string().contains("nothing"), "{e}");
        let mut t = good;
        let dup = t.panels[0].clone();
        t.panels.push(dup);
        assert!(t.validate().is_err());
        assert!(is_kebab("base-cabinet"));
        assert!(!is_kebab("base--cabinet"));
        assert!(!is_kebab("-a"));
    }

    #[test]
    fn outline_points_read_in_every_form() {
        let p: Vec<OutlinePoint> =
            serde_json::from_str(r#"[["0", "{h}"], [10, "5", "0.5"], {"u": "1", "v": "2"}]"#)
                .expect("reads");
        assert_eq!(p[0].v.text(), "{h}");
        assert_eq!(p[1].u.text(), "10");
        assert_eq!(p[1].bulge.as_ref().map(Expr::text), Some("0.5"));
        assert_eq!(p[2].bulge, None);
    }
}
