//! Document variables: named numbers, or live formulas such as `{w}/2+1`
//! that read other variables and are worked out again whenever what they
//! read changes.
//!
//! Formulas use the syntax of the calculation edit boxes: numbers,
//! `+ - * / ^`, brackets, variables as `{name}` (or `[name]`), and in a
//! calculation the letters W or X (material width), H or Y (height), T or Z
//! (thickness), P (pi), I (millimetres to inches), M (inches to
//! millimetres) and `'` after a number for feet, all in the job's unit.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::job::{JobSetup, Units};
use crate::error::CoreError;

/// How a variable is defined: a number, or a formula kept as typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Definition {
    /// A plain value.
    Number(f64),
    /// A formula such as `{w}/2+1`; a text that is a plain number reads as one.
    Formula(String),
}

/// What the calculation letters stand for, in the job's unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constants {
    /// Material width.
    pub width: f64,
    /// Material height.
    pub height: f64,
    /// Material thickness.
    pub thickness: f64,
    /// One foot (12 in an inch job, 304.8 in a millimetre job).
    pub foot: f64,
}

impl Constants {
    /// The letters of `job`, in its unit.
    pub fn of(job: &JobSetup) -> Self {
        let scale = match job.units {
            Units::Mm => 1.0,
            Units::Inch => 1.0 / 25.4,
        };
        Constants {
            width: job.width * scale,
            height: job.height * scale,
            thickness: job.thickness * scale,
            foot: match job.units {
                Units::Mm => 304.8,
                Units::Inch => 12.0,
            },
        }
    }
}

/// The resolved variables: every value, and the formulas of those defined by one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resolved {
    /// Every variable's value.
    pub values: BTreeMap<String, f64>,
    /// The formula of each variable defined by one, as typed.
    pub formulas: BTreeMap<String, String>,
}

/// Whether `name` is a variable name: a letter, then letters, digits or underscores.
pub fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Works out every variable from its definition, formulas after the
/// variables they read.
///
/// # Errors
/// [`CoreError::Validation`] with a plain message for a bad name, a value
/// that is not finite, a formula that does not read, one that uses a name
/// no variable has, or formulas that refer to themselves round a loop.
pub fn resolve(
    definitions: &BTreeMap<String, Definition>,
    constants: &Constants,
) -> Result<Resolved, CoreError> {
    let mut out = Resolved::default();
    let mut formulas: BTreeMap<&str, (String, Vec<String>)> = BTreeMap::new();
    for (name, definition) in definitions {
        if !is_name(name) {
            return Err(CoreError::validation(format!(
                "{name:?} is not a variable name: use letters, digits and underscores, starting with a letter"
            )));
        }
        match definition {
            Definition::Number(value) => {
                if !value.is_finite() {
                    return Err(CoreError::validation(format!("{name} must be a number")));
                }
                out.values.insert(name.clone(), *value);
            }
            Definition::Formula(text) => {
                let text = text.trim();
                if let Ok(value) = text.parse::<f64>() {
                    if value.is_finite() {
                        out.values.insert(name.clone(), value);
                        continue;
                    }
                }
                let Some(list) = tokens(text, true) else {
                    return Err(not_a_number(name, text));
                };
                let mut uses: Vec<String> = Vec::new();
                for token in &list {
                    if let Token::Name(used) = token {
                        if !definitions.contains_key(used) {
                            return Err(CoreError::validation(format!(
                                "{name} uses {{{used}}}, which is not a variable"
                            )));
                        }
                        if !uses.contains(used) {
                            uses.push(used.clone());
                        }
                    }
                }
                formulas.insert(name, (text.to_string(), uses));
            }
        }
    }
    // Depth first in name order: a formula is worked out after the ones it
    // reads; meeting one still open on the way down is a loop.
    let mut open: Vec<&str> = Vec::new();
    let mut done: BTreeSet<&str> = BTreeSet::new();
    for name in formulas.keys() {
        visit(name, &formulas, constants, &mut open, &mut done, &mut out)?;
    }
    Ok(out)
}

