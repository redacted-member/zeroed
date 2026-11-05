//! The maker's own furniture templates: files `<id>.carvefurniture.json`
//! in the "Furniture Library" folder beside the tool database and posts
//! (CarveStudio's folder in the system's data directory), or a folder the
//! host sets.

use std::path::{Path, PathBuf};

use crate::error::CoreError;

use super::template::{builtin_template, Template};

/// The extension of a template file.
pub const EXTENSION: &str = ".carvefurniture.json";

/// The folder of the maker's templates: "Furniture Library" in
/// CarveStudio's data folder.
pub fn user_default_folder() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    base.join("CarveStudio").join("Furniture Library")
}

/// Where the maker's templates live; `None` keeps none (tests).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Library {
    pub folder: Option<PathBuf>,
}

fn io_error(path: &Path, error: &std::io::Error) -> CoreError {
    CoreError::Io {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}

impl Library {
    /// The library in the user's data folder.
    pub fn user_default() -> Library {
        Library {
            folder: Some(user_default_folder()),
        }
    }

    /// The maker's templates that read, sorted by id.
    pub fn user_templates(&self) -> Vec<Template> {
        let Some(folder) = &self.folder else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(folder) else {
            return Vec::new();
        };
        let mut out: Vec<Template> = entries
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(EXTENSION))
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|text| Template::from_json(&text).ok())
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out.dedup_by(|a, b| a.id == b.id);
        out
    }

    /// The template `id`: a built-in one, else the maker's.
    pub fn find(&self, id: &str) -> Option<Template> {
        builtin_template(id).or_else(|| self.user_templates().into_iter().find(|t| t.id == id))
    }

    fn file(&self, id: &str) -> Result<PathBuf, CoreError> {
        let folder = self.folder.as_ref().ok_or_else(|| {
            CoreError::validation("there is no furniture library folder to keep templates in")
        })?;
        Ok(folder.join(format!("{id}{EXTENSION}")))
    }

    /// Writes `template` to the library (replacing the maker's template of
    /// that id). Answers the file written.
    ///
    /// # Errors
    /// [`CoreError::Validation`] when the template is not usable or a
    /// built-in template has its id; [`CoreError::Io`] when it cannot be
    /// written.
    pub fn save(&self, template: &Template) -> Result<PathBuf, CoreError> {
        template.validate()?;
        if builtin_template(&template.id).is_some() {
            return Err(CoreError::validation(format!(
                "a built-in template is called {}; give yours another id",
                template.id
            )));
        }
        let path = self.file(&template.id)?;
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(|e| io_error(folder, &e))?;
        }
        let text = serde_json::to_string_pretty(template)?;
        std::fs::write(&path, text).map_err(|e| io_error(&path, &e))?;
        Ok(path)
    }

    /// Deletes the maker's template `id`.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for a built-in template or one the
    /// library does not have; [`CoreError::Io`] when the file stays.
    pub fn remove(&self, id: &str) -> Result<(), CoreError> {
        if builtin_template(id).is_some() {
            return Err(CoreError::validation(
                "built-in templates cannot be removed",
            ));
        }
        let path = self.file(id)?;
        if !path.exists() {
            return Err(CoreError::validation(format!(
                "your library has no template {id}"
            )));
        }
        std::fs::remove_file(&path).map_err(|e| io_error(&path, &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn templates_save_list_find_and_remove() {
        let dir = TempDir::new("furniture-library");
        let library = Library {
            folder: Some(dir.path().join("Furniture Library")),
        };
        assert!(library.user_templates().is_empty());
        let mut t = builtin_template("bookcase").expect("bookcase");
        // A built-in id is refused.
        assert!(library.save(&t).is_err());
        t.id = "my-bookcase".into();
        t.name = "My Bookcase".into();
        let path = library.save(&t).expect("saves");
        assert!(
            path.ends_with("my-bookcase.carvefurniture.json"),
            "{path:?}"
        );
        assert_eq!(library.user_templates(), vec![t.clone()]);
        assert_eq!(library.find("my-bookcase"), Some(t.clone()));
        // Built-ins are found first and cannot be removed.
        assert_eq!(
            library.find("bookcase").map(|b| b.id),
            Some("bookcase".into())
        );
        assert!(library.remove("bookcase").is_err());
        library.remove("my-bookcase").expect("removes");
        assert!(library.user_templates().is_empty());
        assert!(library.remove("my-bookcase").is_err());
        // A file that does not read is passed over.
        std::fs::write(
            dir.path()
                .join("Furniture Library")
                .join("bad.carvefurniture.json"),
            "{",
        )
        .expect("writes");
        assert!(library.user_templates().is_empty());
        // No folder keeps nothing.
        assert!(Library::default().save(&t).is_err());
    }
}
