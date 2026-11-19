//! Altium files written record by record, for tests (behind the `test-support` feature): a
//! compound file of storages and streams, and the records of pads, tracks, arcs, fills, regions
//! and texts, at the offsets [`super::records`] reads. Lengths are in mm, points in Altium's
//! frame (y up).

const SECTOR: usize = 512;
const MINI: usize = 64;
const CUTOFF: usize = 4096;
const END: u32 = 0xFFFF_FFFE;
const FREE: u32 = 0xFFFF_FFFF;
const FAT_SECTOR: u32 = 0xFFFF_FFFD;
const NONE: u32 = 0xFFFF_FFFF;

/// A compound file to write: storages and the streams in them, by path.
#[derive(Default)]
pub struct Compound {
    /// (path, bytes): a storage has no bytes.
    entries: Vec<(Vec<String>, Option<Vec<u8>>)>,
}

impl Compound {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the stream at `path` (`["Footprint", "Data"]`), and the storages above it.
    pub fn stream(mut self, path: &[&str], bytes: Vec<u8>) -> Self {
        for i in 1..path.len() {
            let storage: Vec<String> = path[..i].iter().map(|s| s.to_string()).collect();
            if !self.entries.iter().any(|(p, _)| *p == storage) {
                self.entries.push((storage, None));
            }
        }
        self.entries
            .push((path.iter().map(|s| s.to_string()).collect(), Some(bytes)));
        self
    }

