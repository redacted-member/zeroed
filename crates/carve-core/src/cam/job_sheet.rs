//! The job sheet: a self contained HTML page that sums up the job for the
//! person at the machine. It shows the layout of the vectors on the
//! material, the material setup (size, Z zero, datum, clearance heights and
//! home position, and the flip of a double sided job), then a summary row
//! and a detail block for every toolpath: tool, depth, feeds, speeds and the
//! estimated cutting time. Toolpaths are grouped by sheet and side.
//!
//! `cam.jobSheet` takes `{path, title?}` and writes the page to `path`.

use std::fmt::Write as _;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::json;

use crate::doc::{Document, FlipAxis, Side, Units, XyOrigin, ZZero};
use crate::error::CoreError;
use crate::geom::{flatten::flatten_path, Tolerance};
use crate::kernel::KernelReply;

use super::tool::ToolGeometry;
use super::toolpath::Toolpath;

/// Width of the layout picture, in CSS pixels.
const LAYOUT_PX: f64 = 480.0;
const MM_PER_INCH: f64 = 25.4;

/// True for the commands [`dispatch`] runs.
pub fn handles(name: &str) -> bool {
    name == "cam.jobSheet"
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobSheetArgs {
    path: PathBuf,
    #[serde(default)]
    title: Option<String>,
}

/// Runs `cam.jobSheet`.
///
/// # Errors
/// [`CoreError::InvalidArgs`] when the arguments do not parse,
/// [`CoreError::Validation`] when the job has no toolpaths, and
/// [`CoreError::Io`] when the file cannot be written.
pub fn dispatch(document: &Document, name: &str, args: &str) -> Result<KernelReply, CoreError> {
    if !handles(name) {
        return Err(CoreError::UnknownCommand(name.to_string()));
    }
    let args: JobSheetArgs =
        serde_json::from_str(args).map_err(|error| CoreError::invalid_args(name, error))?;
    let title = args.title.unwrap_or_default();
    let html = render(document, &title)?;
    std::fs::write(&args.path, html).map_err(|e| CoreError::io(args.path.display(), e))?;
    KernelReply::ok(&json!({ "path": args.path }))
}

/// Escapes text for HTML.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// A length in the job's units with its unit.
fn length(mm: f64, units: Units) -> String {
    match units {
        Units::Mm => format!("{mm:.2} mm"),
        Units::Inch => format!("{:.3} in", mm / MM_PER_INCH),
    }
}

/// A rate given in millimetres per minute, in the job's units.
fn rate(mm_per_min: f64, units: Units) -> String {
    match units {
        Units::Mm => format!("{mm_per_min:.0} mm/min"),
        Units::Inch => format!("{:.1} in/min", mm_per_min / MM_PER_INCH),
    }
}

/// Seconds as hours, minutes and seconds.
pub fn duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    format!("{h}:{m:02}:{s:02}")
}

fn tool_kind(geometry: &ToolGeometry) -> &'static str {
    match geometry {
        ToolGeometry::EndMill { .. } => "End Mill",
        ToolGeometry::BallNose { .. } => "Ball Nose",
        ToolGeometry::VBit { .. } => "V-Bit",
        ToolGeometry::EngravingTool { .. } => "Engraving",
        ToolGeometry::TaperedBallNose { .. } => "Tapered Ball Nose",
        ToolGeometry::RadiusedEndMill { .. } => "Radiused End Mill",
        ToolGeometry::FormTool { .. } => "Form Tool",
        #[allow(unreachable_patterns)]
        _ => "Tool",
    }
}

fn tool_label(toolpath: &Toolpath, units: Units) -> String {
    let tool = &toolpath.tool;
    format!(
        "{} ({}, {})",
        tool.name,
        tool_kind(&tool.tool_type),
        length(tool.diameter(), units)
    )
}

/// How deep the toolpath cuts below the material surface of its side.
fn cut_depth(document: &Document, toolpath: &Toolpath) -> f64 {
    let job = document.job_setup().for_side(toolpath.side);
    toolpath
        .bounds
        .map_or(0.0, |b| (job.surface_z() - b.min.z).max(0.0))
}

fn origin_label(origin: XyOrigin) -> &'static str {
    match origin {
        XyOrigin::Centre => "Center",
        XyOrigin::BottomLeft => "Bottom left",
        XyOrigin::BottomRight => "Bottom right",
        XyOrigin::TopLeft => "Top left",
        XyOrigin::TopRight => "Top right",
    }
}

fn z_zero_label(z: ZZero) -> &'static str {
    match z {
        ZZero::Top => "Material surface",
        ZZero::Bottom => "Machine bed",
    }
}

