//! SVG import and export.
//!
//! SVG user space has Y going down and the document origin at the top left.
//! Job space has Y going up, so both directions flip Y about the top of the
//! drawing area. A file that was exported and imported again therefore comes
//! back where it started, as long as the job has not moved underneath it.
//!
//! Import understands `path` (every command including elliptical arcs and the
//! relative forms), `rect` with corner radii, `circle`, `ellipse`, `line`,
//! `polyline`, `polygon`, `g` and `use` (followed a few levels deep), with
//! `transform` on any of them. Styles are
//! ignored apart from noting that an element has no stroke and no fill, which
//! is treated as a hidden element and skipped. Text is skipped and reported.
//! `image` elements come in as placed bitmaps through [`import_all`], from
//! a `data:` URL or a file beside the SVG, fitted in their box the way
//! `preserveAspectRatio` says (a slice is placed as a meet).
//!
//! Export writes millimetres, one `<g id="...">` per layer, arcs as `A` and
//! cubics as `C`.

use std::fmt::Write as _;

use std::path::Path as FsPath;

use roxmltree::{Document as Xml, Node};

use super::{ImportWarning, ImportedImage, ImportedPath, VectorImport};
use crate::error::CoreError;
use crate::geom::{
    bulge_from_sweep, path_bbox, rounded_rect, sweep_from_bulge, transform_path, ArcGeom, BBox,
    Mat3, Path, Point, RectCorner, Segment, SubPath, Tolerance, Vec2,
};

/// Millimetres in one CSS pixel at the SVG default of 96 dots per inch.
pub const MM_PER_PX: f64 = 25.4 / 96.0;
/// Millimetres in one point.
pub const MM_PER_PT: f64 = 25.4 / 72.0;
/// Millimetres in one inch.
pub const MM_PER_IN: f64 = 25.4;

/// Reads vectors out of SVG text.
///
/// `scale` multiplies every length after unit handling, which is what the
/// import dialog's scale field does.
///
/// # Errors
/// Returns [`CoreError::Format`] when the text is not XML or has no `svg`
/// root element.
pub fn import(
    text: &str,
    scale: f64,
) -> Result<(Vec<ImportedPath>, Vec<ImportWarning>), CoreError> {
    let mut walker = Walker {
        out: Vec::new(),
        images: None,
        warnings: Vec::new(),
        base: None,
        uses: 0,
    };
    read(text, scale, &mut walker)?;
    Ok((walker.out, walker.warnings))
}

/// Reads vectors and placed images out of SVG text. Image files named by a
/// relative reference are looked for in `base`, the SVG file's folder.
///
/// # Errors
/// As [`import`].
pub fn import_all(
    text: &str,
    scale: f64,
    base: Option<&FsPath>,
) -> Result<VectorImport, CoreError> {
    let mut walker = Walker {
        out: Vec::new(),
        images: Some(Vec::new()),
        warnings: Vec::new(),
        base,
        uses: 0,
    };
    read(text, scale, &mut walker)?;
    Ok(VectorImport {
        paths: walker.out,
        images: walker.images.unwrap_or_default(),
        warnings: walker.warnings,
    })
}

/// What a walk over the SVG collects.
struct Walker<'a> {
    out: Vec<ImportedPath>,
    /// Placed images; `None` when images are only reported.
    images: Option<Vec<ImportedImage>>,
    warnings: Vec<ImportWarning>,
    base: Option<&'a FsPath>,
    /// `use` references being followed.
    uses: usize,
}

/// `use` references followed inside one another before giving up, which
/// also ends a reference that reaches itself.
const MAX_USE_DEPTH: usize = 8;

impl Walker<'_> {
    fn warn(&mut self, warning: ImportWarning) {
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }
}

fn read(text: &str, scale: f64, walker: &mut Walker<'_>) -> Result<(), CoreError> {
    let xml = Xml::parse(text)
        .map_err(|error| CoreError::format("SVG", format!("this is not valid XML: {error}")))?;
    let root = xml.root_element();
    if !root.has_tag_name("svg") {
        return Err(CoreError::format("SVG", "the root element is not <svg>"));
    }
    let view_box = parse_view_box(root.attribute("viewBox"));
    let width = root.attribute("width").and_then(parse_length);
    let height = root.attribute("height").and_then(parse_length);
    // User units become millimetres through the ratio of the declared
    // physical size to the viewBox, falling back to 96 dots per inch. A
    // viewBox of another shape is fitted as `preserveAspectRatio` says
    // (by default kept in proportion and centred), as viewers show it.
    let (sx, sy, tx, ty, page_height) = match (view_box, width, height) {
        (Some(vb), Some(w), Some(h)) if vb.2 > 0.0 && vb.3 > 0.0 => {
            let (sx, sy) = (w / vb.2, h / vb.3);
            match aspect(root.attribute("preserveAspectRatio")) {
                None => (sx, sy, 0.0, 0.0, h),
                Some((ax, ay, slice)) => {
                    let k = if slice { sx.max(sy) } else { sx.min(sy) };
                    (k, k, (w - vb.2 * k) * ax, (h - vb.3 * k) * ay, h)
                }
            }
        }
        (Some(vb), _, _) => (MM_PER_PX, MM_PER_PX, 0.0, 0.0, vb.3 * MM_PER_PX),
        _ => (MM_PER_PX, MM_PER_PX, 0.0, 0.0, height.unwrap_or(0.0)),
    };
    let (min_x, min_y, _, _) = view_box.unwrap_or((
        0.0,
        0.0,
        width.map(|w| w / MM_PER_PX).unwrap_or(0.0),
        height.map(|h| h / MM_PER_PX).unwrap_or(0.0),
    ));
    let scale = if scale.is_finite() && scale != 0.0 {
        scale
    } else {
        1.0
    };
    // Flip Y about the bottom of the viewBox so the drawing lands the right
    // way up, and move the viewBox origin to the job origin.
    let root_transform = Mat3::translate(Vec2::new(-min_x, -min_y))
        .then(&Mat3::scale(sx * scale, -sy * scale))
        .then(&Mat3::translate(Vec2::new(
            tx * scale,
            (page_height - ty) * scale,
        )));

    walk(root, root_transform, "Layer 1", walker);
    Ok(())
}

/// How `preserveAspectRatio` places a viewBox in its viewport: the
/// alignment (0 for min, 0.5 for mid, 1 for max) along X and Y, and whether
/// it slices rather than meets; None for `none`, which stretches it.
fn aspect(preserve: Option<&str>) -> Option<(f64, f64, bool)> {
    let mut words = preserve.unwrap_or("xMidYMid meet").split_whitespace();
    let align = words.next().unwrap_or("xMidYMid");
    if align == "none" {
        return None;
    }
    let at = |min: &str, max: &str| {
        if align.contains(min) {
            0.0
        } else if align.contains(max) {
            1.0
        } else {
            0.5
        }
    };
    Some((
        at("xMin", "xMax"),
        at("YMin", "YMax"),
        words.next() == Some("slice"),
    ))
}