    /// The file's bytes: version 3, 512-byte sectors, the streams under 4096 bytes in the mini
    /// stream as the format says.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Directory order: the root, then the entries as added.
        let n = self.entries.len() + 1;
        let parent_of = |path: &[String]| -> usize {
            if path.len() == 1 {
                return 0;
            }
            1 + self
                .entries
                .iter()
                .position(|(p, _)| p.as_slice() == &path[..path.len() - 1])
                .expect("storages are added before their streams")
        };
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, (path, _)) in self.entries.iter().enumerate() {
            children[parent_of(path)].push(i + 1);
        }
        let name_of = |id: usize| -> &str {
            if id == 0 {
                "Root Entry"
            } else {
                self.entries[id - 1].0.last().expect("a name")
            }
        };
        // Siblings in the format's order (shorter names first, then upper case), each the right
        // sibling of the one before: a tree with every node black, which readers accept.
        let mut child = vec![NONE; n];
        let mut right = vec![NONE; n];
        for (parent, list) in children.iter_mut().enumerate() {
            list.sort_by_key(|&id| {
                let name = name_of(id);
                (name.encode_utf16().count(), name.to_uppercase())
            });
            if let Some(&first) = list.first() {
                child[parent] = first as u32;
            }
            for pair in list.windows(2) {
                right[pair[0]] = pair[1] as u32;
            }
        }

        // The mini stream and the large streams.
        let mut mini_stream = Vec::new();
        let mut mini_fat: Vec<u32> = Vec::new();
        let mut starts = vec![END; n];
        let mut sizes = vec![0u64; n];
        let mut large: Vec<(usize, &[u8])> = Vec::new();
        for (i, (_, bytes)) in self.entries.iter().enumerate() {
            let Some(bytes) = bytes else { continue };
            let id = i + 1;
            sizes[id] = bytes.len() as u64;
            if bytes.is_empty() {
                continue;
            }
            if bytes.len() < CUTOFF {
                let first = mini_fat.len();
                let count = bytes.len().div_ceil(MINI);
                for k in 0..count {
                    mini_fat.push(if k + 1 == count {
                        END
                    } else {
                        (first + k + 1) as u32
                    });
                }
                starts[id] = first as u32;
                mini_stream.extend_from_slice(bytes);
                mini_stream.resize((first + count) * MINI, 0);
            } else {
                large.push((id, bytes));
            }
        }
        let dir_sectors = (n * 128).div_ceil(SECTOR);
        let mini_fat_sectors = (mini_fat.len() * 4).div_ceil(SECTOR);
        let mini_stream_sectors = mini_stream.len().div_ceil(SECTOR);
        let large_sectors: usize = large.iter().map(|(_, b)| b.len().div_ceil(SECTOR)).sum();
        let other = dir_sectors + mini_fat_sectors + mini_stream_sectors + large_sectors;
        let mut fat_sectors = 1;
        while fat_sectors * (SECTOR / 4) < other + fat_sectors {
            fat_sectors += 1;
        }
        assert!(fat_sectors <= 109, "a fixture this large needs a DIFAT");
        let total = fat_sectors + other;
        let mut fat = vec![FREE; fat_sectors * SECTOR / 4];
        for entry in fat.iter_mut().take(fat_sectors) {
            *entry = FAT_SECTOR;
        }
        let mut next = fat_sectors;
        let mut chain = |len: usize, fat: &mut Vec<u32>| -> u32 {
            let first = next;
            for k in 0..len {
                fat[first + k] = if k + 1 == len {
                    END
                } else {
                    (first + k + 1) as u32
                };
            }
            next += len;
            if len == 0 { END } else { first as u32 }
        };
        let first_dir = chain(dir_sectors, &mut fat);
        let first_mini_fat = chain(mini_fat_sectors, &mut fat);
        let root_start = chain(mini_stream_sectors, &mut fat);
        starts[0] = root_start;
        sizes[0] = mini_stream.len() as u64;
        for (id, bytes) in &large {
            starts[*id] = chain(bytes.len().div_ceil(SECTOR), &mut fat);
        }

        let mut out = vec![0u8; SECTOR * (total + 1)];
        let header = &mut out[..SECTOR];
        header[..8].copy_from_slice(&super::cfb::SIGNATURE);
        header[24..26].copy_from_slice(&0x3Eu16.to_le_bytes());
        header[26..28].copy_from_slice(&3u16.to_le_bytes());
        header[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
        header[30..32].copy_from_slice(&9u16.to_le_bytes());
        header[32..34].copy_from_slice(&6u16.to_le_bytes());
        header[44..48].copy_from_slice(&(fat_sectors as u32).to_le_bytes());
        header[48..52].copy_from_slice(&first_dir.to_le_bytes());
        header[56..60].copy_from_slice(&(CUTOFF as u32).to_le_bytes());
        header[60..64].copy_from_slice(&first_mini_fat.to_le_bytes());
        header[64..68].copy_from_slice(&(mini_fat_sectors as u32).to_le_bytes());
        header[68..72].copy_from_slice(&END.to_le_bytes());
        for i in 0..109 {
            let v = if i < fat_sectors { i as u32 } else { FREE };
            header[76 + 4 * i..80 + 4 * i].copy_from_slice(&v.to_le_bytes());
        }
        let mut write_at = |sector: usize, bytes: &[u8]| {
            let at = (sector + 1) * SECTOR;
            out[at..at + bytes.len()].copy_from_slice(bytes);
        };
        let fat_bytes: Vec<u8> = fat.iter().flat_map(|v| v.to_le_bytes()).collect();
        write_at(0, &fat_bytes);
        let mut dir = vec![0u8; dir_sectors * SECTOR];
        for id in 0..n {
            let e = &mut dir[id * 128..(id + 1) * 128];
            let name: Vec<u16> = name_of(id).encode_utf16().collect();
            for (k, u) in name.iter().enumerate() {
                e[2 * k..2 * k + 2].copy_from_slice(&u.to_le_bytes());
            }
            e[64..66].copy_from_slice(&(((name.len() + 1) * 2) as u16).to_le_bytes());
            e[66] = match id {
                0 => 5,
                _ if self.entries[id - 1].1.is_some() => 2,
                _ => 1,
            };
            e[67] = 1;
            e[68..72].copy_from_slice(&NONE.to_le_bytes());
            e[72..76].copy_from_slice(&right[id].to_le_bytes());
            e[76..80].copy_from_slice(&child[id].to_le_bytes());
            e[116..120].copy_from_slice(&starts[id].to_le_bytes());
            e[120..128].copy_from_slice(&sizes[id].to_le_bytes());
        }
        for id in n..dir.len() / 128 {
            let e = &mut dir[id * 128..(id + 1) * 128];
            for at in [68, 72, 76] {
                e[at..at + 4].copy_from_slice(&NONE.to_le_bytes());
            }
        }
        write_at(first_dir as usize, &dir);
        if mini_fat_sectors > 0 {
            let mut bytes: Vec<u8> = mini_fat.iter().flat_map(|v| v.to_le_bytes()).collect();
            bytes.resize(mini_fat_sectors * SECTOR, 0xFF);
            write_at(first_mini_fat as usize, &bytes);
        }
        if mini_stream_sectors > 0 {
            write_at(root_start as usize, &mini_stream);
        }
        for (id, bytes) in &large {
            write_at(starts[*id] as usize, bytes);
        }
        out
    }
}

