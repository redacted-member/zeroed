//! Toolpath templates: a job's toolpath settings saved to a file and
//! applied to another job.
//!
//! A template keeps each toolpath's strategy, tool and settings, and in
//! place of vector ids (which mean nothing in another job) the names of the
//! layers its vectors were on. Applying a template picks, for each
//! toolpath, the vectors on layers with those names, so a job drawn with
//! the same layer names machines the same way.

use std::path::Path as FsPath;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::CoreError;

use super::compute::{Request, Strategy};
use super::tool::Tool;

/// The `format` field every template file carries.
pub const TEMPLATE_FORMAT: &str = "zeroed-toolpath-template";

/// The format name of templates saved before the app was named Zeroed.
const OLDER_TEMPLATE_FORMAT: &str = "carvestudio-toolpath-template";
/// The template file version this build writes and reads.
pub const TEMPLATE_VERSION: u32 = 1;

/// One toolpath in a template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateEntry {
    pub name: String,
    pub strategy: Strategy,
    pub tool: Tool,
    #[serde(default)]
    pub params: Value,
    /// Names of the layers the toolpath's vectors were on, in first use
    /// order. Empty for toolpaths that machine no vectors.
    #[serde(default)]
    pub layers: Vec<String>,
}

impl TemplateEntry {
    /// The entry for a request, with the layer names of its vectors.
    pub fn of(request: &Request, name: String, layers: Vec<String>) -> TemplateEntry {
        TemplateEntry {
            name,
            strategy: request.strategy,
            tool: request.tool.clone(),
            params: request.params.clone(),
            layers,
        }
    }

    /// The request that recreates this toolpath on `ids`.
    pub fn request(&self, ids: Vec<u64>) -> Request {
        Request {
            strategy: self.strategy,
            name: Some(self.name.clone()),
            tool: self.tool.clone(),
            params: self.params.clone(),
            ids,
        }
    }
}

/// A saved set of toolpaths, in calculation order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub format: String,
    pub version: u32,
    pub toolpaths: Vec<TemplateEntry>,
}

impl Template {
    pub fn new(toolpaths: Vec<TemplateEntry>) -> Template {
        Template {
            format: TEMPLATE_FORMAT.to_string(),
            version: TEMPLATE_VERSION,
            toolpaths,
        }
    }

    /// Reads a template from JSON text.
    ///
    /// # Errors
    /// Returns [`CoreError::Format`] when the text is not a template, or is
    /// one from a newer version.
    pub fn from_json(text: &str) -> Result<Template, CoreError> {
        let template: Template =
            serde_json::from_str(text).map_err(|e| CoreError::format("toolpath template", e))?;
        if template.format != TEMPLATE_FORMAT && template.format != OLDER_TEMPLATE_FORMAT {
            return Err(CoreError::format(
                "toolpath template",
                "this file is not a toolpath template",
            ));
        }
        if template.version > TEMPLATE_VERSION {
            return Err(CoreError::format(
                "toolpath template",
                "the template was saved by a newer version",
            ));
        }
        Ok(template)
    }

    /// The template as pretty printed JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Reads a template file.
    ///
    /// # Errors
    /// Returns [`CoreError::Io`] when the file cannot be read and the
    /// errors of [`Template::from_json`].
    pub fn read(path: &FsPath) -> Result<Template, CoreError> {
        let text = std::fs::read_to_string(path).map_err(|e| CoreError::io(path.display(), e))?;
        Template::from_json(&text)
    }

    /// Writes a template file.
    ///
    /// # Errors
    /// Returns [`CoreError::Io`] when the file cannot be written.
    pub fn write(&self, path: &FsPath) -> Result<(), CoreError> {
        std::fs::write(path, self.to_json()).map_err(|e| CoreError::io(path.display(), e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool() -> Tool {
        Tool {
            name: "End Mill 6 mm".to_string(),
            ..Tool::default()
        }
    }

    fn entry() -> TemplateEntry {
        TemplateEntry {
            name: "Outside".to_string(),
            strategy: Strategy::Profile,
            tool: tool(),
            params: json!({ "side": "outside", "cutDepth": 6.0 }),
            layers: vec!["Cut Out".to_string()],
        }
    }

    #[test]
    fn a_template_round_trips_through_json() {
        let template = Template::new(vec![entry()]);
        let back = Template::from_json(&template.to_json()).expect("reads");
        assert_eq!(back, template);
    }

    #[test]
    fn an_entry_becomes_a_request_on_new_vectors() {
        let request = entry().request(vec![4, 9]);
        assert_eq!(request.ids, vec![4, 9]);
        assert_eq!(request.strategy, Strategy::Profile);
        assert_eq!(request.name.as_deref(), Some("Outside"));
        assert_eq!(request.params["side"], "outside");
    }

    #[test]
    fn other_files_and_newer_versions_are_refused() {
        assert!(Template::from_json("{}").is_err());
        let other = json!({ "format": "something else", "version": 1, "toolpaths": [] });
        assert!(Template::from_json(&other.to_string()).is_err());
        // Templates saved before the app was named Zeroed still read.
        let older = json!({ "format": OLDER_TEMPLATE_FORMAT, "version": 1, "toolpaths": [] });
        assert!(Template::from_json(&older.to_string()).is_ok());
        let newer = json!({ "format": TEMPLATE_FORMAT, "version": 99, "toolpaths": [] });
        assert!(Template::from_json(&newer.to_string()).is_err());
    }

    #[test]
    fn files_are_written_and_read() {
        let temp = crate::test_support::TempDir::new("template");
        let path = temp.path().join("t.json");
        let template = Template::new(vec![entry()]);
        template.write(&path).expect("writes");
        assert_eq!(Template::read(&path).expect("reads"), template);
    }
}