/// True when the element is not drawn: `display` none, as an attribute or
/// in its `style` (how drawing programs write a hidden layer or object).
fn hidden(node: Node) -> bool {
    let none = |value: &str| value.trim() == "none";
    node.attribute("display").is_some_and(none)
        || node.attribute("style").is_some_and(|style| {
            style.split(';').any(|rule| {
                rule.split_once(':')
                    .is_some_and(|(key, value)| key.trim() == "display" && none(value))
            })
        })
}

/// Walks one element, applying its transform and recursing into groups.
fn walk(node: Node, parent: Mat3, layer: &str, walker: &mut Walker<'_>) {
    if hidden(node) {
        return;
    }
    let here = match node.attribute("transform") {
        Some(text) => parse_transform(text).then(&parent),
        None => parent,
    };
    let name = node.attribute("id").map(|s| s.to_string());
    match node.tag_name().name() {
        "svg" | "g" | "a" | "switch" => {
            // A group names the layer its children land on: by its <title>
            // when it has one (what our export writes, since an id cannot
            // hold spaces), else by its id, which is how most drawing
            // programs write layers.
            let title = node
                .children()
                .find(|child| child.has_tag_name("title"))
                .and_then(|child| child.text())
                .map(str::trim)
                .filter(|text| !text.is_empty());
            let layer = if node.has_tag_name("g") {
                title.or(node.attribute("id")).unwrap_or(layer)
            } else {
                layer
            };
            for child in node.children().filter(Node::is_element) {
                walk(child, here, layer, walker);
            }
        }
        // A symbol draws only where a `use` places it.
        "symbol" if walker.uses > 0 => {
            for child in node.children().filter(Node::is_element) {
                walk(child, here, layer, walker);
            }
        }
        "symbol" => {}
        "text" | "tspan" | "textPath" => walker.warn(ImportWarning::TextSkipped),
        "image" => {
            let placed = walker
                .images
                .is_some()
                .then(|| image_of(node, &here, layer, walker.base))
                .flatten();
            match (placed, walker.images.as_mut()) {
                (Some(image), Some(images)) => images.push(image),
                _ => walker.warn(ImportWarning::ImageSkipped),
            }
        }
        "use" => {
            let target = node
                .attribute(("http://www.w3.org/1999/xlink", "href"))
                .or_else(|| node.attribute("href"))
                .and_then(|href| href.trim().strip_prefix('#'))
                .and_then(|id| {
                    node.document()
                        .descendants()
                        .find(|n| n.is_element() && n.attribute("id") == Some(id))
                });
            match target {
                Some(target) if walker.uses < MAX_USE_DEPTH => {
                    let offset = Mat3::translate(Vec2::new(number(node, "x"), number(node, "y")));
                    walker.uses += 1;
                    walk(target, offset.then(&here), layer, walker);
                    walker.uses -= 1;
                }
                _ => walker.warn(ImportWarning::UseSkipped),
            }
        }
        "defs" | "style" | "title" | "desc" | "metadata" | "clipPath" | "mask" => {}
        _ => {
            if let Some(path) = shape_of(node) {
                if !path.is_empty() {
                    walker.out.push(ImportedPath {
                        layer: layer.to_string(),
                        name,
                        path: transform_path(&path, &here),
                    });
                }
            }
        }
    }
}

/// The bytes an image element refers to: a base64 `data:` URL, or a file
/// (a `file:` URL, an absolute path or one relative to `base`).
fn image_bytes(href: &str, base: Option<&FsPath>) -> Option<Vec<u8>> {
    let href = href.trim();
    if let Some(data) = href.strip_prefix("data:") {
        let (header, _) = data.split_once(',')?;
        if !header.ends_with(";base64") {
            return None;
        }
        return super::view_image::decode_base64(href).ok();
    }
    if href.contains("://") && !href.starts_with("file://") {
        return None;
    }
    let path = href.strip_prefix("file://").unwrap_or(href);
    let path = FsPath::new(path);
    let path = match base {
        Some(base) if path.is_relative() => base.join(path),
        _ => path.to_path_buf(),
    };
    std::fs::read(path).ok()
}

/// A placed image from an `image` element, its box fitted as
/// `preserveAspectRatio` asks.
fn image_of(node: Node, here: &Mat3, layer: &str, base: Option<&FsPath>) -> Option<ImportedImage> {
    let href = node
        .attribute(("http://www.w3.org/1999/xlink", "href"))
        .or_else(|| node.attribute("href"))?;
    let bytes = image_bytes(href, base)?;
    let (pixels_wide, pixels_high) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    if pixels_wide == 0 || pixels_high == 0 {
        return None;
    }
    let (iw, ih) = (f64::from(pixels_wide), f64::from(pixels_high));
    let x = number(node, "x");
    let y = number(node, "y");
    let w = node
        .attribute("width")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(iw);
    let h = node
        .attribute("height")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(ih);
    if !(w > 0.0 && h > 0.0) {
        return None;
    }
    let (x0, y0, w, h) = fit_image(node.attribute("preserveAspectRatio"), x, y, w, h, iw, ih);
    let quad = [
        here.apply(Point::new(x0, y0 + h)),
        here.apply(Point::new(x0 + w, y0 + h)),
        here.apply(Point::new(x0 + w, y0)),
        here.apply(Point::new(x0, y0)),
    ];
    let ext = href
        .rsplit('.')
        .next()
        .filter(|e| e.len() <= 4 && !e.contains('/'))
        .map(str::to_ascii_lowercase);
    Some(ImportedImage {
        layer: layer.to_string(),
        name: node.attribute("id").map(str::to_string),
        bytes,
        ext,
        quad,
    })
}

/// The part of the box `x, y, w, h` an image of `iw` by `ih` pixels fills.
fn fit_image(
    preserve: Option<&str>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    iw: f64,
    ih: f64,
) -> (f64, f64, f64, f64) {
    let align = preserve
        .and_then(|p| p.split_whitespace().next())
        .unwrap_or("xMidYMid");
    if align == "none" {
        return (x, y, w, h);
    }
    let k = (w / iw).min(h / ih);
    let (fw, fh) = (iw * k, ih * k);
    let ax = if align.contains("xMin") {
        0.0
    } else if align.contains("xMax") {
        1.0
    } else {
        0.5
    };
    let ay = if align.contains("YMin") {
        0.0
    } else if align.contains("YMax") {
        1.0
    } else {
        0.5
    };
    (x + (w - fw) * ax, y + (h - fh) * ay, fw, fh)
}

