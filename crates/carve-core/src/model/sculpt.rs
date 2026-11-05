//! Sculpting brushes: freehand edits to one component's heights along a
//! stroke drawn over it.
//!
//! A stroke is a polyline in job space. Dabs of the brush are laid along it
//! every quarter of the brush diameter, and each dab changes the cells under
//! it by an amount weighted by a smooth radial falloff: full weight inside
//! the hard core, easing to zero at the brush radius. Cells beyond the
//! radius are never touched.
//!
//! Every brush returns a field on the input's grid, except [`Brush::Deposit`]
//! with [`BrushParams::grow`], which may return a larger field on the same
//! lattice. Only [`Brush::Erase`] ever empties a cell.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::geom::Point;

use super::field::Field;

/// Most dabs one stroke may lay down; a longer stroke is refused.
pub const MAX_DABS: usize = 200_000;

/// Deposit and Remove change heights by up to `diameter / DEPOSIT_DIVISOR`
/// millimetres per dab at full strength and full weight.
pub const DEPOSIT_DIVISOR: f64 = 20.0;

/// Erase empties a cell once its height has faded to within this many
/// millimetres of zero, or when a dab hits it at full strength and weight.
pub const ERASE_EPSILON: f64 = 1.0e-3;

/// What a brush does to the cells under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Brush {
    /// Moves each height towards the mean of its neighbours within about a
    /// third of the brush radius, softening detail.
    Smooth,
    /// Adds material: raises heights.
    #[default]
    Deposit,
    /// Takes material away: lowers heights.
    Remove,
    /// Drags heights along the direction of the stroke.
    Smudge,
    /// Fades heights towards zero and empties cells that reach it.
    Erase,
    /// Undo brush: moves heights back towards a reference, the component as
    /// it was before sculpting began. Needs [`stroke_restoring`].
    Restore,
}

/// How a stroke's new heights meet the old ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SculptMode {
    /// Heights go up or down as the brush says.
    #[default]
    Normal,
    /// Heights only ever go up: the higher of old and new is kept, good for
    /// filling holes.
    Raise,
    /// Heights only ever go down: the lower is kept, good for spikes.
    Lower,
}

/// The sculpting session's snapshots of each sculpted component: as it was
/// when sculpting began (what the undo brush, Discard and Cancel go back
/// to) and as it was at the last Keep. Not saved.
#[derive(Debug, Default)]
pub struct SculptSession {
    /// The component before its first stroke of the session.
    pub baselines: std::collections::HashMap<u64, Field>,
    /// The component at the last Keep.
    pub kept: std::collections::HashMap<u64, Field>,
}

impl SculptSession {
    /// Forgets every snapshot: a new session begins with the next stroke.
    pub fn clear(&mut self) {
        self.baselines.clear();
        self.kept.clear();
    }
}

/// Settings for [`stroke`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BrushParams {
    /// What the brush does.
    pub brush: Brush,
    /// Brush diameter in millimetres, above 0.
    pub diameter: f64,
    /// How hard each dab acts, 0 to 1 (unitless).
    pub strength: f64,
    /// Softness of the brush edge, 0 to 1: 0 is a hard edge with full weight
    /// out to the radius, 1 eases from the centre all the way out.
    pub falloff: f64,
    /// Height in millimetres that Deposit never raises above and Remove
    /// never lowers below. Heights already past it are left alone.
    pub limit: Option<f64>,
    /// Preserve Transparency off: the brush also works on empty cells
    /// under it (taking them as zero height), growing the field if needed,
    /// so a shape can be sculpted past its edge or into the background.
    pub grow: bool,
    /// Normal, Raise or Lower.
    pub mode: SculptMode,
}

impl Default for BrushParams {
    fn default() -> Self {
        BrushParams {
            brush: Brush::Deposit,
            diameter: 10.0,
            strength: 0.5,
            falloff: 0.5,
            limit: None,
            grow: false,
            mode: SculptMode::Normal,
        }
    }
}

impl BrushParams {
    /// Checks every setting is in range.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] naming the first bad setting.
    pub fn validate(&self) -> Result<(), CoreError> {
        finite(self.diameter, "brush diameter")?;
        finite(self.strength, "brush strength")?;
        finite(self.falloff, "brush falloff")?;
        if self.diameter <= 0.0 {
            return Err(CoreError::validation("brush diameter must be above 0 mm"));
        }
        if !(0.0..=1.0).contains(&self.strength) {
            return Err(CoreError::validation(
                "brush strength must be between 0 and 1",
            ));
        }
        if !(0.0..=1.0).contains(&self.falloff) {
            return Err(CoreError::validation(
                "brush falloff must be between 0 and 1",
            ));
        }
        if let Some(limit) = self.limit {
            finite(limit, "brush height limit")?;
        }
        Ok(())
    }
}

fn finite(value: f64, what: &str) -> Result<(), CoreError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(CoreError::validation(format!("{what} must be a number")))
    }
}

