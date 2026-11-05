//! Wrapping flat toolpaths around a rotary axis at post time.
//!
//! A rotary job is designed and machined on the unwrapped surface of the
//! blank. The coordinate across the axis runs one circumference, pi times the
//! diameter, around the blank, so when the program is written that distance
//! becomes an angle: `distance / (pi * diameter) * 360` degrees. A blank lying
//! along X wraps flat Y; a blank lying along Y wraps flat X. Z is written as
//! it is, measured from whichever Z zero the job set (the axis or the
//! surface), and the wrapped linear axis stays on the axis line, at zero.
//!
//! A post says which flat axis it wraps and which letter takes the angle with
//! `ROTARY_WRAP_Y = A` and or `ROTARY_WRAP_X = A` (or `B`), and writes the
//! angle through a variable on the `A` or `B` token. The older `ROTARY_WRAP =
//! A` is `ROTARY_WRAP_Y = A` and `ROTARY_WRAP = B` is `ROTARY_WRAP_X = B`.
//! `MAXIMUM_ROTATION` breaks longer turns into pieces, and a multiplier on the
//! variable (`[A|A|A|1.3|-1]`) reverses an axis wired the other way.

use serde::{Deserialize, Serialize};

use crate::cam::toolpath::Point3;
use crate::doc::{RotaryAxis, RotarySetup};
use crate::error::CoreError;

use super::{Post, ValueSource};

/// The letter a post writes a wrapped angle on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RotaryWord {
    /// The A word.
    A,
    /// The B word.
    B,
}

impl RotaryWord {
    /// Reads the value of a `ROTARY_WRAP` setting.
    pub fn from_name(name: &str) -> Option<RotaryWord> {
        match name.trim().to_uppercase().as_str() {
            "A" => Some(RotaryWord::A),
            "B" => Some(RotaryWord::B),
            _ => None,
        }
    }

    /// The letter written in a post file.
    pub fn letter(self) -> &'static str {
        match self {
            RotaryWord::A => "A",
            RotaryWord::B => "B",
        }
    }
}

/// What the machine's rotary axis reads a word as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RotaryUnits {
    /// Degrees: once round the blank is 360.
    #[default]
    Degrees,
    /// The distance round the blank in the post's units: once round is pi
    /// times the diameter.
    Linear,
}

/// How a machine's rotary axis is wired, from Machine Configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RotaryWiring {
    /// The letter the controller calls the axis; the post's own when absent.
    pub letter: Option<RotaryWord>,
    /// What the controller reads the word as.
    pub units: RotaryUnits,
    /// The axis turns the other way: a design would come out mirrored.
    pub reversed: bool,
}

impl RotaryWiring {
    /// A machine's wiring until someone says otherwise: A in degrees.
    pub fn machine_default() -> RotaryWiring {
        RotaryWiring {
            letter: Some(RotaryWord::A),
            ..RotaryWiring::default()
        }
    }
}

/// Angle in degrees for a distance around a blank of `diameter`, both in
/// millimetres. One circumference is 360 degrees; negative distances give
/// negative angles.
pub fn angle_degrees(distance: f64, diameter: f64) -> f64 {
    if diameter <= 0.0 || !diameter.is_finite() {
        return 0.0;
    }
    distance / (std::f64::consts::PI * diameter) * 360.0
}

/// Distance around a blank of `diameter` for an angle in degrees, the
/// inverse of [`angle_degrees`]. Millimetres.
pub fn distance_of_angle(degrees: f64, diameter: f64) -> f64 {
    degrees / 360.0 * std::f64::consts::PI * diameter
}

/// How one program is wrapped: which rotary word takes the angle, which way
/// the blank lies (so which flat axis turns into it) and the diameter the
/// flat layout was unwrapped from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wrap {
    /// Rotary axis written.
    pub word: RotaryWord,
    /// The axis the blank lies along: X wraps flat Y, Y wraps flat X.
    pub blank_along: RotaryAxis,
    /// Blank diameter in millimetres.
    pub diameter: f64,
    /// How the machine's axis is wired.
    pub wiring: RotaryWiring,
}