/// The geometry of one shape element, in its own user space.
fn shape_of(node: Node) -> Option<Path> {
    match node.tag_name().name() {
        "path" => parse_path_data(node.attribute("d")?).ok(),
        "rect" => {
            let x = number(node, "x");
            let y = number(node, "y");
            let w = number(node, "width");
            let h = number(node, "height");
            if w <= 0.0 || h <= 0.0 {
                return None;
            }
            let rx = node
                .attribute("rx")
                .and_then(|v| v.parse::<f64>().ok())
                .or_else(|| node.attribute("ry").and_then(|v| v.parse::<f64>().ok()))
                .unwrap_or(0.0);
            let a = Point::new(x, y);
            let b = Point::new(x + w, y + h);
            if rx > 0.0 {
                rounded_rect(
                    a,
                    b,
                    rx.min(w / 2.0).min(h / 2.0),
                    RectCorner::RadiusExternal,
                )
                .ok()
            } else {
                crate::geom::rect(a, b).ok()
            }
        }
        "circle" => {
            let r = number(node, "r");
            crate::geom::circle(Point::new(number(node, "cx"), number(node, "cy")), r).ok()
        }
        "ellipse" => crate::geom::ellipse(
            Point::new(number(node, "cx"), number(node, "cy")),
            number(node, "rx"),
            number(node, "ry"),
            crate::geom::EllipseForm::Cubic,
        )
        .ok(),
        "line" => {
            let a = Point::new(number(node, "x1"), number(node, "y1"));
            let b = Point::new(number(node, "x2"), number(node, "y2"));
            Some(crate::geom::line(a, b))
        }
        "polyline" | "polygon" => {
            let points = parse_points(node.attribute("points")?);
            crate::geom::polyline_path(&points, node.has_tag_name("polygon")).ok()
        }
        _ => None,
    }
}

fn number(node: Node, name: &str) -> f64 {
    node.attribute(name)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Reads `minX minY width height`.
fn parse_view_box(text: Option<&str>) -> Option<(f64, f64, f64, f64)> {
    let values: Vec<f64> = text?
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    match values.as_slice() {
        [a, b, c, d] => Some((*a, *b, *c, *d)),
        _ => None,
    }
}

/// Reads a length with an optional unit and answers millimetres.
pub fn parse_length(text: &str) -> Option<f64> {
    let text = text.trim();
    let split = text
        .find(|c: char| c.is_alphabetic() || c == '%')
        .unwrap_or(text.len());
    let value: f64 = text[..split].trim().parse().ok()?;
    let factor = match text[split..].trim() {
        "" | "px" => MM_PER_PX,
        "mm" => 1.0,
        "cm" => 10.0,
        "m" => 1000.0,
        "in" => MM_PER_IN,
        "pt" => MM_PER_PT,
        "pc" => MM_PER_PT * 12.0,
        // A percentage has no absolute size, so it is not a length we can use.
        _ => return None,
    };
    Some(value * factor)
}

/// Reads the `points` list of a polyline or polygon.
fn parse_points(text: &str) -> Vec<Point> {
    let values: Vec<f64> = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    values
        .chunks_exact(2)
        .map(|p| Point::new(p[0], p[1]))
        .collect()
}

/// Reads a `transform` attribute into one matrix.
pub fn parse_transform(text: &str) -> Mat3 {
    let mut result = Mat3::default();
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let name = rest[..open]
            .trim_matches(|c: char| !c.is_alphabetic())
            .trim();
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let args: Vec<f64> = rest[open + 1..open + close]
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<f64>().ok())
            .collect();
        let step = match (name, args.as_slice()) {
            ("translate", [x]) => Mat3::translate(Vec2::new(*x, 0.0)),
            ("translate", [x, y, ..]) => Mat3::translate(Vec2::new(*x, *y)),
            ("scale", [k]) => Mat3::scale(*k, *k),
            ("scale", [x, y, ..]) => Mat3::scale(*x, *y),
            ("rotate", [a]) => Mat3::rotate(a.to_radians()),
            ("rotate", [a, cx, cy, ..]) => Mat3::rotate_about(a.to_radians(), Point::new(*cx, *cy)),
            ("matrix", [a, b, c, d, e, f, ..]) => Mat3 {
                a: *a,
                b: *b,
                c: *c,
                d: *d,
                e: *e,
                f: *f,
            },
            ("skewX", [a]) => Mat3 {
                a: 1.0,
                b: 0.0,
                c: a.to_radians().tan(),
                d: 1.0,
                e: 0.0,
                f: 0.0,
            },
            ("skewY", [a]) => Mat3 {
                a: 1.0,
                b: a.to_radians().tan(),
                c: 0.0,
                d: 1.0,
                e: 0.0,
                f: 0.0,
            },
            _ => Mat3::default(),
        };
        // SVG applies the leftmost transform last, so each one goes before
        // what has been collected so far.
        result = step.then(&result);
        rest = &rest[open + close + 1..];
    }
    result
}

// ------------------------------------------------------------ path parsing

/// Pulls commands and numbers out of a `d` attribute.
struct Scanner<'a> {
    text: &'a [u8],
    at: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner {
            text: text.as_bytes(),
            at: 0,
        }
    }

    fn skip_separators(&mut self) {
        while self.at < self.text.len() {
            match self.text[self.at] {
                b' ' | b'\t' | b'\r' | b'\n' | b',' => self.at += 1,
                _ => break,
            }
        }
    }

    fn peek_command(&mut self) -> Option<u8> {
        self.skip_separators();
        let byte = *self.text.get(self.at)?;
        byte.is_ascii_alphabetic().then_some(byte)
    }

    fn take_command(&mut self) -> Option<u8> {
        let byte = self.peek_command()?;
        self.at += 1;
        Some(byte)
    }

    fn number(&mut self) -> Option<f64> {
        self.skip_separators();
        let start = self.at;
        if matches!(self.text.get(self.at), Some(b'+') | Some(b'-')) {
            self.at += 1;
        }
        let mut seen_digit = false;
        while let Some(byte) = self.text.get(self.at) {
            if byte.is_ascii_digit() {
                seen_digit = true;
                self.at += 1;
            } else {
                break;
            }
        }
        if self.text.get(self.at) == Some(&b'.') {
            self.at += 1;
            while let Some(byte) = self.text.get(self.at) {
                if byte.is_ascii_digit() {
                    seen_digit = true;
                    self.at += 1;
                } else {
                    break;
                }
            }
        }
        if !seen_digit {
            self.at = start;
            return None;
        }
        if matches!(self.text.get(self.at), Some(b'e') | Some(b'E')) {
            let mark = self.at;
            self.at += 1;
            if matches!(self.text.get(self.at), Some(b'+') | Some(b'-')) {
                self.at += 1;
            }
            let mut exponent = false;
            while let Some(byte) = self.text.get(self.at) {
                if byte.is_ascii_digit() {
                    exponent = true;
                    self.at += 1;
                } else {
                    break;
                }
            }
            if !exponent {
                self.at = mark;
            }
        }
        std::str::from_utf8(&self.text[start..self.at])
            .ok()?
            .parse()
            .ok()
    }

    /// A flag in an arc command, which may be written without a separator.
    fn flag(&mut self) -> Option<bool> {
        self.skip_separators();
        match self.text.get(self.at) {
            Some(b'0') => {
                self.at += 1;
                Some(false)
            }
            Some(b'1') => {
                self.at += 1;
                Some(true)
            }
            _ => None,
        }
    }

    fn point(&mut self) -> Option<Point> {
        let x = self.number()?;
        let y = self.number()?;
        Some(Point::new(x, y))
    }

    fn at_end(&mut self) -> bool {
        self.skip_separators();
        self.at >= self.text.len()
    }
}