fn visit<'a>(
    name: &'a str,
    formulas: &'a BTreeMap<&'a str, (String, Vec<String>)>,
    constants: &Constants,
    open: &mut Vec<&'a str>,
    done: &mut BTreeSet<&'a str>,
    out: &mut Resolved,
) -> Result<(), CoreError> {
    if done.contains(name) {
        return Ok(());
    }
    let Some((text, uses)) = formulas.get(name) else {
        return Ok(());
    };
    if let Some(at) = open.iter().position(|n| *n == name) {
        let mut round: Vec<&str> = open[at..].to_vec();
        round.push(name);
        return Err(CoreError::validation(if round.len() == 2 {
            format!("{name} refers to itself")
        } else {
            format!(
                "{} refer to each other in a loop ({})",
                round[..round.len() - 1].join(", "),
                round.join(" > ")
            )
        }));
    }
    open.push(name);
    for used in uses {
        visit(used, formulas, constants, open, done, out)?;
    }
    open.pop();
    let value = evaluate(text, &out.values, constants).ok_or_else(|| not_a_number(name, text))?;
    out.values.insert(name.to_string(), value);
    out.formulas.insert(name.to_string(), text.clone());
    done.insert(name);
    Ok(())
}

fn not_a_number(name: &str, text: &str) -> CoreError {
    CoreError::validation(format!("{name} = {text} does not work out to a number"))
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Letter(char),
    Name(String),
    Op(char),
}

/// The tokens of `body`; letters are kept only when `letters` is set.
fn tokens(text: &str, letters: bool) -> Option<Vec<Token>> {
    let body = text.trim();
    let body = body.strip_prefix('=').unwrap_or(body);
    let body = body.strip_suffix('=').unwrap_or(body);
    let chars: Vec<char> = body.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next_is_word = chars
            .get(i + 1)
            .is_some_and(|n| n.is_ascii_alphanumeric() || *n == '_');
        if c == ' ' || c == '\t' {
            i += 1;
        } else if "+-*/^()'".contains(c) {
            out.push(Token::Op(c));
            i += 1;
        } else if c.is_ascii_alphabetic() && !next_is_word {
            if !letters {
                return None;
            }
            out.push(Token::Letter(c.to_ascii_lowercase()));
            i += 1;
        } else if c == '{' || c == '[' {
            let close = if c == '{' { '}' } else { ']' };
            let end = chars[i..].iter().position(|n| *n == close)? + i;
            let name: String = chars[i + 1..end].iter().collect();
            out.push(Token::Name(name.trim().to_string()));
            i = end + 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < chars.len()
                && (chars[i] == 'e' || chars[i] == 'E')
                && chars
                    .get(i + 1)
                    .is_some_and(|n| n.is_ascii_digit() || *n == '+' || *n == '-')
            {
                i += 2;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let number: String = chars[start..i].iter().collect();
            out.push(Token::Number(number.parse().ok()?));
        } else {
            return None;
        }
    }
    Some(out)
}

/// The value of a calculation, or None when it does not read, names a
/// variable `values` lacks, divides by zero or is not finite. A letter
/// alone is not a number, as in the edit boxes.
pub fn evaluate(text: &str, values: &BTreeMap<String, f64>, constants: &Constants) -> Option<f64> {
    let trimmed = text.trim();
    let calculation = trimmed
        .strip_prefix('-')
        .unwrap_or(trimmed)
        .contains(['-', '+', '*', '/', '^', '(', ')', '\'', '=']);
    let list = tokens(trimmed, calculation)?;
    if list.is_empty() {
        return None;
    }
    let mut parser = Parser {
        list: &list,
        at: 0,
        values,
        constants,
    };
    let value = parser.expr()?;
    (parser.at == list.len() && value.is_finite()).then_some(value)
}

struct Parser<'a> {
    list: &'a [Token],
    at: usize,
    values: &'a BTreeMap<String, f64>,
    constants: &'a Constants,
}