/// Altium's layers, by the byte a record gives them.
pub mod layer {
    pub const TOP: u8 = 1;
    pub const BOTTOM: u8 = 32;
    pub const TOP_OVERLAY: u8 = 33;
    pub const KEEP_OUT: u8 = 56;
    pub const MULTI: u8 = 74;
    /// Mechanical layer `n` (1 to 16).
    pub const fn mechanical(n: u8) -> u8 {
        56 + n
    }
}

/// A length in mm as Altium's unit.
fn unit(mm: f64) -> i32 {
    (mm / super::records::UNIT_MM).round() as i32
}

/// Bytes of a record.
#[derive(Default)]
struct Bytes(Vec<u8>);

impl Bytes {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn len(&mut self, mm: f64) -> &mut Self {
        self.0.extend(unit(mm).to_le_bytes());
        self
    }
    fn point(&mut self, (x, y): (f64, f64)) -> &mut Self {
        self.len(x).len(y)
    }
    fn f64(&mut self, v: f64) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn zeros(&mut self, n: usize) -> &mut Self {
        self.0.extend(std::iter::repeat_n(0, n));
        self
    }
    /// The layer byte, the flags, the net, the polygon and the component a primitive starts with.
    fn common(&mut self, layer: u8, component: u16) -> &mut Self {
        self.u8(layer)
            .u8(0x04)
            .u8(0)
            .u16(0xFFFF)
            .u16(0xFFFF)
            .u16(component)
            .zeros(4)
    }
}

