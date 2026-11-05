//! Objects: the entities this importer draws, the layers they sit on and
//! the block definitions that inserts place.

use std::collections::HashMap;

use super::bits::Bits;
use super::header::{string_stream, Handles, Streams, Strings};
use super::{Bad, Sections, Version};

/// A point or vector.
pub(super) type P3 = [f64; 3];

/// One drawn entity.
#[derive(Debug, Clone)]
pub(super) struct Entity {
    pub(super) layer: String,
    pub(super) kind: Kind,
}

/// The geometry of an entity.
#[derive(Debug, Clone)]
pub(super) enum Kind {
    Line {
        start: P3,
        end: P3,
    },
    Circle {
        centre: P3,
        radius: f64,
        normal: P3,
    },
    Arc {
        centre: P3,
        radius: f64,
        start: f64,
        end: f64,
        normal: P3,
    },
    Ellipse {
        centre: P3,
        major: P3,
        normal: P3,
        ratio: f64,
        start: f64,
        end: f64,
    },
    /// Vertices with the bulge of the segment that leaves them.
    Polyline {
        points: Vec<(f64, f64, f64)>,
        closed: bool,
        normal: P3,
    },
    Spline {
        degree: usize,
        knots: Vec<f64>,
        control: Vec<P3>,
        weights: Vec<f64>,
        fit: Vec<P3>,
        closed: bool,
    },
    Insert {
        block: u64,
        at: P3,
        scale: P3,
        rotation: f64,
        normal: P3,
        /// Columns, rows, column spacing and row spacing of a MINSERT.
        array: Option<(usize, usize, f64, f64)>,
    },
    Text,
    Image,
}

/// A block definition.
#[derive(Debug, Clone, Default)]
pub(super) struct Block {
    pub(super) base: P3,
    pub(super) entities: Vec<Entity>,
}

/// What this importer takes from a drawing.
#[derive(Debug, Clone, Default)]
pub struct Drawing {
    /// `$INSUNITS`.
    pub(super) insunits: i16,
    pub(super) model: Vec<Entity>,
    /// Block definitions by the handle of their block header.
    pub(super) blocks: HashMap<u64, Block>,
}

impl Drawing {
    /// The `$INSUNITS` code of the drawing.
    pub fn insunits(&self) -> i16 {
        self.insunits
    }
}

/// Where an entity belongs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Owner {
    Model,
    Paper,
    Block(u64),
}

/// An entity as read, before its polyline vertices are gathered.
enum Shape {
    Done(Kind),
    Polyline2d {
        closed: bool,
        normal: P3,
        vertices: Vec<u64>,
    },
    Polyline3d {
        closed: bool,
        vertices: Vec<u64>,
    },
}

/// Everything read from one object.
enum Read {
    Entity {
        owner: Owner,
        layer: u64,
        shape: Shape,
    },
    Vertex {
        owner: Owner,
        point: P3,
        bulge: f64,
        /// Spline frame control points are not on the curve.
        frame: bool,
    },
    Layer(String),
    BlockHeader {
        name: String,
        base: P3,
        entities: Vec<u64>,
    },
}

/// Object type numbers.
mod ty {
    pub const TEXT: u16 = 1;
    pub const INSERT: u16 = 7;
    pub const MINSERT: u16 = 8;
    pub const VERTEX_2D: u16 = 0x0A;
    pub const VERTEX_3D: u16 = 0x0B;
    pub const POLYLINE_2D: u16 = 0x0F;
    pub const POLYLINE_3D: u16 = 0x10;
    pub const ARC: u16 = 0x11;
    pub const CIRCLE: u16 = 0x12;
    pub const LINE: u16 = 0x13;
    pub const ELLIPSE: u16 = 0x23;
    pub const SPLINE: u16 = 0x24;
    pub const MTEXT: u16 = 0x2C;
    pub const BLOCK_HEADER: u16 = 0x31;
    pub const LAYER: u16 = 0x33;
    pub const LWPOLYLINE: u16 = 0x4D;
}

/// What a type number stands for, taking class numbers into account.
#[derive(Clone, Copy, PartialEq, Eq)]
enum What {
    Fixed(u16),
    LwPolyline,
    Image,
    Other,
}

