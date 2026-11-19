//! The binary records of an Altium library or board that a footprint is drawn with: pads,
//! tracks, arcs, fills and regions, and the properties of a board's components.
//!
//! A record starts with a byte that says its type, then one or more sub-records, each a 32-bit
//! length and that many bytes. A sub-record is read from the fields this reader knows, at their
//! documented offsets, and the rest of it is skipped by its length, so a longer sub-record of a
//! newer version reads the same. Lengths are in Altium's unit, 1/10000 mil (2.54 nm), and a
//! point is in its frame, y up.

use std::collections::BTreeMap;

/// One Altium unit (1/10000 mil) in mm.
pub(crate) const UNIT_MM: f64 = 0.000_002_54;

pub(crate) const ARC: u8 = 1;
pub(crate) const PAD: u8 = 2;
pub(crate) const VIA: u8 = 3;
pub(crate) const TRACK: u8 = 4;
pub(crate) const TEXT: u8 = 5;
pub(crate) const FILL: u8 = 6;
pub(crate) const REGION: u8 = 11;
pub(crate) const BODY: u8 = 12;

/// A component index that names no component: a primitive of the board itself.
pub(crate) const NO_COMPONENT: u16 = 0xFFFF;

/// Bytes read in order, every read checked against the end.
pub(crate) struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
    /// Where the current sub-record ends.
    end: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Cursor {
            bytes,
            at: 0,
            end: bytes.len(),
        }
    }

    pub(crate) fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.end);
        let Some(end) = end else {
            return Err(format!(
                "a record ends at byte {} before its fields do",
                self.end
            ));
        };
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }

    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_le_bytes(
            self.take(8)?.try_into().expect("eight bytes"),
        ))
    }

    /// A length in mm.
    fn length(&mut self) -> Result<f64, String> {
        Ok(f64::from(self.i32()?) * UNIT_MM)
    }

    fn point(&mut self) -> Result<Point, String> {
        Ok(Point {
            x: self.length()?,
            y: self.length()?,
        })
    }

    fn skip(&mut self, n: usize) -> Result<(), String> {
        self.take(n).map(|_| ())
    }

    /// Starts a sub-record: reads its length, and bounds the reads that follow to it.
    pub(crate) fn begin(&mut self) -> Result<usize, String> {
        self.end = self.bytes.len();
        let len = self.u32()? as usize;
        let end = self.at.checked_add(len).filter(|&e| e <= self.bytes.len());
        let Some(end) = end else {
            return Err(format!(
                "a record of {len} bytes at byte {} runs past the end of its stream",
                self.at
            ));
        };
        self.end = end;
        Ok(len)
    }

    /// Ends a sub-record: skips what of it was not read.
    pub(crate) fn finish(&mut self) {
        self.at = self.end;
        self.end = self.bytes.len();
    }

    /// A string of a length byte and that many bytes (Altium's names are in a single-byte
    /// code page; a byte past ASCII is read as Latin-1).
    pub(crate) fn short_string(&mut self) -> Result<String, String> {
        let n = self.u8()? as usize;
        Ok(self.take(n)?.iter().map(|&b| char::from(b)).collect())
    }

    /// A properties block: a 32-bit length and `|KEY=VALUE|KEY=VALUE` text, the keys upper case.
    pub(crate) fn properties(&mut self) -> Result<BTreeMap<String, String>, String> {
        let n = self.u32()? as usize;
        let raw = self.take(n)?;
        Ok(properties(raw))
    }
}

/// The `|KEY=VALUE|...` text of a properties block as a map (a key is upper case; the text ends
/// at a zero byte).
pub(crate) fn properties(raw: &[u8]) -> BTreeMap<String, String> {
    let raw = raw.split(|&b| b == 0).next().unwrap_or(&[]);
    let text: String = raw.iter().map(|&b| char::from(b)).collect();
    text.split('|')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.trim().to_ascii_uppercase(), v.to_string()))
        .collect()
}

/// A point in mm, in Altium's frame (y up).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Point {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

/// Altium's pad shapes (a round rectangle is a rectangle with a corner radius in the pad's
/// size-and-shape sub-record).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PadShape {
    Round,
    Rect,
    Octagon,
    RoundRect { percent: u8 },
}