/// Applies one brush stroke to a component's field.
///
/// `points` is the stroke in job coordinates (mm). Dabs are placed at the
/// first point and then every `diameter / 4` mm along the polyline. Per dab
/// and per cell, with `w` the falloff weight (0 to 1) and `s` the strength:
///
/// - Deposit adds `s * w * diameter / 20` mm, capped at `limit`.
/// - Remove subtracts the same, floored at `limit`.
/// - Smooth moves the height `s * w` of the way to its local mean.
/// - Smudge moves the height `s * w` of the way to the height one dab step
///   back along the stroke (the first dab has no direction and does
///   nothing).
/// - Erase multiplies the height by `1 - s * w` and empties the cell once
///   it is within [`ERASE_EPSILON`] mm of zero or `s * w` is 1.
///
/// An empty `points` slice returns the field unchanged.
///
/// # Errors
/// Returns [`CoreError::Validation`] for out of range settings, a
/// non-finite point, a field with a bad cell size, or a stroke needing more
/// than [`MAX_DABS`] dabs.
pub fn stroke(field: &Field, points: &[Point], params: &BrushParams) -> Result<Field, CoreError> {
    params.validate()?;
    if points.iter().any(|p| !p.is_finite()) {
        return Err(CoreError::validation("stroke points must be numbers"));
    }
    if points.is_empty() {
        return Ok(field.clone());
    }
    if !(field.cell.is_finite() && field.cell > 0.0) || field.z.len() != field.width * field.height
    {
        return Err(CoreError::validation(
            "the component's heights are damaged and cannot be sculpted",
        ));
    }
    let spacing = params.diameter / 4.0;
    let length: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    if length / spacing >= MAX_DABS as f64 {
        return Err(CoreError::validation(
            "the stroke is too long for the brush size; use a larger brush or a shorter stroke",
        ));
    }
    let dabs = dab_centres(points, spacing);
    let brush = Dab::new(params);

    let grow = params.grow && params.brush != Brush::Erase;
    let (mut out, keep) = if grow {
        grown(field, &dabs, brush.radius)
    } else {
        (field.clone(), None)
    };
    if out.width == 0 || out.height == 0 {
        return Ok(out);
    }

    let mut scratch = Scratch::default();
    for (n, &centre) in dabs.iter().enumerate() {
        match params.brush {
            Brush::Deposit | Brush::Remove => brush.add(&mut out, centre, grow),
            Brush::Smooth => {
                if grow {
                    brush.fill_empty(&mut out, centre);
                }
                brush.smooth(&mut out, centre, &mut scratch);
            }
            Brush::Smudge => {
                if grow {
                    brush.fill_empty(&mut out, centre);
                }
                if n > 0 {
                    brush.smudge(&mut out, dabs[n - 1], centre, &mut scratch);
                }
            }
            Brush::Erase => brush.erase(&mut out, centre),
            Brush::Restore => {
                return Err(CoreError::validation(
                    "the undo brush has nothing to restore to",
                ))
            }
        }
    }

    if params.mode != SculptMode::Normal {
        keep_side(field, &mut out, params.mode);
    }
    Ok(match keep {
        Some(rect) => trimmed(&out, rect),
        None => out,
    })
}

/// Raise and Lower: where a cell had a height before the stroke, keep the
/// higher (Raise) or the lower (Lower) of the old and new heights. `after`
/// is on `before`'s lattice, maybe grown.
fn keep_side(before: &Field, after: &mut Field, mode: SculptMode) {
    for j in 0..after.height {
        for i in 0..after.width {
            let index = j * after.width + i;
            let new = after.z[index];
            let old = before.at(after.centre(i, j));
            if new.is_nan() || old.is_nan() {
                continue;
            }
            after.z[index] = match mode {
                SculptMode::Raise => new.max(old),
                SculptMode::Lower => new.min(old),
                SculptMode::Normal => new,
            };
        }
    }
}