fn what(kind: u16, classes: &HashMap<u16, String>) -> What {
    if kind < 500 {
        return What::Fixed(kind);
    }
    match classes.get(&kind).map(String::as_str) {
        Some("LWPOLYLINE") => What::LwPolyline,
        Some("IMAGE") => What::Image,
        _ => What::Other,
    }
}

/// Reads the drawing out of its sections.
pub(super) fn drawing(sections: &Sections) -> Result<Drawing, Bad> {
    let map = super::header::object_map(sections)?;
    let classes = super::header::classes(sections);
    let mut read: HashMap<u64, Read> = HashMap::with_capacity(map.len());
    let mut order: Vec<u64> = Vec::with_capacity(map.len());
    for &(handle, offset) in &map {
        // One damaged object is left out; the rest of the drawing stands.
        if let Ok(Some(object)) = object(sections, &classes, handle, offset) {
            read.insert(handle, object);
            order.push(handle);
        }
    }
    Ok(assemble(sections, read, &order))
}

/// Reads the object at `offset` of the objects section.
fn object(
    sections: &Sections,
    classes: &HashMap<u16, String>,
    handle: u64,
    offset: usize,
) -> Result<Option<Read>, Bad> {
    let v = sections.version;
    let objects = &sections.objects;
    let at = sections.objects_base.saturating_add(offset);
    let mut head = Bits::slice(objects, at, 16, v);
    let size = usize::try_from(head.ms()?).map_err(|_| Bad("an object too large"))?;
    let handle_bits = if v >= Version::R2010 {
        usize::try_from(head.umc()?).map_err(|_| Bad("an object too large"))?
    } else {
        0
    };
    let start = at + head.position() / 8;
    let data = objects
        .get(start..start.saturating_add(size))
        .ok_or(Bad("an object runs past the end of its section"))?;
    let mut dat = Bits::new(data, v);
    let kind = what(dat.ot()?, classes);
    let wanted = matches!(
        kind,
        What::LwPolyline
            | What::Image
            | What::Fixed(
                ty::TEXT
                    | ty::INSERT
                    | ty::MINSERT
                    | ty::VERTEX_2D
                    | ty::VERTEX_3D
                    | ty::POLYLINE_2D
                    | ty::POLYLINE_3D
                    | ty::ARC
                    | ty::CIRCLE
                    | ty::LINE
                    | ty::ELLIPSE
                    | ty::SPLINE
                    | ty::MTEXT
                    | ty::BLOCK_HEADER
                    | ty::LAYER
                    | ty::LWPOLYLINE
            )
    );
    if !wanted {
        return Ok(None);
    }
    // Where the plain data ends and the handles start.
    let mut data_end = if (Version::R2000..Version::R2010).contains(&v) {
        Some(dat.rl()? as usize)
    } else if v >= Version::R2010 {
        Some(
            (size * 8)
                .checked_sub(handle_bits)
                .ok_or(Bad("an object's handles are larger than the object"))?,
        )
    } else {
        None
    };
    let own = dat.h()?.value;
    if own != handle {
        return Err(Bad("an object that is not where the map says"));
    }
    skip_eed(&mut dat)?;
    let entity = !matches!(
        kind,
        What::Fixed(ty::BLOCK_HEADER | ty::LAYER) | What::Other
    );
    if entity && dat.b()? {
        let bytes = if v >= Version::R2010 {
            usize::try_from(dat.bll()?).map_err(|_| Bad("a graphic too large"))?
        } else {
            dat.rl()? as usize
        };
        dat.skip(bytes.saturating_mul(8))?;
    }
    if v < Version::R2000 {
        data_end = Some(dat.rl()? as usize);
    }
    let data_end = data_end.unwrap_or(0);
    let mut hdl = dat.clone();
    hdl.seek(data_end)?;
    let strs = if v >= Version::R2007 {
        match string_stream(&dat, data_end)? {
            Some(s) => Strings::Stream(s),
            None => Strings::Skip,
        }
    } else {
        Strings::Inline
    };
    let mut s = Streams {
        dat,
        strs,
        hdl: Handles::Stream(hdl),
    };
    let own = handle;
    if entity {
        entity_object(&mut s, kind, own).map(Some)
    } else {
        table_object(&mut s, kind, own).map(Some)
    }
}

