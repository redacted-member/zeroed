//! The posts the app knows about: the ones shipped inside the binary and the
//! ones the user has imported into their own posts directory.

use std::collections::BTreeMap;
use std::path::{Path as FsPath, PathBuf};

use crate::error::CoreError;

use super::parser::parse_post;
use super::{Post, PostSummary};

/// Extension a post processor file carries.
pub const POST_EXTENSION: &str = "pp";

/// The posts shipped with the app, as pairs of identity and file text.
pub const SHIPPED_POSTS: [(&str, &str); 30] = [
    (
        "generic_gcode_mm",
        include_str!("../../assets/posts/generic_gcode_mm.pp"),
    ),
    (
        "generic_gcode_inch",
        include_str!("../../assets/posts/generic_gcode_inch.pp"),
    ),
    ("grbl_mm", include_str!("../../assets/posts/grbl_mm.pp")),
    ("grbl_inch", include_str!("../../assets/posts/grbl_inch.pp")),
    ("mach3_mm", include_str!("../../assets/posts/mach3_mm.pp")),
    (
        "linuxcnc_mm",
        include_str!("../../assets/posts/linuxcnc_mm.pp"),
    ),
    (
        "shopbot_mm",
        include_str!("../../assets/posts/shopbot_mm.pp"),
    ),
    (
        "laser_grbl",
        include_str!("../../assets/posts/laser_grbl.pp"),
    ),
    (
        "grbl_rotary_a_mm",
        include_str!("../../assets/posts/grbl_rotary_a_mm.pp"),
    ),
    (
        "snapmaker_cnc_200w_mm",
        include_str!("../../assets/posts/snapmaker_cnc_200w_mm.pp"),
    ),
    (
        "snapmaker_cnc_50w_mm",
        include_str!("../../assets/posts/snapmaker_cnc_50w_mm.pp"),
    ),
    (
        "laser_snapmaker_mm",
        include_str!("../../assets/posts/laser_snapmaker_mm.pp"),
    ),
    (
        "mach3_rotary_mm",
        include_str!("../../assets/posts/mach3_rotary_mm.pp"),
    ),
    (
        "linuxcnc_rotary_mm",
        include_str!("../../assets/posts/linuxcnc_rotary_mm.pp"),
    ),
    (
        "linuxcnc_rotary_inch",
        include_str!("../../assets/posts/linuxcnc_rotary_inch.pp"),
    ),
    (
        "linuxcnc_manual_rotary_mm",
        include_str!("../../assets/posts/linuxcnc_manual_rotary_mm.pp"),
    ),
    (
        "linuxcnc_manual_rotary_inch",
        include_str!("../../assets/posts/linuxcnc_manual_rotary_inch.pp"),
    ),
    (
        "uccnc_rotary_mm",
        include_str!("../../assets/posts/uccnc_rotary_mm.pp"),
    ),
    (
        "uccnc_manual_rotary_mm",
        include_str!("../../assets/posts/uccnc_manual_rotary_mm.pp"),
    ),
    (
        "mach3_inch",
        include_str!("../../assets/posts/mach3_inch.pp"),
    ),
    (
        "linuxcnc_inch",
        include_str!("../../assets/posts/linuxcnc_inch.pp"),
    ),
    (
        "shopbot_inch",
        include_str!("../../assets/posts/shopbot_inch.pp"),
    ),
    ("uccnc_mm", include_str!("../../assets/posts/uccnc_mm.pp")),
    ("fanuc_mm", include_str!("../../assets/posts/fanuc_mm.pp")),
    (
        "heidenhain_mm",
        include_str!("../../assets/posts/heidenhain_mm.pp"),
    ),
    (
        "centroid_mm",
        include_str!("../../assets/posts/centroid_mm.pp"),
    ),
    (
        "drill_bank_mm",
        include_str!("../../assets/posts/drill_bank_mm.pp"),
    ),
    (
        "linuxcnc_manual_mm",
        include_str!("../../assets/posts/linuxcnc_manual_mm.pp"),
    ),
    (
        "linuxcnc_manual_inch",
        include_str!("../../assets/posts/linuxcnc_manual_inch.pp"),
    ),
    (
        "uccnc_manual_mm",
        include_str!("../../assets/posts/uccnc_manual_mm.pp"),
    ),
];