/// The undo brush: one stroke moving `field` back towards `reference`,
/// the same component before sculpting began.
///
/// Per dab and per cell, with `w` the falloff weight and `s` the strength,
/// the height moves `s * w` of the way to the reference height. A cell the
/// reference leaves empty fades like Erase and empties; an empty cell the
/// reference fills comes back from zero. The field grows as needed to take
/// back cells sculpting erased, and is trimmed to the cells in use.
///
/// # Errors
/// As [`stroke`], and [`CoreError::Validation`] when the reference is on
/// another lattice (a different cell size, or cells not lined up), which
/// happens when the component was moved or rebuilt since.
pub fn stroke_restoring(
    field: &Field,
    reference: &Field,
    points: &[Point],
    params: &BrushParams,
) -> Result<Field, CoreError> {
    let as_deposit = BrushParams {
        brush: Brush::Deposit,
        ..*params
    };
    as_deposit.validate()?;
    if points.iter().any(|p| !p.is_finite()) {
        return Err(CoreError::validation("stroke points must be numbers"));
    }
    let lined_up = (field.cell - reference.cell).abs() <= 1e-9 * field.cell.abs().max(1.0) && {
        let fi = (reference.origin.x - field.origin.x) / field.cell;
        let fj = (reference.origin.y - field.origin.y) / field.cell;
        (fi - fi.round()).abs() < 1e-6 && (fj - fj.round()).abs() < 1e-6
    };
    if !lined_up {
        return Err(CoreError::validation(
            "the component changed since sculpting began; close and reopen Sculpt to undo from here",
        ));
    }
    if points.is_empty() {
        return Ok(field.clone());
    }
    if !(field.cell.is_finite() && field.cell > 0.0) || field.z.len() != field.width * field.height
    {
        return Err(CoreError::validation(
            "the component's heights are damaged and cannot be sculpted",
        ));
    }
    let spacing = params.diameter / 4.0;
    let length: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    if length / spacing >= MAX_DABS as f64 {
        return Err(CoreError::validation(
            "the stroke is too long for the brush size; use a larger brush or a shorter stroke",
        ));
    }
    let dabs = dab_centres(points, spacing);
    let brush = Dab::new(params);
    let (mut out, keep) = grown(field, &dabs, brush.radius);
    let lat = Lattice::of(&out);
    let di = ((out.origin.x - reference.origin.x) / out.cell).round() as i64;
    let dj = ((out.origin.y - reference.origin.y) / out.cell).round() as i64;
    let strength = brush.strength;
    for &centre in &dabs {
        let z = &mut out.z;
        brush.each(lat, centre, |i, j, w| {
            let slot = &mut z[j * lat.width + i];
            let target = f64::from(reference.get(i as i64 + di, j as i64 + dj));
            let t = (strength * w).min(1.0);
            if target.is_nan() {
                if slot.is_nan() {
                    return;
                }
                let new = f64::from(*slot) * (1.0 - t);
                *slot = if t >= 1.0 || new.abs() <= ERASE_EPSILON {
                    f32::NAN
                } else {
                    new as f32
                };
            } else {
                let base = if slot.is_nan() { 0.0 } else { f64::from(*slot) };
                *slot = if t >= 1.0 {
                    target as f32
                } else {
                    (base + (target - base) * t) as f32
                };
            }
        });
    }
    Ok(match keep {
        Some(rect) => trimmed(&out, rect),
        None => out,
    })
}

/// Dab centres along the polyline: the first point, then one every
/// `spacing` mm of travel.
pub(crate) fn dab_centres(points: &[Point], spacing: f64) -> Vec<Point> {
    let mut out = Vec::new();
    let Some(&first) = points.first() else {
        return out;
    };
    out.push(first);
    let mut since = 0.0;
    for seg in points.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let len = a.distance(b);
        if len <= 0.0 {
            continue;
        }
        let mut d = spacing - since;
        while d <= len {
            out.push(a.lerp(b, d / len));
            d += spacing;
        }
        since = len - (d - spacing);
    }
    out
}

/// Inclusive cell rectangle `(i0, j0, i1, j1)`.
pub(crate) type Rect = (i64, i64, i64, i64);

/// The field enlarged on its own lattice to hold every cell any dab can
/// reach, plus where the original field sits inside it.
pub(crate) fn grown(field: &Field, dabs: &[Point], radius: f64) -> (Field, Option<Rect>) {
    let (w, h) = (field.width as i64, field.height as i64);
    let (mut i0, mut j0, mut i1, mut j1) = (0i64, 0i64, w - 1, h - 1);
    for &c in dabs {
        let (a0, b0, a1, b1) = Lattice::of(field).reach(c, radius);
        if a0 <= a1 && b0 <= b1 {
            i0 = i0.min(a0);
            j0 = j0.min(b0);
            i1 = i1.max(a1);
            j1 = j1.max(b1);
        }
    }
    let (nw, nh) = ((i1 - i0 + 1).max(0) as usize, (j1 - j0 + 1).max(0) as usize);
    let origin = Point::new(
        field.origin.x + i0 as f64 * field.cell,
        field.origin.y + j0 as f64 * field.cell,
    );
    let mut out = Field::empty(nw, nh, field.cell, origin);
    let (di, dj) = ((-i0) as usize, (-j0) as usize);
    for j in 0..field.height {
        let src = &field.z[j * field.width..(j + 1) * field.width];
        let at = (j + dj) * nw + di;
        out.z[at..at + field.width].copy_from_slice(src);
    }
    (out, Some((-i0, -j0, -i0 + w - 1, -j0 + h - 1)))
}

/// A field's grid without its heights, so cells can be walked while the
/// heights are borrowed mutably.
#[derive(Clone, Copy)]
pub(crate) struct Lattice {
    pub(crate) origin: Point,
    pub(crate) cell: f64,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

impl Lattice {
    pub(crate) fn of(field: &Field) -> Lattice {
        Lattice {
            origin: field.origin,
            cell: field.cell,
            width: field.width,
            height: field.height,
        }
    }

    /// Cells a dab at `c` can reach, unclamped: indices may lie outside the
    /// field. Empty when `i0 > i1` or `j0 > j1`.
    fn reach(self, c: Point, radius: f64) -> Rect {
        let fi = |x: f64| (x - self.origin.x) / self.cell;
        let fj = |y: f64| (y - self.origin.y) / self.cell;
        (
            fi(c.x - radius).ceil() as i64,
            fj(c.y - radius).ceil() as i64,
            fi(c.x + radius).floor() as i64,
            fj(c.y + radius).floor() as i64,
        )
    }

