//! Machining time estimates.
//!
//! Each move contributes its length divided by the rate it runs at: rapids at
//! the machine rapid rate, plunges at the toolpath plunge rate, everything
//! else at the feed rate. A dwell adds its own seconds. The total is then
//! multiplied by a scale factor, which is how a shop dials the estimate in to
//! what their machine really does.
//!
//! When the machine's axis accelerations are known, the estimate plans the
//! speed along the moves the way a motion controller does instead: each
//! move may accelerate at the rate its slowest axis allows along its
//! direction, corners slow the tool to the rate times the cosine of the
//! turn (a full stop at a right angle or sharper), the tool starts and ends
//! at rest and dwells stop it, and arcs keep within their centripetal limit.
//! A backward and a forward pass settle the speed at every junction, then
//! each move takes the time of its trapezoid (or triangle) speed profile.

use serde::{Deserialize, Serialize};

use super::toolpath::{MoveKind, Point3, Toolpath};

/// Machine settings the estimate needs, alongside the toolpath's own feeds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeEstimate {
    /// Rapid travel rate in millimetres per minute.
    pub rapid_rate: f64,
    /// Rapid rate for moves straight up or down, in millimetres per minute.
    /// Many machines travel slower in Z than in XY.
    pub rapid_z_rate: f64,
    /// Multiplier applied to the whole estimate. 1.0 is the raw sum, higher
    /// values allow for a machine that does not reach its programmed rates.
    pub scale: f64,
    /// The machine's axis accelerations, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceleration: Option<Acceleration>,
    /// The rotary axis's angular acceleration in degrees per second
    /// squared, when known; on a rotary job it stands in for the axis that
    /// wraps round the blank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotary_acceleration: Option<f64>,
}

/// Axis accelerations in millimetres per second squared.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Acceleration {
    /// X axis.
    pub x: f64,
    /// Y axis.
    pub y: f64,
    /// Z axis.
    pub z: f64,
}

impl Acceleration {
    /// The accelerations when every axis has a positive one, else `None`.
    pub fn known(x: f64, y: f64, z: f64) -> Option<Acceleration> {
        let ok = |v: f64| v.is_finite() && v > 0.0;
        (ok(x) && ok(y) && ok(z)).then_some(Acceleration { x, y, z })
    }

    /// Most the tool may accelerate along the unit direction `u`: the
    /// axis that would exceed its own limit first sets it.
    fn along(&self, u: [f64; 3]) -> f64 {
        let mut best = f64::INFINITY;
        for (component, limit) in u.iter().zip([self.x, self.y, self.z]) {
            if component.abs() > 1e-9 {
                best = best.min(limit / component.abs());
            }
        }
        if best.is_finite() {
            best
        } else {
            self.x.min(self.y).min(self.z)
        }
    }
}

impl Default for TimeEstimate {
    fn default() -> Self {
        TimeEstimate {
            rapid_rate: 3000.0,
            rapid_z_rate: 1500.0,
            scale: 1.0,
            acceleration: None,
            rotary_acceleration: None,
        }
    }
}

impl TimeEstimate {
    /// Settings with the given rapid rate in millimetres per minute, the same
    /// rate in Z, and no scaling.
    pub fn with_rapid_rate(rapid_rate: f64) -> Self {
        TimeEstimate {
            rapid_rate,
            rapid_z_rate: rapid_rate,
            scale: 1.0,
            acceleration: None,
            rotary_acceleration: None,
        }
    }

    /// The settings for a rotary job wrapped round a blank of `diameter`
    /// millimetres: the flat axis that wraps (Y for a blank along X, X for
    /// one along Y) accelerates as fast as the rotary axis turns the
    /// surface, when its angular acceleration is known.
    pub fn for_rotary(self, along_x: bool, diameter: f64) -> TimeEstimate {
        let (Some(mut accel), Some(turn)) = (self.acceleration, self.rotary_acceleration) else {
            return self;
        };
        let surface = turn.to_radians() * diameter / 2.0;
        if !(surface.is_finite() && surface > 0.0) {
            return self;
        }
        if along_x {
            accel.y = surface;
        } else {
            accel.x = surface;
        }
        TimeEstimate {
            acceleration: Some(accel),
            ..self
        }
    }

