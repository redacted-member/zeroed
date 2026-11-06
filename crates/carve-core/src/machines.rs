//! Machine configurations: the CNC machines the user owns, each with its
//! units, working area and the post processors that drive it.
//!
//! They are preferences of the user rather than parts of a document, so they
//! live in the `machines` group of [`crate::prefs::Preferences`]. The active
//! machine chooses the post processor Save Toolpaths starts with. Machine
//! names are the same names the tool database keeps cutting data under.

use serde::{Deserialize, Serialize};

use crate::doc::Units;
use crate::error::CoreError;

/// Name of the machine a fresh install starts with. The shipped tool database
/// keeps its cutting data under the same name.
pub const DEFAULT_MACHINE: &str = "Default";

/// Longest machine name.
pub const MAX_NAME_LENGTH: usize = 80;

/// Largest working area on any axis, in millimetres.
pub const MAX_TRAVEL_MM: f64 = 100_000.0;

/// One machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineConfig {
    /// Unique name, also the tool database machine name.
    pub name: String,
    /// Units the machine is set up in.
    pub units: Units,
    /// Travel in X, millimetres.
    pub bed_x: f64,
    /// Travel in Y, millimetres.
    pub bed_y: f64,
    /// Travel in Z, millimetres.
    pub bed_z: f64,
    /// Ids of the post processors that drive this machine, in list order.
    #[serde(default)]
    pub posts: Vec<String>,
    /// The post Save Toolpaths picks first; one of `posts`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_post: Option<String>,
    /// Rapid travel rate, millimetres per minute, for time estimates.
    #[serde(default = "default_rapid_rate")]
    pub rapid_rate: f64,
    /// Axis accelerations X, Y and Z in millimetres per second squared,
    /// for time estimates; zero when not known, which leaves them out.
    #[serde(default)]
    pub acceleration: [f64; 3],
    /// The rotary axis's acceleration in degrees per second squared, zero
    /// when not known or the machine has no rotary axis.
    #[serde(default)]
    pub rotary_acceleration: f64,
    /// How the machine's rotary axis is wired: A in degrees until set.
    #[serde(default = "crate::post::rotary::RotaryWiring::machine_default")]
    pub rotary: crate::post::rotary::RotaryWiring,
    /// The unit Machine Configuration shows the accelerations in, as the
    /// user picked it; they are always stored per second squared.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub acceleration_units: String,
}

fn default_rapid_rate() -> f64 {
    3000.0
}

impl MachineConfig {
    /// A machine with a 24 x 24 x 4 inch working area, 120 inches a minute
    /// rapids and no posts yet.
    pub fn named(name: &str) -> Self {
        MachineConfig {
            name: name.to_string(),
            units: Units::Inch,
            bed_x: 24.0 * 25.4,
            bed_y: 24.0 * 25.4,
            bed_z: 4.0 * 25.4,
            posts: Vec::new(),
            default_post: None,
            rapid_rate: 120.0 * 25.4,
            acceleration: [0.0; 3],
            rotary_acceleration: 0.0,
            rotary: crate::post::rotary::RotaryWiring::machine_default(),
            acceleration_units: String::new(),
        }
    }

    /// How this machine's jobs are timed, stretched by `scale`: its rapid
    /// rate (also in Z) and, when every axis has one, its accelerations.
    pub fn time_estimate(&self, scale: f64) -> crate::cam::TimeEstimate {
        let [x, y, z] = self.acceleration;
        crate::cam::TimeEstimate {
            rapid_rate: self.rapid_rate,
            rapid_z_rate: self.rapid_rate,
            scale,
            acceleration: crate::cam::estimate::Acceleration::known(x, y, z),
            rotary_acceleration: (self.rotary_acceleration.is_finite()
                && self.rotary_acceleration > 0.0)
                .then_some(self.rotary_acceleration),
        }
    }

    /// The post processor this machine saves with: its default, else its
    /// first post, else `None`.
    pub fn save_post(&self) -> Option<&str> {
        self.default_post
            .as_deref()
            .or_else(|| self.posts.first().map(String::as_str))
    }
}

/// Every machine and which one is active.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MachinePrefs {
    /// Name of the active machine; empty only when there are no machines.
    pub active: String,
    /// The machines, in the order the user added them.
    pub list: Vec<MachineConfig>,
}

impl Default for MachinePrefs {
    fn default() -> Self {
        MachinePrefs {
            active: DEFAULT_MACHINE.to_string(),
            list: vec![MachineConfig::named(DEFAULT_MACHINE)],
        }
    }
}

impl MachinePrefs {
    /// The active machine, if any.
    pub fn active_machine(&self) -> Option<&MachineConfig> {
        self.list.iter().find(|m| m.name == self.active)
    }

    /// Makes the active name point at a machine: the first one when the name
    /// is stale, empty when there are none. Trims names.
    pub fn normalise(&mut self) {
        for machine in &mut self.list {
            machine.name = machine.name.trim().to_string();
            machine.posts.dedup();
            if machine
                .default_post
                .as_ref()
                .is_some_and(|post| !machine.posts.contains(post))
            {
                machine.default_post = None;
            }
        }
        if self.active_machine().is_none() {
            self.active = self
                .list
                .first()
                .map(|m| m.name.clone())
                .unwrap_or_default();
        }
    }

