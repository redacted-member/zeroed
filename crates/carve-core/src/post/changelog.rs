//! The change log of a post processor: the History table in its leading
//! comments, one row per change with who made it, when and what changed.
//!
//! ```text
//! ' History
//! ' Who          When        What
//! ' ===========  ==========  ==================================
//! ' CarveStudio  2026-09-20  First version.
//! '                          A line that carries on the change above.
//! ```
//!
//! Columns are split on runs of two or more spaces (or tabs). A row with a
//! single space between who and a date still reads, and a row with one
//! column carries on the change above it. The table ends at the first line
//! that is not a comment, or at an empty comment once rows have started.

use serde::Serialize;

/// One row of a post's change log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeEntry {
    /// Who made the change.
    pub who: String,
    /// When, as written in the post.
    pub when: String,
    /// What changed.
    pub what: String,
}

/// The change log in `text`, oldest row first as written; empty when the
/// post has no History table.
pub fn change_log(text: &str) -> Vec<ChangeEntry> {
    let mut entries: Vec<ChangeEntry> = Vec::new();
    let mut in_history = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(comment) = trimmed.strip_prefix('\'') else {
            if in_history {
                break;
            }
            continue;
        };
        let content = comment.trim();
        if !in_history {
            let lower = content.to_ascii_lowercase();
            in_history = lower == "history" || lower == "history:";
            continue;
        }
        if content.is_empty() {
            if entries.is_empty() {
                continue;
            }
            break;
        }
        if is_rule(content) || is_header(content) {
            continue;
        }
        let fields = columns(content);
        match fields.as_slice() {
            [who, when, rest @ ..] if !rest.is_empty() => entries.push(ChangeEntry {
                who: who.clone(),
                when: when.clone(),
                what: rest.join(" "),
            }),
            _ => match single_spaced(content) {
                Some(entry) => entries.push(entry),
                None => match entries.last_mut() {
                    Some(last) => {
                        if !last.what.is_empty() {
                            last.what.push(' ');
                        }
                        last.what.push_str(content);
                    }
                    None => entries.push(ChangeEntry {
                        who: String::new(),
                        when: String::new(),
                        what: content.to_string(),
                    }),
                },
            },
        }
    }
    entries
}

/// A separator line such as `=====  ====` or `-----`.
fn is_rule(content: &str) -> bool {
    content
        .chars()
        .all(|c| c == '=' || c == '-' || c.is_whitespace())
}

/// The column titles row.
fn is_header(content: &str) -> bool {
    let words: Vec<String> = content
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    words == ["who", "when", "what"]
}

/// Columns split on runs of two or more spaces or on tabs.
fn columns(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut spaces = 0;
    for c in content.chars() {
        if c == '\t' {
            spaces = 2;
            continue;
        }
        if c == ' ' {
            spaces += 1;
            continue;
        }
        if spaces >= 2 && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        } else if spaces == 1 && !current.is_empty() {
            current.push(' ');
        }
        spaces = 0;
        current.push(c);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// `Who When What` with single spaces, read when the second word looks like
/// a date (digits with `/`, `-` or `.` between them).
fn single_spaced(content: &str) -> Option<ChangeEntry> {
    let mut words = content.splitn(3, ' ');
    let who = words.next()?;
    let when = words.next()?;
    let what = words.next()?.trim();
    let date_like = when.chars().any(|c| c.is_ascii_digit())
        && when.chars().any(|c| matches!(c, '/' | '-' | '.'))
        && when
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '/' | '-' | '.'));
    (date_like && !what.is_empty()).then(|| ChangeEntry {
        who: who.to_string(),
        when: when.to_string(),
        what: what.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(who: &str, when: &str, what: &str) -> ChangeEntry {
        ChangeEntry {
            who: who.to_string(),
            when: when.to_string(),
            what: what.to_string(),
        }
    }

    #[test]
    fn a_history_table_becomes_rows() {
        let text = "' A post for testing.\n'\n' History\n' Who          When        What\n' ===========  ==========  =====\n' CarveStudio  2026-09-20  First version.\n' Sam         2026-09-24  Arcs as helices.\n'                          Also the header.\n\nPOST_NAME = \"x\"\n";
        assert_eq!(
            change_log(text),
            vec![
                entry("CarveStudio", "2026-09-20", "First version."),
                entry("Sam", "2026-09-24", "Arcs as helices. Also the header."),
            ]
        );
    }

    #[test]
    fn single_spaced_rows_and_tabs_still_read() {
        let text =
            "' History:\n' Mark 25/03/2008 Added support for arcs\n' Tony\t14.07.2006\tWritten\n";
        assert_eq!(
            change_log(text),
            vec![
                entry("Mark", "25/03/2008", "Added support for arcs"),
                entry("Tony", "14.07.2006", "Written"),
            ]
        );
    }

    #[test]
    fn a_post_without_history_has_no_change_log() {
        assert!(change_log("' Just a comment\nPOST_NAME = \"x\"\n").is_empty());
    }

    #[test]
    fn every_shipped_post_has_a_change_log() {
        for (id, text) in crate::post::library::SHIPPED_POSTS {
            let log = change_log(text);
            assert!(!log.is_empty(), "{id} has no History table");
            assert_eq!(log[0].what, "First version.", "{id}");
        }
    }
}