/// The vectors of `sheet` inside the material outline, as inline SVG.
fn layout_svg(document: &Document, sheet: u64) -> String {
    let bounds = document.job_bounds();
    let (w, h) = (bounds.width().max(1e-6), bounds.height().max(1e-6));
    let scale = LAYOUT_PX / w.max(h);
    let (px_w, px_h) = (w * scale, h * scale);
    let mut svg = String::new();
    let _ = write!(
        svg,
        "<svg class=\"layout\" xmlns=\"http://www.w3.org/2000/svg\" width=\"{px_w:.0}\" height=\"{px_h:.0}\" viewBox=\"{:.3} {:.3} {w:.3} {h:.3}\" role=\"img\" aria-label=\"Job layout\">",
        bounds.min.x,
        -bounds.max.y,
    );
    let stroke = 1.0 / scale;
    let _ = write!(
        svg,
        "<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{w:.3}\" height=\"{h:.3}\" fill=\"#f4efe6\" stroke=\"#555\" stroke-width=\"{:.4}\"/>",
        bounds.min.x,
        -bounds.max.y,
        stroke * 2.0,
    );
    let visible: Vec<u64> = document
        .layers()
        .iter()
        .filter(|l| l.visible)
        .map(|l| l.id)
        .collect();
    let tol = Tolerance::new(0.5 / scale).unwrap_or_default();
    for entity in document.entities().values() {
        let on_sheet = entity.sheet == 0 || entity.sheet == sheet;
        if !on_sheet || !visible.contains(&entity.layer) {
            continue;
        }
        let Some(path) = entity.path() else {
            continue;
        };
        let mut d = String::new();
        for line in flatten_path(&path, tol) {
            for (i, p) in line.points.iter().enumerate() {
                let _ = write!(
                    d,
                    "{}{:.3} {:.3} ",
                    if i == 0 { 'M' } else { 'L' },
                    p.x,
                    -p.y
                );
            }
            if line.closed {
                d.push('Z');
            }
        }
        if !d.is_empty() {
            let _ = write!(
                svg,
                "<path d=\"{}\" fill=\"none\" stroke=\"#1c1c1c\" stroke-width=\"{stroke:.4}\"/>",
                d.trim_end()
            );
        }
    }
    svg.push_str("</svg>");
    svg
}

const STYLE: &str = "body{font-family:-apple-system,Helvetica,Arial,sans-serif;color:#1c1c1c;margin:24px;max-width:960px}\
h1{font-size:22px;margin:0 0 4px}h2{font-size:17px;margin:28px 0 8px;border-bottom:1px solid #ccc;padding-bottom:4px}\
h3{font-size:15px;margin:16px 0 6px}table{border-collapse:collapse;width:100%;font-size:13px}\
th,td{text-align:left;padding:4px 8px;border-bottom:1px solid #e3e3e3}th{background:#f2f2f2}\
td.num,th.num{text-align:right}.layout{display:block;margin:8px 0;max-width:100%;height:auto}\
dl{display:grid;grid-template-columns:max-content 1fr;gap:4px 16px;font-size:13px;margin:0}dt{color:#555}dd{margin:0}\
@media print{body{margin:0}h2{break-after:avoid}}";