fn skip_eed(dat: &mut Bits) -> Result<(), Bad> {
    loop {
        let size = dat.bs()? as u16 as usize;
        if size == 0 {
            return Ok(());
        }
        dat.h()?;
        dat.skip(size.saturating_mul(8))?;
    }
}

/// The start of an entity's data that every entity shares.
struct Common {
    entmode: u8,
    reactors: usize,
    xdic: bool,
    ltype_by_handle: bool,
    links: bool,
    colour_book: bool,
    plotstyle: bool,
    material: bool,
    visual_styles: usize,
}

fn common_entity(s: &mut Streams) -> Result<Common, Bad> {
    let v = s.dat.version;
    let d = &mut s.dat;
    let entmode = d.bb()?;
    let reactors = d.count(8)?;
    let xdic = if v >= Version::R2004 { !d.b()? } else { true };
    if v >= Version::R2013 {
        d.b()?;
    }
    let mut ltype_by_handle = false;
    if v < Version::R2000 {
        ltype_by_handle = !d.b()?;
    }
    // From R2004 the links are never stored and neither is their bit.
    let links = if v <= Version::R2000 { !d.b()? } else { false };
    let mut colour_book = false;
    if v >= Version::R2004 {
        let flags = d.bs()? as u16;
        if flags & 0x8000 != 0 {
            d.bl()?;
        }
        if flags & 0x2000 != 0 {
            d.bl()?;
        }
        colour_book = flags & 0x4000 != 0;
    } else {
        d.bs()?;
    }
    d.bd()?;
    let mut plotstyle = false;
    let mut material = false;
    let mut visual_styles = 0;
    if v >= Version::R2000 {
        ltype_by_handle = d.bb()? == 3;
        plotstyle = d.bb()? == 3;
    }
    if v >= Version::R2007 {
        material = d.bb()? == 3;
        d.rc()?;
    }
    if v >= Version::R2010 {
        for _ in 0..3 {
            visual_styles += usize::from(d.b()?);
        }
    }
    d.bs()?;
    if v >= Version::R2000 {
        d.rc()?;
    }
    Ok(Common {
        entmode,
        reactors,
        xdic,
        ltype_by_handle,
        links,
        colour_book,
        plotstyle,
        material,
        visual_styles,
    })
}

/// The handles every entity starts its handle data with: answers where it
/// belongs and its layer.
fn common_handles(s: &mut Streams, c: &Common, own: u64) -> Result<(Owner, u64), Bad> {
    let v = s.dat.version;
    let owner = match c.entmode {
        0 => Owner::Block(s.h()?.resolve(own)),
        1 => Owner::Paper,
        _ => Owner::Model,
    };
    for _ in 0..c.reactors {
        s.h()?;
    }
    if c.xdic {
        s.h()?;
    }
    let mut layer = 0;
    if v < Version::R2000 {
        layer = s.h()?.resolve(own);
        if c.ltype_by_handle {
            s.h()?;
        }
    }
    if v <= Version::R2000 && c.links {
        s.h()?;
        s.h()?;
    }
    if v >= Version::R2004 && c.colour_book {
        s.h()?;
    }
    if v >= Version::R2000 {
        layer = s.h()?.resolve(own);
        if c.ltype_by_handle {
            s.h()?;
        }
        if v >= Version::R2007 && c.material {
            s.h()?;
        }
        if c.plotstyle {
            s.h()?;
        }
        for _ in 0..c.visual_styles {
            s.h()?;
        }
    }
    Ok((owner, layer))
}

