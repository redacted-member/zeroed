//! What Save Toolpaths writes: which toolpaths go in which file, and the
//! checks a file holding several tools must pass.
//!
//! Three outputs, as the Save Toolpaths form offers them: the selected
//! toolpath on its own; the visible toolpaths in one file, which needs a
//! post that can change tools once more than one tool is used and a
//! different tool number for each cutter; or the visible toolpaths in a
//! file each, optionally grouping neighbours cut with the same tool into
//! one file.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::post::Post;

use super::toolpath::Toolpath;

/// How the chosen toolpaths are spread over files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveOutput {
    /// One toolpath, one file.
    Selected,
    /// Every chosen toolpath in one file.
    #[default]
    OneFile,
    /// A file for each toolpath.
    MultipleFiles,
}

/// One file to write: the text added to the chosen file name (none for the
/// name itself) and the toolpaths in it, in cutting order.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedFile<'a> {
    pub suffix: Option<String>,
    pub toolpaths: Vec<&'a Toolpath>,
}

/// True when two toolpaths use the same cutter: same name and shape. Feeds
/// and speeds may differ between them.
pub fn same_cutter(a: &Toolpath, b: &Toolpath) -> bool {
    a.tool.name == b.tool.name && a.tool.tool_type == b.tool.tool_type
}

/// True when the machine keeps the same tool in going from `a` to `b`.
pub fn same_tool(a: &Toolpath, b: &Toolpath) -> bool {
    a.tool_number == b.tool_number && same_cutter(a, b)
}

/// The files `output` writes for `toolpaths`. Grouping puts neighbours cut
/// with the same tool in one file named by their places in the list, as in
/// `_1-3`.
///
/// # Errors
/// [`CoreError::Validation`] when nothing is chosen, or more than one
/// toolpath for the selected toolpath output.
pub fn plan(
    toolpaths: &[Toolpath],
    output: SaveOutput,
    group_where_possible: bool,
) -> Result<Vec<PlannedFile<'_>>, CoreError> {
    if toolpaths.is_empty() {
        return Err(CoreError::validation("no toolpaths were chosen to save"));
    }
    match output {
        SaveOutput::Selected if toolpaths.len() > 1 => Err(CoreError::validation(
            "select one toolpath to save it on its own",
        )),
        SaveOutput::Selected | SaveOutput::OneFile => Ok(vec![PlannedFile {
            suffix: None,
            toolpaths: toolpaths.iter().collect(),
        }]),
        SaveOutput::MultipleFiles if !group_where_possible => Ok(distinct(
            toolpaths
                .iter()
                .map(|t| PlannedFile {
                    suffix: Some(safe_file_name(&t.name)),
                    toolpaths: vec![t],
                })
                .collect(),
        )),
        SaveOutput::MultipleFiles => {
            let mut runs: Vec<(usize, Vec<&Toolpath>)> = Vec::new();
            for (index, toolpath) in toolpaths.iter().enumerate() {
                match runs.last_mut() {
                    Some((_, run))
                        if run.last().is_some_and(|last| {
                            same_tool(last, toolpath)
                                && last.side == toolpath.side
                                && last.sheet == toolpath.sheet
                        }) =>
                    {
                        run.push(toolpath);
                    }
                    _ => runs.push((index, vec![toolpath])),
                }
            }
            Ok(distinct(
                runs.into_iter()
                    .map(|(first, run)| {
                        let suffix = if run.len() == 1 {
                            safe_file_name(&run[0].name)
                        } else {
                            format!("{}-{}", first + 1, first + run.len())
                        };
                        PlannedFile {
                            suffix: Some(suffix),
                            toolpaths: run,
                        }
                    })
                    .collect(),
            ))
        }
    }
}

