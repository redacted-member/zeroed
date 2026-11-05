//! The depth check after the job size changes: toolpaths that now cut
//! further below the bottom of their material (into the spoil board or the
//! bed) than they were set to. A through cut calculated to go 0.2 mm past
//! the material keeps that allowance, so only a real problem is reported.
//! Every toolpath is checked when the Check depth of all toolpaths
//! preference is on, only the visible ones when it is off.

use serde::Serialize;

use crate::cam::toolpath::Toolpath;
use crate::doc::{Document, SheetMaterial};

/// How far below the material bottom a toolpath may reach before it is
/// reported, in millimetres.
const DEPTH_SLACK: f64 = 1e-3;

/// A toolpath that cuts deeper than its material is thick.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TooDeep {
    /// The toolpath.
    pub id: u64,
    /// Its name in the list.
    pub name: String,
    /// How deep it cuts below the material surface, in millimetres.
    pub depth: f64,
    /// The thickness of its material, in millimetres.
    pub thickness: f64,
}

/// How deep `toolpath` cuts below its material surface and how thick that
/// material is, in millimetres, or `None` for an empty toolpath.
fn depth_and_thickness(document: &Document, toolpath: &Toolpath) -> Option<(f64, f64)> {
    let bounds = toolpath.bounds?;
    let material = document
        .sheet_material(toolpath.sheet)
        .unwrap_or_else(|_| SheetMaterial::of(document.job()));
    let job = material
        .applied_to(document.job_setup())
        .for_side(toolpath.side);
    Some((job.surface_z() - bounds.min.z, job.thickness))
}

/// How far `toolpath` cuts below the bottom of its material as the job
/// stands now, in millimetres: zero when it stays inside, `None` when it
/// has no moves or the job is rotary.
pub fn through_cut(document: &Document, toolpath: &Toolpath) -> Option<f64> {
    if document.job().is_rotary() {
        return None;
    }
    depth_and_thickness(document, toolpath).map(|(depth, thickness)| (depth - thickness).max(0.0))
}