fn entity_object(s: &mut Streams, kind: What, own: u64) -> Result<Read, Bad> {
    let v = s.dat.version;
    let c = common_entity(s)?;
    let d = &mut s.dat;
    enum Pending {
        Shape(Shape),
        Vertex {
            point: P3,
            bulge: f64,
            frame: bool,
        },
        Insert(Kind),
        Polyline2d {
            closed: bool,
            normal: P3,
            count: usize,
        },
        Polyline3d {
            closed: bool,
            count: usize,
        },
    }
    let pending = match kind {
        What::Fixed(ty::LINE) => {
            let (start, end) = if v < Version::R2000 {
                (d.bd3()?, d.bd3()?)
            } else {
                let flat = d.b()?;
                let x1 = d.rd()?;
                let x2 = d.dd(x1)?;
                let y1 = d.rd()?;
                let y2 = d.dd(y1)?;
                let (z1, z2) = if flat {
                    (0.0, 0.0)
                } else {
                    let z1 = d.rd()?;
                    (z1, d.dd(z1)?)
                };
                ([x1, y1, z1], [x2, y2, z2])
            };
            d.bt()?;
            d.be()?;
            Pending::Shape(Shape::Done(Kind::Line { start, end }))
        }
        What::Fixed(ty::CIRCLE) => {
            let centre = d.bd3()?;
            let radius = d.bd()?;
            d.bt()?;
            let normal = d.be()?;
            Pending::Shape(Shape::Done(Kind::Circle {
                centre,
                radius,
                normal,
            }))
        }
        What::Fixed(ty::ARC) => {
            let centre = d.bd3()?;
            let radius = d.bd()?;
            d.bt()?;
            let normal = d.be()?;
            let start = d.bd()?;
            let end = d.bd()?;
            Pending::Shape(Shape::Done(Kind::Arc {
                centre,
                radius,
                start,
                end,
                normal,
            }))
        }
        What::Fixed(ty::ELLIPSE) => {
            let centre = d.bd3()?;
            let major = d.bd3()?;
            let normal = d.bd3()?;
            let ratio = d.bd()?;
            let start = d.bd()?;
            let end = d.bd()?;
            Pending::Shape(Shape::Done(Kind::Ellipse {
                centre,
                major,
                normal,
                ratio,
                start,
                end,
            }))
        }
        What::Fixed(ty::SPLINE) => Pending::Shape(Shape::Done(spline(d)?)),
        What::Fixed(ty::LWPOLYLINE) | What::LwPolyline => {
            Pending::Shape(Shape::Done(lwpolyline(d)?))
        }
        What::Fixed(ty::POLYLINE_2D) => {
            let flags = d.bs()?;
            d.bs()?;
            d.bd()?;
            d.bd()?;
            d.bt()?;
            d.bd()?;
            let normal = d.be()?;
            let count = if v >= Version::R2004 { d.count(8)? } else { 0 };
            Pending::Polyline2d {
                closed: flags & 1 != 0,
                normal,
                count,
            }
        }
        What::Fixed(ty::POLYLINE_3D) => {
            d.rc()?;
            let flags = d.rc()?;
            let count = if v >= Version::R2004 { d.count(8)? } else { 0 };
            Pending::Polyline3d {
                closed: flags & 1 != 0,
                count,
            }
        }
        What::Fixed(ty::VERTEX_2D) => {
            let flags = d.rc()?;
            let point = d.bd3()?;
            let start_width = d.bd()?;
            if start_width >= 0.0 {
                d.bd()?;
            }
            let bulge = d.bd()?;
            Pending::Vertex {
                point,
                bulge,
                frame: flags & 16 != 0,
            }
        }
        What::Fixed(ty::VERTEX_3D) => {
            let flags = d.rc()?;
            let point = d.bd3()?;
            Pending::Vertex {
                point,
                bulge: 0.0,
                frame: flags & 16 != 0,
            }
        }
        What::Fixed(kind @ (ty::INSERT | ty::MINSERT)) => {
            let at = d.bd3()?;
            let scale = if v < Version::R2000 {
                d.bd3()?
            } else {
                match d.bb()? {
                    3 => [1.0, 1.0, 1.0],
                    1 => [1.0, d.dd(1.0)?, d.dd(1.0)?],
                    2 => {
                        let x = d.rd()?;
                        [x, x, x]
                    }
                    _ => {
                        let x = d.rd()?;
                        [x, d.dd(x)?, d.dd(x)?]
                    }
                }
            };
            let rotation = d.bd()?;
            let normal = d.bd3()?;
            // The count of attributes owned is there only when there are any.
            if d.b()? && v >= Version::R2004 {
                d.bl()?;
            }
            let array = if kind == ty::MINSERT {
                let columns = d.bs()?.max(1) as usize;
                let rows = d.bs()?.max(1) as usize;
                Some((columns, rows, d.bd()?, d.bd()?))
            } else {
                None
            };
            Pending::Insert(Kind::Insert {
                block: 0,
                at,
                scale,
                rotation,
                normal,
                array,
            })
        }
        What::Fixed(ty::TEXT | ty::MTEXT) => Pending::Shape(Shape::Done(Kind::Text)),
        What::Image => Pending::Shape(Shape::Done(Kind::Image)),
        _ => return Err(Bad("not an entity this reader draws")),
    };
    let (owner, layer) = common_handles(s, &c, own)?;
    Ok(match pending {
        Pending::Shape(shape) => Read::Entity {
            owner,
            layer,
            shape,
        },
        Pending::Vertex {
            point,
            bulge,
            frame,
        } => Read::Vertex {
            owner,
            point,
            bulge,
            frame,
        },
        Pending::Insert(Kind::Insert {
            at,
            scale,
            rotation,
            normal,
            array,
            ..
        }) => {
            let block = s.h()?.resolve(own);
            Read::Entity {
                owner,
                layer,
                shape: Shape::Done(Kind::Insert {
                    block,
                    at,
                    scale,
                    rotation,
                    normal,
                    array,
                }),
            }
        }
        Pending::Insert(_) => return Err(Bad("an insert that is not one")),
        Pending::Polyline2d {
            closed,
            normal,
            count,
        } => Read::Entity {
            owner,
            layer,
            shape: Shape::Polyline2d {
                closed,
                normal,
                vertices: vertex_handles(s, own, count)?,
            },
        },
        Pending::Polyline3d { closed, count } => Read::Entity {
            owner,
            layer,
            shape: Shape::Polyline3d {
                closed,
                vertices: vertex_handles(s, own, count)?,
            },
        },
    })
}