/// The whole job sheet as an HTML page.
///
/// # Errors
/// [`CoreError::Validation`] when the job has no toolpaths.
pub fn render(document: &Document, title: &str) -> Result<String, CoreError> {
    let toolpaths = document.toolpaths();
    if toolpaths.is_empty() {
        return Err(CoreError::validation(
            "there are no toolpaths to put on a job sheet",
        ));
    }
    let job = document.job_setup();
    let units = job.units;
    let title = if title.trim().is_empty() {
        "Job Sheet".to_string()
    } else {
        title.trim().to_string()
    };
    let mut html = String::new();
    let _ = write!(
        html,
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>{t}</title><style>{STYLE}</style></head><body><main><h1>{t}</h1>",
        t = esc(&title)
    );
    let total: f64 = toolpaths.iter().map(|t| t.estimated_seconds).sum();
    let _ = write!(
        html,
        "<p>Toolpaths: {}. Estimated cutting time: {}.</p>",
        toolpaths.len(),
        duration(total)
    );

    // Job layout and material setup.
    let _ = write!(html, "<section><h2>Job Layout</h2>");
    html.push_str(&layout_svg(document, document.active_sheet()));
    html.push_str("</section><section><h2>Material Setup</h2><dl>");
    let mut row = |label: &str, value: String| {
        let _ = write!(html, "<dt>{}</dt><dd>{}</dd>", esc(label), esc(&value));
    };
    row("Width (X)", length(job.width, units));
    row("Height (Y)", length(job.height, units));
    row("Thickness (Z)", length(job.thickness, units));
    row("Z zero", z_zero_label(job.z_zero).to_string());
    row("XY datum", origin_label(job.xy_origin).to_string());
    row("Clearance (Z1)", length(job.machine.safe_z, units));
    row("Plunge (Z2)", length(job.machine.plunge_gap, units));
    row(
        "Home position",
        format!(
            "X {} Y {} Z {}",
            length(job.machine.home_x, units),
            length(job.machine.home_y, units),
            length(job.machine.home_z, units)
        ),
    );
    if job.is_double_sided() {
        row(
            "Flip direction",
            match job.double_sided.flip {
                FlipAxis::Horizontal => "Horizontal".to_string(),
                FlipAxis::Vertical => "Vertical".to_string(),
            },
        );
        row(
            "Bottom side Z zero",
            z_zero_label(job.double_sided.bottom_z_zero).to_string(),
        );
    }
    html.push_str("</dl></section>");

    // Groups: sheet, then side.
    let sheet_name = |id: u64| {
        document
            .sheets()
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.clone())
    };
    let mut groups: Vec<(u64, Side)> = Vec::new();
    for t in &toolpaths {
        let key = (t.sheet, t.side);
        if !groups.contains(&key) {
            groups.push(key);
        }
    }
    let many_sheets = document.sheets().len() > 1;
    for (sheet, side) in groups {
        let members: Vec<&&Toolpath> = toolpaths
            .iter()
            .filter(|t| t.sheet == sheet && t.side == side)
            .collect();
        let mut heading = String::from("Toolpaths Summary");
        if many_sheets {
            if let Some(name) = sheet_name(sheet) {
                heading = format!("{heading}: {name}");
            }
        }
        if job.is_double_sided() {
            heading.push_str(match side {
                Side::Top => " (Top side)",
                Side::Bottom => " (Bottom side)",
            });
        }
        let _ = write!(
            html,
            "<section><h2>{}</h2><table><thead><tr><th scope=\"col\">#</th><th scope=\"col\">Toolpath</th><th scope=\"col\">Tool</th><th scope=\"col\" class=\"num\">Depth</th><th scope=\"col\" class=\"num\">Time</th></tr></thead><tbody>",
            esc(&heading)
        );
        for (index, t) in members.iter().enumerate() {
            let _ = write!(
                html,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
                index + 1,
                esc(&t.name),
                esc(&tool_label(t, units)),
                esc(&length(cut_depth(document, t), units)),
                duration(t.estimated_seconds)
            );
        }
        let subtotal: f64 = members.iter().map(|t| t.estimated_seconds).sum();
        let _ = write!(
            html,
            "</tbody><tfoot><tr><th scope=\"row\" colspan=\"4\">Total</th><td class=\"num\">{}</td></tr></tfoot></table>",
            duration(subtotal)
        );
        html.push_str("<h2>Toolpaths List</h2>");
        for (index, t) in members.iter().enumerate() {
            let _ = write!(html, "<h3>{}. {}</h3><dl>", index + 1, esc(&t.name));
            let mut row = |label: &str, value: String| {
                let _ = write!(html, "<dt>{}</dt><dd>{}</dd>", esc(label), esc(&value));
            };
            row("Tool", tool_label(t, units));
            row("Tool number", t.tool_number.to_string());
            row("Cut depth", length(cut_depth(document, t), units));
            row("Pass depth", length(t.tool.pass_depth, units));
            row("Stepover", length(t.tool.stepover, units));
            row("Spindle speed", format!("{:.0} r.p.m.", t.spindle));
            row("Feed rate", rate(t.feed, units));
            row("Plunge rate", rate(t.plunge, units));
            row("Estimated time", duration(t.estimated_seconds));
            if !t.visible {
                row("Visible", "No".to_string());
            }
            html.push_str("</dl>");
        }
        html.push_str("</section>");
    }
    html.push_str("</main></body></html>\n");
    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_as_hours_minutes_and_seconds() {
        assert_eq!(duration(0.0), "0:00:00");
        assert_eq!(duration(61.4), "0:01:01");
        assert_eq!(duration(3725.0), "1:02:05");
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(esc("a<b & \"c\""), "a&lt;b &amp; &quot;c&quot;");
    }

    #[test]
    fn inch_jobs_show_inches() {
        assert_eq!(length(25.4, Units::Inch), "1.000 in");
        assert_eq!(rate(2540.0, Units::Inch), "100.0 in/min");
        assert_eq!(length(3.0, Units::Mm), "3.00 mm");
    }
}