/// Makes the files' suffixes differ, ignoring case as the disk may: a
/// suffix seen before gets `_2`, `_3` and so on, so no file overwrites
/// another written in the same save.
fn distinct(mut files: Vec<PlannedFile<'_>>) -> Vec<PlannedFile<'_>> {
    let mut seen = std::collections::BTreeSet::new();
    for file in &mut files {
        let Some(base) = file.suffix.clone() else {
            continue;
        };
        let mut suffix = base.clone();
        let mut n = 1;
        while !seen.insert(suffix.to_lowercase()) {
            n += 1;
            suffix = format!("{base}_{n}");
        }
        file.suffix = Some(suffix);
    }
    files
}

/// Checks one file's toolpaths against the post: more than one tool needs a
/// post with a tool change, and each cutter its own tool number.
///
/// # Errors
/// [`CoreError::Validation`] naming the toolpaths that clash.
pub fn check_tool_changes(post: &Post, toolpaths: &[&Toolpath]) -> Result<(), CoreError> {
    for (i, a) in toolpaths.iter().enumerate() {
        for b in &toolpaths[i + 1..] {
            if a.tool_number == b.tool_number && !same_cutter(a, b) {
                return Err(CoreError::validation(format!(
                    "{} ({}) and {} ({}) both use tool number {}: give each cutter its own number",
                    a.name, a.tool.name, b.name, b.tool.name, a.tool_number
                )));
            }
        }
    }
    let changes = toolpaths.windows(2).any(|w| !same_tool(w[0], w[1]));
    if changes && !post.supports_tool_change() {
        return Err(CoreError::validation(format!(
            "the {} post processor cannot change tools: save each tool's toolpaths to its own file",
            post.name
        )));
    }
    Ok(())
}

/// Turns a toolpath name into something every file system accepts.
pub fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "toolpath".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::Tool;
    use crate::cam::toolpath::StrategyKind;
    use crate::post::parser::parse_post;

    fn toolpath(name: &str, tool: &str, number: u32) -> Toolpath {
        let tool = Tool {
            name: tool.to_string(),
            ..Tool::default()
        };
        let mut t = Toolpath::new(1, name, tool, StrategyKind::Profile);
        t.tool_number = number;
        t
    }

    #[test]
    fn neighbours_with_one_tool_share_a_file_when_grouping() {
        let list = vec![
            toolpath("Rough", "End Mill 6", 1),
            toolpath("Pocket", "End Mill 6", 1),
            toolpath("V Carve", "V Bit", 2),
            toolpath("Profile", "End Mill 6", 1),
        ];
        let grouped = plan(&list, SaveOutput::MultipleFiles, true).expect("plan");
        let suffixes: Vec<_> = grouped.iter().filter_map(|f| f.suffix.clone()).collect();
        assert_eq!(suffixes, ["1-2", "V_Carve", "Profile"]);
        assert_eq!(grouped[0].toolpaths.len(), 2);
        let each = plan(&list, SaveOutput::MultipleFiles, false).expect("plan");
        assert_eq!(each.len(), 4);
        assert_eq!(each[0].suffix.as_deref(), Some("Rough"));
        let one = plan(&list, SaveOutput::OneFile, true).expect("plan");
        assert_eq!(one.len(), 1);
        assert!(one[0].suffix.is_none());
        assert!(plan(&list, SaveOutput::Selected, false).is_err());
        assert!(plan(&list[..1], SaveOutput::Selected, false).is_ok());
        assert!(plan(&[], SaveOutput::OneFile, false).is_err());
    }

    #[test]
    fn grouping_keeps_each_sheet_in_its_own_file() {
        let first = toolpath("Parts", "End Mill 6", 1);
        let mut second = toolpath("More parts", "End Mill 6", 1);
        second.sheet = 2;
        let list = vec![first, second];
        let grouped = plan(&list, SaveOutput::MultipleFiles, true).expect("plan");
        assert_eq!(grouped.len(), 2);
    }

    #[test]
    fn one_file_with_several_tools_needs_a_tool_change_and_distinct_numbers() {
        let shipped = |id: &str| {
            let (_, text) = crate::post::SHIPPED_POSTS
                .iter()
                .find(|(name, _)| *name == id)
                .expect("shipped post");
            parse_post(id, text).expect("post")
        };
        let changer = shipped("grbl_mm");
        let plain = shipped("laser_grbl");
        let a = toolpath("One", "End Mill 6", 1);
        let b = toolpath("Two", "V Bit", 2);
        let clash = toolpath("Three", "Ball Nose", 1);
        let same = toolpath("Four", "End Mill 6", 1);
        assert!(check_tool_changes(&changer, &[&a, &b]).is_ok());
        let error = check_tool_changes(&plain, &[&a, &b]).expect_err("no tool change");
        assert!(error.to_string().contains("cannot change tools"), "{error}");
        assert!(check_tool_changes(&plain, &[&a, &same]).is_ok());
        let error = check_tool_changes(&changer, &[&a, &clash]).expect_err("clash");
        assert!(error.to_string().contains("tool number 1"), "{error}");
    }

    #[test]
    fn every_file_gets_its_own_name_even_when_toolpath_names_clash() {
        let list = vec![
            toolpath("\u{8f2a}\u{90ed}", "End Mill 6", 1),
            toolpath("\u{30dd}\u{30b1}\u{30c3}\u{30c8}", "V Bit", 2),
            toolpath("Profile 1", "End Mill 6", 1),
            toolpath("Profile_1", "V Bit", 2),
            toolpath("profile 1", "End Mill 6", 1),
        ];
        for group in [false, true] {
            let files = plan(&list, SaveOutput::MultipleFiles, group).expect("plan");
            let mut names: Vec<String> = files
                .iter()
                .map(|f| f.suffix.clone().expect("suffix").to_lowercase())
                .collect();
            let count = names.len();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), count, "{names:?}");
        }
    }
}