/// The vertex handles of a polyline: listed from R2004, before that the
/// first and last, with the vertices between them found by their owner.
fn vertex_handles(s: &mut Streams, own: u64, count: usize) -> Result<Vec<u64>, Bad> {
    if s.dat.version >= Version::R2004 {
        (0..count).map(|_| Ok(s.h()?.resolve(own))).collect()
    } else {
        Ok(Vec::new())
    }
}

fn spline(d: &mut Bits) -> Result<Kind, Bad> {
    let v = d.version;
    let mut scenario = d.bl()?;
    let mut closed = false;
    if v >= Version::R2013 {
        let flags = d.bl()?;
        let knot_parameter = d.bl()?;
        closed = flags & 4 != 0;
        // The stored scenario is kept; only a value outside 1 and 2 is
        // worked out from the flags.
        if scenario != 1 && scenario != 2 {
            scenario = if knot_parameter == 15 || flags & 1 == 0 {
                1
            } else {
                2
            };
        }
    }
    // Degrees past this are not drawn as curves (and would only be slow).
    let degree = usize::try_from(d.bl()?).unwrap_or(0).min(64);
    let mut knots = Vec::new();
    let mut control = Vec::new();
    let mut weights = Vec::new();
    let mut fit = Vec::new();
    if scenario == 2 {
        d.bd()?;
        d.bd3()?;
        d.bd3()?;
        let count = d.count(6)?;
        for _ in 0..count {
            fit.push(d.bd3()?);
        }
    } else {
        let rational = d.b()?;
        closed = d.b()?;
        let periodic = d.b()?;
        closed |= periodic;
        d.bd()?;
        d.bd()?;
        let knot_count = d.count(2)?;
        let control_count = d.count(6)?;
        let weighted = d.b()? || rational;
        for _ in 0..knot_count {
            knots.push(d.bd()?);
        }
        for _ in 0..control_count {
            control.push(d.bd3()?);
            if weighted {
                weights.push(d.bd()?);
            }
        }
        if !rational && weights.iter().all(|w| (*w - 1.0).abs() < 1e-12) {
            weights.clear();
        }
    }
    Ok(Kind::Spline {
        degree,
        knots,
        control,
        weights,
        fit,
        closed,
    })
}

