//! Commands driven through [`Kernel::apply`] as a host would call them.

use super::*;
use crate::cam::compute::Request;
use crate::cam::preview_cmd::MOVE_RETRACT;
use crate::doc::JobSetup;
use crate::doc::{JobKind, Units, XyOrigin, ZZero};
use crate::geom::{self, rect, BBox, Path, Point};

mod document;
mod furniture;
mod joints;
mod relief;
mod sequences;
mod toolpaths;

fn envelope(reply: KernelReply) -> Value {
    match reply {
        KernelReply::Json(text) => serde_json::from_str(&text).expect("reply is JSON"),
        KernelReply::Bytes(_) => panic!("expected a JSON reply"),
    }
}

fn result_of(reply: KernelReply) -> Value {
    let value = envelope(reply);
    assert_eq!(value["ok"], Value::Bool(true), "{value}");
    value["result"].clone()
}

fn run(kernel: &mut Kernel, name: &str, args: &str) -> Value {
    result_of(
        kernel
            .apply(name, args)
            .unwrap_or_else(|error| panic!("{name} failed: {error}")),
    )
}

fn rect_args(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    let path = rect(Point::new(x0, y0), Point::new(x1, y1)).expect("rect");
    let entity = json!({
        "entity": {
            "kind": "path",
            "layer": 1,
            "sheet": 1,
            "path": path,
        }
    });
    entity.to_string()
}

fn make_rect(kernel: &mut Kernel, x0: f64, y0: f64, x1: f64, y1: f64) -> u64 {
    run(kernel, "entities.create", &rect_args(x0, y0, x1, y1))["id"]
        .as_u64()
        .expect("an id came back")
}

fn temp_kernel(label: &str) -> (crate::test_support::TempDir, Kernel) {
    let temp = crate::test_support::TempDir::new(label);
    let cam = CamState::new(
        crate::cam::tooldb::ToolStore::at(temp.path().join("tools.json")),
        crate::post::PostLibrary::at(temp.path().join("posts")),
    );
    (temp, Kernel::with_cam(cam))
}

fn end_mill() -> Value {
    serde_json::to_value(crate::cam::tool::Tool {
        name: "End Mill 6 mm".to_string(),
        pass_depth: 3.0,
        ..crate::cam::tool::Tool::default()
    })
    .expect("tool serialises")
}

fn line_entity(kernel: &mut Kernel, a: (f64, f64), b: (f64, f64)) -> u64 {
    let args = json!({ "entity": { "kind": "path", "path": {
        "subpaths": [{ "start": { "x": a.0, "y": a.1 },
                       "segments": [{ "kind": "line", "to": { "x": b.0, "y": b.1 } }],
                       "closed": false }] } } });
    run(kernel, "entities.create", &args.to_string())["id"]
        .as_u64()
        .expect("an id")
}

fn close(a: BBox, b: BBox) -> bool {
    (a.min.x - b.min.x).abs() < 1e-4
        && (a.min.y - b.min.y).abs() < 1e-4
        && (a.max.x - b.max.x).abs() < 1e-4
        && (a.max.y - b.max.y).abs() < 1e-4
}