    /// Checks names are present and unique and every travel is in range.
    ///
    /// # Errors
    /// [`CoreError::Validation`] naming the first problem.
    pub fn validate(&self) -> Result<(), CoreError> {
        for (index, machine) in self.list.iter().enumerate() {
            let name = machine.name.trim();
            if name.is_empty() {
                return Err(CoreError::validation("every machine needs a name"));
            }
            if name.chars().count() > MAX_NAME_LENGTH {
                return Err(CoreError::validation(format!(
                    "machine names are at most {MAX_NAME_LENGTH} characters"
                )));
            }
            if self.list[..index]
                .iter()
                .any(|other| other.name.trim().eq_ignore_ascii_case(name))
            {
                return Err(CoreError::validation(format!(
                    "two machines cannot both be named {name}"
                )));
            }
            for (axis, value) in [
                ("X", machine.bed_x),
                ("Y", machine.bed_y),
                ("Z", machine.bed_z),
            ] {
                if !(value.is_finite() && value > 0.0 && value <= MAX_TRAVEL_MM) {
                    return Err(CoreError::validation(format!(
                        "the {axis} travel of {name} must be above 0 and at most {MAX_TRAVEL_MM} mm"
                    )));
                }
            }
            if !(machine.rapid_rate.is_finite() && machine.rapid_rate > 0.0) {
                return Err(CoreError::validation(format!(
                    "the rapid rate of {name} must be above 0"
                )));
            }
            if machine
                .acceleration
                .iter()
                .chain(std::iter::once(&machine.rotary_acceleration))
                .any(|a| !(a.is_finite() && *a >= 0.0))
            {
                return Err(CoreError::validation(format!(
                    "the accelerations of {name} must be 0 or more"
                )));
            }
            if machine.posts.iter().any(|post| post.trim().is_empty()) {
                return Err(CoreError::validation(format!(
                    "{name} lists a post processor without an id"
                )));
            }
            if let Some(post) = &machine.default_post {
                if !machine.posts.contains(post) {
                    return Err(CoreError::validation(format!(
                        "the default post processor of {name} is not one of its posts"
                    )));
                }
            }
        }
        if !self.list.is_empty() && self.active_machine().is_none() {
            return Err(CoreError::validation(format!(
                "there is no machine named {}",
                self.active
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(list: Vec<MachineConfig>, active: &str) -> MachinePrefs {
        MachinePrefs {
            active: active.to_string(),
            list,
        }
    }

    #[test]
    fn the_default_is_one_valid_active_machine() {
        let prefs = MachinePrefs::default();
        assert!(prefs.validate().is_ok());
        assert_eq!(
            prefs.active_machine().map(|m| m.name.as_str()),
            Some(DEFAULT_MACHINE)
        );
    }

    #[test]
    fn names_must_be_present_and_unique() {
        let prefs = with(
            vec![MachineConfig::named("Shop"), MachineConfig::named(" shop ")],
            "Shop",
        );
        assert!(prefs.validate().is_err());
        let prefs = with(vec![MachineConfig::named("  ")], "");
        assert!(prefs.validate().is_err());
    }

    #[test]
    fn travel_must_be_positive_and_finite() {
        let mut machine = MachineConfig::named("A");
        machine.bed_y = 0.0;
        assert!(with(vec![machine.clone()], "A").validate().is_err());
        machine.bed_y = f64::NAN;
        assert!(with(vec![machine], "A").validate().is_err());
    }

    #[test]
    fn the_default_post_must_be_one_of_the_posts() {
        let mut machine = MachineConfig::named("A");
        machine.posts = vec!["grbl_mm".to_string()];
        machine.default_post = Some("mach3_mm".to_string());
        assert!(with(vec![machine.clone()], "A").validate().is_err());
        machine.default_post = Some("grbl_mm".to_string());
        assert!(with(vec![machine], "A").validate().is_ok());
    }

    #[test]
    fn normalising_repairs_a_stale_active_name_and_default_post() {
        let mut machine = MachineConfig::named(" B ");
        machine.posts = vec!["x".to_string()];
        machine.default_post = Some("gone".to_string());
        let mut prefs = with(vec![machine], "A");
        prefs.normalise();
        assert_eq!(prefs.active, "B");
        assert_eq!(prefs.list[0].default_post, None);
        assert!(prefs.validate().is_ok());
        let mut empty = with(Vec::new(), "A");
        empty.normalise();
        assert_eq!(empty.active, "");
        assert!(empty.validate().is_ok());
    }

    #[test]
    fn machines_are_set_through_the_preferences() {
        let prefs = crate::prefs::Preferences::default();
        let merged = prefs
            .merged(&serde_json::json!({ "machines": {
                "active": "Router",
                "list": [{ "name": "Router", "units": "inch", "bedX": 1219.2,
                           "bedY": 2438.4, "bedZ": 150.0, "posts": ["grbl_inch"],
                           "defaultPost": "grbl_inch" }],
            }}))
            .expect("merged");
        let active = merged.machines.active_machine().expect("active");
        assert_eq!(active.units, Units::Inch);
        assert_eq!(active.save_post(), Some("grbl_inch"));
        assert!(prefs
            .merged(&serde_json::json!({ "machines": { "list": [
                { "name": "A", "units": "mm", "bedX": -1.0, "bedY": 1.0, "bedZ": 1.0 }
            ]}}))
            .is_err());
    }

    #[test]
    fn the_save_post_prefers_the_default_then_the_first() {
        let mut machine = MachineConfig::named("A");
        assert_eq!(machine.save_post(), None);
        machine.posts = vec!["a".to_string(), "b".to_string()];
        assert_eq!(machine.save_post(), Some("a"));
        machine.default_post = Some("b".to_string());
        assert_eq!(machine.save_post(), Some("b"));
    }
}