/// The toolpaths, in list order, whose deepest move goes below the bottom
/// of their sheet's material by more than the toolpath's own through cut
/// allowance ([`Toolpath::through_cut`]). `all` checks hidden toolpaths
/// too. Rotary jobs have no bottom to cut through and answer nothing.
pub fn too_deep(document: &Document, all: bool) -> Vec<TooDeep> {
    if document.job().is_rotary() {
        return Vec::new();
    }
    let mut found = Vec::new();
    for toolpath in document.toolpaths() {
        if !all && !toolpath.visible {
            continue;
        }
        let Some((depth, thickness)) = depth_and_thickness(document, toolpath) else {
            continue;
        };
        let allowed = toolpath.through_cut.unwrap_or(0.0);
        if depth > thickness + allowed + DEPTH_SLACK {
            found.push(TooDeep {
                id: toolpath.id,
                name: toolpath.name.clone(),
                depth,
                thickness,
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::compute::{calculate, Request, Strategy};
    use crate::cam::tool::Tool;
    use crate::doc::{Entity, EntityKind};
    use crate::geom::{rect, Point};

    fn doc_with_profile(cut_depth: f64) -> (Document, u64) {
        let mut doc = Document::new();
        let path = rect(Point::new(0.0, 0.0), Point::new(40.0, 30.0)).expect("rect");
        doc.begin("Create");
        let id = doc.insert_entity(Entity {
            rotation: 0.0,
            id: 0,
            layer: doc.active_layer(),
            sheet: doc.active_sheet(),
            revision: 0,
            name: None,
            parent: None,
            kind: EntityKind::Path { path },
        });
        doc.commit();
        let request = Request {
            strategy: Strategy::Profile,
            tool: Tool {
                name: "End Mill".to_string(),
                pass_depth: 3.0,
                ..Tool::default()
            },
            params: serde_json::json!({ "cutDepth": cut_depth, "side": "outside" }),
            ids: vec![id],
            name: None,
        };
        let (toolpath, _) = calculate(&doc, &request).expect("profile");
        doc.begin("Calculate");
        let tp = doc.add_toolpath(toolpath);
        doc.commit();
        (doc, tp)
    }

    fn set_thickness(doc: &mut Document, thickness: f64) {
        let mut job = *doc.job();
        job.thickness = thickness;
        doc.set_job(job).expect("job");
    }

    #[test]
    fn a_toolpath_through_thinner_material_is_found() {
        let (mut doc, id) = doc_with_profile(12.0);
        assert!(too_deep(&doc, true).is_empty(), "fits the default job");
        set_thickness(&mut doc, 10.0);
        let found = too_deep(&doc, true);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, id);
        assert!((found[0].depth - 12.0).abs() < 1e-6, "{found:?}");
        assert!((found[0].thickness - 10.0).abs() < 1e-9);
    }

    #[test]
    fn hidden_toolpaths_are_checked_only_when_all_are() {
        let (mut doc, id) = doc_with_profile(12.0);
        set_thickness(&mut doc, 10.0);
        doc.begin("Hide");
        doc.update_toolpath(id, |t| t.visible = false)
            .expect("hide");
        doc.commit();
        assert!(too_deep(&doc, false).is_empty());
        assert_eq!(too_deep(&doc, true).len(), 1);
    }

    #[test]
    fn an_ordinary_through_cut_keeps_its_allowance() {
        // The default job is 18 mm thick: cut 0.2 mm through it.
        let (mut doc, id) = doc_with_profile(18.2);
        let allowance = doc.toolpath(id).expect("toolpath").through_cut;
        assert!(
            allowance.is_some_and(|a| (a - 0.2).abs() < 1e-6),
            "{allowance:?}"
        );
        assert!(
            too_deep(&doc, true).is_empty(),
            "a through cut is no problem"
        );
        // Moving the XY datum changes no depth.
        let mut job = *doc.job();
        job.xy_origin = crate::doc::XyOrigin::Centre;
        doc.set_job(job).expect("job");
        assert!(too_deep(&doc, true).is_empty());
        // Thicker material: still fine.
        set_thickness(&mut doc, 25.0);
        assert!(too_deep(&doc, true).is_empty());
        // Thinner material: now it cuts 2.2 mm into the spoil board.
        set_thickness(&mut doc, 16.0);
        let found = too_deep(&doc, true);
        assert_eq!(found.len(), 1);
        assert!((found[0].depth - 18.2).abs() < 1e-6, "{found:?}");
    }

    #[test]
    fn a_recalculation_from_stored_settings_keeps_the_allowance() {
        let (mut doc, id) = doc_with_profile(18.2);
        set_thickness(&mut doc, 10.0);
        let mut again = doc.toolpath(id).expect("toolpath").clone();
        again.through_cut = None;
        doc.begin("Recalculate");
        doc.replace_toolpath(id, again).expect("replace");
        doc.commit();
        assert_eq!(too_deep(&doc, true).len(), 1, "still 8.2 mm too deep");
        // New settings from the form measure it again.
        doc.begin("Edit");
        doc.remeasure_through_cut(id).expect("remeasure");
        doc.commit();
        assert!(too_deep(&doc, true).is_empty());
    }

    #[test]
    fn toolpaths_from_older_files_get_the_allowance_of_their_job() {
        let (doc, id) = doc_with_profile(18.2);
        let mut file = doc.to_file();
        for toolpath in &mut file.toolpaths {
            toolpath.through_cut = None;
        }
        let text = serde_json::to_string(&file).expect("save");
        assert!(!text.contains("throughCut"));
        let loaded =
            Document::from_file(serde_json::from_str(&text).expect("read"), 1).expect("load");
        let allowance = loaded.toolpath(id).expect("toolpath").through_cut;
        assert!(
            allowance.is_some_and(|a| (a - 0.2).abs() < 1e-6),
            "{allowance:?}"
        );
    }
}