/// Every post the app can write with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostLibrary {
    user_dir: PathBuf,
}

impl PostLibrary {
    /// A library whose imported posts live in `user_dir`.
    pub fn at(user_dir: impl Into<PathBuf>) -> Self {
        PostLibrary {
            user_dir: user_dir.into(),
        }
    }

    /// The library in the user data directory, `Zeroed/posts`. Falls
    /// back to the temporary directory on a platform that has no data
    /// directory, so that construction never fails.
    pub fn user_default() -> Self {
        let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
        PostLibrary::at(base.join("Zeroed").join("posts"))
    }

    /// Where imported posts are kept.
    pub fn user_dir(&self) -> &FsPath {
        &self.user_dir
    }

    /// Every post, shipped first and then imported ones, both by identity so
    /// that the list is the same every time. An imported post whose identity
    /// matches a shipped one replaces it.
    ///
    /// # Errors
    /// Returns [`CoreError::Io`] when the user directory cannot be read.
    pub fn load_all(&self) -> Result<Vec<Post>, CoreError> {
        let mut posts: BTreeMap<String, Post> = BTreeMap::new();
        for (id, text) in SHIPPED_POSTS {
            let post = parse_post(id, text)?;
            posts.insert(post.id.clone(), post);
        }
        for path in self.user_files()? {
            let id = file_stem(&path);
            let text = std::fs::read_to_string(&path)
                .map_err(|error| CoreError::io(path.display(), error))?;
            let mut post = parse_post(&id, &text)?;
            post.source_path = path.display().to_string();
            posts.insert(post.id.clone(), post);
        }
        Ok(posts.into_values().collect())
    }

    /// A summary of every post, for the post list in the save dialog.
    ///
    /// # Errors
    /// Returns [`CoreError::Io`] when the user directory cannot be read.
    pub fn summaries(&self) -> Result<Vec<PostSummary>, CoreError> {
        Ok(self.load_all()?.iter().map(Post::summary).collect())
    }

    /// One post by identity.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when no post has that identity.
    pub fn get(&self, id: &str) -> Result<Post, CoreError> {
        for path in self.user_files()? {
            if file_stem(&path) == id {
                let text = std::fs::read_to_string(&path)
                    .map_err(|error| CoreError::io(path.display(), error))?;
                let mut post = parse_post(id, &text)?;
                post.source_path = path.display().to_string();
                return Ok(post);
            }
        }
        for (shipped, text) in SHIPPED_POSTS {
            if shipped == id {
                return parse_post(id, text);
            }
        }
        Err(CoreError::validation(format!(
            "no post processor named {id}"
        )))
    }

    /// Copies a post file into the user posts directory and answers its
    /// identity.
    ///
    /// # Errors
    /// Returns [`CoreError::Io`] when the file cannot be read or written and
    /// [`CoreError::Validation`] when it is not a post this build can read.
    pub fn import(&self, path: &FsPath) -> Result<String, CoreError> {
        let text =
            std::fs::read_to_string(path).map_err(|error| CoreError::io(path.display(), error))?;
        let id = file_stem(path);
        if id.is_empty() {
            return Err(CoreError::validation(
                "a post file needs a name to be imported under",
            ));
        }
        parse_post(&id, &text)?;
        std::fs::create_dir_all(&self.user_dir)
            .map_err(|error| CoreError::io(self.user_dir.display(), error))?;
        let target = self.user_dir.join(format!("{id}.{POST_EXTENSION}"));
        std::fs::write(&target, text).map_err(|error| CoreError::io(target.display(), error))?;
        Ok(id)
    }