/// `bytes` as a sub-record: its length, then it.
fn sub(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend((bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

/// The component of a library footprint's primitive: none.
pub const NO_COMPONENT: u16 = 0xFFFF;

/// A pad's shape, as a record gives it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Round,
    Rect,
    Octagon,
    /// A rectangle whose corner radius is `percent` of half its short side.
    RoundRect(u8),
}

/// A pad to write.
#[derive(Debug, Clone)]
pub struct Pad {
    pub name: String,
    pub layer: u8,
    pub component: u16,
    pub at: (f64, f64),
    pub size: (f64, f64),
    pub shape: Shape,
    pub hole: f64,
    /// A slot's length and its angle from the pad's x, for a slotted hole.
    pub slot: Option<(f64, f64)>,
    pub angle: f64,
    pub plated: bool,
}

impl Pad {
    /// A surface pad on the top layer of a library footprint.
    pub fn smd(name: &str, at: (f64, f64), size: (f64, f64), shape: Shape) -> Self {
        Pad {
            name: name.into(),
            layer: layer::TOP,
            component: NO_COMPONENT,
            at,
            size,
            shape,
            hole: 0.0,
            slot: None,
            angle: 0.0,
            plated: false,
        }
    }

    /// A plated through-hole pad of a library footprint.
    pub fn through(name: &str, at: (f64, f64), size: (f64, f64), shape: Shape, hole: f64) -> Self {
        Pad {
            layer: layer::MULTI,
            hole,
            plated: true,
            ..Pad::smd(name, at, size, shape)
        }
    }

    pub fn record(&self) -> Vec<u8> {
        let mut out = vec![super::records::PAD];
        let mut name = Bytes::default();
        name.u8(self.name.len() as u8);
        name.0.extend(self.name.bytes());
        sub(&mut out, &name.0);
        sub(&mut out, &[]);
        sub(&mut out, b"\x04|&|0");
        sub(&mut out, &[]);
        let (shape, alt, radius) = match self.shape {
            Shape::Round => (1, 1, 0),
            Shape::Rect => (2, 2, 0),
            Shape::Octagon => (3, 3, 0),
            Shape::RoundRect(p) => (2, 9, p),
        };
        let mut main = Bytes::default();
        main.common(self.layer, self.component)
            .point(self.at)
            .point(self.size)
            .point(self.size)
            .point(self.size)
            .len(self.hole)
            .u8(shape)
            .u8(shape)
            .u8(shape)
            .f64(self.angle)
            .u8(u8::from(self.plated))
            .zeros(1)
            .u8(0) // simple pad mode
            .zeros(23)
            .len(0.0)
            .len(0.0)
            .zeros(7)
            .u8(0)
            .u8(0)
            .zeros(3)
            .f64(0.0)
            .zeros(6); // the layers a newer version adds
        assert!(main.0.len() >= super::records::PAD_MAIN_LEN);
        sub(&mut out, &main.0);
        let mut shapes = Bytes::default();
        for _ in 0..29 * 2 {
            shapes.len(self.size.0);
        }
        shapes.zeros(29).zeros(1);
        let (kind, slot, slot_angle) = match self.slot {
            Some((length, angle)) => (2, length, angle),
            None => (0, 0.0, 0.0),
        };
        shapes.u8(kind).len(slot).f64(slot_angle).zeros(32 * 8 + 1);
        for _ in 0..32 {
            shapes.u8(alt);
        }
        for _ in 0..32 {
            shapes.u8(radius);
        }
        assert_eq!(shapes.0.len(), super::records::PAD_SHAPE_LEN);
        sub(&mut out, &shapes.0);
        out
    }
}

/// A track from `start` to `end`.
pub fn track(layer: u8, component: u16, start: (f64, f64), end: (f64, f64), width: f64) -> Vec<u8> {
    let mut b = Bytes::default();
    b.common(layer, component)
        .point(start)
        .point(end)
        .len(width)
        .u16(0)
        .zeros(1)
        .zeros(5);
    let mut out = vec![super::records::TRACK];
    sub(&mut out, &b.0);
    out
}

/// An arc counter-clockwise from `start` to `end` degrees (equal: a circle).
pub fn arc(
    layer: u8,
    component: u16,
    center: (f64, f64),
    radius: f64,
    (start, end): (f64, f64),
    width: f64,
) -> Vec<u8> {
    let mut b = Bytes::default();
    b.common(layer, component)
        .point(center)
        .len(radius)
        .f64(start)
        .f64(end)
        .len(width)
        .u16(0)
        .zeros(5);
    let mut out = vec![super::records::ARC];
    sub(&mut out, &b.0);
    out
}

/// A filled rectangle between two corners, turned by `angle` degrees round its centre.
pub fn fill(layer: u8, component: u16, a: (f64, f64), b: (f64, f64), angle: f64) -> Vec<u8> {
    let mut r = Bytes::default();
    r.common(layer, component)
        .point(a)
        .point(b)
        .f64(angle)
        .zeros(4);
    let mut out = vec![super::records::FILL];
    sub(&mut out, &r.0);
    out
}

/// A region (`KIND=0`) with the outline `points`, as a library footprint or a board's
/// `Regions6` stream holds it.
pub fn region(layer: u8, component: u16, points: &[(f64, f64)]) -> Vec<u8> {
    let mut r = Bytes::default();
    r.u8(layer)
        .u8(0x04)
        .u8(0)
        .u16(0xFFFF)
        .u16(0xFFFF)
        .u16(component)
        .zeros(5)
        .u16(0)
        .zeros(2);
    let props = b"|V7_LAYER=MECHANICAL15|NAME= |KIND=0|SUBPOLYINDEX=-1|UNIONINDEX=0|ARCRESOLUTION=0.5mil|ISSHAPEBASED=FALSE|\0";
    let props: &[u8] = if layer == layer::mechanical(15) {
        props
    } else {
        b"|NAME= |KIND=0|ISSHAPEBASED=FALSE|\0"
    };
    r.u32(props.len() as u32);
    r.0.extend_from_slice(props);
    r.u32(points.len() as u32);
    for &(x, y) in points {
        r.f64(f64::from(unit(x))).f64(f64::from(unit(y)));
    }
    let mut out = vec![super::records::REGION];
    sub(&mut out, &r.0);
    out
}

/// A text (a designator or a comment string), which the import leaves out.
pub fn text(layer: u8, component: u16, text: &str) -> Vec<u8> {
    let mut b = Bytes::default();
    b.common(layer, component).zeros(40);
    let mut out = vec![super::records::TEXT];
    sub(&mut out, &b.0);
    let mut s = vec![text.len() as u8];
    s.extend(text.bytes());
    sub(&mut out, &s);
    out
}

/// A library (`.PcbLib`) of footprints, each a name and its records.
pub fn pcblib(footprints: &[(&str, Vec<Vec<u8>>)]) -> Vec<u8> {
    let mut file = Compound::new()
        .stream(&["FileHeader"], b"\x1bPCB 6.0 Binary Library File".to_vec())
        .stream(&["Library", "Data"], props(&[("HEADER", "Protel")]));
    for (name, records) in footprints {
        let storage: String = name.chars().take(31).collect();
        let mut data = Vec::new();
        let mut n = vec![name.len() as u8];
        n.extend(name.bytes());
        sub(&mut data, &n);
        for r in records {
            data.extend_from_slice(r);
        }
        file = file
            .stream(
                &[&storage, "Header"],
                (records.len() as u32).to_le_bytes().to_vec(),
            )
            .stream(&[&storage, "Data"], data)
            .stream(
                &[&storage, "Parameters"],
                props(&[("PATTERN", name), ("HEIGHT", "0mil")]),
            );
    }
    file.to_bytes()
}

/// A properties block: its length and `|KEY=VALUE|...` with a closing zero.
pub fn props(pairs: &[(&str, &str)]) -> Vec<u8> {
    let mut text: String = pairs.iter().map(|(k, v)| format!("|{k}={v}")).collect();
    text.push('\0');
    let mut out = (text.len() as u32).to_le_bytes().to_vec();
    out.extend(text.bytes());
    out
}

/// A component of a board: its designator, footprint, place (mm, y up), angle and face.
pub struct Component<'a> {
    pub designator: &'a str,
    pub pattern: &'a str,
    pub at: (f64, f64),
    pub angle: f64,
    pub bottom: bool,
}

/// A board (`.PcbDoc`) with these components and the primitive streams (`Pads6`, `Tracks6`, ...,
/// each the records of its kind, a primitive naming its component by index).
pub fn pcbdoc(components: &[Component<'_>], streams: &[(&str, Vec<Vec<u8>>)]) -> Vec<u8> {
    let mut list = Vec::new();
    for c in components {
        let x = format!("{}mil", c.at.0 / 0.0254);
        let y = format!("{}mil", c.at.1 / 0.0254);
        let angle = format!("{:.3}", c.angle);
        list.extend(props(&[
            ("LAYER", if c.bottom { "BOTTOM" } else { "TOP" }),
            ("X", &x),
            ("Y", &y),
            ("PATTERN", c.pattern),
            ("SOURCEDESIGNATOR", c.designator),
            ("ROTATION", &angle),
        ]));
    }
    let mut file = Compound::new()
        .stream(&["FileHeader"], b"\x1bPCB 6.0 Binary File".to_vec())
        .stream(&["Board6", "Data"], props(&[("FILENAME", "board.PcbDoc")]))
        .stream(&["Components6", "Data"], list);
    for (name, records) in streams {
        file = file.stream(&[name, "Data"], records.concat());
    }
    file.to_bytes()
}
