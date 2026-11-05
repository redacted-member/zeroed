//! Scaling the design with the job.
//!
//! When Job Setup changes the material size with "Scale design with job
//! size" on, every vector and relief component stretches and moves with the
//! material, so the design keeps its place on the sheet. The old material
//! rectangle maps onto the new one: a point at a fraction of the old width
//! sits at the same fraction of the new width. With no datum offset that
//! is a scale about the XY datum, which stays put.
//!
//! Relief heights are never scaled; only their footprint changes.
//!
//! Rotary jobs can also turn the design with the material when the
//! cylinder moves from one machine axis to the other (`job.setFlipped`):
//! the design turns a quarter turn about the old material centre and lands
//! on the new centre, so it keeps its place on the cylinder.
//!
//! Commands: `job.setScaled`, `job.setFlipped`; see
//! `docs/KERNEL_API.md`.

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;

use crate::doc::{Document, JobSetup};
use crate::error::CoreError;
use crate::geom::{BBox, Mat3, Point};
use crate::kernel::KernelReply;
use crate::model::field::Field;
use crate::model::transform::{transform, TransformParams};

/// Label of the undo step `job.setScaled` records.
pub const UNDO_LABEL: &str = "Job Setup";

/// Scale factors closer to one than this count as unchanged.
const SAME: f64 = 1e-12;

/// Whether `name` is a command of this module.
pub fn handles(name: &str) -> bool {
    name == "job.setScaled" || name == "job.setFlipped"
}