    /// The same settings with the scale factor replaced.
    pub fn scaled(self, scale: f64) -> Self {
        TimeEstimate { scale, ..self }
    }

    /// Seconds one toolpath is expected to take.
    ///
    /// Rates that are not positive are ignored rather than dividing by zero,
    /// so a broken toolpath estimates as zero instead of infinity.
    pub fn seconds(&self, toolpath: &Toolpath) -> f64 {
        if let Some(acceleration) = self.acceleration {
            return self.planned_seconds(toolpath, &acceleration) * self.scale.max(0.0);
        }
        let mut seconds = 0.0;
        for (index, (from, one)) in toolpath.iter_moves().enumerate() {
            let length = one.length(from);
            let rates = toolpath.rates_at(index);
            seconds += match one.kind {
                MoveKind::Dwell { seconds: wait } => wait.max(0.0),
                MoveKind::Rapid => {
                    let vertical =
                        (from.x - one.to.x).abs() < 1e-9 && (from.y - one.to.y).abs() < 1e-9;
                    let rate = if vertical {
                        self.rapid_z_rate
                    } else {
                        self.rapid_rate
                    };
                    minutes(length, rate)
                }
                MoveKind::Plunge => minutes(length, rates.plunge),
                MoveKind::Feed | MoveKind::ArcCw { .. } | MoveKind::ArcCcw { .. } => {
                    minutes(length, rates.feed)
                }
            };
        }
        seconds * self.scale.max(0.0)
    }

    /// The rate a move runs at, in millimetres per minute.
    fn rate_of(
        &self,
        toolpath: &Toolpath,
        index: usize,
        from: Point3,
        one: &super::toolpath::Move,
    ) -> f64 {
        match one.kind {
            MoveKind::Dwell { .. } => 0.0,
            MoveKind::Rapid => {
                let vertical = (from.x - one.to.x).abs() < 1e-9 && (from.y - one.to.y).abs() < 1e-9;
                if vertical {
                    self.rapid_z_rate
                } else {
                    self.rapid_rate
                }
            }
            MoveKind::Plunge => toolpath.rates_at(index).plunge,
            MoveKind::Feed | MoveKind::ArcCw { .. } | MoveKind::ArcCcw { .. } => {
                toolpath.rates_at(index).feed
            }
        }
    }

    /// Seconds with the speed planned through accelerations and corners,
    /// before scaling.
    fn planned_seconds(&self, toolpath: &Toolpath, acceleration: &Acceleration) -> f64 {
        let mut spans: Vec<Span> = Vec::with_capacity(toolpath.moves.len());
        let mut dwell = 0.0;
        for (index, (from, one)) in toolpath.iter_moves().enumerate() {
            if let MoveKind::Dwell { seconds } = one.kind {
                dwell += seconds.max(0.0);
                // The tool stands still: the next move starts from rest.
                if let Some(last) = spans.last_mut() {
                    last.stop_after = true;
                }
                continue;
            }
            let length = one.length(from);
            let rate = self.rate_of(toolpath, index, from, one) / 60.0;
            if !(length.is_finite() && length > 1e-9 && rate > 0.0) {
                continue;
            }
            let (start, end, radius) = directions(from, one);
            let accel = acceleration.along(start).min(acceleration.along(end));
            // An arc cannot turn faster than its centripetal limit allows.
            let cap = match radius {
                Some(r) if r > 1e-9 => rate.min((accel * r).sqrt()),
                _ => rate,
            };
            spans.push(Span {
                length,
                cap,
                accel,
                start,
                end,
                stop_after: false,
            });
        }
        let n = spans.len();
        if n == 0 {
            return dwell;
        }
        // Highest speed at each junction: the slower span's rate eased by
        // the turn; the ends and dwells are at rest.
        let mut junction = vec![0.0; n + 1];
        for i in 1..n {
            let (a, b) = (&spans[i - 1], &spans[i]);
            if a.stop_after {
                continue;
            }
            let cos = (a.end[0] * b.start[0] + a.end[1] * b.start[1] + a.end[2] * b.start[2])
                .clamp(-1.0, 1.0);
            junction[i] = a.cap.min(b.cap) * cos.max(0.0);
        }
        // Backward: slow enough to stop in time for what comes next.
        for i in (0..n).rev() {
            let reach = (junction[i + 1].powi(2) + 2.0 * spans[i].accel * spans[i].length).sqrt();
            junction[i] = junction[i].min(reach);
        }
        // Forward: no faster than the span before can speed up to.
        for i in 0..n {
            let reach = (junction[i].powi(2) + 2.0 * spans[i].accel * spans[i].length).sqrt();
            junction[i + 1] = junction[i + 1].min(reach);
        }
        let moving: f64 = spans
            .iter()
            .enumerate()
            .map(|(i, span)| profile_seconds(span, junction[i], junction[i + 1]))
            .sum();
        moving + dwell
    }