/// Reads an SVG `d` attribute into a path, in the element's own user space.
///
/// # Errors
/// Returns [`CoreError::Format`] when the data does not start with a move.
pub fn parse_path_data(data: &str) -> Result<Path, CoreError> {
    let mut scanner = Scanner::new(data);
    let mut path = Path::new();
    let mut current: Option<SubPath> = None;
    let mut cursor = Point::ORIGIN;
    let mut start = Point::ORIGIN;
    let mut last_cubic: Option<Point> = None;
    let mut last_quad: Option<Point> = None;
    let mut command = 0u8;
    if !matches!(scanner.peek_command(), Some(b'M') | Some(b'm')) {
        return Err(CoreError::format(
            "SVG",
            "path data must start with a move command",
        ));
    }
    loop {
        if scanner.at_end() {
            break;
        }
        // Every pass must read something: numbers after a close path with
        // no command before them read nothing, and are an error that ends
        // the data as the SVG rules say, not a loop that never ends.
        let before = scanner.at;
        if let Some(next) = scanner.peek_command() {
            command = next;
            scanner.take_command();
        } else if command == b'M' {
            command = b'L';
        } else if command == b'm' {
            command = b'l';
        } else if command == 0 {
            return Err(CoreError::format(
                "SVG",
                "path data must start with a move command",
            ));
        }
        let relative = command.is_ascii_lowercase();
        let base = if relative { cursor } else { Point::ORIGIN };
        match command.to_ascii_uppercase() {
            b'M' => {
                let Some(p) = scanner.point() else { break };
                let p = Point::new(base.x + p.x, base.y + p.y);
                if let Some(sub) = current.take() {
                    push_if_real(&mut path, sub);
                }
                current = Some(SubPath::open(p));
                cursor = p;
                start = p;
                last_cubic = None;
                last_quad = None;
            }
            b'L' => {
                let Some(p) = scanner.point() else { break };
                let p = Point::new(base.x + p.x, base.y + p.y);
                line_to(&mut current, &mut cursor, p);
                last_cubic = None;
                last_quad = None;
            }
            b'H' => {
                let Some(x) = scanner.number() else { break };
                let p = Point::new(base.x + x, cursor.y);
                line_to(&mut current, &mut cursor, p);
                last_cubic = None;
                last_quad = None;
            }
            b'V' => {
                let Some(y) = scanner.number() else { break };
                let p = Point::new(cursor.x, base.y + y);
                line_to(&mut current, &mut cursor, p);
                last_cubic = None;
                last_quad = None;
            }
            b'C' => {
                let (Some(c1), Some(c2), Some(to)) =
                    (scanner.point(), scanner.point(), scanner.point())
                else {
                    break;
                };
                let c1 = Point::new(base.x + c1.x, base.y + c1.y);
                let c2 = Point::new(base.x + c2.x, base.y + c2.y);
                let to = Point::new(base.x + to.x, base.y + to.y);
                cubic_to(&mut current, &mut cursor, c1, c2, to);
                last_cubic = Some(c2);
                last_quad = None;
            }
            b'S' => {
                let (Some(c2), Some(to)) = (scanner.point(), scanner.point()) else {
                    break;
                };
                let c2 = Point::new(base.x + c2.x, base.y + c2.y);
                let to = Point::new(base.x + to.x, base.y + to.y);
                let c1 = reflect(cursor, last_cubic);
                cubic_to(&mut current, &mut cursor, c1, c2, to);
                last_cubic = Some(c2);
                last_quad = None;
            }
            b'Q' => {
                let (Some(q), Some(to)) = (scanner.point(), scanner.point()) else {
                    break;
                };
                let q = Point::new(base.x + q.x, base.y + q.y);
                let to = Point::new(base.x + to.x, base.y + to.y);
                let (c1, c2) = quad_to_cubic(cursor, q, to);
                cubic_to(&mut current, &mut cursor, c1, c2, to);
                last_quad = Some(q);
                last_cubic = Some(c2);
            }
            b'T' => {
                let Some(to) = scanner.point() else { break };
                let to = Point::new(base.x + to.x, base.y + to.y);
                let q = reflect(cursor, last_quad);
                let (c1, c2) = quad_to_cubic(cursor, q, to);
                cubic_to(&mut current, &mut cursor, c1, c2, to);
                last_quad = Some(q);
                last_cubic = Some(c2);
            }
            b'A' => {
                let (Some(rx), Some(ry), Some(rotation)) =
                    (scanner.number(), scanner.number(), scanner.number())
                else {
                    break;
                };
                let (Some(large), Some(sweep)) = (scanner.flag(), scanner.flag()) else {
                    break;
                };
                let Some(to) = scanner.point() else { break };
                let to = Point::new(base.x + to.x, base.y + to.y);
                let segments = arc_segments(cursor, to, rx, ry, rotation, large, sweep);
                if let Some(sub) = current.as_mut() {
                    sub.segments.extend(segments);
                }
                cursor = to;
                last_cubic = None;
                last_quad = None;
            }
            b'Z' => {
                if let Some(mut sub) = current.take() {
                    if !sub.segments.is_empty() {
                        if !sub.end().approx_eq(sub.start, Tolerance::EQUALITY) {
                            sub.line_to(sub.start);
                        } else if let Some(last) = sub.segments.last_mut() {
                            last.set_end(sub.start);
                        }
                        sub.closed = true;
                        push_if_real(&mut path, sub);
                    }
                }
                cursor = start;
                current = Some(SubPath::open(start));
                last_cubic = None;
                last_quad = None;
            }
            _ => break,
        }
        if scanner.at == before {
            break;
        }
    }
    if let Some(sub) = current.take() {
        push_if_real(&mut path, sub);
    }
    Ok(path)
}

fn push_if_real(path: &mut Path, sub: SubPath) {
    if !sub.segments.is_empty() {
        path.push(sub);
    }
}

fn line_to(current: &mut Option<SubPath>, cursor: &mut Point, to: Point) {
    if let Some(sub) = current.as_mut() {
        sub.line_to(to);
    }
    *cursor = to;
}

fn cubic_to(current: &mut Option<SubPath>, cursor: &mut Point, c1: Point, c2: Point, to: Point) {
    if let Some(sub) = current.as_mut() {
        sub.cubic_to(c1, c2, to);
    }
    *cursor = to;
}

/// The reflection of the previous control point, which the smooth commands
/// use. With no previous curve the control point sits on the cursor.
fn reflect(cursor: Point, previous: Option<Point>) -> Point {
    match previous {
        Some(p) => Point::new(2.0 * cursor.x - p.x, 2.0 * cursor.y - p.y),
        None => cursor,
    }
}

