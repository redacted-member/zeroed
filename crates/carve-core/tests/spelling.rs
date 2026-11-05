//! The text the kernel ships for people to read is US English: the bundled
//! gadgets' manifests and dialog text, the default tool database and the
//! English fallback sentences of kernel warnings. Keys, codes and command
//! names are identifiers and are not checked.

use std::path::{Path, PathBuf};

use serde_json::Value;

const BRITISH: &[&str] = &[
    "centre",
    "centres",
    "centred",
    "centring",
    "colour",
    "grey",
    "modell",
    "mould",
    "metre",
    "licence",
    "behaviour",
    "favourite",
    "optimis",
    "organis",
    "analyse",
    "analysed",
    "analysing",
    "neighbour",
    "cancelled",
    "cancelling",
    "labelled",
    "labelling",
    "fibre",
    "aluminium",
    "dialogue",
    "catalogue",
];

/// The British forms in `text`: whole words for the short ones, any word
/// holding the stem for the others.
fn british(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|word| {
            BRITISH.iter().any(|form| match *form {
                "colour" | "modell" | "mould" | "metre" | "licence" | "behaviour" | "favourite"
                | "optimis" | "organis" | "neighbour" | "fibre" | "aluminium" | "dialogue"
                | "catalogue" => word.contains(form),
                "grey" => word.starts_with("grey"),
                _ => *word == *form,
            })
        })
        .map(str::to_string)
        .collect()
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every string value in `value`, object keys left out.
fn json_strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => out.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|v| json_strings(v, out)),
        Value::Object(map) => map.values().for_each(|v| json_strings(v, out)),
        _ => {}
    }
}

/// The double quoted literals of a source file, line comments (starting
/// with `comment`) left out. Literals without a space are codes and names.
fn prose_literals(source: &str, comment: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in source.lines() {
        let mut chars = line.chars().peekable();
        let mut rest = line;
        while let Some(c) = chars.next() {
            match current.as_mut() {
                Some(text) => match c {
                    '\\' => {
                        chars.next();
                    }
                    '"' => {
                        let done = current.take().unwrap_or_default();
                        if done.contains(' ') {
                            out.push(done);
                        }
                    }
                    _ => text.push(c),
                },
                None => {
                    if rest.trim_start().starts_with(comment) {
                        break;
                    }
                    if c == '"' {
                        current = Some(String::new());
                    }
                }
            }
            rest = &rest[c.len_utf8()..];
        }
        if let Some(text) = current.as_mut() {
            text.push(' ');
        }
    }
    out
}

fn check(name: &str, texts: &[String]) -> Vec<String> {
    texts
        .iter()
        .filter(|t| !british(t).is_empty())
        .map(|t| format!("{name}: {t}"))
        .collect()
}

#[test]
fn the_check_finds_british_forms_only() {
    assert_eq!(british("the Centre and a colour"), ["centre", "colour"]);
    assert!(british("center, color, gray, diameter, analysis").is_empty());
    assert_eq!(
        prose_literals(
            "let a = \"a b\"; // \"x centre\"\n\"code.x\" \"c \\\" d\"",
            "//"
        ),
        ["a b", "c  d"]
    );
}

#[test]
fn bundled_text_uses_us_spelling() {
    let mut bad = Vec::new();
    let gadgets = root().join("src/gadgets/bundled");
    let mut dirs: Vec<_> = std::fs::read_dir(&gadgets)
        .expect("bundled gadgets")
        .map(|e| e.expect("entry").path())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty());
    for dir in dirs {
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .expect("gadget folder")
            .map(|e| e.expect("entry").path())
            .collect();
        files.sort();
        for file in files {
            let text = read(&file);
            let name = file.display().to_string();
            match file.extension().and_then(|e| e.to_str()) {
                Some("json") => {
                    let mut strings = Vec::new();
                    json_strings(&serde_json::from_str(&text).expect("json"), &mut strings);
                    bad.extend(check(&name, &strings));
                }
                Some("lua") => bad.extend(check(&name, &prose_literals(&text, "--"))),
                _ => {}
            }
        }
    }
    let tools = root().join("assets/tools/default.json");
    let mut strings = Vec::new();
    json_strings(
        &serde_json::from_str(&read(&tools)).expect("json"),
        &mut strings,
    );
    bad.extend(check("tools/default.json", &strings));
    let warnings = read(&root().join("src/warning.rs"));
    bad.extend(check("warning.rs", &prose_literals(&warnings, "//")));
    assert!(bad.is_empty(), "British spellings:\n{}", bad.join("\n"));
}