impl Wrap {
    /// The wrap for a rotary job written through `post`.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when the post cannot drive the
    /// rotary axis the job needs.
    pub fn for_job(post: &Post, rotary: &RotarySetup) -> Result<Wrap, CoreError> {
        let Some(word) = post.wrap_word(rotary.axis) else {
            let (along, flat) = match rotary.axis {
                RotaryAxis::X => ("X", "Y"),
                RotaryAxis::Y => ("Y", "X"),
            };
            return Err(CoreError::validation(format!(
                "the post {} cannot wrap a rotary job along {along} (it needs a post that \
                 wraps flat {flat} onto a rotary axis)",
                post.name
            )));
        };
        if rotary.diameter <= 0.0 || !rotary.diameter.is_finite() {
            return Err(CoreError::validation(
                "a rotary job needs a diameter greater than zero",
            ));
        }
        Ok(Wrap {
            word,
            blank_along: rotary.axis,
            diameter: rotary.diameter,
            wiring: RotaryWiring::default(),
        })
    }

    /// This wrap on a machine wired as `wiring`.
    pub fn with_wiring(self, wiring: RotaryWiring) -> Wrap {
        Wrap { wiring, ..self }
    }

    /// The letter the program writes the turn on.
    pub fn letter(&self) -> RotaryWord {
        self.wiring.letter.unwrap_or(self.word)
    }

    /// The word a `[A]` or `[B]` token writes in place of its own, or `None`
    /// to write nothing: the token that carries the turn takes the machine's
    /// letter, and when that differs from the post's own the other token is
    /// silent so one line cannot name the axis twice.
    pub fn token_word(&self, source: ValueSource) -> Option<RotaryWord> {
        let own = match source {
            ValueSource::AngleA => RotaryWord::A,
            ValueSource::AngleB => RotaryWord::B,
            _ => return None,
        };
        if own == self.word {
            Some(self.letter())
        } else if self.letter() == self.word {
            Some(own)
        } else {
            None
        }
    }

    /// True when the rotary word holds a length, which the post's units scale.
    pub fn is_linear(&self) -> bool {
        self.wiring.units == RotaryUnits::Linear
    }

    /// Angle in degrees of a flat point.
    pub fn angle_of(&self, point: Point3) -> f64 {
        let across = match self.blank_along {
            RotaryAxis::X => point.y,
            RotaryAxis::Y => point.x,
        };
        angle_degrees(across, self.diameter)
    }

    /// How many pieces a straight move takes so that none turns further than
    /// `maximum` degrees; one with no limit.
    pub fn pieces(&self, from: Point3, to: Point3, maximum: Option<f64>) -> usize {
        let Some(maximum) = maximum.filter(|m| *m > 0.0) else {
            return 1;
        };
        let turn = (self.angle_of(to) - self.angle_of(from)).abs();
        // A turn of exactly the limit is one piece, whatever the rounding.
        ((turn / maximum - 1e-9).ceil() as usize).max(1)
    }