/// A quadratic curve as a cubic with the same shape.
fn quad_to_cubic(from: Point, q: Point, to: Point) -> (Point, Point) {
    (
        Point::new(
            from.x + 2.0 / 3.0 * (q.x - from.x),
            from.y + 2.0 / 3.0 * (q.y - from.y),
        ),
        Point::new(
            to.x + 2.0 / 3.0 * (q.x - to.x),
            to.y + 2.0 / 3.0 * (q.y - to.y),
        ),
    )
}

/// Turns an SVG endpoint arc into segments.
///
/// A circular arc with no rotation stays an arc, which keeps the geometry
/// exact for machining. An ellipse becomes cubics.
fn arc_segments(
    from: Point,
    to: Point,
    rx: f64,
    ry: f64,
    rotation_deg: f64,
    large: bool,
    sweep: bool,
) -> Vec<Segment> {
    let (rx, ry) = (rx.abs(), ry.abs());
    if rx <= 0.0 || ry <= 0.0 || from.approx_eq(to, Tolerance::EQUALITY) {
        return vec![Segment::Line { to }];
    }
    let phi = rotation_deg.to_radians();
    let (cos, sin) = (phi.cos(), phi.sin());
    // Centre parameterisation, following the SVG implementation notes.
    let dx = (from.x - to.x) / 2.0;
    let dy = (from.y - to.y) / 2.0;
    let x1 = cos * dx + sin * dy;
    let y1 = -sin * dx + cos * dy;
    let lambda = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    let (rx, ry) = if lambda > 1.0 {
        let k = lambda.sqrt();
        (rx * k, ry * k)
    } else {
        (rx, ry)
    };
    let numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let factor = if denominator.abs() < 1e-12 {
        0.0
    } else {
        (numerator / denominator).max(0.0).sqrt()
    };
    let factor = if large == sweep { -factor } else { factor };
    let cx1 = factor * rx * y1 / ry;
    let cy1 = -factor * ry * x1 / rx;
    let centre = Point::new(
        cos * cx1 - sin * cy1 + (from.x + to.x) / 2.0,
        sin * cx1 + cos * cy1 + (from.y + to.y) / 2.0,
    );
    let angle = |x: f64, y: f64| ((y - cy1) / ry).atan2((x - cx1) / rx);
    let start_angle = angle(x1, y1);
    let end_angle = angle(-x1, -y1);
    let mut delta = end_angle - start_angle;
    if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    } else if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    }

    if (rx - ry).abs() <= 1e-9 {
        // A circle: one arc segment carries it exactly.
        return vec![Segment::Arc {
            to,
            bulge: bulge_from_sweep(delta),
        }];
    }

    // An ellipse: split into quarter turns and fit a cubic to each.
    let steps = ((delta.abs() / std::f64::consts::FRAC_PI_2).ceil() as usize).max(1);
    let step = delta / steps as f64;
    let alpha = 4.0 / 3.0 * (step / 4.0).tan();
    let mut out = Vec::with_capacity(steps);
    let mut theta = start_angle;
    let mut point = from;
    for _ in 0..steps {
        let next = theta + step;
        let at = |t: f64| {
            let (ct, st) = (t.cos(), t.sin());
            Point::new(
                centre.x + rx * ct * cos - ry * st * sin,
                centre.y + rx * ct * sin + ry * st * cos,
            )
        };
        let derivative = |t: f64| {
            let (ct, st) = (t.cos(), t.sin());
            Vec2::new(
                -rx * st * cos - ry * ct * sin,
                -rx * st * sin + ry * ct * cos,
            )
        };
        let end = at(next);
        let d0 = derivative(theta);
        let d1 = derivative(next);
        out.push(Segment::Cubic {
            c1: point.offset(d0.scaled(alpha)),
            c2: end.offset(d1.scaled(-alpha)),
            to: end,
        });
        theta = next;
        point = end;
    }
    if let Some(last) = out.last_mut() {
        last.set_end(to);
    }
    out
}

// ----------------------------------------------------------------- export

/// One group of vectors to write out, with the layer they came from.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportLayer {
    /// Layer name, written as the group id.
    pub name: String,
    /// Layer colour as `#rrggbb`, written as the stroke.
    pub colour: String,
    /// The vectors on that layer.
    pub paths: Vec<Path>,
}

/// Writes vectors as an SVG document in millimetres.
///
/// The viewport covers `bounds`, and Y is flipped so the drawing comes out
/// the right way up. Importing the result puts the vectors back where they
/// were, as long as `bounds` starts at the job origin.
pub fn export(layers: &[ExportLayer], bounds: BBox) -> String {
    let width = bounds.width().max(1.0);
    let height = bounds.height().max(1.0);
    let mut out = String::with_capacity(1024);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" \
         width=\"{width}mm\" height=\"{height}mm\" viewBox=\"0 0 {width} {height}\">"
    );
    for (index, layer) in layers.iter().enumerate() {
        if layer.paths.is_empty() {
            continue;
        }
        let _ = writeln!(
            out,
            "  <g id=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"0.1\">",
            escape(&xml_id(&layer.name, index)),
            escape(&layer.colour)
        );
        let _ = writeln!(out, "    <title>{}</title>", escape(&layer.name));
        for path in &layer.paths {
            let data = path_data(path, bounds);
            if data.is_empty() {
                continue;
            }
            let _ = writeln!(out, "    <path d=\"{data}\"/>");
        }
        out.push_str("  </g>\n");
    }
    out.push_str("</svg>\n");
    out
}

/// The bounding box an export should cover: the job, grown to hold the
/// geometry when something sticks out of it.
pub fn export_bounds(job: BBox, paths: &[Path]) -> BBox {
    paths
        .iter()
        .filter_map(path_bbox)
        .fold(job, |acc, bbox| acc.union(bbox))
}

