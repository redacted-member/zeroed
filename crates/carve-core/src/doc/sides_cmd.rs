//! `job.otherSidePaths`: the other side's vectors of a double sided job,
//! turned over into the active side's job space, for Multi Sided View. A
//! view only; nothing changes.

use serde_json::json;

use crate::doc::Document;
use crate::error::CoreError;
use crate::kernel::KernelReply;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    name == "job.otherSidePaths"
}

/// Runs one of the commands [`handles`] accepts.
///
/// # Errors
/// [`CoreError::UnknownCommand`] for a name [`handles`] refuses.
pub fn dispatch(document: &Document, name: &str, _args: &str) -> Result<KernelReply, CoreError> {
    match name {
        "job.otherSidePaths" => KernelReply::ok(&json!({
            "revision": document.revision(),
            "paths": document.other_side_paths(),
        })),
        other => Err(CoreError::UnknownCommand(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Entity, EntityKind, JobKind, JobSetup, Side};
    use crate::geom::{rect, Point};

    #[test]
    fn the_other_side_vectors_come_back_turned_over() {
        let mut doc = Document::new();
        let job = JobSetup {
            kind: JobKind::DoubleSided,
            ..JobSetup::default()
        };
        doc.begin("Setup");
        doc.set_job(job).expect("job");
        doc.set_active_side(Side::Bottom).expect("bottom");
        let path = rect(Point::new(10.0, 20.0), Point::new(40.0, 50.0)).expect("rect");
        let (layer, sheet) = (doc.active_layer(), doc.active_sheet());
        doc.insert_entity(Entity {
            id: 0,
            layer,
            sheet,
            revision: 0,
            name: None,
            parent: None,
            rotation: 0.0,
            kind: EntityKind::Path { path },
        });
        doc.set_active_side(Side::Top).expect("top");
        doc.commit();
        let reply = match dispatch(&doc, "job.otherSidePaths", "{}").expect("runs") {
            KernelReply::Json(text) => {
                serde_json::from_str::<serde_json::Value>(&text).expect("json")["result"].clone()
            }
            KernelReply::Bytes(_) => panic!("json expected"),
        };
        let paths: Vec<crate::geom::Path> =
            serde_json::from_value(reply["paths"].clone()).expect("paths");
        assert_eq!(paths.len(), 1);
        let bbox = crate::doc::ops::bbox_of_path(&paths[0]).expect("bbox");
        // A 300 wide job turned over about its vertical middle.
        assert!((bbox.min.x - 260.0).abs() < 1e-9 && (bbox.max.x - 290.0).abs() < 1e-9);
        assert!((bbox.min.y - 20.0).abs() < 1e-9);
        let single = Document::new();
        assert_eq!(single.other_side_paths().len(), 0);
    }
}