    /// The text of a post file and where it was read from (empty for a
    /// shipped post), for the post processor manager's viewer.
    ///
    /// # Errors
    /// [`CoreError::Validation`] when no post has that identity.
    pub fn text(&self, id: &str) -> Result<(String, String), CoreError> {
        if let Some(path) = self.user_file(id)? {
            let text = std::fs::read_to_string(&path)
                .map_err(|error| CoreError::io(path.display(), error))?;
            return Ok((text, path.display().to_string()));
        }
        SHIPPED_POSTS
            .iter()
            .find(|(shipped, _)| *shipped == id)
            .map(|(_, text)| ((*text).to_string(), String::new()))
            .ok_or_else(|| CoreError::validation(format!("no post processor named {id}")))
    }

    /// Deletes a post the user installed. Shipped posts cannot be removed.
    ///
    /// # Errors
    /// [`CoreError::Validation`] when the user has no post of that identity
    /// and [`CoreError::Io`] when the file cannot be deleted.
    pub fn remove(&self, id: &str) -> Result<(), CoreError> {
        let path = self.user_file(id)?.ok_or_else(|| {
            CoreError::validation(format!(
                "{id} is not an installed post processor, so it cannot be removed"
            ))
        })?;
        std::fs::remove_file(&path).map_err(|error| CoreError::io(path.display(), error))
    }

    /// Copies a post into the user directory under a new identity so that it
    /// can be edited, naming it after the original with a numbered suffix.
    /// Answers the new identity.
    ///
    /// # Errors
    /// [`CoreError::Validation`] for an unknown post and [`CoreError::Io`]
    /// when the copy cannot be written.
    pub fn customize(&self, id: &str) -> Result<String, CoreError> {
        let (text, _) = self.text(id)?;
        let taken: Vec<String> = self.load_all()?.into_iter().map(|p| p.id).collect();
        let mut index = 1;
        let new_id = loop {
            let candidate = if index == 1 {
                format!("{id}_custom")
            } else {
                format!("{id}_custom_{index}")
            };
            if !taken.contains(&candidate) {
                break candidate;
            }
            index += 1;
        };
        std::fs::create_dir_all(&self.user_dir)
            .map_err(|error| CoreError::io(self.user_dir.display(), error))?;
        let target = self.user_dir.join(format!("{new_id}.{POST_EXTENSION}"));
        let text = mark_customized(&text);
        std::fs::write(&target, text).map_err(|error| CoreError::io(target.display(), error))?;
        Ok(new_id)
    }

    /// The file of a post the user installed or customised, `None` for a
    /// shipped post or an unknown identity. Only files in the user posts
    /// directory are ever answered.
    ///
    /// # Errors
    /// [`CoreError::Io`] when the user directory cannot be read.
    pub fn user_file(&self, id: &str) -> Result<Option<PathBuf>, CoreError> {
        Ok(self
            .user_files()?
            .into_iter()
            .find(|path| file_stem(path) == id))
    }

    /// The post files in the user directory, sorted by path.
    fn user_files(&self) -> Result<Vec<PathBuf>, CoreError> {
        if !self.user_dir.is_dir() {
            return Ok(Vec::new());
        }
        let entries = std::fs::read_dir(&self.user_dir)
            .map_err(|error| CoreError::io(self.user_dir.display(), error))?;
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| CoreError::io(self.user_dir.display(), error))?;
            let path = entry.path();
            let is_post = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(POST_EXTENSION));
            if is_post {
                files.push(path);
            }
        }
        files.sort();
        Ok(files)
    }
}

impl Default for PostLibrary {
    fn default() -> Self {
        PostLibrary::user_default()
    }
}

/// The post text with "Custom " put in front of its POST_NAME, so that the
/// copy and the original are told apart in the post lists.
fn mark_customized(text: &str) -> String {
    let mut done = false;
    let mut out = String::with_capacity(text.len() + 8);
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if !done && trimmed.starts_with("POST_NAME") {
            if let Some(quote) = line.find('"') {
                out.push_str(&line[..=quote]);
                out.push_str("Custom ");
                out.push_str(&line[quote + 1..]);
                done = true;
                continue;
            }
        }
        out.push_str(line);
    }
    out
}