fn lwpolyline(d: &mut Bits) -> Result<Kind, Bad> {
    let v = d.version;
    let flags = d.bs()? as u16;
    if flags & 4 != 0 {
        d.bd()?;
    }
    if flags & 8 != 0 {
        d.bd()?;
    }
    if flags & 2 != 0 {
        d.bd()?;
    }
    let normal = if flags & 1 != 0 {
        d.bd3()?
    } else {
        [0.0, 0.0, 1.0]
    };
    let count = d.count(4)?;
    let bulges = if flags & 16 != 0 { d.count(2)? } else { 0 };
    let ids = if v >= Version::R2010 && flags & 1024 != 0 {
        d.count(2)?
    } else {
        0
    };
    let widths = if flags & 32 != 0 { d.count(4)? } else { 0 };
    let mut points: Vec<(f64, f64, f64)> = Vec::with_capacity(count);
    for i in 0..count {
        if v < Version::R2000 || i == 0 {
            let [x, y] = d.rd2()?;
            points.push((x, y, 0.0));
        } else {
            let (px, py, _) = points[i - 1];
            let x = d.dd(px)?;
            let y = d.dd(py)?;
            points.push((x, y, 0.0));
        }
    }
    for i in 0..bulges {
        let bulge = d.bd()?;
        if let Some(point) = points.get_mut(i) {
            point.2 = bulge;
        }
    }
    for _ in 0..ids {
        d.bl()?;
    }
    for _ in 0..widths {
        d.bd()?;
        d.bd()?;
    }
    Ok(Kind::Polyline {
        points,
        closed: flags & 512 != 0,
        normal,
    })
}

/// Layers and block headers.
fn table_object(s: &mut Streams, kind: What, own: u64) -> Result<Read, Bad> {
    let v = s.dat.version;
    let reactors = s.dat.count(8)?;
    let xdic = if v >= Version::R2004 {
        !s.dat.b()?
    } else {
        true
    };
    if v >= Version::R2013 {
        s.dat.b()?;
    }
    let name = s.tv()?;
    if kind == What::Fixed(ty::LAYER) {
        return Ok(Read::Layer(name));
    }
    let d = &mut s.dat;
    if v >= Version::R2007 {
        // The 64 flag, the xref index and the xref dependence are one
        // short of flags from R2007.
        d.bs()?;
    } else {
        d.b()?;
        d.bs()?;
        d.b()?;
    }
    d.b()?;
    d.b()?;
    let xref = d.b()?;
    let overlaid = d.b()?;
    if v >= Version::R2000 {
        d.b()?;
    }
    let owned = if v >= Version::R2004 { d.count(8)? } else { 0 };
    let base = d.bd3()?;
    s.tv()?;
    // The handles hold the entities of R2004 and later blocks.
    s.h()?;
    for _ in 0..reactors {
        s.h()?;
    }
    if xdic {
        s.h()?;
    }
    s.h()?;
    s.h()?;
    let mut entities = Vec::new();
    if v >= Version::R2004 {
        for _ in 0..owned {
            entities.push(s.h()?.resolve(own));
        }
    } else if !xref && !overlaid {
        s.h()?;
        s.h()?;
    }
    Ok(Read::BlockHeader {
        name,
        base,
        entities,
    })
}