    /// The value a position token takes in a wrapped program, or `None` when
    /// the token is not one the wrap changes.
    pub fn value_of(&self, source: ValueSource, point: Point3) -> Option<f64> {
        let wrapped_linear = match self.blank_along {
            RotaryAxis::X => matches!(source, ValueSource::Y | ValueSource::HomeY),
            RotaryAxis::Y => matches!(source, ValueSource::X | ValueSource::HomeX),
        };
        if wrapped_linear {
            return Some(0.0);
        }
        match (source, self.word) {
            (ValueSource::AngleA, RotaryWord::A) | (ValueSource::AngleB, RotaryWord::B) => {
                let value = match self.wiring.units {
                    RotaryUnits::Degrees => self.angle_of(point),
                    RotaryUnits::Linear => match self.blank_along {
                        RotaryAxis::X => point.y,
                        RotaryAxis::Y => point.x,
                    },
                };
                Some(if self.wiring.reversed { -value } else { value })
            }
            (ValueSource::AngleA | ValueSource::AngleB, _) => Some(0.0),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn one_circumference_is_a_full_turn() {
        let d = 50.0;
        let around = std::f64::consts::PI * d;
        assert!(close(angle_degrees(around, d), 360.0));
        assert!(close(angle_degrees(around / 4.0, d), 90.0));
        assert!(close(angle_degrees(-around / 2.0, d), -180.0));
        assert!(close(angle_degrees(0.0, d), 0.0));
    }

    #[test]
    fn distance_and_angle_are_inverses() {
        for degrees in [-270.0, -1.5, 0.0, 12.25, 359.0, 720.0] {
            let back = angle_degrees(distance_of_angle(degrees, 63.5), 63.5);
            assert!(close(back, degrees), "{degrees} came back as {back}");
        }
    }

    #[test]
    fn a_zero_diameter_gives_no_angle() {
        assert_eq!(angle_degrees(10.0, 0.0), 0.0);
        assert_eq!(angle_degrees(10.0, f64::NAN), 0.0);
    }

    fn post_with(wrap_y: Option<RotaryWord>, wrap_x: Option<RotaryWord>) -> Post {
        Post {
            name: "Rotary".to_string(),
            rotary_wrap_y: wrap_y,
            rotary_wrap_x: wrap_x,
            ..Post::default()
        }
    }

    #[test]
    fn a_blank_along_x_wraps_y_into_a() {
        let rotary = RotarySetup {
            diameter: 40.0,
            axis: RotaryAxis::X,
            ..RotarySetup::default()
        };
        let wrap = Wrap::for_job(&post_with(Some(RotaryWord::A), None), &rotary).expect("wraps");
        let quarter = std::f64::consts::PI * 40.0 / 4.0;
        let point = Point3::new(12.0, quarter, -3.0);
        assert!(close(wrap.angle_of(point), 90.0));
        assert_eq!(wrap.value_of(ValueSource::Y, point), Some(0.0));
        assert_eq!(wrap.value_of(ValueSource::X, point), None);
        assert_eq!(wrap.value_of(ValueSource::Z, point), None);
        assert_eq!(wrap.value_of(ValueSource::AngleB, point), Some(0.0));
        let a = wrap.value_of(ValueSource::AngleA, point).expect("angle");
        assert!(close(a, 90.0));
    }

    #[test]
    fn a_blank_along_y_wraps_x_into_b() {
        let rotary = RotarySetup {
            diameter: 40.0,
            axis: RotaryAxis::Y,
            ..RotarySetup::default()
        };
        let post = post_with(Some(RotaryWord::A), Some(RotaryWord::B));
        let wrap = Wrap::for_job(&post, &rotary).expect("wraps");
        assert_eq!(wrap.word, RotaryWord::B);
        let half = std::f64::consts::PI * 40.0 / 2.0;
        let point = Point3::new(half, 7.0, 0.0);
        assert!(close(wrap.angle_of(point), 180.0));
        assert_eq!(wrap.value_of(ValueSource::X, point), Some(0.0));
        assert_eq!(wrap.value_of(ValueSource::Y, point), None);
    }

    #[test]
    fn a_post_without_the_axis_is_refused() {
        let rotary = RotarySetup {
            axis: RotaryAxis::Y,
            ..RotarySetup::default()
        };
        assert!(Wrap::for_job(&post_with(Some(RotaryWord::A), None), &rotary).is_err());
        assert!(Wrap::for_job(&post_with(None, None), &RotarySetup::default()).is_err());
    }

    #[test]
    fn a_blank_along_y_can_wrap_flat_x_onto_the_a_word() {
        let rotary = RotarySetup {
            diameter: 40.0,
            axis: RotaryAxis::Y,
            ..RotarySetup::default()
        };
        let post = post_with(Some(RotaryWord::A), Some(RotaryWord::A));
        let wrap = Wrap::for_job(&post, &rotary).expect("wraps");
        assert_eq!(wrap.word, RotaryWord::A);
        let half = std::f64::consts::PI * 40.0 / 2.0;
        let point = Point3::new(half, 7.0, 0.0);
        let a = wrap.value_of(ValueSource::AngleA, point).expect("angle");
        assert!(close(a, 180.0));
        assert_eq!(wrap.value_of(ValueSource::AngleB, point), Some(0.0));
        assert_eq!(wrap.value_of(ValueSource::X, point), Some(0.0));
        assert_eq!(wrap.value_of(ValueSource::Y, point), None);
    }

    #[test]
    fn home_on_the_wrapped_axis_is_the_axis_line() {
        let wrap = Wrap {
            word: RotaryWord::A,
            blank_along: RotaryAxis::X,
            diameter: 40.0,
            wiring: RotaryWiring::default(),
        };
        let at = Point3::new(1.0, 2.0, 3.0);
        assert_eq!(wrap.value_of(ValueSource::HomeY, at), Some(0.0));
        assert_eq!(wrap.value_of(ValueSource::HomeX, at), None);
        let across = Wrap {
            blank_along: RotaryAxis::Y,
            ..wrap
        };
        assert_eq!(across.value_of(ValueSource::HomeX, at), Some(0.0));
        assert_eq!(across.value_of(ValueSource::HomeY, at), None);
    }

    #[test]
    fn a_turn_is_cut_into_pieces_no_longer_than_the_maximum() {
        let wrap = Wrap {
            word: RotaryWord::A,
            blank_along: RotaryAxis::X,
            diameter: 40.0,
            wiring: RotaryWiring::default(),
        };
        let round = std::f64::consts::PI * 40.0;
        let start = Point3::new(0.0, 0.0, 0.0);
        let at = |f: f64| Point3::new(5.0, round * f, 0.0);
        assert_eq!(wrap.pieces(start, at(1.0), None), 1);
        assert_eq!(wrap.pieces(start, at(1.0), Some(90.0)), 4);
        assert_eq!(wrap.pieces(start, at(0.25), Some(90.0)), 1);
        assert_eq!(wrap.pieces(start, at(0.26), Some(90.0)), 2);
        assert_eq!(wrap.pieces(at(1.0), start, Some(179.0)), 3);
        assert_eq!(wrap.pieces(start, start, Some(90.0)), 1);
    }

    #[test]
    fn wiring_sets_the_letter_units_and_direction() {
        let base = Wrap {
            word: RotaryWord::A,
            blank_along: RotaryAxis::X,
            diameter: 40.0,
            wiring: RotaryWiring::default(),
        };
        let quarter = std::f64::consts::PI * 40.0 / 4.0;
        let at = Point3::new(0.0, quarter, 0.0);
        let turn = |wrap: Wrap| wrap.value_of(ValueSource::AngleA, at).expect("turn");
        assert!(close(turn(base), 90.0));
        let reversed = RotaryWiring {
            reversed: true,
            ..RotaryWiring::default()
        };
        assert!(close(turn(base.with_wiring(reversed)), -90.0));
        let linear = RotaryWiring {
            units: RotaryUnits::Linear,
            ..RotaryWiring::default()
        };
        assert!(close(turn(base.with_wiring(linear)), quarter));
        // A long turn still breaks up by degrees, whatever the word holds.
        assert_eq!(
            base.with_wiring(linear)
                .pieces(Point3::new(0.0, 0.0, 0.0), at, Some(45.0)),
            2
        );

        // The post's own letter unless the machine names another.
        assert_eq!(base.token_word(ValueSource::AngleA), Some(RotaryWord::A));
        assert_eq!(base.token_word(ValueSource::AngleB), Some(RotaryWord::B));
        let on_b = base.with_wiring(RotaryWiring {
            letter: Some(RotaryWord::B),
            ..RotaryWiring::default()
        });
        assert_eq!(on_b.token_word(ValueSource::AngleA), Some(RotaryWord::B));
        assert_eq!(on_b.token_word(ValueSource::AngleB), None);
        assert_eq!(on_b.token_word(ValueSource::X), None);
    }

    #[test]
    fn words_read_from_the_post_file() {
        assert_eq!(RotaryWord::from_name(" a "), Some(RotaryWord::A));
        assert_eq!(RotaryWord::from_name("B"), Some(RotaryWord::B));
        assert_eq!(RotaryWord::from_name("C"), None);
    }
}