impl Parser<'_> {
    fn is_op(&self, op: char) -> bool {
        matches!(self.list.get(self.at), Some(Token::Op(o)) if *o == op)
    }

    fn expr(&mut self) -> Option<f64> {
        let mut left = self.term()?;
        while self.is_op('+') || self.is_op('-') {
            let minus = self.is_op('-');
            self.at += 1;
            let right = self.term()?;
            left = if minus { left - right } else { left + right };
        }
        Some(left)
    }

    fn term(&mut self) -> Option<f64> {
        let mut left = self.unary()?;
        while self.is_op('*') || self.is_op('/') {
            let divide = self.is_op('/');
            self.at += 1;
            let right = self.unary()?;
            if divide && right == 0.0 {
                return None;
            }
            left = if divide { left / right } else { left * right };
        }
        Some(left)
    }

    fn unary(&mut self) -> Option<f64> {
        if self.is_op('-') || self.is_op('+') {
            let minus = self.is_op('-');
            self.at += 1;
            let value = self.unary()?;
            return Some(if minus { -value } else { value });
        }
        self.power()
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.feet()?;
        if !self.is_op('^') {
            return Some(base);
        }
        self.at += 1;
        let exponent = self.unary()?;
        Some(base.powf(exponent))
    }

    fn feet(&mut self) -> Option<f64> {
        let value = self.atom()?;
        if self.is_op('\'') {
            self.at += 1;
            return Some(value * self.constants.foot);
        }
        Some(value)
    }

    fn atom(&mut self) -> Option<f64> {
        let token = self.list.get(self.at)?.clone();
        self.at += 1;
        match token {
            Token::Number(value) => Some(value),
            Token::Name(name) => self.values.get(&name).copied(),
            Token::Letter(letter) => match letter {
                'w' | 'x' => Some(self.constants.width),
                'h' | 'y' => Some(self.constants.height),
                't' | 'z' => Some(self.constants.thickness),
                'p' => Some(std::f64::consts::PI),
                'i' => Some(1.0 / 25.4),
                'm' => Some(25.4),
                _ => None,
            },
            Token::Op('(') => {
                let inner = self.expr()?;
                if !self.is_op(')') {
                    return None;
                }
                self.at += 1;
                Some(inner)
            }
            Token::Op(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MM: Constants = Constants {
        width: 300.0,
        height: 200.0,
        thickness: 18.0,
        foot: 304.8,
    };

    fn defs(list: &[(&str, Definition)]) -> BTreeMap<String, Definition> {
        list.iter()
            .map(|(name, d)| ((*name).to_string(), d.clone()))
            .collect()
    }

    fn f(text: &str) -> Definition {
        Definition::Formula(text.to_string())
    }

    #[test]
    fn calculations_read_as_the_edit_boxes_do() {
        let values: BTreeMap<String, f64> = [("w".to_string(), 10.0)].into_iter().collect();
        let eval = |text: &str| evaluate(text, &values, &MM);
        assert_eq!(eval("{w}/2+1"), Some(6.0));
        assert_eq!(eval("[w] * 3"), Some(30.0));
        assert_eq!(eval("2^3"), Some(8.0));
        assert_eq!(eval("-(1+2)*2"), Some(-6.0));
        assert_eq!(eval("W/2="), Some(150.0));
        assert_eq!(eval("1'"), Some(304.8));
        assert_eq!(eval("1e2+1"), Some(101.0));
        // A letter alone is not a number; names must exist; no division by zero.
        assert_eq!(eval("W"), None);
        assert_eq!(eval("{v}+1"), None);
        assert_eq!(eval("1/0"), None);
        assert_eq!(eval("1+"), None);
        assert_eq!(eval("abc"), None);
    }

    #[test]
    fn formulas_resolve_after_what_they_read() {
        let resolved = resolve(
            &defs(&[
                ("a", f("{b}*2")),
                ("b", f("{w}/2+1")),
                ("w", Definition::Number(10.0)),
                ("t", f("T+1")),
                ("n", f(" 4 ")),
            ]),
            &MM,
        )
        .expect("resolves");
        assert_eq!(resolved.values["b"], 6.0);
        assert_eq!(resolved.values["a"], 12.0);
        assert_eq!(resolved.values["t"], 19.0);
        assert_eq!(resolved.values["n"], 4.0);
        assert_eq!(resolved.formulas["b"], "{w}/2+1");
        // A text that is a plain number is kept as a number.
        assert!(!resolved.formulas.contains_key("n"));
    }

    #[test]
    fn loops_and_unknown_names_are_refused_plainly() {
        let message = |list: &[(&str, Definition)]| match resolve(&defs(list), &MM) {
            Err(CoreError::Validation(text)) => text,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert_eq!(message(&[("a", f("{a}+1"))]), "a refers to itself");
        assert_eq!(
            message(&[("a", f("{b}+1")), ("b", f("{a}*2"))]),
            "a, b refer to each other in a loop (a > b > a)"
        );
        assert_eq!(
            message(&[("a", f("{zz}+1"))]),
            "a uses {zz}, which is not a variable"
        );
        assert_eq!(
            message(&[("a", f("2+*"))]),
            "a = 2+* does not work out to a number"
        );
        assert!(message(&[("1a", Definition::Number(1.0))]).contains("not a variable name"));
    }

    #[test]
    fn inch_jobs_read_letters_in_inches() {
        let job = JobSetup {
            units: Units::Inch,
            width: 254.0,
            ..JobSetup::default()
        };
        let c = Constants::of(&job);
        assert!((c.width - 10.0).abs() < 1e-9);
        assert_eq!(c.foot, 12.0);
    }
}