/// Writes one path as an SVG `d` attribute, flipping Y into SVG space.
fn path_data(path: &Path, bounds: BBox) -> String {
    let to_svg = |p: Point| Point::new(p.x - bounds.min.x, bounds.max.y - p.y);
    let mut out = String::new();
    for sub in &path.subpaths {
        if sub.segments.is_empty() {
            continue;
        }
        let start = to_svg(sub.start);
        let _ = write!(out, "M {} {}", fmt(start.x), fmt(start.y));
        let last = sub.segments.len().saturating_sub(1);
        for (index, (from, segment)) in sub.iter().enumerate() {
            let skip_close = sub.closed && index == last;
            match *segment {
                Segment::Line { to } => {
                    if skip_close {
                        continue;
                    }
                    let p = to_svg(to);
                    let _ = write!(out, " L {} {}", fmt(p.x), fmt(p.y));
                }
                Segment::Arc { to, bulge } => {
                    let p = to_svg(to);
                    match ArcGeom::from_bulge(from, to, bulge) {
                        Some(arc) => {
                            let sweep = sweep_from_bulge(bulge);
                            let large = if sweep.abs() > std::f64::consts::PI {
                                1
                            } else {
                                0
                            };
                            // Y is flipped on the way out, so a counter
                            // clockwise sweep here is clockwise in SVG.
                            let direction = if sweep > 0.0 { 0 } else { 1 };
                            let _ = write!(
                                out,
                                " A {} {} 0 {} {} {} {}",
                                fmt(arc.radius),
                                fmt(arc.radius),
                                large,
                                direction,
                                fmt(p.x),
                                fmt(p.y)
                            );
                        }
                        None => {
                            let _ = write!(out, " L {} {}", fmt(p.x), fmt(p.y));
                        }
                    }
                }
                Segment::Cubic { c1, c2, to } => {
                    let (a, b, p) = (to_svg(c1), to_svg(c2), to_svg(to));
                    let _ = write!(
                        out,
                        " C {} {}, {} {}, {} {}",
                        fmt(a.x),
                        fmt(a.y),
                        fmt(b.x),
                        fmt(b.y),
                        fmt(p.x),
                        fmt(p.y)
                    );
                }
            }
        }
        if sub.closed {
            out.push_str(" Z");
        }
        out.push(' ');
    }
    out.trim_end().to_string()
}