    /// Seconds a whole list of toolpaths is expected to take, plus the time
    /// spent travelling home between them.
    pub fn total_seconds(&self, toolpaths: &[Toolpath]) -> f64 {
        toolpaths.iter().map(|tp| self.seconds(tp)).sum()
    }

    /// Fills in [`Toolpath::estimated_seconds`] and returns the value.
    pub fn apply(&self, toolpath: &mut Toolpath) -> f64 {
        let seconds = self.seconds(toolpath);
        toolpath.estimated_seconds = seconds;
        seconds
    }
}

/// One move as the speed planner sees it. Speeds are millimetres per
/// second, accelerations millimetres per second squared.
struct Span {
    length: f64,
    cap: f64,
    accel: f64,
    start: [f64; 3],
    end: [f64; 3],
    stop_after: bool,
}

/// Unit directions of a move at its start and end, and an arc's radius.
fn directions(from: Point3, one: &super::toolpath::Move) -> ([f64; 3], [f64; 3], Option<f64>) {
    let unit = |v: [f64; 3]| {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len > 1e-12 {
            [v[0] / len, v[1] / len, v[2] / len]
        } else {
            [1.0, 0.0, 0.0]
        }
    };
    match one.kind {
        MoveKind::ArcCw { centre } | MoveKind::ArcCcw { centre } => {
            let sweep = one.arc_sweep(from).unwrap_or(0.0);
            let radius = centre.distance(from.xy());
            let flat = sweep.abs() * radius;
            let rise = if flat > 1e-12 {
                (one.to.z - from.z) / flat
            } else {
                0.0
            };
            let sign = if sweep >= 0.0 { 1.0 } else { -1.0 };
            let tangent = |x: f64, y: f64| {
                let (rx, ry) = (x - centre.x, y - centre.y);
                unit([-ry * sign, rx * sign, rise * rx.hypot(ry)])
            };
            (
                tangent(from.x, from.y),
                tangent(one.to.x, one.to.y),
                Some(radius),
            )
        }
        _ => {
            let d = unit([one.to.x - from.x, one.to.y - from.y, one.to.z - from.z]);
            (d, d, None)
        }
    }
}

/// Seconds to cover a span entering at `v0` and leaving at `v1`, speeding
/// up and slowing down at its acceleration and cruising at its cap between.
fn profile_seconds(span: &Span, v0: f64, v1: f64) -> f64 {
    let (a, top, length) = (span.accel, span.cap, span.length);
    if !(a.is_finite() && a > 0.0) {
        return length / top;
    }
    let speed_up = (top * top - v0 * v0) / (2.0 * a);
    let slow_down = (top * top - v1 * v1) / (2.0 * a);
    if speed_up + slow_down <= length {
        (top - v0) / a + (top - v1) / a + (length - speed_up - slow_down) / top
    } else {
        let peak = ((2.0 * a * length + v0 * v0 + v1 * v1) / 2.0).sqrt();
        ((peak - v0) / a + (peak - v1) / a).max(0.0)
    }
}

/// Seconds taken to travel `length` millimetres at `rate` millimetres per
/// minute.
fn minutes(length: f64, rate: f64) -> f64 {
    if rate > 0.0 && length.is_finite() {
        length / rate * 60.0
    } else {
        0.0
    }
}