fn file_stem(path: &FsPath) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn only_the_users_own_posts_have_a_file_to_edit() {
        let temp = TempDir::new("post-user-file");
        let library = PostLibrary::at(temp.path().join("posts"));
        assert_eq!(library.user_file("grbl_mm").expect("read"), None);
        let id = library.customize("grbl_mm").expect("customize");
        let file = library.user_file(&id).expect("read").expect("a file");
        assert!(file.starts_with(library.user_dir()));
        assert_eq!(library.user_file("../grbl_mm").expect("read"), None);
    }

    #[test]
    fn every_shipped_post_parses_and_is_usable() {
        for (id, text) in SHIPPED_POSTS {
            let post = parse_post(id, text)
                .unwrap_or_else(|error| panic!("shipped post {id} does not parse: {error}"));
            assert_eq!(post.id, id);
            assert!(!post.name.trim().is_empty(), "{id} has no name");
            assert!(!post.file_extension.is_empty(), "{id} has no extension");
            post.validate()
                .unwrap_or_else(|error| panic!("shipped post {id}: {error}"));
        }
    }

    #[test]
    fn every_shipped_post_ends_at_the_home_height() {
        // The moves end at home (20 above the surface, safe Z is 5): no
        // post may send the tool back down after it got there.
        use crate::cam::tool::Tool;
        use crate::cam::toolpath::{Move, Point3, StrategyKind, Toolpath};
        use crate::post::interp::{post_process, PostJob};
        let job = PostJob {
            home: Point3::new(0.0, 0.0, 20.0),
            safe_z: 5.0,
            ..PostJob::default()
        };
        for (id, text) in SHIPPED_POSTS {
            let post = parse_post(id, text).expect("parses");
            let mut tp = Toolpath::new(1, "p", Tool::default(), StrategyKind::Profile);
            tp.home = job.home;
            tp.moves = vec![
                Move::rapid(Point3::new(10.0, 10.0, 5.0)),
                Move::plunge(Point3::new(10.0, 10.0, -2.0)),
                Move::feed(Point3::new(40.0, 10.0, -2.0)),
                Move::rapid(Point3::new(40.0, 10.0, 5.0)),
                Move::rapid(Point3::new(40.0, 10.0, 20.0)),
                Move::rapid(Point3::new(0.0, 0.0, 20.0)),
            ];
            let out = post_process(&post, &job, &[tp]).expect("posts");
            let Some(last) = last_z(&out) else {
                // A laser post writes no heights at all.
                assert!(post.supports_jet(), "{id}: no Z in {out}");
                continue;
            };
            let home = if post.units == super::super::PostUnits::Inch {
                20.0 / 25.4
            } else {
                20.0
            };
            assert!((last - home).abs() < 1e-3, "{id} ends at Z {last}:\n{out}");
        }
    }

    #[test]
    fn the_linuxcnc_and_uccnc_posts_write_a_dwell_in_seconds() {
        // Both controls read G4 P as seconds, so a dwell of 1.5 s at the
        // bottom of a hole is P1.5 (milliseconds would be P1500, and a
        // pause of 25 minutes).
        use crate::cam::tool::Tool;
        use crate::cam::toolpath::{Move, Point3, StrategyKind, Toolpath};
        use crate::post::interp::{post_process, PostJob};
        let ids = [
            "linuxcnc_mm",
            "linuxcnc_inch",
            "linuxcnc_manual_mm",
            "linuxcnc_manual_inch",
            "uccnc_mm",
            "uccnc_manual_mm",
        ];
        for id in ids {
            let (_, text) = SHIPPED_POSTS
                .iter()
                .find(|(name, _)| *name == id)
                .expect("shipped");
            let post = parse_post(id, text).expect("parses");
            let mut tp = Toolpath::new(1, "holes", Tool::default(), StrategyKind::Drill);
            tp.moves = vec![
                Move::rapid(Point3::new(10.0, 10.0, 5.0)),
                Move::plunge(Point3::new(10.0, 10.0, -3.0)),
                Move::dwell(1.5, Point3::new(10.0, 10.0, -3.0)),
                Move::rapid(Point3::new(10.0, 10.0, 5.0)),
                Move::rapid(Point3::new(20.0, 10.0, 5.0)),
                Move::plunge(Point3::new(20.0, 10.0, -3.0)),
                Move::dwell(0.25, Point3::new(20.0, 10.0, -3.0)),
                Move::rapid(Point3::new(20.0, 10.0, 5.0)),
            ];
            let out = post_process(&post, &PostJob::default(), &[tp]).expect("posts");
            let dwells: Vec<&str> = out
                .lines()
                .map(|l| l.trim_start_matches(|c: char| c == 'N' || c.is_ascii_digit()))
                .filter(|l| l.starts_with("G4P"))
                .collect();
            assert_eq!(dwells, ["G4P1.50", "G4P0.25"], "{id}:\n{out}");
        }
    }

    #[test]
    fn every_g_code_post_writes_whole_words() {
        // Each word of a G code line is one letter and a number: no letter
        // may follow another (the tool length offset once came out G43HT1).
        use crate::cam::tool::Tool;
        use crate::cam::toolpath::{Move, Point3, StrategyKind, Toolpath};
        use crate::post::interp::{post_process, PostJob};
        for (id, text) in SHIPPED_POSTS {
            if id.starts_with("shopbot") || id.starts_with("heidenhain") {
                continue;
            }
            let post = parse_post(id, text).expect("parses");
            let mut tp = Toolpath::new(1, "p", Tool::default(), StrategyKind::Profile);
            tp.moves = vec![
                Move::rapid(Point3::new(10.0, 10.0, 5.0)),
                Move::plunge(Point3::new(10.0, 10.0, -2.0)),
                Move::feed(Point3::new(40.0, 10.0, -2.0)),
                Move::rapid(Point3::new(40.0, 10.0, 5.0)),
            ];
            let out = post_process(&post, &PostJob::default(), &[tp]).expect("posts");
            for line in out.lines() {
                let code = line.split(';').next().unwrap_or_default();
                let mut bare = String::new();
                let mut depth = 0;
                for c in code.chars() {
                    match c {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ if depth == 0 && !c.is_whitespace() && c != '%' => bare.push(c),
                        _ => {}
                    }
                }
                let chars: Vec<char> = bare.chars().collect();
                for (i, c) in chars.iter().enumerate() {
                    if c.is_ascii_alphabetic() {
                        let next = chars.get(i + 1);
                        assert!(
                            next.is_some_and(|n| n.is_ascii_digit() || matches!(n, '-' | '+' | '.')),
                            "{id}: {line:?} has a letter without a number"
                        );
                    }
                }
            }
        }
    }

    /// The last Z height a program writes (`Z5.000`, `Z+5.000`, `JZ,5.000`),
    /// leaving out a return to the machine's reference point (`G28`).
    fn last_z(text: &str) -> Option<f64> {
        let mut found = None;
        for line in text.lines().filter(|l| !l.contains("G28")) {
            for (i, b) in line.bytes().enumerate() {
                if b != b'Z' {
                    continue;
                }
                let rest = line[i + 1..].trim_start_matches([',', '+', ' ']);
                let end = rest
                    .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                    .unwrap_or(rest.len());
                if let Ok(value) = rest[..end].parse::<f64>() {
                    found = Some(value);
                }
            }
        }
        found
    }

    #[test]
    fn the_library_lists_the_shipped_posts_in_a_stable_order() {
        let temp = TempDir::new("posts-list");
        let library = PostLibrary::at(temp.path());
        let first = library.summaries().expect("lists");
        assert_eq!(first.len(), SHIPPED_POSTS.len());
        let ids: Vec<&str> = first.iter().map(|s| s.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
        let again = library.summaries().expect("lists");
        assert_eq!(first, again);
    }

    #[test]
    fn a_summary_says_what_the_post_can_do() {
        let library = PostLibrary::at(TempDir::new("posts-summary").path());
        let summaries = library.summaries().expect("lists");
        let grbl = summaries
            .iter()
            .find(|s| s.id == "grbl_mm")
            .expect("grbl is shipped");
        assert_eq!(grbl.file_extension, "gcode");
        assert!(grbl.arcs);
        assert!(!grbl.jet);
        let laser = summaries
            .iter()
            .find(|s| s.id == "laser_grbl")
            .expect("laser is shipped");
        assert!(laser.jet);
    }

    #[test]
    fn a_post_is_fetched_by_its_identity() {
        let library = PostLibrary::at(TempDir::new("posts-get").path());
        let post = library.get("mach3_mm").expect("mach3 is shipped");
        assert_eq!(post.id, "mach3_mm");
        assert!(library.get("not_a_post").is_err());
    }

    #[test]
    fn importing_copies_the_file_and_it_wins_over_a_shipped_post() {
        let source = TempDir::new("posts-source");
        let user = TempDir::new("posts-user");
        let text = "POST_NAME = \"Mine (mm) (*.nc)\"\nFILE_EXTENSION = nc\nUNITS = MM\n\
                    VAR X_POSITION = [X|A|X|1.3]\n\
                    begin RAPID_MOVE\n+ \"G0[X]\"\nend\n\
                    begin FEED_MOVE\n+ \"G1[X]\"\nend\n";
        let path = source.path().join("grbl_mm.pp");
        std::fs::write(&path, text).expect("fixture is written");
        let library = PostLibrary::at(user.path());
        let id = library.import(&path).expect("imports");
        assert_eq!(id, "grbl_mm");
        assert!(user.path().join("grbl_mm.pp").exists());
        let post = library.get("grbl_mm").expect("gets");
        assert_eq!(post.name, "Mine (mm) (*.nc)");
        assert!(!post.source_path.is_empty());
        // The list still holds one entry per identity.
        assert_eq!(
            library.summaries().expect("lists").len(),
            SHIPPED_POSTS.len()
        );
    }

    #[test]
    fn importing_a_file_that_is_not_a_post_is_refused() {
        let source = TempDir::new("posts-bad");
        let user = TempDir::new("posts-bad-user");
        let path = source.path().join("broken.pp");
        std::fs::write(&path, "this is not a post").expect("fixture is written");
        let library = PostLibrary::at(user.path());
        assert!(library.import(&path).is_err());
        assert!(!user.path().join("broken.pp").exists());
        assert!(library.import(&source.path().join("missing.pp")).is_err());
    }

    #[test]
    fn a_missing_user_directory_is_not_an_error() {
        let library = PostLibrary::at(PathBuf::from("/definitely/not/here/carve-posts"));
        assert_eq!(
            library.summaries().expect("lists").len(),
            SHIPPED_POSTS.len()
        );
    }

    #[test]
    fn the_user_library_sits_under_the_app_name() {
        let library = PostLibrary::user_default();
        let text = library.user_dir().display().to_string();
        assert!(text.contains("Zeroed"), "{text}");
        assert!(text.ends_with("posts"), "{text}");
    }

    #[test]
    fn a_post_can_be_viewed_customized_and_removed() {
        let user = TempDir::new("posts-manage");
        let library = PostLibrary::at(user.path());
        let (text, path) = library.text("grbl_mm").expect("shipped text");
        assert!(text.contains("POST_NAME"));
        assert!(path.is_empty());
        assert!(library.remove("grbl_mm").is_err(), "shipped posts stay");
        let copy = library.customize("grbl_mm").expect("customize");
        assert_eq!(copy, "grbl_mm_custom");
        let again = library.customize("grbl_mm").expect("customize");
        assert_eq!(again, "grbl_mm_custom_2");
        let (copied, path) = library.text(&copy).expect("user text");
        assert_eq!(copied.lines().count(), text.lines().count());
        let post = library.get(&copy).expect("the copy parses");
        let original = library.get("grbl_mm").expect("original");
        assert_eq!(post.name, format!("Custom {}", original.name));
        assert!(path.ends_with("grbl_mm_custom.pp"), "{path}");
        let summaries = library.summaries().expect("lists");
        assert_eq!(summaries.len(), SHIPPED_POSTS.len() + 2);
        library.remove(&copy).expect("removes");
        assert!(library.text(&copy).is_err());
        assert!(library.text("nothing").is_err());
    }
}