/// Six decimal places is a micron at metre scale, which is plenty and keeps
/// the file readable.
fn fmt(value: f64) -> String {
    let text = format!("{value:.6}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// A valid XML id for a layer: letters, digits, `-`, `_` and `.` kept, the
/// rest replaced with `_`, starting with a letter or `_`. Layer names can
/// repeat, so the layer's position is appended when the name is not usable.
fn xml_id(name: &str, index: usize) -> String {
    let mut id: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if !id.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        id.insert(0, '_');
    }
    if id.chars().all(|c| c == '_') {
        id = format!("layer_{}", index + 1);
    }
    id
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{circle, path_length, rect};

    fn bbox_of(paths: &[Path]) -> BBox {
        paths
            .iter()
            .filter_map(path_bbox)
            .reduce(|a, b| a.union(b))
            .expect("some geometry")
    }

    // Review 033 row 11: hidden elements are not imported.
    #[test]
    fn hidden_elements_are_left_out() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <g id="off" style="display:none"><rect width="10" height="10"/></g>
            <rect x="20" width="10" height="10" display="none"/>
            <rect x="40" width="10" height="10" style="fill: red; display : none"/>
            <rect x="60" width="10" height="10" style="display:inline"/>
        </svg>"#;
        let (paths, _) = import(svg, 1.0).expect("svg");
        assert_eq!(paths.len(), 1);
        let b = crate::geom::path_bbox(&paths[0].path).expect("bbox");
        assert!((b.min.x - 60.0).abs() < 1e-9, "{b:?}");
    }

    // Review 033 row 12: a viewBox of another shape keeps its proportions,
    // centred, as viewers draw it; `none` stretches it.
    #[test]
    fn a_view_box_of_another_shape_keeps_its_aspect() {
        let svg = |preserve: &str| {
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm" viewBox="0 0 100 100" {preserve}><circle cx="50" cy="50" r="40"/></svg>"#
            )
        };
        let size = |text: &str| {
            let (paths, _) = import(text, 1.0).expect("svg");
            let b = crate::geom::path_bbox(&paths[0].path).expect("bbox");
            (b.max.x - b.min.x, b.max.y - b.min.y, b.min.x, b.min.y)
        };
        let (w, h, x, y) = size(&svg(""));
        assert!(
            (w - 40.0).abs() < 1e-6 && (h - 40.0).abs() < 1e-6,
            "{w} x {h}"
        );
        assert!((x - 30.0).abs() < 1e-6 && (y - 5.0).abs() < 1e-6, "{x} {y}");
        let (_, _, x, _) = size(&svg(r#"preserveAspectRatio="xMinYMin meet""#));
        assert!((x - 5.0).abs() < 1e-6, "{x}");
        let (w, h, _, _) = size(&svg(r#"preserveAspectRatio="none""#));
        assert!(
            (w - 80.0).abs() < 1e-6 && (h - 40.0).abs() < 1e-6,
            "{w} x {h}"
        );
    }

    #[test]
    fn numbers_after_a_close_path_end_the_data() {
        // A damaged file (found by tests/fuzz_readers.rs) once looped
        // forever here: "Z" takes no numbers, so nothing was read.
        let path = parse_path_data("M 0 0 L 10 0 L 10 10 Z5 0 0 0 67.5 ").expect("reads");
        assert_eq!(path.subpaths.len(), 1);
        assert!(path.subpaths[0].closed);
        assert_eq!(path.subpaths[0].segments.len(), 3);
    }

    #[test]
    fn a_path_with_moves_and_lines_parses() {
        let path = parse_path_data("M 0 0 L 10 0 L 10 10 Z").expect("parses");
        assert_eq!(path.subpaths.len(), 1);
        assert!(path.subpaths[0].closed);
        assert_eq!(path.subpaths[0].segments.len(), 3);
    }

    #[test]
    fn relative_commands_are_followed() {
        let path = parse_path_data("m 5 5 l 10 0 l 0 10 z").expect("parses");
        let nodes = path.subpaths[0].nodes();
        assert!((nodes[0].x - 5.0).abs() < 1e-9);
        assert!((nodes[1].x - 15.0).abs() < 1e-9);
        assert!((nodes[2].y - 15.0).abs() < 1e-9);
    }

    #[test]
    fn repeated_coordinates_continue_the_last_command() {
        let path = parse_path_data("M0,0 10,0 10,10").expect("parses");
        assert_eq!(path.subpaths[0].segments.len(), 2);
    }

    #[test]
    fn horizontal_and_vertical_commands_work() {
        let path = parse_path_data("M 0 0 H 20 V 30 h -5 v -5").expect("parses");
        let nodes = path.subpaths[0].nodes();
        assert!((nodes[1].x - 20.0).abs() < 1e-9);
        assert!((nodes[2].y - 30.0).abs() < 1e-9);
        assert!((nodes[3].x - 15.0).abs() < 1e-9);
        assert!((nodes[4].y - 25.0).abs() < 1e-9);
    }

    #[test]
    fn smooth_cubics_reflect_the_previous_control_point() {
        let path = parse_path_data("M 0 0 C 10 10 20 10 30 0 S 50 -10 60 0").expect("parses");
        assert_eq!(path.subpaths[0].segments.len(), 2);
        match path.subpaths[0].segments[1] {
            Segment::Cubic { c1, .. } => assert!((c1.x - 40.0).abs() < 1e-9, "{c1:?}"),
            other => panic!("expected a cubic, got {other:?}"),
        }
    }

    #[test]
    fn quadratics_become_cubics() {
        let path = parse_path_data("M 0 0 Q 10 20 20 0").expect("parses");
        assert!(matches!(
            path.subpaths[0].segments[0],
            Segment::Cubic { .. }
        ));
    }

    #[test]
    fn a_circular_arc_stays_an_arc() {
        let path = parse_path_data("M 0 0 A 10 10 0 0 1 20 0").expect("parses");
        match path.subpaths[0].segments[0] {
            Segment::Arc { bulge, .. } => assert!(bulge.abs() > 0.5, "{bulge}"),
            other => panic!("expected an arc, got {other:?}"),
        }
    }

    #[test]
    fn an_elliptical_arc_becomes_cubics() {
        let path = parse_path_data("M 0 0 A 20 10 0 0 1 40 0").expect("parses");
        assert!(path.subpaths[0]
            .segments
            .iter()
            .all(|s| matches!(s, Segment::Cubic { .. })));
    }

    #[test]
    fn scientific_notation_parses() {
        let path = parse_path_data("M 1e1 0 L 2e1 0").expect("parses");
        assert!((path.subpaths[0].start.x - 10.0).abs() < 1e-9);
    }

    #[test]
    fn path_data_must_start_with_a_move() {
        assert!(parse_path_data("L 10 10").is_err());
    }

    #[test]
    fn lengths_carry_their_units() {
        assert!((parse_length("25.4mm").expect("mm") - 25.4).abs() < 1e-9);
        assert!((parse_length("1in").expect("in") - 25.4).abs() < 1e-9);
        assert!((parse_length("96").expect("px") - 25.4).abs() < 1e-9);
        assert!((parse_length("72pt").expect("pt") - 25.4).abs() < 1e-9);
        assert!(parse_length("50%").is_none());
    }

    #[test]
    fn transforms_combine_left_to_right() {
        let m = parse_transform("translate(10 0) scale(2)");
        let p = m.apply(Point::new(5.0, 0.0));
        assert!((p.x - 20.0).abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn shapes_import_with_the_right_sizes() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm"
            viewBox="0 0 100 100">
            <rect x="10" y="10" width="30" height="20"/>
            <circle cx="60" cy="60" r="15"/>
            <line x1="0" y1="0" x2="10" y2="10"/>
            <polygon points="70,10 90,10 90,30"/>
        </svg>"#;
        let (paths, warnings) = import(svg, 1.0).expect("imports");
        assert_eq!(paths.len(), 4);
        assert!(warnings.is_empty());
        let rect_bbox = path_bbox(&paths[0].path).expect("bbox");
        assert!((rect_bbox.width() - 30.0).abs() < 1e-6, "{rect_bbox:?}");
        assert!((rect_bbox.height() - 20.0).abs() < 1e-6, "{rect_bbox:?}");
        // Y is flipped: an element 10 from the top sits 70 up from the bottom.
        assert!((rect_bbox.min.y - 70.0).abs() < 1e-6, "{rect_bbox:?}");
    }

    #[test]
    fn groups_become_layers_and_carry_their_transform() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm"
            viewBox="0 0 100 100">
            <g id="Cut" transform="translate(10 0)">
              <rect x="0" y="0" width="10" height="10"/>
            </g>
        </svg>"#;
        let (paths, _) = import(svg, 1.0).expect("imports");
        assert_eq!(paths[0].layer, "Cut");
        let bbox = path_bbox(&paths[0].path).expect("bbox");
        assert!((bbox.min.x - 10.0).abs() < 1e-6, "{bbox:?}");
    }

    #[test]
    fn text_is_skipped_and_reported() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm"
            viewBox="0 0 10 10"><text x="1" y="1">hello</text></svg>"#;
        let (paths, warnings) = import(svg, 1.0).expect("imports");
        assert!(paths.is_empty());
        assert_eq!(warnings, vec![ImportWarning::TextSkipped]);
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let buffer = image::ImageBuffer::from_pixel(width, height, image::Luma([0u8]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        buffer
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("png");
        bytes.into_inner()
    }

    fn base64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let v = (u32::from(chunk[0]) << 16)
                | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for k in 0..4 {
                if k <= chunk.len() {
                    out.push(ALPHABET[((v >> (18 - 6 * k)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn embedded_images_are_placed_in_their_box() {
        let data = base64(&png(2, 1));
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
            width="100mm" height="100mm" viewBox="0 0 100 100">
            <g id="Photos">
              <image id="logo" x="10" y="10" width="40" height="40"
                xlink:href="data:image/png;base64,{data}"/>
              <image x="60" y="10" width="40" height="40" preserveAspectRatio="none"
                href="data:image/png;base64,{data}"/>
            </g></svg>"#
        );
        let VectorImport {
            paths,
            images,
            warnings,
        } = import_all(&svg, 1.0, None).expect("imports");
        assert!(paths.is_empty() && warnings.is_empty(), "{warnings:?}");
        assert_eq!(images.len(), 2);
        let logo = &images[0];
        assert_eq!(logo.layer, "Photos");
        assert_eq!(logo.name.as_deref(), Some("logo"));
        assert_eq!(logo.bytes, png(2, 1));
        // A 2 by 1 picture in a 40 square: 40 by 20, centred up and down.
        let expected = [(10.0, 60.0), (50.0, 60.0), (50.0, 80.0), (10.0, 80.0)];
        for (corner, (x, y)) in logo.quad.iter().zip(expected) {
            assert!(close(*corner, Point::new(x, y)), "{:?}", logo.quad);
        }
        // Stretched to the whole box.
        assert!(close(images[1].quad[0], Point::new(60.0, 50.0)));
        assert!(close(images[1].quad[2], Point::new(100.0, 90.0)));
        // The plain reader only reports them.
        let (_, warnings) = import(&svg, 1.0).expect("imports");
        assert_eq!(warnings, vec![ImportWarning::ImageSkipped]);
    }

    #[test]
    fn linked_images_are_read_beside_the_file() {
        let temp = crate::test_support::TempDir::new("svg-image");
        std::fs::write(temp.path().join("pic.png"), png(4, 4)).expect("writes");
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm"
            viewBox="0 0 10 10"><image href="pic.png" width="10" height="10"/>
            <image href="missing.png" width="10" height="10"/>
            <image href="https://example.com/a.png" width="10" height="10"/></svg>"#;
        let VectorImport {
            images, warnings, ..
        } = import_all(svg, 1.0, Some(temp.path())).expect("imports");
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].ext.as_deref(), Some("png"));
        assert!(close(images[0].quad[0], Point::new(0.0, 0.0)));
        assert!(close(images[0].quad[2], Point::new(10.0, 10.0)));
        assert_eq!(warnings, vec![ImportWarning::ImageSkipped]);
    }

    #[test]
    fn use_references_draw_their_target_where_they_are_placed() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
            width="100mm" height="100mm" viewBox="0 0 100 100">
            <defs><rect id="tile" width="10" height="10"/>
              <symbol id="pair"><use href="#tile"/><use href="#tile" x="20"/></symbol></defs>
            <use xlink:href="#tile" x="50" y="0"/>
            <use href="#pair" transform="translate(0 50)"/>
            <use href="#self" id="self"/>
            <use href="#nothing"/></svg>"##;
        let (paths, warnings) = import(svg, 1.0).expect("imports");
        assert_eq!(paths.len(), 3, "{paths:#?}");
        let first = path_bbox(&paths[0].path).expect("bbox");
        assert!((first.min.x - 50.0).abs() < 1e-9 && (first.max.y - 100.0).abs() < 1e-9);
        let paired = path_bbox(&paths[2].path).expect("bbox");
        assert!((paired.min.x - 20.0).abs() < 1e-9 && (paired.max.y - 50.0).abs() < 1e-9);
        assert_eq!(warnings, vec![ImportWarning::UseSkipped]);
    }

    #[test]
    fn a_file_that_is_not_svg_is_reported() {
        assert!(import("<html></html>", 1.0).is_err());
        assert!(import("not xml at all", 1.0).is_err());
    }

    #[test]
    fn user_units_without_a_physical_size_are_pixels() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96">
            <rect x="0" y="0" width="96" height="96"/></svg>"#;
        let (paths, _) = import(svg, 1.0).expect("imports");
        let bbox = path_bbox(&paths[0].path).expect("bbox");
        assert!((bbox.width() - 25.4).abs() < 1e-6, "{bbox:?}");
    }

    #[test]
    fn the_scale_option_multiplies_everything() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm"
            viewBox="0 0 100 100"><rect x="0" y="0" width="10" height="10"/></svg>"#;
        let (paths, _) = import(svg, 2.0).expect("imports");
        let bbox = path_bbox(&paths[0].path).expect("bbox");
        assert!((bbox.width() - 20.0).abs() < 1e-6, "{bbox:?}");
    }

    #[test]
    fn a_rectangle_survives_an_export_and_import() {
        let source = rect(Point::new(10.0, 20.0), Point::new(60.0, 50.0)).expect("rect");
        let bounds = BBox::new(Point::new(0.0, 0.0), Point::new(300.0, 200.0));
        let text = export(
            &[ExportLayer {
                name: "Layer 1".to_string(),
                colour: "#000000".to_string(),
                paths: vec![source.clone()],
            }],
            bounds,
        );
        let (back, _) = import(&text, 1.0).expect("imports");
        assert_eq!(back.len(), 1);
        let a = path_bbox(&source).expect("bbox");
        let b = path_bbox(&back[0].path).expect("bbox");
        assert!((a.min.x - b.min.x).abs() < 1e-6, "{a:?} {b:?}");
        assert!((a.min.y - b.min.y).abs() < 1e-6, "{a:?} {b:?}");
        assert!((a.max.x - b.max.x).abs() < 1e-6, "{a:?} {b:?}");
        assert!((a.max.y - b.max.y).abs() < 1e-6, "{a:?} {b:?}");
    }

    #[test]
    fn a_circle_survives_an_export_and_import_as_arcs() {
        let source = circle(Point::new(100.0, 80.0), 25.0).expect("circle");
        let bounds = BBox::new(Point::new(0.0, 0.0), Point::new(300.0, 200.0));
        let text = export(
            &[ExportLayer {
                name: "Layer 1".to_string(),
                colour: "#000000".to_string(),
                paths: vec![source.clone()],
            }],
            bounds,
        );
        assert!(text.contains(" A "), "arcs are written as A commands");
        let (back, _) = import(&text, 1.0).expect("imports");
        let tol = Tolerance::default();
        let a = path_length(&source, tol);
        let b = path_length(&back[0].path, tol);
        assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        let ba = path_bbox(&source).expect("bbox");
        let bb = path_bbox(&back[0].path).expect("bbox");
        assert!(
            (ba.centre().x - bb.centre().x).abs() < 1e-6,
            "{ba:?} {bb:?}"
        );
        assert!(
            (ba.centre().y - bb.centre().y).abs() < 1e-6,
            "{ba:?} {bb:?}"
        );
    }

    #[test]
    fn layer_ids_are_valid_xml_names() {
        assert_eq!(xml_id("Layer 1", 0), "Layer_1");
        assert_eq!(xml_id("2D cuts", 0), "_2D_cuts");
        assert_eq!(xml_id("   ", 3), "layer_4");
    }

    #[test]
    fn a_layer_name_with_spaces_survives_export_and_import() {
        let text = export(
            &[ExportLayer {
                name: "Layer 1".to_string(),
                colour: "#000000".to_string(),
                paths: vec![rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect")],
            }],
            BBox::new(Point::new(0.0, 0.0), Point::new(100.0, 100.0)),
        );
        let (paths, _) = import(&text, 1.0).expect("imports");
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].layer, "Layer 1");
    }

    #[test]
    fn export_names_the_layer_and_its_colour() {
        let text = export(
            &[ExportLayer {
                name: "Cut & Score".to_string(),
                colour: "#d62828".to_string(),
                paths: vec![rect(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).expect("rect")],
            }],
            BBox::new(Point::new(0.0, 0.0), Point::new(100.0, 100.0)),
        );
        assert!(text.contains("id=\"Cut___Score\""), "{text}");
        assert!(text.contains("<title>Cut &amp; Score</title>"), "{text}");
        assert!(text.contains("stroke=\"#d62828\""), "{text}");
    }

    #[test]
    fn export_bounds_grow_to_hold_stray_geometry() {
        let job = BBox::new(Point::new(0.0, 0.0), Point::new(100.0, 100.0));
        let stray = rect(Point::new(-20.0, 0.0), Point::new(-10.0, 10.0)).expect("rect");
        let bounds = export_bounds(job, std::slice::from_ref(&stray));
        assert!((bounds.min.x + 20.0).abs() < 1e-9, "{bounds:?}");
        let text = export(
            &[ExportLayer {
                name: "Layer 1".to_string(),
                colour: "#000000".to_string(),
                paths: vec![stray],
            }],
            bounds,
        );
        let (back, _) = import(&text, 1.0).expect("imports");
        let bbox = bbox_of(&[back[0].path.clone()]);
        assert!((bbox.width() - 10.0).abs() < 1e-6, "{bbox:?}");
    }

    #[test]
    fn an_open_vector_stays_open_through_a_round_trip() {
        let mut open = SubPath::open(Point::new(0.0, 0.0));
        open.line_to(Point::new(50.0, 0.0))
            .line_to(Point::new(50.0, 40.0));
        let source = open.into_path();
        let bounds = BBox::new(Point::new(0.0, 0.0), Point::new(100.0, 100.0));
        let text = export(
            &[ExportLayer {
                name: "Layer 1".to_string(),
                colour: "#000000".to_string(),
                paths: vec![source],
            }],
            bounds,
        );
        let (back, _) = import(&text, 1.0).expect("imports");
        assert!(!back[0].path.is_closed());
        assert_eq!(back[0].path.subpaths[0].segments.len(), 2);
    }
}