/// Formats a number of seconds as hours, minutes and seconds, which is what
/// the toolpath list shows.
pub fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    format!("{hours}:{minutes:02}:{secs:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::{Tool, ToolGeometry};
    use crate::cam::toolpath::{Move, Point3, StrategyKind};
    use crate::geom::Point;

    fn toolpath() -> Toolpath {
        let tool = Tool {
            name: "6 mm end mill".to_string(),
            tool_type: ToolGeometry::EndMill { diameter: 6.0 },
            ..Tool::default()
        };
        let mut tp = Toolpath::new(1, "profile 1", tool, StrategyKind::Profile);
        tp.home = Point3::new(0.0, 0.0, 10.0);
        tp.feed = 1200.0;
        tp.plunge = 300.0;
        tp
    }

    #[test]
    fn feed_moves_take_length_over_feed_rate() {
        let mut tp = toolpath();
        tp.moves = vec![Move::feed(Point3::new(120.0, 0.0, 10.0))];
        // 120 mm at 1200 mm per minute is one tenth of a minute.
        assert!((TimeEstimate::default().seconds(&tp) - 6.0).abs() < 1e-9);
    }

    #[test]
    fn plunges_use_the_plunge_rate_and_rapids_the_machine_rate() {
        let mut tp = toolpath();
        tp.moves = vec![
            // 30 mm of rapid across the job at 3000 mm per minute: 0.6 s.
            Move::rapid(Point3::new(30.0, 0.0, 10.0)),
            // 10 mm straight down at the Z rapid rate of 1500: 0.4 s.
            Move::rapid(Point3::new(30.0, 0.0, 0.0)),
            // 5 mm plunge at 300 mm per minute: 1 s.
            Move::plunge(Point3::new(30.0, 0.0, -5.0)),
        ];
        let seconds = TimeEstimate::default().seconds(&tp);
        assert!((seconds - 2.0).abs() < 1e-9, "{seconds}");
    }

    #[test]
    fn arcs_are_measured_along_the_sweep() {
        let mut tp = toolpath();
        tp.home = Point3::new(10.0, 0.0, -1.0);
        tp.moves = vec![Move::arc_ccw(Point::ORIGIN, Point3::new(-10.0, 0.0, -1.0))];
        let expected = std::f64::consts::PI * 10.0 / 1200.0 * 60.0;
        assert!((TimeEstimate::default().seconds(&tp) - expected).abs() < 1e-9);
    }

    #[test]
    fn dwells_add_their_own_seconds() {
        let mut tp = toolpath();
        tp.moves = vec![Move::dwell(2.5, tp.home)];
        assert!((TimeEstimate::default().seconds(&tp) - 2.5).abs() < 1e-9);
    }

    #[test]
    fn the_scale_factor_stretches_the_estimate() {
        let mut tp = toolpath();
        tp.moves = vec![Move::feed(Point3::new(120.0, 0.0, 10.0))];
        let settings = TimeEstimate::default().scaled(1.5);
        assert!((settings.seconds(&tp) - 9.0).abs() < 1e-9);
    }

    #[test]
    fn a_rate_of_zero_estimates_as_zero_rather_than_forever() {
        let mut tp = toolpath();
        tp.feed = 0.0;
        tp.moves = vec![Move::feed(Point3::new(120.0, 0.0, 10.0))];
        assert_eq!(TimeEstimate::default().seconds(&tp), 0.0);
    }

    #[test]
    fn apply_stores_the_estimate_on_the_toolpath() {
        let mut tp = toolpath();
        tp.moves = vec![Move::feed(Point3::new(120.0, 0.0, 10.0))];
        let settings = TimeEstimate::with_rapid_rate(6000.0);
        let seconds = settings.apply(&mut tp);
        assert!((tp.estimated_seconds - seconds).abs() < 1e-12);
        assert!((settings.total_seconds(&[tp]) - seconds).abs() < 1e-9);
    }

    #[test]
    fn durations_format_as_hours_minutes_seconds() {
        assert_eq!(format_duration(0.0), "0:00:00");
        assert_eq!(format_duration(61.4), "0:01:01");
        assert_eq!(format_duration(3725.0), "1:02:05");
        assert_eq!(format_duration(-5.0), "0:00:00");
    }

    fn accelerated(a: f64) -> TimeEstimate {
        TimeEstimate {
            acceleration: Acceleration::known(a, a, a),
            ..TimeEstimate::default()
        }
    }

    #[test]
    fn acceleration_adds_the_time_to_speed_up_and_slow_down() {
        let mut tp = toolpath();
        tp.moves = vec![Move::feed(Point3::new(120.0, 0.0, 10.0))];
        // 20 mm/s cruise, 100 mm/s2: 0.2 s and 2 mm each way, so 116 mm at
        // cruise (5.8 s) plus 0.4 s.
        let seconds = accelerated(100.0).seconds(&tp);
        assert!((seconds - 6.2).abs() < 1e-9, "{seconds}");
        // A short move never reaches its rate: a triangle.
        tp.moves = vec![Move::feed(Point3::new(1.0, 0.0, 10.0))];
        let short = accelerated(100.0).seconds(&tp);
        assert!(
            (short - 2.0 * (1.0f64 / 100.0).sqrt()).abs() < 1e-9,
            "{short}"
        );
    }

    #[test]
    fn straight_on_keeps_speed_and_a_right_angle_stops() {
        let mut tp = toolpath();
        tp.home = Point3::new(0.0, 0.0, 0.0);
        tp.moves = vec![
            Move::feed(Point3::new(60.0, 0.0, 0.0)),
            Move::feed(Point3::new(120.0, 0.0, 0.0)),
        ];
        let straight = accelerated(100.0).seconds(&tp);
        assert!((straight - 6.2).abs() < 1e-9, "{straight}");
        tp.moves = vec![
            Move::feed(Point3::new(60.0, 0.0, 0.0)),
            Move::feed(Point3::new(60.0, 60.0, 0.0)),
        ];
        let corner = accelerated(100.0).seconds(&tp);
        assert!((corner - 6.4).abs() < 1e-9, "{corner}");
        // Without accelerations both are the plain 6 s.
        assert!((TimeEstimate::default().seconds(&tp) - 6.0).abs() < 1e-9);
    }

    #[test]
    fn a_slow_axis_and_a_tight_arc_take_longer() {
        let mut tp = toolpath();
        tp.home = Point3::new(0.0, 0.0, 0.0);
        tp.moves = vec![Move::feed(Point3::new(0.0, 0.0, -120.0))];
        let slow_z = TimeEstimate {
            acceleration: Acceleration::known(100.0, 100.0, 10.0),
            ..TimeEstimate::default()
        };
        assert!(slow_z.seconds(&tp) > accelerated(100.0).seconds(&tp) + 1.0);
        // A 1 mm radius arc at 100 mm/s2 turns at most 10 mm/s.
        tp.home = Point3::new(1.0, 0.0, 0.0);
        tp.moves = vec![Move::arc_ccw(Point::ORIGIN, Point3::new(-1.0, 0.0, 0.0))];
        let tight = accelerated(100.0).seconds(&tp);
        let plain = TimeEstimate::default().seconds(&tp);
        assert!(tight > plain * 1.5, "{tight} {plain}");
        assert!(Acceleration::known(0.0, 1.0, 1.0).is_none());
    }

    #[test]
    fn a_rotary_job_accelerates_round_the_blank_at_the_rotary_rate() {
        let base = TimeEstimate {
            acceleration: Acceleration::known(100.0, 100.0, 100.0),
            rotary_acceleration: Some(90.0),
            ..TimeEstimate::default()
        };
        let wrapped = base.for_rotary(true, 40.0);
        let accel = wrapped.acceleration.expect("accelerations");
        // 90 degrees per second squared on a 20 mm radius.
        assert!((accel.y - std::f64::consts::FRAC_PI_2 * 20.0).abs() < 1e-9);
        assert!((accel.x - 100.0).abs() < 1e-12);
        let along_y = base
            .for_rotary(false, 40.0)
            .acceleration
            .expect("accelerations");
        assert!((along_y.x - std::f64::consts::FRAC_PI_2 * 20.0).abs() < 1e-9);
        let unknown = TimeEstimate {
            rotary_acceleration: None,
            ..base
        };
        assert_eq!(unknown.for_rotary(true, 40.0), unknown);
    }
}