    /// Cells a dab at `c` can reach, clamped to the field.
    pub(crate) fn span(self, c: Point, radius: f64) -> Option<(usize, usize, usize, usize)> {
        let (i0, j0, i1, j1) = self.reach(c, radius);
        let i0 = i0.max(0);
        let j0 = j0.max(0);
        let i1 = i1.min(self.width as i64 - 1);
        let j1 = j1.min(self.height as i64 - 1);
        if i0 > i1 || j0 > j1 {
            return None;
        }
        Some((i0 as usize, j0 as usize, i1 as usize, j1 as usize))
    }
}

/// The grown field cut back to the original rectangle plus every cell that
/// now holds material.
pub(crate) fn trimmed(field: &Field, keep: Rect) -> Field {
    let (mut i0, mut j0, mut i1, mut j1) = keep;
    for j in 0..field.height {
        let row = &field.z[j * field.width..(j + 1) * field.width];
        for (i, v) in row.iter().enumerate() {
            if !v.is_nan() {
                let (i, j) = (i as i64, j as i64);
                i0 = i0.min(i);
                j0 = j0.min(j);
                i1 = i1.max(i);
                j1 = j1.max(j);
            }
        }
    }
    let (i0, j0, i1, j1) = (
        i0.max(0) as usize,
        j0.max(0) as usize,
        (i1.max(0) as usize).min(field.width.saturating_sub(1)),
        (j1.max(0) as usize).min(field.height.saturating_sub(1)),
    );
    if i0 == 0 && j0 == 0 && i1 + 1 == field.width && j1 + 1 == field.height {
        return field.clone();
    }
    let (w, h) = (i1 - i0 + 1, j1 - j0 + 1);
    let mut z = Vec::with_capacity(w * h);
    for j in j0..=j1 {
        z.extend_from_slice(&field.z[j * field.width + i0..=j * field.width + i1]);
    }
    Field {
        width: w,
        height: h,
        cell: field.cell,
        origin: field.centre(i0, j0),
        z,
    }
}

/// Buffers reused from dab to dab.
#[derive(Default)]
struct Scratch {
    a: Vec<f64>,
    b: Vec<f64>,
    c: Vec<f64>,
    d: Vec<f64>,
    snap: Vec<f32>,
}

/// One brush, ready to lay dabs.
struct Dab {
    radius: f64,
    inner: f64,
    strength: f64,
    /// Signed height change per dab at full weight, mm.
    step: f64,
    limit: Option<f64>,
}

impl Dab {
    fn new(params: &BrushParams) -> Dab {
        let radius = params.diameter / 2.0;
        let rate = params.strength * params.diameter / DEPOSIT_DIVISOR;
        Dab {
            radius,
            inner: radius * (1.0 - params.falloff),
            strength: params.strength,
            step: if params.brush == Brush::Remove {
                -rate
            } else {
                rate
            },
            limit: params.limit,
        }
    }

    /// Falloff weight at `d` mm from the dab centre: 1 inside the hard core,
    /// easing smoothly (zero slope at both ends) to 0 at the radius.
    fn weight(&self, d: f64) -> f64 {
        if d >= self.radius {
            return 0.0;
        }
        if d <= self.inner {
            return 1.0;
        }
        let s = (d - self.inner) / (self.radius - self.inner);
        let u = 1.0 - s * s;
        u * u
    }

    /// Calls `f(i, j, weight)` for every cell of `lat` under a dab at `c`
    /// with a weight above zero.
    fn each(&self, lat: Lattice, c: Point, mut f: impl FnMut(usize, usize, f64)) {
        let Some((i0, j0, i1, j1)) = lat.span(c, self.radius) else {
            return;
        };
        for j in j0..=j1 {
            let dy = lat.origin.y + j as f64 * lat.cell - c.y;
            for i in i0..=i1 {
                let dx = lat.origin.x + i as f64 * lat.cell - c.x;
                let w = self.weight((dx * dx + dy * dy).sqrt());
                if w > 0.0 {
                    f(i, j, w);
                }
            }
        }
    }

    /// Deposit or Remove.
    fn add(&self, field: &mut Field, c: Point, grow: bool) {
        let lat = Lattice::of(field);
        let (step, limit) = (self.step, self.limit);
        let z = &mut field.z;
        self.each(lat, c, |i, j, w| {
            let slot = &mut z[j * lat.width + i];
            let base = if slot.is_nan() {
                if !grow {
                    return;
                }
                0.0
            } else {
                f64::from(*slot)
            };
            let mut new = base + step * w;
            if let Some(limit) = limit {
                new = if step >= 0.0 {
                    base.max(new.min(limit))
                } else {
                    base.min(new.max(limit))
                };
            }
            *slot = new as f32;
        });
    }