/// Runs `job.setScaled`.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses,
/// [`CoreError::InvalidArgs`] for arguments that do not parse, and the
/// errors of [`set_scaled`].
pub fn dispatch(document: &mut Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "job.setScaled" => {
            let args: SetScaledArgs = parse_args(name, args)?;
            let (scale, revision) = set_scaled(document, args.job)?;
            KernelReply::ok(&json!({
                "job": document.job_setup(),
                "scaleX": scale.scale_x,
                "scaleY": scale.scale_y,
                "revision": revision,
            }))
        }
        "job.setFlipped" => {
            let args: SetScaledArgs = parse_args(name, args)?;
            let revision = set_flipped(document, args.job)?;
            KernelReply::ok(&json!({ "job": document.job_setup(), "revision": revision }))
        }
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

/// Replaces the job setup and turns the design of the active side a
/// quarter turn counter clockwise about the old material centre, moving it
/// to the new centre, as one undo step. Answers the new revision.
///
/// # Errors
/// [`CoreError::Validation`] for a job that is not machinable; the
/// document is left untouched.
pub fn set_flipped(document: &mut Document, job: JobSetup) -> Result<u64, CoreError> {
    let old = document.job_bounds().centre();
    document.begin(UNDO_LABEL);
    let result = (|| -> Result<(), CoreError> {
        document.set_job(job)?;
        let new = document.job_bounds().centre();
        let params = TransformParams {
            offset: Point::new(new.x - old.x, new.y - old.y),
            scale_x: 1.0,
            scale_y: 1.0,
            rotate_deg: 90.0,
            centre: Some(old),
        };
        let sheet = document.active_sheet();
        let fields: Vec<(u64, Field)> = sheet_components(document, sheet)
            .into_iter()
            .map(|component| Ok((component.id, transform(&component.field, &params)?)))
            .collect::<Result<_, CoreError>>()?;
        let ids = sheet_entities(document, sheet);
        let turn = quarter_turn(old, new);
        document.transform_entities(&ids, &turn);
        for (id, field) in fields {
            document.update_component(id, |component| moved_with(component, field, &turn))?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(document.commit()),
        Err(error) => {
            document.rollback();
            Err(error)
        }
    }
}

/// A quarter turn counter clockwise about `from`, then a move to `to`.
fn quarter_turn(from: Point, to: Point) -> Mat3 {
    // x' = -(y - from.y) + to.x, y' = (x - from.x) + to.y
    Mat3::new(0.0, 1.0, -1.0, 0.0, to.x + from.y, to.y - from.x)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetScaledArgs {
    job: JobSetup,
}

fn parse_args<T: DeserializeOwned>(command: &str, args: &str) -> Result<T, CoreError> {
    serde_json::from_str(args).map_err(|error| CoreError::invalid_args(command, error))
}

/// How the design moves from one material rectangle to another: a point
/// `p` goes to `to + (p - from) * scale`, per axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DesignScale {
    /// Stretch along X (new width over old width).
    pub scale_x: f64,
    /// Stretch along Y (new height over old height).
    pub scale_y: f64,
    /// The old material's lower left corner, job millimetres.
    pub from: Point,
    /// The new material's lower left corner, job millimetres.
    pub to: Point,
}

impl DesignScale {
    /// The mapping of the `old` material rectangle onto `new`.
    ///
    /// # Errors
    /// [`CoreError::Validation`] when either rectangle has no width or
    /// height, or the factors are not finite.
    pub fn between(old: BBox, new: BBox) -> Result<Self, CoreError> {
        let (ow, oh) = (old.width(), old.height());
        if !(ow > 0.0 && oh > 0.0 && new.width() > 0.0 && new.height() > 0.0) {
            return Err(CoreError::validation(
                "the design can only be scaled between jobs with a size",
            ));
        }
        let scale = DesignScale {
            scale_x: new.width() / ow,
            scale_y: new.height() / oh,
            from: old.min,
            to: new.min,
        };
        if !(scale.scale_x.is_finite() && scale.scale_y.is_finite()) {
            return Err(CoreError::validation("the design scale is not a number"));
        }
        Ok(scale)
    }

    /// True when nothing moves.
    pub fn is_identity(&self) -> bool {
        (self.scale_x - 1.0).abs() < SAME
            && (self.scale_y - 1.0).abs() < SAME
            && (self.from.x - self.to.x).abs() < SAME
            && (self.from.y - self.to.y).abs() < SAME
    }

    /// Where `p` goes.
    pub fn apply(&self, p: Point) -> Point {
        Point::new(
            self.to.x + (p.x - self.from.x) * self.scale_x,
            self.to.y + (p.y - self.from.y) * self.scale_y,
        )
    }

    /// The same mapping as a matrix for vectors.
    pub fn matrix(&self) -> Mat3 {
        Mat3::new(
            self.scale_x,
            0.0,
            0.0,
            self.scale_y,
            self.to.x - self.from.x * self.scale_x,
            self.to.y - self.from.y * self.scale_y,
        )
    }

    /// The same mapping for a relief component's heights.
    pub fn field_params(&self) -> TransformParams {
        TransformParams {
            offset: Point::new(self.to.x - self.from.x, self.to.y - self.from.y),
            scale_x: self.scale_x,
            scale_y: self.scale_y,
            rotate_deg: 0.0,
            centre: Some(self.from),
        }
    }
}

/// Puts a component's moved heights in place. Its vectors moved with it
/// just before (and took their move back from its placement), so a shape
/// keeps lying where they put it.
fn moved_with(component: &mut crate::model::Component, field: Field, m: &crate::geom::Mat3) {
    component.field = field;
    if let Some(shape) = component.extra.shape.as_deref_mut() {
        shape.moved(m, false);
    }
}

/// Replaces the job setup and scales the design of the active side with
/// it, as one undo step. Answers the mapping used and the new revision.
///
/// # Errors
/// [`CoreError::Validation`] for a job that is not machinable or a
/// component that would grow too large; the document is left untouched.
pub fn set_scaled(document: &mut Document, job: JobSetup) -> Result<(DesignScale, u64), CoreError> {
    let old = document.job_bounds();
    document.begin(UNDO_LABEL);
    match apply(document, job, old) {
        Ok(scale) => {
            let revision = document.commit();
            Ok((scale, revision))
        }
        Err(error) => {
            document.rollback();
            Err(error)
        }
    }
}

fn apply(document: &mut Document, job: JobSetup, old: BBox) -> Result<DesignScale, CoreError> {
    document.set_job(job)?;
    let sheet = document.active_sheet();
    scale_sheet_design(document, sheet, old, document.job_bounds())
}

/// The material rectangle of a job setup in job space, in millimetres.
pub fn material_bounds(job: &JobSetup) -> BBox {
    let (x, y) = job.datum();
    BBox::new(
        Point::new(-x, -y),
        Point::new(job.width - x, job.height - y),
    )
}

/// The components on `sheet`.
fn sheet_components(document: &Document, sheet: u64) -> Vec<&crate::model::Component> {
    document
        .all_components()
        .into_iter()
        .filter(|c| document.sheet_of_component(c) == sheet)
        .collect()
}

/// The top level entities on `sheet` (those on a sheet that no longer
/// exists count as on the first sheet).
fn sheet_entities(document: &Document, sheet: u64) -> Vec<u64> {
    let first = document.sheets().first().map_or(sheet, |s| s.id);
    document
        .order()
        .iter()
        .copied()
        .filter(|id| {
            document.entity(*id).is_ok_and(|e| {
                e.sheet == sheet
                    || (sheet == first && !document.sheets().iter().any(|s| s.id == e.sheet))
            })
        })
        .collect()
}

/// Scales the design on one sheet (its vectors and components) from the
/// material rectangle `old` to `new`, as Design Scaling does when a sheet
/// is resized: the other sheets are left alone. Answers the mapping.
///
/// # Errors
/// [`CoreError::Validation`] for an empty rectangle or a component that
/// would grow too large.
pub fn scale_sheet_design(
    document: &mut Document,
    sheet: u64,
    old: BBox,
    new: BBox,
) -> Result<DesignScale, CoreError> {
    let scale = DesignScale::between(old, new)?;
    if scale.is_identity() {
        return Ok(scale);
    }
    let params = scale.field_params();
    let fields: Vec<(u64, Field)> = sheet_components(document, sheet)
        .into_iter()
        .map(|component| Ok((component.id, transform(&component.field, &params)?)))
        .collect::<Result<_, CoreError>>()?;
    let ids = sheet_entities(document, sheet);
    let m = scale.matrix();
    document.transform_entities(&ids, &m);
    for (id, field) in fields {
        document.update_component(id, |component| moved_with(component, field, &m))?;
    }
    Ok(scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Entity, EntityKind, XyOrigin};
    use crate::geom::rect;

    fn job(width: f64, height: f64, origin: XyOrigin) -> JobSetup {
        JobSetup {
            width,
            height,
            xy_origin: origin,
            ..JobSetup::default()
        }
    }

    fn with_rect(origin: XyOrigin, a: Point, b: Point) -> (Document, u64) {
        let mut doc = Document::new();
        doc.set_job(job(100.0, 50.0, origin)).expect("job");
        doc.begin("Create");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: rect(a, b).expect("rect"),
            },
        });
        doc.commit();
        (doc, id)
    }

    fn bbox_of(doc: &Document, id: u64) -> BBox {
        doc.entities()
            .get(&id)
            .and_then(Entity::bbox)
            .expect("entity has a box")
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn design_scaling_touches_only_the_resized_sheet() {
        let (mut doc, on_first) = with_rect(
            XyOrigin::BottomLeft,
            Point::new(10.0, 10.0),
            Point::new(20.0, 20.0),
        );
        doc.begin("Sheet");
        let second = doc.add_sheet(None);
        let on_second = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: second,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: rect(Point::new(10.0, 10.0), Point::new(20.0, 20.0)).expect("rect"),
            },
        });
        doc.commit();
        // Resizing the active (first) sheet leaves the second sheet's design.
        let args = json!({ "job": job(200.0, 100.0, XyOrigin::BottomLeft) }).to_string();
        dispatch(&mut doc, "job.setScaled", &args).expect("scaled");
        assert!(close(bbox_of(&doc, on_first).max.x, 40.0));
        assert!(close(bbox_of(&doc, on_second).max.x, 20.0));
        // Edit Sheet with Design Scaling scales that sheet's design by its
        // own old to new size (100 x 50, as added, to 50 x 25).
        let mut material = doc.sheet_material(second).expect("material");
        assert!(close(material.width, 100.0));
        let before = material.width;
        material.width = before / 2.0;
        material.height /= 2.0;
        let edit = json!({ "ids": [second], "material": material, "scaleDesign": true });
        crate::doc::layers_cmd::dispatch(&mut doc, "sheets.edit", &edit.to_string())
            .expect("edit sheet");
        let b = bbox_of(&doc, on_second);
        assert!(close(b.max.x, 10.0) && close(b.min.x, 5.0), "{b:?}");
        assert!(close(bbox_of(&doc, on_first).max.x, 40.0));
    }

    #[test]
    fn flipping_turns_the_design_a_quarter_turn_onto_the_new_material() {
        // A 100 x 50 sheet with a 10 x 5 box in its lower left corner.
        let (mut doc, id) = with_rect(
            XyOrigin::BottomLeft,
            Point::new(0.0, 0.0),
            Point::new(10.0, 5.0),
        );
        let args = json!({ "job": job(50.0, 100.0, XyOrigin::BottomLeft) }).to_string();
        dispatch(&mut doc, "job.setFlipped", &args).expect("flipped");
        // Old centre (50, 25) goes to the new centre (25, 50): x' = 50 - y,
        // y' = x, so the box spans x 45..50 and y 0..10.
        let b = bbox_of(&doc, id);
        assert!(close(b.min.x, 45.0) && close(b.max.x, 50.0), "{b:?}");
        assert!(close(b.min.y, 0.0) && close(b.max.y, 10.0), "{b:?}");
        assert!(close(doc.job().width, 50.0));
        doc.undo().expect("undo");
        let back = bbox_of(&doc, id);
        assert!(close(back.max.x, 10.0) && close(back.max.y, 5.0));
    }

    #[test]
    fn a_new_rotary_diameter_scales_the_design_round_the_blank() {
        use crate::doc::{JobKind, RotaryAxis, RotarySetup};
        use std::f64::consts::PI;
        let rotary = |diameter: f64| JobSetup {
            kind: JobKind::Rotary,
            rotary: RotarySetup {
                diameter,
                axis: RotaryAxis::X,
                length: 200.0,
                ..RotarySetup::default()
            },
            ..JobSetup::default()
        };
        let mut doc = Document::new();
        doc.set_job(rotary(60.0)).expect("job");
        // A ring groove drawn all the way round, at X 20.
        doc.begin("Create");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: 1,
            sheet: 1,
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path {
                path: rect(Point::new(20.0, 0.0), Point::new(21.0, PI * 60.0)).expect("rect"),
            },
        });
        doc.commit();
        let args = json!({ "job": rotary(62.0) }).to_string();
        dispatch(&mut doc, "job.setScaled", &args).expect("scaled");
        let b = bbox_of(&doc, id);
        assert!(close(doc.job().height, PI * 62.0), "{}", doc.job().height);
        assert!(close(b.min.y, 0.0) && close(b.max.y, PI * 62.0), "{b:?}");
        assert!(close(b.min.x, 20.0) && close(b.max.x, 21.0), "{b:?}");
    }

    #[test]
    fn the_mapping_keeps_fractions_of_the_material() {
        let old = BBox::new(Point::new(0.0, 0.0), Point::new(100.0, 50.0));
        let new = BBox::new(Point::new(0.0, 0.0), Point::new(200.0, 150.0));
        let scale = DesignScale::between(old, new).expect("scale");
        assert!(close(scale.scale_x, 2.0) && close(scale.scale_y, 3.0));
        let p = scale.apply(Point::new(25.0, 10.0));
        assert!(close(p.x, 50.0) && close(p.y, 30.0));
        let m = scale.matrix();
        assert!(close(m.a * 25.0 + m.e, 50.0) && close(m.d * 10.0 + m.f, 30.0));
    }

    #[test]
    fn a_centred_datum_stays_put() {
        let old = BBox::new(Point::new(-50.0, -25.0), Point::new(50.0, 25.0));
        let new = BBox::new(Point::new(-100.0, -50.0), Point::new(100.0, 50.0));
        let scale = DesignScale::between(old, new).expect("scale");
        let origin = scale.apply(Point::ORIGIN);
        assert!(close(origin.x, 0.0) && close(origin.y, 0.0));
        let p = scale.apply(Point::new(10.0, -5.0));
        assert!(close(p.x, 20.0) && close(p.y, -10.0));
    }

    #[test]
    fn an_empty_rectangle_is_refused() {
        let old = BBox::new(Point::new(0.0, 0.0), Point::new(0.0, 50.0));
        let new = BBox::new(Point::new(0.0, 0.0), Point::new(10.0, 50.0));
        assert!(DesignScale::between(old, new).is_err());
    }

    #[test]
    fn vectors_scale_about_the_bottom_left_datum() {
        let (mut doc, id) = with_rect(
            XyOrigin::BottomLeft,
            Point::new(10.0, 10.0),
            Point::new(20.0, 30.0),
        );
        let (scale, _) =
            set_scaled(&mut doc, job(200.0, 100.0, XyOrigin::BottomLeft)).expect("scaled");
        assert!(close(scale.scale_x, 2.0) && close(scale.scale_y, 2.0));
        let b = bbox_of(&doc, id);
        assert!(close(b.min.x, 20.0) && close(b.min.y, 20.0));
        assert!(close(b.max.x, 40.0) && close(b.max.y, 60.0));
        assert!(close(doc.job_setup().width, 200.0));
    }

    #[test]
    fn vectors_scale_about_a_centre_datum() {
        let (mut doc, id) = with_rect(
            XyOrigin::Centre,
            Point::new(-10.0, -5.0),
            Point::new(10.0, 5.0),
        );
        set_scaled(&mut doc, job(300.0, 50.0, XyOrigin::Centre)).expect("scaled");
        let b = bbox_of(&doc, id);
        assert!(close(b.min.x, -30.0) && close(b.max.x, 30.0));
        assert!(close(b.min.y, -5.0) && close(b.max.y, 5.0));
    }

    #[test]
    fn one_undo_step_restores_the_job_and_the_design() {
        let (mut doc, id) = with_rect(
            XyOrigin::BottomLeft,
            Point::new(10.0, 10.0),
            Point::new(20.0, 30.0),
        );
        let depth = doc.journal().undo_depth();
        set_scaled(&mut doc, job(50.0, 25.0, XyOrigin::BottomLeft)).expect("scaled");
        assert_eq!(doc.journal().undo_depth(), depth + 1);
        assert_eq!(doc.journal().undo_label(), Some(UNDO_LABEL));
        assert_eq!(doc.undo().as_deref(), Some(UNDO_LABEL));
        assert!(close(doc.job_setup().width, 100.0));
        let b = bbox_of(&doc, id);
        assert!(close(b.min.x, 10.0) && close(b.max.y, 30.0));
    }

    #[test]
    fn a_bad_job_changes_nothing() {
        let (mut doc, id) = with_rect(
            XyOrigin::BottomLeft,
            Point::new(10.0, 10.0),
            Point::new(20.0, 30.0),
        );
        let revision = doc.revision();
        assert!(set_scaled(&mut doc, job(-5.0, 25.0, XyOrigin::BottomLeft)).is_err());
        assert_eq!(doc.revision(), revision);
        assert!(close(bbox_of(&doc, id).max.x, 20.0));
        assert!(close(doc.job_setup().width, 100.0));
    }

    #[test]
    fn components_stretch_but_keep_their_heights() {
        let mut doc = Document::new();
        doc.set_job(job(100.0, 50.0, XyOrigin::BottomLeft))
            .expect("job");
        let mut field = Field::empty(20, 10, 1.0, Point::new(10.5, 10.5));
        field.z.iter_mut().for_each(|z| *z = 2.0);
        doc.begin("Add");
        let id = doc.add_component(crate::model::Component {
            id: 0,
            name: "Pad".to_string(),
            visible: true,
            combine: crate::model::Combine::Add,
            height_scale: 1.0,
            level: 0,
            extra: Default::default(),
            field,
        });
        doc.commit();
        set_scaled(&mut doc, job(200.0, 50.0, XyOrigin::BottomLeft)).expect("scaled");
        let scaled = &doc.component(id).expect("component").field;
        assert_eq!(scaled.width, 40);
        assert_eq!(scaled.height, 10);
        assert!(close(scaled.cell, 1.0));
        assert!(scaled.z.iter().all(|z| close(f64::from(*z), 2.0)));
        let left = scaled.origin.x - scaled.cell / 2.0;
        assert!(close(left, 20.0), "left edge {left}");
    }

    #[test]
    fn the_command_answers_the_job_and_the_factors() {
        let (mut doc, _) = with_rect(
            XyOrigin::BottomLeft,
            Point::new(10.0, 10.0),
            Point::new(20.0, 30.0),
        );
        let args = json!({ "job": job(150.0, 50.0, XyOrigin::BottomLeft) }).to_string();
        let reply = dispatch(&mut doc, "job.setScaled", &args).expect("reply");
        let KernelReply::Json(text) = reply else {
            panic!("json reply");
        };
        let value: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(value["result"]["job"]["width"], 150.0);
        assert_eq!(value["result"]["scaleX"], 1.5);
        assert_eq!(value["result"]["scaleY"], 1.0);
        assert_eq!(value["result"]["revision"], doc.revision());
        assert!(dispatch(&mut doc, "job.setScaled", "{}").is_err());
    }
}