/// Puts the objects read together into the drawing.
fn assemble(sections: &Sections, mut read: HashMap<u64, Read>, order: &[u64]) -> Drawing {
    let v = sections.version;
    let layers: HashMap<u64, String> = read
        .iter()
        .filter_map(|(h, r)| match r {
            Read::Layer(name) => Some((*h, name.clone())),
            _ => None,
        })
        .collect();
    let layer_of = |h: u64| layers.get(&h).cloned().unwrap_or_else(|| "0".to_string());

    // Vertices by their polyline, for releases that do not list them.
    let mut vertices_of: HashMap<u64, Vec<u64>> = HashMap::new();
    for h in order {
        if let Some(Read::Vertex {
            owner: Owner::Block(parent),
            ..
        }) = read.get(h)
        {
            vertices_of.entry(*parent).or_default().push(*h);
        }
    }

    let mut model_header = None;
    let mut headers: Vec<(u64, P3, Vec<u64>)> = Vec::new();
    for h in order {
        if let Some(Read::BlockHeader {
            name,
            base,
            entities,
        }) = read.get(h)
        {
            if name.eq_ignore_ascii_case("*Model_Space") {
                model_header = Some(*h);
            }
            headers.push((*h, *base, entities.clone()));
        }
    }

    // Entities by where they belong, in handle order.
    let mut by_owner: HashMap<u64, Vec<u64>> = HashMap::new();
    let mut model_list: Vec<u64> = Vec::new();
    for h in order {
        if let Some(Read::Entity { owner, .. }) = read.get(h) {
            match owner {
                Owner::Model => model_list.push(*h),
                Owner::Block(b) if Some(*b) == model_header => model_list.push(*h),
                Owner::Block(b) => by_owner.entry(*b).or_default().push(*h),
                Owner::Paper => {}
            }
        }
    }

    // Finished entities by handle.
    let mut done: HashMap<u64, Entity> = HashMap::new();
    let entity_handles: Vec<u64> = order
        .iter()
        .copied()
        .filter(|h| matches!(read.get(h), Some(Read::Entity { .. })))
        .collect();
    for h in entity_handles {
        let Some(Read::Entity { layer, shape, .. }) = read.remove(&h) else {
            continue;
        };
        let kind = match shape {
            Shape::Done(kind) => kind,
            Shape::Polyline2d {
                closed,
                normal,
                vertices,
            } => {
                let list = if v >= Version::R2004 {
                    vertices
                } else {
                    vertices_of.get(&h).cloned().unwrap_or_default()
                };
                let points = list
                    .iter()
                    .filter_map(|vh| match read.get(vh) {
                        Some(Read::Vertex {
                            point,
                            bulge,
                            frame: false,
                            ..
                        }) => Some((point[0], point[1], *bulge)),
                        _ => None,
                    })
                    .collect();
                Kind::Polyline {
                    points,
                    closed,
                    normal,
                }
            }
            Shape::Polyline3d { closed, vertices } => {
                let list = if v >= Version::R2004 {
                    vertices
                } else {
                    vertices_of.get(&h).cloned().unwrap_or_default()
                };
                let points = list
                    .iter()
                    .filter_map(|vh| match read.get(vh) {
                        Some(Read::Vertex {
                            point,
                            frame: false,
                            ..
                        }) => Some((point[0], point[1], 0.0)),
                        _ => None,
                    })
                    .collect();
                Kind::Polyline {
                    points,
                    closed,
                    normal: [0.0, 0.0, 1.0],
                }
            }
        };
        done.insert(
            h,
            Entity {
                layer: layer_of(layer),
                kind,
            },
        );
    }

    // A block's entities in the order its header lists them, else in
    // handle order. Each entity belongs to one block and is taken out as it
    // is placed, so a damaged file that lists one entity in many blocks
    // cannot multiply it.
    let mut listed = |list: &[u64], fallback: &[u64]| -> Vec<Entity> {
        let source = if list.is_empty() { fallback } else { list };
        source.iter().filter_map(|h| done.remove(h)).collect()
    };

    let model_listed = headers
        .iter()
        .find(|(h, _, _)| Some(*h) == model_header)
        .map_or(&[][..], |(_, _, list)| list.as_slice());
    let model = listed(model_listed, &model_list);
    let mut blocks = HashMap::new();
    for (h, base, list) in &headers {
        if Some(*h) != model_header {
            let fallback = by_owner.get(h).map(Vec::as_slice).unwrap_or(&[]);
            blocks.insert(
                *h,
                Block {
                    base: *base,
                    entities: listed(list, fallback),
                },
            );
        }
    }
    Drawing {
        insunits: super::header::insunits(sections),
        model,
        blocks,
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;

    #[test]
    fn an_entity_listed_by_many_blocks_is_taken_once() {
        // 2,000 block headers of a few bytes each listing the same large
        // polyline cloned it 2,000 times while the drawing was assembled.
        let points: Vec<(f64, f64, f64)> = (0..1001).map(|i| (f64::from(i), 0.0, 0.0)).collect();
        let mut read = HashMap::new();
        read.insert(
            1,
            Read::Entity {
                owner: Owner::Block(100),
                layer: 0,
                shape: Shape::Done(Kind::Polyline {
                    points,
                    closed: false,
                    normal: [0.0, 0.0, 1.0],
                }),
            },
        );
        let mut order = vec![1];
        for h in 100..2100 {
            read.insert(
                h,
                Read::BlockHeader {
                    name: format!("B{h}"),
                    base: [0.0; 3],
                    entities: vec![1],
                },
            );
            order.push(h);
        }
        let sections = Sections {
            version: Version::R2004,
            maintenance: 0,
            header: Vec::new(),
            classes: Vec::new(),
            handles: Vec::new(),
            objects: Vec::new(),
            objects_base: 0,
        };
        let drawing = assemble(&sections, read, &order);
        let held: usize = drawing.blocks.values().map(|b| b.entities.len()).sum();
        assert_eq!(held, 1);
        assert_eq!(drawing.blocks[&100].entities.len(), 1);
    }
}