    /// Preserve Transparency off: empty cells under the brush become zero
    /// height, so smoothing and smudging can reach past the edge.
    fn fill_empty(&self, field: &mut Field, c: Point) {
        let lat = Lattice::of(field);
        let z = &mut field.z;
        self.each(lat, c, |i, j, _| {
            let slot = &mut z[j * lat.width + i];
            if slot.is_nan() {
                *slot = 0.0;
            }
        });
    }

    /// Erase: fade towards zero, then empty.
    fn erase(&self, field: &mut Field, c: Point) {
        let lat = Lattice::of(field);
        let strength = self.strength;
        let z = &mut field.z;
        self.each(lat, c, |i, j, w| {
            let slot = &mut z[j * lat.width + i];
            if slot.is_nan() {
                return;
            }
            let keep = 1.0 - strength * w;
            let new = f64::from(*slot) * keep;
            *slot = if keep <= 0.0 || new.abs() <= ERASE_EPSILON {
                f32::NAN
            } else {
                new as f32
            };
        });
    }

    /// Smooth: blend towards a box mean over non-empty neighbours.
    fn smooth(&self, field: &mut Field, c: Point, s: &mut Scratch) {
        let lat = Lattice::of(field);
        let Some((i0, j0, i1, j1)) = lat.span(c, self.radius) else {
            return;
        };
        let k = ((self.radius / 3.0) / field.cell).round().max(1.0) as usize;
        let wi0 = i0.saturating_sub(k);
        let wj0 = j0.saturating_sub(k);
        let wi1 = (i1 + k).min(field.width - 1);
        let wj1 = (j1 + k).min(field.height - 1);
        let (ww, wh) = (wi1 - wi0 + 1, wj1 - wj0 + 1);

        // Horizontal box sums of heights (a) and counts (b) per window row.
        s.a.clear();
        s.a.resize(ww * wh, 0.0);
        s.b.clear();
        s.b.resize(ww * wh, 0.0);
        s.c.clear();
        s.c.resize(ww + 1, 0.0);
        s.d.clear();
        s.d.resize(ww + 1, 0.0);
        for r in 0..wh {
            let row = &field.z[(wj0 + r) * field.width + wi0..=(wj0 + r) * field.width + wi1];
            for (x, v) in row.iter().enumerate() {
                let (sv, sc) = if v.is_nan() {
                    (0.0, 0.0)
                } else {
                    (f64::from(*v), 1.0)
                };
                s.c[x + 1] = s.c[x] + sv;
                s.d[x + 1] = s.d[x] + sc;
            }
            for x in 0..ww {
                let lo = x.saturating_sub(k);
                let hi = (x + k).min(ww - 1) + 1;
                s.a[r * ww + x] = s.c[hi] - s.c[lo];
                s.b[r * ww + x] = s.d[hi] - s.d[lo];
            }
        }

        // Vertical box sums over those, only for the dab's own cells. Cells
        // are visited row by row, so keep per column prefix sums of the
        // horizontal sums: c for heights, d for counts, (wh + 1) per column.
        s.c.clear();
        s.c.resize(ww * (wh + 1), 0.0);
        s.d.clear();
        s.d.resize(ww * (wh + 1), 0.0);
        for x in 0..ww {
            let base = x * (wh + 1);
            for r in 0..wh {
                s.c[base + r + 1] = s.c[base + r] + s.a[r * ww + x];
                s.d[base + r + 1] = s.d[base + r] + s.b[r * ww + x];
            }
        }
        let strength = self.strength;
        let (sc, sd) = (&s.c, &s.d);
        let z = &mut field.z;
        self.each(lat, c, |i, j, w| {
            let slot = &mut z[j * lat.width + i];
            if slot.is_nan() {
                return;
            }
            let base = (i - wi0) * (wh + 1);
            let y = j - wj0;
            let lo = base + y.saturating_sub(k);
            let hi = base + (y + k).min(wh - 1) + 1;
            let count = sd[hi] - sd[lo];
            if count <= 0.0 {
                return;
            }
            let mean = (sc[hi] - sc[lo]) / count;
            let old = f64::from(*slot);
            *slot = (old + strength * w * (mean - old)) as f32;
        });
    }