/// The shape of a pad's hole.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Hole {
    Round,
    Square,
    /// A slot `length` long, turned by `angle` degrees from the pad's own x.
    Slot {
        length: f64,
        angle: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pad {
    pub(crate) name: String,
    pub(crate) layer: u8,
    pub(crate) component: u16,
    pub(crate) at: Point,
    pub(crate) size: Point,
    pub(crate) shape: PadShape,
    /// The hole's diameter (its width for a slot), 0 for a surface pad.
    pub(crate) hole: f64,
    pub(crate) hole_shape: Hole,
    pub(crate) angle: f64,
    pub(crate) plated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Track {
    pub(crate) layer: u8,
    pub(crate) component: u16,
    pub(crate) start: Point,
    pub(crate) end: Point,
    pub(crate) width: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Arc {
    pub(crate) layer: u8,
    pub(crate) component: u16,
    pub(crate) center: Point,
    pub(crate) radius: f64,
    /// Counter-clockwise from `start` to `end`, degrees.
    pub(crate) start: f64,
    pub(crate) end: f64,
    pub(crate) width: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fill {
    pub(crate) layer: u8,
    pub(crate) component: u16,
    pub(crate) corner1: Point,
    pub(crate) corner2: Point,
    pub(crate) angle: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Region {
    pub(crate) layer: u8,
    pub(crate) component: u16,
    /// `KIND`: 0 copper, 1 a cutout of a pour, 2 a board outline, 4 a cavity, 5 a board cutout.
    pub(crate) kind: u32,
    /// `V7_LAYER`, which names a mechanical layer past the 16 of the layer byte.
    pub(crate) layer_name: Option<String>,
    pub(crate) outline: Vec<Point>,
    /// Whether an arc of the outline was drawn as its end points.
    pub(crate) had_arcs: bool,
}

/// A primitive of a library footprint or a board.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Primitive {
    Pad(Pad),
    Track(Track),
    Arc(Arc),
    Fill(Fill),
    Region(Region),
    /// A record that draws nothing of the land pattern (a text, a via, a 3D body): its type and
    /// its component.
    Other(u8, u16),
}

/// The records of a stream: a library footprint's `Data` after its name, or one of a board's
/// primitive streams (`Pads6/Data`, `Tracks6/Data`, ...). `shape_based` reads a region's
/// vertices as a `ShapeBasedRegions6` stream holds them.
pub(crate) fn primitives(
    cursor: &mut Cursor<'_>,
    shape_based: bool,
) -> Result<Vec<Primitive>, String> {
    let mut out = Vec::new();
    while !cursor.done() {
        let kind = cursor.u8()?;
        let p = match kind {
            PAD => Primitive::Pad(pad(cursor)?),
            TRACK => Primitive::Track(track(cursor)?),
            ARC => Primitive::Arc(arc(cursor)?),
            FILL => Primitive::Fill(fill(cursor)?),
            REGION => Primitive::Region(region(cursor, shape_based)?),
            VIA => {
                cursor.begin()?;
                cursor.finish();
                Primitive::Other(kind, NO_COMPONENT)
            }
            BODY | TEXT => {
                cursor.begin()?;
                let (_, component) = common(cursor)?;
                cursor.finish();
                if kind == TEXT {
                    // The string.
                    cursor.begin()?;
                    cursor.finish();
                }
                Primitive::Other(kind, component)
            }
            other => {
                return Err(format!(
                    "a record of type {other}, which netsmith does not read, at byte {}",
                    cursor.at - 1
                ));
            }
        };
        out.push(p);
    }
    Ok(out)
}

/// The layer byte, the flags, the net and the component that a primitive starts with.
fn common(c: &mut Cursor<'_>) -> Result<(u8, u16), String> {
    let layer = c.u8()?;
    c.skip(2)?; // flags
    c.skip(2)?; // net
    c.skip(2)?; // polygon (a pad: unknown)
    let component = c.u16()?;
    c.skip(4)?;
    Ok((layer, component))
}

fn track(c: &mut Cursor<'_>) -> Result<Track, String> {
    c.begin()?;
    let (layer, component) = common(c)?;
    let start = c.point()?;
    let end = c.point()?;
    let width = c.length()?;
    c.finish();
    Ok(Track {
        layer,
        component,
        start,
        end,
        width,
    })
}

fn arc(c: &mut Cursor<'_>) -> Result<Arc, String> {
    c.begin()?;
    let (layer, component) = common(c)?;
    let center = c.point()?;
    let radius = c.length()?;
    let start = c.f64()?;
    let end = c.f64()?;
    let width = c.length()?;
    c.finish();
    Ok(Arc {
        layer,
        component,
        center,
        radius,
        start,
        end,
        width,
    })
}

fn fill(c: &mut Cursor<'_>) -> Result<Fill, String> {
    c.begin()?;
    let (layer, component) = common(c)?;
    let corner1 = c.point()?;
    let corner2 = c.point()?;
    let angle = c.f64()?;
    c.finish();
    Ok(Fill {
        layer,
        component,
        corner1,
        corner2,
        angle,
    })
}

/// The length of a pad's main sub-record up to the hole's angle, the last field read.
pub(crate) const PAD_MAIN_LEN: usize = 114;
/// The length of a pad's size-and-shape sub-record that holds the corner radii.
pub(crate) const PAD_SHAPE_LEN: usize = 596;

fn pad(c: &mut Cursor<'_>) -> Result<Pad, String> {
    c.begin()?;
    let name = c.short_string()?;
    c.finish();
    for _ in 0..3 {
        c.begin()?;
        c.finish();
    }
    let len = c.begin()?;
    if len < PAD_MAIN_LEN {
        return Err(format!(
            "pad `{name}`: its record is {len} bytes, shorter than the {PAD_MAIN_LEN} it needs"
        ));
    }
    let (layer, component) = common(c)?;
    let at = c.point()?;
    let size = c.point()?;
    c.skip(16)?; // the middle and bottom sizes: a simple pad is the same on every layer
    let hole = c.length()?;
    let top_shape = c.u8()?;
    c.skip(2)?; // the middle and bottom shapes
    let angle = c.f64()?;
    let plated = c.u8()? != 0;
    c.finish();
    let mut shape = match top_shape {
        1 => PadShape::Round,
        2 => PadShape::Rect,
        3 => PadShape::Octagon,
        other => {
            return Err(format!(
                "pad `{name}`: a shape {other} netsmith does not know"
            ));
        }
    };
    let mut hole_shape = Hole::Round;
    let len = c.begin()?;
    if len >= PAD_SHAPE_LEN {
        // 29 inner sizes in x, in y, 29 inner shapes, a byte, then the hole.
        c.skip(29 * 4 * 2 + 29 + 1)?;
        let kind = c.u8()?;
        let slot = c.length()?;
        let slot_angle = c.f64()?;
        c.skip(32 * 4 * 2 + 1)?; // the hole's offsets on the 32 layers
        let top_alt = c.u8()?;
        c.skip(31)?;
        let top_radius = c.u8()?;
        hole_shape = match kind {
            1 => Hole::Square,
            2 => Hole::Slot {
                length: slot,
                angle: slot_angle,
            },
            _ => Hole::Round,
        };
        if top_alt == 9 && shape == PadShape::Rect {
            shape = PadShape::RoundRect {
                percent: top_radius,
            };
        }
    }
    c.finish();
    Ok(Pad {
        name,
        layer,
        component,
        at,
        size,
        shape,
        hole,
        hole_shape,
        angle,
        plated,
    })
}

fn region(c: &mut Cursor<'_>, shape_based: bool) -> Result<Region, String> {
    c.begin()?;
    let layer = c.u8()?;
    c.skip(2 + 2 + 2)?; // flags, net, polygon
    let component = c.u16()?;
    c.skip(5)?;
    let holes = c.u16()?;
    c.skip(2)?;
    let props = c.properties()?;
    let mut count = c.u32()? as usize;
    let mut outline = Vec::new();
    let mut had_arcs = false;
    if shape_based {
        // A shape-based outline does not count its closing vertex.
        count += 1;
        for _ in 0..count {
            let round = c.u8()? != 0;
            let p = c.point()?;
            c.skip(4 * 3 + 16)?; // the arc's centre, radius and angles
            had_arcs |= round;
            outline.push(p);
        }
    } else {
        for _ in 0..count {
            let x = c.f64()? * UNIT_MM;
            let y = c.f64()? * UNIT_MM;
            outline.push(Point { x, y });
        }
    }
    // The holes of a region are not drawn on the layers a footprint is taken from.
    for _ in 0..holes {
        let n = c.u32()? as usize;
        c.skip(16 * n)?;
    }
    c.finish();
    if outline.len() > 1 && outline.first() == outline.last() {
        outline.pop();
    }
    Ok(Region {
        layer,
        component,
        kind: props.get("KIND").and_then(|k| k.parse().ok()).unwrap_or(0),
        layer_name: props.get("V7_LAYER").cloned(),
        outline,
        had_arcs,
    })
}

/// A board's component: the properties of one record of `Components6/Data`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Component {
    pub(crate) designator: String,
    pub(crate) pattern: String,
    pub(crate) at: Point,
    pub(crate) angle: f64,
    pub(crate) bottom: bool,
}

/// The components of `Components6/Data`, in order: a primitive names one by its index.
pub(crate) fn components(bytes: &[u8]) -> Result<Vec<Component>, String> {
    let mut c = Cursor::new(bytes);
    let mut out = Vec::new();
    while !c.done() {
        let props = c.properties()?;
        let get = |k: &str| props.get(k).map(String::as_str).unwrap_or("");
        out.push(Component {
            designator: get("SOURCEDESIGNATOR").to_string(),
            pattern: get("PATTERN").to_string(),
            at: Point {
                x: length_text(get("X"))?,
                y: length_text(get("Y"))?,
            },
            angle: get("ROTATION").trim().parse().unwrap_or(0.0),
            bottom: get("LAYER").eq_ignore_ascii_case("BOTTOM"),
        });
    }
    Ok(out)
}

/// `1234.5mil` (or `mm`) in mm; nothing is 0.
pub(crate) fn length_text(text: &str) -> Result<f64, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(0.0);
    }
    let (number, scale) = if let Some(n) = t.strip_suffix("mil") {
        (n, 0.0254)
    } else if let Some(n) = t.strip_suffix("mm") {
        (n, 1.0)
    } else {
        (t, 0.0254)
    };
    number
        .trim()
        .parse::<f64>()
        .map(|n| n * scale)
        .map_err(|_| format!("`{text}` is not a length"))
}