    /// Smudge: pull heights from one dab step back along the stroke.
    fn smudge(&self, field: &mut Field, from: Point, to: Point, s: &mut Scratch) {
        let lat = Lattice::of(field);
        let Some((i0, j0, i1, j1)) = lat.span(to, self.radius) else {
            return;
        };
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let m = ((dx.abs().max(dy.abs())) / field.cell).ceil() as usize + 1;
        let wi0 = i0.saturating_sub(m);
        let wj0 = j0.saturating_sub(m);
        let wi1 = (i1 + m).min(field.width - 1);
        let wj1 = (j1 + m).min(field.height - 1);
        let ww = wi1 - wi0 + 1;
        s.snap.clear();
        for j in wj0..=wj1 {
            s.snap
                .extend_from_slice(&field.z[j * field.width + wi0..=j * field.width + wi1]);
        }
        let wh = wj1 - wj0 + 1;
        let sample = |fx: f64, fy: f64| -> f64 {
            // Window relative cell coordinates.
            let (x, y) = (fx - wi0 as f64, fy - wj0 as f64);
            if x < 0.0 || y < 0.0 || x > (ww - 1) as f64 || y > (wh - 1) as f64 {
                return f64::NAN;
            }
            let (x0, y0) = (x.floor() as usize, y.floor() as usize);
            let (x1, y1) = ((x0 + 1).min(ww - 1), (y0 + 1).min(wh - 1));
            let (tx, ty) = (x - x0 as f64, y - y0 as f64);
            let g = |a: usize, b: usize| f64::from(s.snap[b * ww + a]);
            let (v00, v10, v01, v11) = (g(x0, y0), g(x1, y0), g(x0, y1), g(x1, y1));
            if v00.is_nan() || v10.is_nan() || v01.is_nan() || v11.is_nan() {
                // Near an edge of the material: nearest cell only.
                let (nx, ny) = (x.round() as usize, y.round() as usize);
                return g(nx.min(ww - 1), ny.min(wh - 1));
            }
            let top = v00 + (v10 - v00) * tx;
            let bottom = v01 + (v11 - v01) * tx;
            top + (bottom - top) * ty
        };
        let (sx, sy) = (dx / field.cell, dy / field.cell);
        let strength = self.strength;
        let z = &mut field.z;
        self.each(lat, to, |i, j, w| {
            let slot = &mut z[j * lat.width + i];
            if slot.is_nan() {
                return;
            }
            let src = sample(i as f64 - sx, j as f64 - sy);
            if src.is_nan() {
                return;
            }
            let old = f64::from(*slot);
            *slot = (old + strength * w * (src - old)) as f32;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: usize, h: usize, cell: f64, z: f32) -> Field {
        let mut f = Field::empty(w, h, cell, Point::ORIGIN);
        f.z.fill(z);
        f
    }

    fn params(brush: Brush) -> BrushParams {
        BrushParams {
            brush,
            diameter: 10.0,
            strength: 1.0,
            falloff: 1.0,
            limit: None,
            grow: false,
            mode: SculptMode::Normal,
        }
    }

    #[test]
    fn deposit_raises_the_centre_most_and_stays_inside_the_radius() {
        let f = flat(41, 41, 0.5, 1.0);
        let c = Point::new(10.0, 10.0);
        let out = stroke(&f, &[c], &params(Brush::Deposit)).expect("stroke");
        let top = out.at(c);
        assert!((f64::from(top) - (1.0 + 10.0 / 20.0)).abs() < 1e-6);
        for j in 0..out.height {
            for i in 0..out.width {
                let d = out.centre(i, j).distance(c);
                let v = out.z[j * out.width + i];
                assert!(v <= top);
                if d >= 5.0 {
                    assert_eq!(v, 1.0, "cell at {d} mm changed");
                } else if d < 4.0 {
                    assert!(v > 1.0);
                }
            }
        }
        assert_eq!((out.width, out.height, out.origin), (41, 41, f.origin));
    }

    #[test]
    fn the_limit_caps_deposit_and_floors_remove() {
        let f = flat(41, 41, 0.5, 1.0);
        let line = [Point::new(2.0, 10.0), Point::new(18.0, 10.0)];
        let mut p = params(Brush::Deposit);
        p.limit = Some(1.2);
        let out = stroke(&f, &line, &p).expect("stroke");
        assert!((out.max().expect("set") - 1.2).abs() < 1e-6);
        p.brush = Brush::Remove;
        p.limit = Some(0.9);
        let out = stroke(&f, &line, &p).expect("stroke");
        assert!((out.min().expect("set") - 0.9).abs() < 1e-6);
    }

    #[test]
    fn remove_lowers() {
        let f = flat(41, 41, 0.5, 3.0);
        let c = Point::new(10.0, 10.0);
        let out = stroke(&f, &[c], &params(Brush::Remove)).expect("stroke");
        assert!((f64::from(out.at(c)) - 2.5).abs() < 1e-6);
        assert!(out.z.iter().all(|&v| v <= 3.0));
    }

    #[test]
    fn smooth_flattens_a_spike_and_keeps_a_plane() {
        let mut f = flat(41, 41, 0.5, 2.0);
        f.z[20 * 41 + 20] = 10.0;
        let c = Point::new(10.0, 10.0);
        let out = stroke(&f, &[c], &params(Brush::Smooth)).expect("stroke");
        assert!(out.at(c) < 4.0);
        assert!(out.z.iter().all(|v| !v.is_nan()));

        // A tilted plane with a ragged edge of empty cells stays a plane.
        let mut plane = Field::empty(41, 41, 0.5, Point::ORIGIN);
        for j in 0..41 {
            for i in 0..41 {
                if i + j > 12 {
                    plane.z[j * 41 + i] = 5.0;
                }
            }
        }
        let line = [Point::new(3.0, 3.0), Point::new(15.0, 12.0)];
        let out = stroke(&plane, &line, &params(Brush::Smooth)).expect("stroke");
        assert_eq!(out, plane);
    }

    #[test]
    fn smudge_drags_a_bump_along_the_stroke() {
        let mut f = flat(80, 30, 1.0, 1.0);
        let bump = Point::new(15.0, 15.0);
        for j in 0..30 {
            for i in 0..80 {
                let d = f.centre(i, j).distance(bump);
                f.z[j * 80 + i] += (4.0 * (-(d * d) / 8.0).exp()) as f32;
            }
        }
        let centroid = |f: &Field| {
            let (mut m, mut mx) = (0.0, 0.0);
            for j in 0..f.height {
                for i in 0..f.width {
                    let v = f64::from(f.z[j * f.width + i]) - 1.0;
                    m += v;
                    mx += v * f.centre(i, j).x;
                }
            }
            mx / m
        };
        let mut p = params(Brush::Smudge);
        p.falloff = 0.5;
        let out = stroke(&f, &[bump, Point::new(45.0, 15.0)], &p).expect("stroke");
        assert!(out.at(bump) < 3.0);
        assert!(centroid(&out) > centroid(&f) + 5.0);
        assert!(out.z.iter().all(|v| !v.is_nan()));
    }

    #[test]
    fn erase_empties_under_the_brush() {
        let f = flat(41, 41, 0.5, 2.0);
        let c = Point::new(10.0, 10.0);
        let mut p = params(Brush::Erase);
        p.falloff = 0.0;
        let out = stroke(&f, &[c], &p).expect("stroke");
        assert!(out.at(c).is_nan());
        assert!(out.at(Point::new(10.0, 14.5)).is_nan());
        assert_eq!(out.at(Point::new(10.0, 15.5)), 2.0);
        assert_eq!((out.width, out.height), (41, 41));
    }

    #[test]
    fn grow_extends_the_field_on_its_lattice() {
        let f = flat(10, 10, 0.5, 1.0);
        let origin = Point::new(0.25, 0.25);
        let f = Field { origin, ..f };
        let mut p = params(Brush::Deposit);
        p.falloff = 0.0;
        p.grow = true;
        let c = Point::new(6.0, 2.5);
        let out = stroke(&f, &[c], &p).expect("stroke");
        // The brush reaches x 1 to 11 and y -2.5 to 7.5; the original field
        // covers x and y 0.25 to 4.75.
        assert_eq!((out.width, out.height), (22, 20));
        assert_eq!(out.origin, Point::new(0.25, -2.25));
        assert!((out.at(Point::new(8.0, 2.5)) - 0.5).abs() < 1e-6);
        assert!((out.at(Point::new(4.0, 2.5)) - 1.5).abs() < 1e-6);
        assert_eq!(out.at(Point::new(0.25, 0.25)), 1.0);
        assert!(out.at(Point::new(1.0, 6.0)).is_nan());
        // Without grow the grid is kept and empty cells stay empty.
        p.grow = false;
        let mut holey = f.clone();
        holey.z[5 * 10 + 9] = f32::NAN;
        let out = stroke(&holey, &[c], &p).expect("stroke");
        assert_eq!((out.width, out.height, out.origin), (10, 10, origin));
        assert!(out.z[5 * 10 + 9].is_nan());
    }

    #[test]
    fn bad_settings_are_refused() {
        let f = flat(5, 5, 1.0, 1.0);
        let pts = [Point::new(2.0, 2.0)];
        let bad = [
            BrushParams {
                diameter: 0.0,
                ..BrushParams::default()
            },
            BrushParams {
                diameter: f64::NAN,
                ..BrushParams::default()
            },
            BrushParams {
                strength: 1.5,
                ..BrushParams::default()
            },
            BrushParams {
                strength: -0.1,
                ..BrushParams::default()
            },
            BrushParams {
                falloff: f64::INFINITY,
                ..BrushParams::default()
            },
            BrushParams {
                limit: Some(f64::NAN),
                ..BrushParams::default()
            },
        ];
        for p in bad {
            assert!(stroke(&f, &pts, &p).is_err(), "{p:?} accepted");
        }
        let nan = [Point::new(f64::NAN, 1.0)];
        assert!(stroke(&f, &nan, &BrushParams::default()).is_err());
    }

    #[test]
    fn no_points_leave_the_field_alone() {
        let f = flat(5, 5, 1.0, 1.0);
        let out = stroke(&f, &[], &BrushParams::default()).expect("stroke");
        assert_eq!(out, f);
    }

    #[test]
    fn dabs_are_a_quarter_diameter_apart() {
        let d = dab_centres(&[Point::new(0.0, 0.0), Point::new(10.0, 0.0)], 2.5);
        assert_eq!(d.len(), 5);
        assert!((d[4].x - 10.0).abs() < 1e-12);
        let d = dab_centres(
            &[
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(3.0, 0.0),
            ],
            2.5,
        );
        assert_eq!(d.len(), 2);
        assert!((d[1].x - 2.5).abs() < 1e-12);
    }

    #[test]
    fn params_use_camel_case() {
        let p: BrushParams =
            serde_json::from_str(r#"{"brush":"smudge","diameter":4,"limit":2.5}"#).expect("parse");
        assert_eq!(p.brush, Brush::Smudge);
        assert_eq!(p.limit, Some(2.5));
        assert_eq!(p.strength, BrushParams::default().strength);
    }

    #[test]
    fn a_long_stroke_on_a_large_field_is_quick() {
        let f = flat(1000, 1000, 0.5, 1.0);
        let pts: Vec<Point> = (0..200)
            .map(|k| {
                let t = f64::from(k) / 199.0;
                Point::new(20.0 + 460.0 * t, 250.0 + 150.0 * (t * 12.0).sin())
            })
            .collect();
        for brush in [
            Brush::Deposit,
            Brush::Remove,
            Brush::Smooth,
            Brush::Smudge,
            Brush::Erase,
        ] {
            let p = BrushParams {
                brush,
                diameter: 20.0,
                grow: true,
                ..BrushParams::default()
            };
            let out = stroke(&f, &pts, &p).expect("stroke");
            assert_eq!((out.width, out.height), (1000, 1000));
        }
    }

    #[test]
    fn the_undo_brush_restores_sculpted_and_erased_cells() {
        let original = flat(41, 41, 0.5, 2.0);
        let c = Point::new(10.0, 10.0);
        let dug = stroke(&original, &[c], &params(Brush::Deposit)).expect("deposit");
        assert!(dug.at(c) > 2.0);
        let back =
            stroke_restoring(&dug, &original, &[c], &params(Brush::Restore)).expect("restore");
        assert!((back.at(c) - 2.0).abs() < 1e-6, "{}", back.at(c));

        // Erase a corner and crop it away, then paint it back.
        let mut cut = original.clone();
        for j in 0..10 {
            for i in 0..41 {
                cut.z[j * 41 + i] = f32::NAN;
            }
        }
        let cut = cut.cropped().expect("rest");
        assert!(cut.at(Point::new(10.0, 1.0)).is_nan());
        let back = stroke_restoring(
            &cut,
            &original,
            &[Point::new(10.0, 1.0)],
            &params(Brush::Restore),
        )
        .expect("restore");
        assert!((back.at(Point::new(10.0, 1.0)) - 2.0).abs() < 1e-6);

        // Cells the reference never had fade out.
        let grown = stroke(
            &original,
            &[Point::new(21.0, 10.0)],
            &BrushParams {
                grow: true,
                ..params(Brush::Deposit)
            },
        )
        .expect("grow");
        assert!(!grown.at(Point::new(21.0, 10.0)).is_nan());
        let back = stroke_restoring(
            &grown,
            &original,
            &[Point::new(21.0, 10.0)],
            &params(Brush::Restore),
        )
        .expect("restore");
        assert!(back.at(Point::new(21.0, 10.0)).is_nan());

        let shifted = Field {
            origin: Point::new(0.25, 0.0),
            ..original.clone()
        };
        assert!(stroke_restoring(&dug, &shifted, &[c], &params(Brush::Restore)).is_err());
        assert!(stroke(&dug, &[c], &params(Brush::Restore)).is_err());
    }

    #[test]
    fn raise_and_lower_keep_one_side_of_a_smoothing() {
        // A dip and a spike side by side.
        let mut f = flat(41, 41, 0.5, 2.0);
        f.z[20 * 41 + 16] = 0.0;
        f.z[20 * 41 + 24] = 6.0;
        let line = [Point::new(8.0, 10.0), Point::new(12.0, 10.0)];
        let raise = BrushParams {
            mode: SculptMode::Raise,
            ..params(Brush::Smooth)
        };
        let out = stroke(&f, &line, &raise).expect("stroke");
        assert!(out.at(Point::new(8.0, 10.0)) > 0.5, "the dip fills");
        assert_eq!(out.at(Point::new(12.0, 10.0)), 6.0, "the spike stays");
        assert!(out.z.iter().zip(&f.z).all(|(a, b)| a >= b));
        let lower = BrushParams {
            mode: SculptMode::Lower,
            ..raise
        };
        let out = stroke(&f, &line, &lower).expect("stroke");
        assert!(out.at(Point::new(12.0, 10.0)) < 5.5, "the spike comes down");
        assert_eq!(out.at(Point::new(8.0, 10.0)), 0.0, "the dip stays");
        assert!(out.z.iter().zip(&f.z).all(|(a, b)| a <= b));
    }

    #[test]
    fn without_preserve_transparency_smoothing_spreads_past_the_edge() {
        // A 10 wide block with empty cells round it.
        let mut f = Field::empty(40, 40, 0.5, Point::ORIGIN);
        for j in 10..30 {
            for i in 10..30 {
                f.z[j * 40 + i] = 4.0;
            }
        }
        let edge = [Point::new(4.5, 10.0)];
        let kept = stroke(&f, &edge, &params(Brush::Smooth)).expect("stroke");
        assert!(kept.at(Point::new(3.5, 10.0)).is_nan(), "transparency kept");
        let spread = BrushParams {
            grow: true,
            ..params(Brush::Smooth)
        };
        let out = stroke(&f, &edge, &spread).expect("stroke");
        let outside = out.at(Point::new(4.0, 10.0));
        assert!(!outside.is_nan() && outside > 0.0, "{outside}");
        assert!(out.at(Point::new(5.5, 10.0)) < 4.0, "the edge softens");
    }
}
