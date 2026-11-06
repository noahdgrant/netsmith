//! The rules a drawing must keep whatever it looks like, checked from the drawing alone:
//!
//! 1. **The drawing is the netlist.** Connectivity is rebuilt from the layout without
//!    trusting which net a wire says it is: pins, wire ends, wires that meet (an end on
//!    another wire, a crossing with a junction dot), power symbols, flags and labels are
//!    joined, and the pieces are compared with the design's nets. Two nets in one piece is a
//!    [`Violation::Short`]; one net's pins in several pieces, with no power symbol, flag or
//!    label of that net on each piece to join them by name, is a [`Violation::Open`] (so a
//!    net that is named must be named at every one of its pieces); a pin of no net that a
//!    wire touches is a [`Violation::ExtraPin`].
//! 2. **No wire along another net's wire** ([`Violation::WireOverlap`]).
//! 3. **No wire through a symbol's body** ([`Violation::WireThroughBody`]), and none along a
//!    pin's line ([`Violation::WireAlongPin`]).
//!    A wire also keeps off the outline of a body or a flag by 0.7 mm
//!    ([`Violation::WireHugsSymbol`]).
//!    No two texts overlap ([`Violation::TextOverlap`]) and no wire runs through one
//!    ([`Violation::WireOverText`]).
//! 4. **Nothing drawn that serves nothing**: every end of a wire touches a pin, a power symbol,
//!    a flag, a label, an X or another wire of its net ([`Violation::DeadEnd`]); no wire lies
//!    along another of its own net ([`Violation::RedundantWire`]); no wire is a point
//!    ([`Violation::ZeroLengthWire`]); no wire leaves a point and comes back within a grid step
//!    of itself ([`Violation::UTurn`]).
//! 5. **A dot where, and only where, three or more arms of a net meet**
//!    ([`Violation::MissingJunction`], [`Violation::UnusedJunction`]). A straight wire through
//!    a point is two arms, an end one, and a pin, power symbol, flag or label there one.
//!
//! 6. **A bus is not a conductor** ([`Violation::BusMember`]). The thick line of a bus joins
//!    nothing: each member's entry carries its net and joins, by name, the net of that name at
//!    the other end, as a label does. The members a bus declares must be the nets of its
//!    entries, once at each end, at places of the line that are not shared; and nothing else
//!    touches the line, nor lies along it.
//!
//! 7. **A note is no conductor and lies on nothing** ([`Violation::NoteOverlap`],
//!    [`Violation::NoteTextOverflow`]). A note (a framed box with the text of a doc comment or a
//!    relation, `layout/notes.rs`) joins no net: it is left out of the netlist above. Its frame
//!    overlaps no symbol, text, wire, thick line of a bus, power symbol, flag, dot, X or other
//!    note, and every line of its text, measured as it is drawn, stays inside its frame.
//!
//! Each sheet of a board drawn by module is checked on its own: a net that goes to another
//! sheet is joined there by its flag. [`check`] is a pure function of the design and its
//! layout. It reports every violation and fixes none.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use netsmith_core::Design;

use crate::geom::Pt;
use crate::layout::Layout;
use crate::sheetview::{
    Fault, OVERLAP, Rel, Seg, SheetView, body_strokes, bus_of, bus_segments, overlaps, relate,
    runs_through, runs_through_body, same, symbol_box, texts, views, wire_faults,
};

/// A rule the drawing breaks. Every variant names the sheet and the nets or parts involved.
#[derive(Debug, Clone, PartialEq)]
pub enum Violation {
    /// Two nets (or more) are joined by wires that touch, or by a wire that ends on a pin.
    Short {
        sheet: String,
        nets: Vec<String>,
        at: Pt,
    },
    /// The pins of one net on a sheet are in `pieces` separate pieces of wire, and no power
    /// symbol or flag of the net joins them by name.
    Open {
        sheet: String,
        net: String,
        pieces: usize,
        pins: Vec<String>,
    },
    /// A pin that is in no net is touched by a wire, a power symbol or a flag.
    ExtraPin {
        sheet: String,
        pin: String,
        touches: String,
    },
    /// A pin the design puts in a net, not drawn or drawn as the pin of another net.
    PinNet {
        sheet: String,
        pin: String,
        design: Option<String>,
        drawn: Option<String>,
    },
    /// A pad of a part drawn in more than one of the part's boxes (a part with several units
    /// draws each pad in exactly one).
    UnitPin {
        sheet: String,
        pin: String,
        boxes: usize,
    },
    /// A piece of wire that touches no pin, power symbol or flag.
    FloatingWire { sheet: String, net: String, at: Pt },
    /// A power symbol or a flag that touches no wire and no pin.
    FloatingFlag { sheet: String, net: String, at: Pt },
    /// A label (the name of a net at the end of a stub) that touches no wire and no pin.
    FloatingLabel { sheet: String, net: String, at: Pt },
    /// Wires of two nets lie along one line over `len` millimetres.
    WireOverlap {
        sheet: String,
        nets: (String, String),
        at: Pt,
        len: f64,
    },
    /// A wire runs through the body of a part's symbol.
    WireThroughBody {
        sheet: String,
        net: String,
        part: String,
        at: Pt,
    },
    /// A wire runs along the line of a pin.
    WireAlongPin {
        sheet: String,
        net: String,
        pin: String,
        at: Pt,
    },
    /// Two texts (or a text and an X on a pin) overlap: pin numbers and names, references,
    /// values, names of flags and labels.
    TextOverlap { sheet: String, at: Pt },
    /// A wire runs through a text.
    WireOverText { sheet: String, net: String, at: Pt },
    /// A wire runs along the outline of a part's body, or of a flag, nearer than 0.7 mm
    /// without being on it: it looks as if it touched.
    WireHugsSymbol {
        sheet: String,
        net: String,
        symbol: String,
        at: Pt,
    },
    /// An end of a wire touches nothing: no pin, symbol, flag, label, X, or other wire of its
    /// net.
    DeadEnd { sheet: String, net: String, at: Pt },
    /// A wire lies along another wire of its own net over `len` millimetres: a double line.
    RedundantWire {
        sheet: String,
        net: String,
        at: Pt,
        len: f64,
    },
    /// A wire that is a point, or has two equal points in a row.
    ZeroLengthWire { sheet: String, net: String, at: Pt },
    /// A wire goes out, across and back along itself within a grid step, a hook.
    UTurn { sheet: String, net: String, at: Pt },
    /// A dot where fewer than three arms of the net meet, or where there is no wire, or a
    /// second dot at one point.
    UnusedJunction {
        sheet: String,
        net: String,
        at: Pt,
        arms: usize,
    },
    /// Three or more arms of a net meet and there is no dot.
    MissingJunction {
        sheet: String,
        net: String,
        at: Pt,
        arms: usize,
    },
    /// A bus whose entries are not its members, or that something touches: an entry of a net
    /// that is not a member, a member with no entry (or two) at an end, two entries at one
    /// place of the line, an entry away from the line, or a wire that touches the line.
    BusMember {
        sheet: String,
        bus: String,
        reason: String,
    },
    /// The frame of a note lies on something else of the sheet: a symbol, a text, a wire, a
    /// bus's thick line, a power symbol, a flag, a dot, an X or another note.
    NoteOverlap {
        sheet: String,
        note: String,
        with: String,
        at: Pt,
    },
    /// A line of a note's text reaches out of its frame.
    NoteTextOverflow {
        sheet: String,
        note: String,
        line: usize,
        reason: String,
    },
}

impl Violation {
    /// A short name for the kind: `short`, `open`, `extra-pin`, `pin-net`, `unit-pin`, `floating-wire`,
    /// `floating-flag`, `floating-label`, `wire-overlap`, `wire-through-body`,
    /// `wire-along-pin`, `text-overlap`, `wire-over-text`, `wire-hugs-symbol`, `dead-end`, `redundant-wire`, `zero-length-wire`, `u-turn`,
    /// `unused-junction`, `missing-junction`, `bus-member`, `note-overlap`, `note-text-overflow`.
    pub fn kind(&self) -> &'static str {
        match self {
            Violation::Short { .. } => "short",
            Violation::Open { .. } => "open",
            Violation::ExtraPin { .. } => "extra-pin",
            Violation::PinNet { .. } => "pin-net",
            Violation::UnitPin { .. } => "unit-pin",
            Violation::FloatingWire { .. } => "floating-wire",
            Violation::FloatingFlag { .. } => "floating-flag",
            Violation::FloatingLabel { .. } => "floating-label",
            Violation::WireOverlap { .. } => "wire-overlap",
            Violation::WireThroughBody { .. } => "wire-through-body",
            Violation::WireAlongPin { .. } => "wire-along-pin",
            Violation::TextOverlap { .. } => "text-overlap",
            Violation::WireOverText { .. } => "wire-over-text",
            Violation::WireHugsSymbol { .. } => "wire-hugs-symbol",
            Violation::DeadEnd { .. } => "dead-end",
            Violation::RedundantWire { .. } => "redundant-wire",
            Violation::ZeroLengthWire { .. } => "zero-length-wire",
            Violation::UTurn { .. } => "u-turn",
            Violation::UnusedJunction { .. } => "unused-junction",
            Violation::MissingJunction { .. } => "missing-junction",
            Violation::BusMember { .. } => "bus-member",
            Violation::NoteOverlap { .. } => "note-overlap",
            Violation::NoteTextOverflow { .. } => "note-text-overflow",
        }
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = |p: &Pt| format!("({:.2}, {:.2})", p.x, p.y);
        let opt = |n: &Option<String>| n.clone().unwrap_or_else(|| "no net".into());
        match self {
            Violation::Short { sheet, nets, at: p } => write!(
                f,
                "{sheet}: short: nets {} are one piece of wire near {}",
                nets.join(", "),
                at(p)
            ),
            Violation::Open {
                sheet,
                net,
                pieces,
                pins,
            } => write!(
                f,
                "{sheet}: open: net {net} is in {pieces} pieces with nothing to join them by name; pins {}",
                pins.join(", ")
            ),
            Violation::ExtraPin {
                sheet,
                pin,
                touches,
            } => write!(
                f,
                "{sheet}: extra pin: {pin} is in no net but touches {touches}"
            ),
            Violation::PinNet {
                sheet,
                pin,
                design,
                drawn,
            } => write!(
                f,
                "{sheet}: pin {pin} is in {} in the design and {} in the drawing",
                opt(design),
                opt(drawn)
            ),
            Violation::UnitPin { sheet, pin, boxes } => write!(
                f,
                "{sheet}: pin {pin} is drawn in {boxes} boxes of its part: each pad is in exactly one unit"
            ),
            Violation::FloatingWire { sheet, net, at: p } => {
                write!(f, "{sheet}: floating wire of {net} near {}", at(p))
            }
            Violation::FloatingFlag { sheet, net, at: p } => write!(
                f,
                "{sheet}: a power symbol or flag of {net} touches no wire and no pin near {}",
                at(p)
            ),
            Violation::FloatingLabel { sheet, net, at: p } => write!(
                f,
                "{sheet}: the label of {net} touches no wire and no pin near {}",
                at(p)
            ),
            Violation::WireOverlap {
                sheet,
                nets,
                at: p,
                len,
            } => write!(
                f,
                "{sheet}: wires of {} and {} run along one line for {len:.2} mm from {}",
                nets.0,
                nets.1,
                at(p)
            ),
            Violation::WireThroughBody {
                sheet,
                net,
                part,
                at: p,
            } => write!(
                f,
                "{sheet}: a wire of {net} runs through the body of {part} near {}",
                at(p)
            ),
            Violation::WireAlongPin {
                sheet,
                net,
                pin,
                at: p,
            } => write!(
                f,
                "{sheet}: a wire of {net} runs along the line of pin {pin} near {}",
                at(p)
            ),
            Violation::TextOverlap { sheet, at: p } => {
                write!(f, "{sheet}: two texts overlap near {}", at(p))
            }
            Violation::WireOverText { sheet, net, at: p } => {
                write!(
                    f,
                    "{sheet}: a wire of {net} runs through a text near {}",
                    at(p)
                )
            }
            Violation::WireHugsSymbol {
                sheet,
                net,
                symbol,
                at: p,
            } => write!(
                f,
                "{sheet}: a wire of {net} runs along the outline of {symbol} within 0.7 mm near {}",
                at(p)
            ),
            Violation::DeadEnd { sheet, net, at: p } => write!(
                f,
                "{sheet}: dead end: a wire of {net} ends at {} and touches nothing",
                at(p)
            ),
            Violation::RedundantWire {
                sheet,
                net,
                at: p,
                len,
            } => write!(
                f,
                "{sheet}: redundant wire: two wires of {net} lie along one line for {len:.2} mm from {}",
                at(p)
            ),
            Violation::ZeroLengthWire { sheet, net, at: p } => {
                write!(f, "{sheet}: zero-length wire of {net} at {}", at(p))
            }
            Violation::UTurn { sheet, net, at: p } => write!(
                f,
                "{sheet}: u-turn: a wire of {net} goes out and back within a grid step near {}",
                at(p)
            ),
            Violation::UnusedJunction {
                sheet,
                net,
                at: p,
                arms,
            } => write!(
                f,
                "{sheet}: unused junction: a dot of {net} at {} where {arms} arms meet",
                at(p)
            ),
            Violation::MissingJunction {
                sheet,
                net,
                at: p,
                arms,
            } => write!(
                f,
                "{sheet}: missing junction: {arms} arms of {net} meet at {} with no dot",
                at(p)
            ),
            Violation::BusMember { sheet, bus, reason } => {
                write!(f, "{sheet}: bus-member: bus {bus}: {reason}")
            }
            Violation::NoteOverlap {
                sheet,
                note,
                with,
                at: p,
            } => write!(
                f,
                "{sheet}: note-overlap: the note of {note} lies on {with} near {}",
                at(p)
            ),
            Violation::NoteTextOverflow {
                sheet,
                note,
                line,
                reason,
            } => write!(
                f,
                "{sheet}: note-text-overflow: line {line} of the note of {note} {reason}"
            ),
        }
    }
}

/// Every rule `layout` breaks, sheet by sheet, in a fixed order.
pub fn check(design: &Design, layout: &Layout) -> Vec<Violation> {
    let mut out = Vec::new();
    let truth = pin_nets(design);
    for v in views(layout) {
        netsmith_core::job::checkpoint();
        check_sheet(design, layout, &truth, &v, &mut out);
        netsmith_core::job::checkpoint();
        check_wires(design, layout, &v, &mut out);
        check_notes(design, layout, &v, &mut out);
    }
    out
}

/// What a note's frame lies on.
pub(crate) enum On {
    Part(String),
    Text,
    Wire(usize),
    Thick(usize),
    Symbol,
    Flag,
    Dot,
    BusFlag,
    Note(usize),
}

/// Every place a note of sheet `v` lies on something else of it: `(note, what, where)`, with
/// the note an index into `layout.notes`. Each pair of notes is told once, by the first.
pub(crate) fn note_overlaps_of(l: &Layout, v: &SheetView, out: &mut Vec<(usize, On, Pt)>) {
    let written = texts(l, v);
    let runs: Vec<Seg> = v
        .segments
        .iter()
        .copied()
        .chain(bus_segments(l, v))
        .filter(|s| s.is_axis_aligned())
        .collect();
    for &i in &v.notes {
        let frame = l.notes[i].frame;
        let mut on = |what: On, at: Pt| out.push((i, what, at));
        for &p in &v.parts {
            let b = symbol_box(&l.parts[p]);
            if overlaps(&frame, &b, OVERLAP) {
                on(On::Part(l.parts[p].reference.text.clone()), b.min);
            }
        }
        for t in written.iter().filter(|t| overlaps(&frame, t, OVERLAP)) {
            on(On::Text, t.min);
        }
        for s in &runs {
            if runs_through(s, &frame) {
                on(
                    match bus_of(s.net) {
                        Some(b) => On::Thick(b),
                        None => On::Wire(s.net),
                    },
                    s.a,
                );
            }
        }
        for &k in &v.symbols {
            let b = l.symbols[k].glyph_extent();
            if overlaps(&frame, &b, OVERLAP) {
                on(On::Symbol, b.min);
            }
        }
        for &k in &v.ports {
            let b = l.ports[k].extent();
            if overlaps(&frame, &b, OVERLAP) {
                on(On::Flag, b.min);
            }
        }
        for &k in &v.junctions {
            let at = l.junctions[k].at;
            if at.x > frame.min.x && at.x < frame.max.x && at.y > frame.min.y && at.y < frame.max.y
            {
                on(On::Dot, at);
            }
        }
        for &k in &v.buses {
            if let Some(f) = &l.buses[k].flag {
                let b = f.extent();
                if overlaps(&frame, &b, OVERLAP) {
                    on(On::BusFlag, b.min);
                }
            }
        }
        for &j in &v.notes {
            if j > i && overlaps(&frame, &l.notes[j].frame, OVERLAP) {
                on(On::Note(j), l.notes[j].frame.min);
            }
        }
    }
}

/// The rules about notes: none lies on anything else of its sheet, and its text stays in its
/// frame. A note joins nothing, so nothing of the netlist reads it.
fn check_notes(design: &Design, l: &Layout, v: &SheetView, out: &mut Vec<Violation>) {
    use crate::layout::notes::text_box;
    let sheet = &v.name;
    let name_of = |i: usize| {
        let n = &l.notes[i];
        let path = if n.path.is_empty() {
            "the board".to_string()
        } else {
            n.path.clone()
        };
        format!("{path} (note {})", n.note)
    };
    let mut found = Vec::new();
    note_overlaps_of(l, v, &mut found);
    for (i, what, at) in found {
        let with = match what {
            On::Part(r) => format!("part {r}"),
            On::Text => "a text".to_string(),
            On::Wire(net) => format!("a wire of net {}", net_name(design, net)),
            On::Thick(b) => format!("the thick line of bus {}", l.buses[b].path),
            On::Symbol => "a power symbol".to_string(),
            On::Flag => "a flag".to_string(),
            On::Dot => "a dot".to_string(),
            On::BusFlag => "the flag of a bus".to_string(),
            On::Note(j) => format!("the note of {}", name_of(j)),
        };
        out.push(Violation::NoteOverlap {
            sheet: sheet.clone(),
            note: name_of(i),
            with,
            at,
        });
    }
    for &i in &v.notes {
        let n = &l.notes[i];
        let frame = n.frame;
        // The text stays in the frame: measured as it is drawn.
        let inside = |r: crate::geom::Rect| {
            let slack = 0.3;
            if r.min.x < frame.min.x + slack {
                Some("starts left of its frame".to_string())
            } else if r.max.x > frame.max.x - slack {
                Some(format!(
                    "is {:.2} mm wide and the frame holds {:.2} mm",
                    r.width(),
                    frame.width() - 2.0 * slack
                ))
            } else if r.min.y < frame.min.y + slack || r.max.y > frame.max.y - slack {
                Some("is above or below its frame".to_string())
            } else {
                None
            }
        };
        if let Some(h) = &n.heading
            && let Some(why) = inside(text_box(h, crate::layout::HEADING_TEXT, false))
        {
            out.push(Violation::NoteTextOverflow {
                sheet: sheet.clone(),
                note: name_of(i),
                line: 0,
                reason: why,
            });
        }
        for (k, line) in n.lines.iter().enumerate() {
            if let Some(why) = inside(text_box(line, n.size, n.mono)) {
                out.push(Violation::NoteTextOverflow {
                    sheet: sheet.clone(),
                    note: name_of(i),
                    line: k + 1,
                    reason: why,
                });
            }
        }
    }
}

/// The net each `(refdes, pad)` is in, in the design.
fn pin_nets(design: &Design) -> HashMap<(&str, &str), usize> {
    let mut out = HashMap::new();
    for (n, net) in design.nets.iter().enumerate() {
        for p in &net.pins {
            out.entry((p.refdes.as_str(), p.pin.as_str())).or_insert(n);
        }
    }
    out
}

struct Union(Vec<usize>);

impl Union {
    fn new(n: usize) -> Self {
        Union((0..n).collect())
    }

    fn find(&mut self, i: usize) -> usize {
        let mut root = i;
        while self.0[root] != root {
            root = self.0[root];
        }
        let mut at = i;
        while self.0[at] != root {
            let next = self.0[at];
            self.0[at] = root;
            at = next;
        }
        root
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}

/// A pin of the sheet, with the net the design gives it.
struct SheetPin {
    label: String,
    at: Pt,
    hidden: bool,
    design: Option<usize>,
    drawn: Option<usize>,
}

fn net_name(design: &Design, n: usize) -> String {
    design.nets[n].name.clone()
}

fn check_sheet(
    design: &Design,
    l: &Layout,
    truth: &HashMap<(&str, &str), usize>,
    v: &SheetView,
    out: &mut Vec<Violation>,
) {
    let sheet = v.name.clone();

    // The pins of this sheet, and for each part the pads its boxes draw (a part with several
    // units has a box for each, and each pad is in exactly one).
    let mut pins: Vec<SheetPin> = Vec::new();
    let mut drawn_pads: BTreeMap<usize, BTreeMap<&str, usize>> = BTreeMap::new();
    for &i in &v.parts {
        let p = &l.parts[i];
        let refdes = design.parts[p.part].refdes.as_str();
        let pads = drawn_pads.entry(p.part).or_default();
        for (g, placed) in p.glyph.pins.iter().zip(&p.pins) {
            *pads.entry(placed.id.as_str()).or_default() += 1;
            pins.push(SheetPin {
                label: format!("{refdes}.{}", placed.id),
                at: placed.at,
                hidden: g.hidden,
                design: truth.get(&(refdes, placed.id.as_str())).copied(),
                drawn: placed.net,
            });
        }
    }
    for (&part, pads) in &drawn_pads {
        let refdes = design.parts[part].refdes.as_str();
        for (pad, &boxes) in pads {
            if boxes > 1 {
                out.push(Violation::UnitPin {
                    sheet: sheet.clone(),
                    pin: format!("{refdes}.{pad}"),
                    boxes,
                });
            }
        }
        // A pad the design uses that no box of the part draws.
        for net in &design.nets {
            for r in net.pins.iter().filter(|r| r.refdes == refdes) {
                if !pads.contains_key(r.pin.as_str()) {
                    out.push(Violation::PinNet {
                        sheet: sheet.clone(),
                        pin: format!("{refdes}.{}", r.pin),
                        design: Some(net.name.clone()),
                        drawn: None,
                    });
                }
            }
        }
    }
    for pin in &pins {
        if pin.design != pin.drawn {
            out.push(Violation::PinNet {
                sheet: sheet.clone(),
                pin: pin.label.clone(),
                design: pin.design.map(|n| net_name(design, n)),
                drawn: pin.drawn.map(|n| net_name(design, n)),
            });
        }
    }

    // Elements: pins, then segments, then power symbols, then flags, then labels.
    let (np, ns) = (pins.len(), v.segments.len());
    let seg_at = np;
    let sym_at = np + ns;
    let port_at = sym_at + v.symbols.len();
    let label_at = port_at + v.ports.len();
    // The places where a member's wire meets the thick line of its bus.
    let entries: Vec<(usize, usize)> = v
        .buses
        .iter()
        .flat_map(|&b| (0..l.buses[b].entries.len()).map(move |e| (b, e)))
        .collect();
    let entry_at = label_at + v.labels.len();
    let mut u = Union::new(entry_at + entries.len());

    let dots: Vec<Pt> = v.junctions.iter().map(|&i| l.junctions[i].at).collect();
    for (i, s) in v.segments.iter().enumerate() {
        for (j, t) in v.segments.iter().enumerate().skip(i + 1) {
            let joined = match relate(s, t) {
                Rel::Apart => false,
                Rel::Touch(_) | Rel::Overlap { .. } => true,
                Rel::Cross(p) => dots.iter().any(|&d| same(d, p)),
            };
            if joined {
                u.join(seg_at + i, seg_at + j);
            }
        }
    }
    let touching = |p: Pt| {
        v.segments
            .iter()
            .enumerate()
            .filter(move |(_, s)| s.holds(p))
            .map(|(i, _)| seg_at + i)
    };
    // What sits at a point joins what else is at it: key points by millimetre thousandths.
    let key = |p: Pt| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
    let mut here: BTreeMap<(i64, i64), usize> = BTreeMap::new();
    let mut place = |u: &mut Union, p: Pt, id: usize| {
        for s in touching(p) {
            u.join(id, s);
        }
        match here.get(&key(p)) {
            Some(&other) => u.join(id, other),
            None => {
                here.insert(key(p), id);
            }
        }
    };
    for (i, pin) in pins.iter().enumerate() {
        place(&mut u, pin.at, i);
    }
    for (k, &i) in v.symbols.iter().enumerate() {
        place(&mut u, l.symbols[i].at, sym_at + k);
    }
    for (k, &i) in v.ports.iter().enumerate() {
        place(&mut u, l.ports[i].at, port_at + k);
    }
    for (k, &i) in v.labels.iter().enumerate() {
        place(&mut u, l.labels[i].at, label_at + k);
    }
    for (k, &(b, e)) in entries.iter().enumerate() {
        place(&mut u, l.buses[b].entries[e].at, entry_at + k);
    }

    // What each piece claims to be.
    #[derive(Default)]
    struct Piece {
        nets: BTreeSet<usize>,
        pins: usize,
        named: usize,
        labels: usize,
        segments: usize,
        at: Option<Pt>,
        net_of_segment: Option<usize>,
    }
    let mut pieces: BTreeMap<usize, Piece> = BTreeMap::new();
    for (i, pin) in pins.iter().enumerate() {
        let p = pieces.entry(u.find(i)).or_default();
        p.pins += 1;
        p.at.get_or_insert(pin.at);
        if let Some(n) = pin.design {
            p.nets.insert(n);
        }
    }
    for (i, s) in v.segments.iter().enumerate() {
        let p = pieces.entry(u.find(seg_at + i)).or_default();
        p.segments += 1;
        p.nets.insert(s.net);
        p.net_of_segment.get_or_insert(s.net);
        p.at = Some(match p.at {
            Some(q) if (q.x, q.y) < (s.a.x, s.a.y) => q,
            _ => s.a,
        });
    }
    for (k, &i) in v.symbols.iter().enumerate() {
        let p = pieces.entry(u.find(sym_at + k)).or_default();
        p.named += 1;
        p.nets.insert(l.symbols[i].net);
        p.at.get_or_insert(l.symbols[i].at);
    }
    for (k, &i) in v.ports.iter().enumerate() {
        let p = pieces.entry(u.find(port_at + k)).or_default();
        p.named += 1;
        p.nets.insert(l.ports[i].net);
        p.at.get_or_insert(l.ports[i].at);
    }
    for (k, &i) in v.labels.iter().enumerate() {
        let p = pieces.entry(u.find(label_at + k)).or_default();
        p.labels += 1;
        p.nets.insert(l.labels[i].net);
        p.at.get_or_insert(l.labels[i].at);
    }
    for (k, &(b, e)) in entries.iter().enumerate() {
        let entry = &l.buses[b].entries[e];
        let p = pieces.entry(u.find(entry_at + k)).or_default();
        p.labels += 1;
        p.nets.insert(entry.net);
        p.at.get_or_insert(entry.at);
    }
    for p in pieces.values() {
        let at = p.at.unwrap_or(Pt::new(0.0, 0.0));
        if p.nets.len() > 1 {
            out.push(Violation::Short {
                sheet: sheet.clone(),
                nets: p.nets.iter().map(|&n| net_name(design, n)).collect(),
                at,
            });
        } else if p.segments > 0 && p.pins == 0 && p.named + p.labels == 0 {
            out.push(Violation::FloatingWire {
                sheet: sheet.clone(),
                net: net_name(design, p.net_of_segment.unwrap_or(0)),
                at,
            });
        } else if p.segments == 0 && p.pins == 0 && p.named + p.labels > 0 {
            let net = p
                .nets
                .iter()
                .next()
                .map_or_else(String::new, |&n| net_name(design, n));
            out.push(if p.named == 0 {
                Violation::FloatingLabel {
                    sheet: sheet.clone(),
                    net,
                    at,
                }
            } else {
                Violation::FloatingFlag {
                    sheet: sheet.clone(),
                    net,
                    at,
                }
            });
        }
    }
    // A pin of no net, touched by anything.
    for (i, pin) in pins.iter().enumerate() {
        if pin.design.is_some() || pin.hidden {
            continue;
        }
        let root = u.find(i);
        let others = pieces
            .get(&root)
            .map_or(0, |p| p.segments + p.named + p.labels);
        if others > 0 {
            let touches = pieces[&root]
                .nets
                .iter()
                .map(|&n| net_name(design, n))
                .collect::<Vec<_>>()
                .join(", ");
            out.push(Violation::ExtraPin {
                sheet: sheet.clone(),
                pin: pin.label.clone(),
                touches: format!("a wire or symbol of {touches}"),
            });
        }
    }

    // Opens: join the pieces that carry a power symbol, flag or label of the same net, by name.
    let mut by_name: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (k, &i) in v.symbols.iter().enumerate() {
        by_name
            .entry(l.symbols[i].net)
            .or_default()
            .push(sym_at + k);
    }
    for (k, &i) in v.ports.iter().enumerate() {
        by_name.entry(l.ports[i].net).or_default().push(port_at + k);
    }
    for (k, &i) in v.labels.iter().enumerate() {
        by_name
            .entry(l.labels[i].net)
            .or_default()
            .push(label_at + k);
    }
    for (k, &(b, e)) in entries.iter().enumerate() {
        by_name
            .entry(l.buses[b].entries[e].net)
            .or_default()
            .push(entry_at + k);
    }
    let mut named = Union::new(u.0.len());
    for (i, root) in (0..u.0.len()).map(|i| (i, u.find(i))) {
        named.join(i, root);
    }
    for ids in by_name.values() {
        for pair in ids.windows(2) {
            named.join(pair[0], pair[1]);
        }
    }
    let mut by_net: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, pin) in pins.iter().enumerate() {
        if let Some(n) = pin.design {
            by_net.entry(n).or_default().push(i);
        }
    }
    for (net, ids) in by_net {
        let roots: BTreeSet<usize> = ids.iter().map(|&i| named.find(i)).collect();
        if roots.len() > 1 {
            out.push(Violation::Open {
                sheet: sheet.clone(),
                net: net_name(design, net),
                pieces: roots.len(),
                pins: ids.iter().map(|&i| pins[i].label.clone()).collect(),
            });
        }
    }

    // Wires along each other, and through bodies.
    for (i, s) in v.segments.iter().enumerate() {
        for t in &v.segments[i + 1..] {
            if s.net == t.net {
                continue;
            }
            if let Rel::Overlap { at, len } = relate(s, t) {
                out.push(Violation::WireOverlap {
                    sheet: sheet.clone(),
                    nets: (net_name(design, s.net), net_name(design, t.net)),
                    at,
                    len,
                });
            }
        }
    }
    for &i in &v.parts {
        let strokes = body_strokes(&l.parts[i]);
        for s in v.segments.iter().filter(|s| runs_through_body(s, &strokes)) {
            out.push(Violation::WireThroughBody {
                sheet: sheet.clone(),
                net: net_name(design, s.net),
                part: design.parts[l.parts[i].part].refdes.clone(),
                at: s.a,
            });
        }
        for s in bus_segments(l, v)
            .iter()
            .filter(|s| runs_through_body(s, &strokes))
        {
            out.push(Violation::WireThroughBody {
                sheet: sheet.clone(),
                net: format!("bus {}", l.buses[bus_of(s.net).unwrap_or(0)].path),
                part: design.parts[l.parts[i].part].refdes.clone(),
                at: s.a,
            });
        }
    }
    check_buses(design, l, v, out);
}

/// The rules about buses: what is on the thick line, and what touches it.
fn check_buses(design: &Design, l: &Layout, v: &SheetView, out: &mut Vec<Violation>) {
    let sheet = &v.name;
    let key = |p: Pt| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
    let bad = |out: &mut Vec<Violation>, bus: &crate::layout::Bus, reason: String| {
        out.push(Violation::BusMember {
            sheet: sheet.clone(),
            bus: bus.path.clone(),
            reason,
        });
    };
    let runs = bus_segments(l, v);
    for &i in &v.buses {
        let bus = &l.buses[i];
        let members: BTreeSet<usize> = bus.members.iter().map(|m| m.net).collect();
        let on_line = |p: Pt| {
            bus.runs()
                .iter()
                .any(|&(a, b)| Seg { net: 0, a, b }.holds(p))
        };
        for (end, part) in bus.ends.iter().enumerate() {
            let here: Vec<&crate::layout::BusEntry> =
                bus.entries.iter().filter(|e| e.end == end).collect();
            for e in &here {
                if !members.contains(&e.net) {
                    bad(
                        out,
                        bus,
                        format!(
                            "an entry of net {} at {}, which is not a member (members: {})",
                            net_name(design, e.net),
                            at_text(e.at),
                            member_names(design, bus)
                        ),
                    );
                }
                if !on_line(e.at) {
                    bad(
                        out,
                        bus,
                        format!(
                            "the entry of net {} at {} is not on the thick line",
                            net_name(design, e.net),
                            at_text(e.at)
                        ),
                    );
                }
            }
            for &net in &members {
                let n = here.iter().filter(|e| e.net == net).count();
                if n != 1 {
                    bad(
                        out,
                        bus,
                        format!(
                            "member {} has {n} entries at its end at {}, not one",
                            net_name(design, net),
                            design.parts[*part].refdes
                        ),
                    );
                }
            }
            let mut seen: BTreeMap<(i64, i64), usize> = BTreeMap::new();
            for e in &here {
                if let Some(&other) = seen.get(&key(e.at))
                    && other != e.net
                {
                    bad(
                        out,
                        bus,
                        format!(
                            "members {} and {} are joined at one place of the line, {}",
                            net_name(design, other),
                            net_name(design, e.net),
                            at_text(e.at)
                        ),
                    );
                }
                seen.entry(key(e.at)).or_insert(e.net);
            }
        }
    }
    // What touches or lies along a thick line.
    for run in &runs {
        let Some(b) = bus_of(run.net) else { continue };
        let bus = &l.buses[b];
        for s in &v.segments {
            match relate(run, s) {
                Rel::Overlap { at, len } => out.push(Violation::WireOverlap {
                    sheet: sheet.clone(),
                    nets: (format!("bus {}", bus.path), net_name(design, s.net)),
                    at,
                    len,
                }),
                Rel::Touch(p) => {
                    let entry = bus.entries.iter().any(|e| e.net == s.net && same(e.at, p));
                    if !entry {
                        bad(
                            out,
                            bus,
                            format!(
                                "a wire of net {} touches the thick line at {}, away from an entry",
                                net_name(design, s.net),
                                at_text(p)
                            ),
                        );
                    }
                }
                Rel::Apart | Rel::Cross(_) => {}
            }
        }
    }
}

fn at_text(p: Pt) -> String {
    format!("({:.2}, {:.2})", p.x, p.y)
}

fn member_names(design: &Design, bus: &crate::layout::Bus) -> String {
    bus.members
        .iter()
        .map(|m| net_name(design, m.net))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The rules about what the wires of a sheet draw, whatever the nets are.
fn check_wires(design: &Design, l: &Layout, v: &SheetView, out: &mut Vec<Violation>) {
    let sheet = &v.name;
    let written = texts(l, v);
    for (i, a) in written.iter().enumerate() {
        for b in &written[i + 1..] {
            if overlaps(a, b, OVERLAP) {
                out.push(Violation::TextOverlap {
                    sheet: sheet.clone(),
                    at: a.min,
                });
            }
        }
    }
    for s in v
        .segments
        .iter()
        .copied()
        .chain(bus_segments(l, v))
        .filter(|s| s.is_axis_aligned())
    {
        for t in written.iter().filter(|t| runs_through(&s, t)) {
            out.push(Violation::WireOverText {
                sheet: sheet.clone(),
                net: match bus_of(s.net) {
                    Some(b) => format!("bus {}", l.buses[b].path),
                    None => net_name(design, s.net),
                },
                at: t.min,
            });
        }
    }
    for f in wire_faults(l, v) {
        let net = net_name(design, f.net);
        let (sheet, at) = (sheet.clone(), f.at);
        out.push(match f.fault {
            Fault::Zero => Violation::ZeroLengthWire { sheet, net, at },
            Fault::Redundant { len } => Violation::RedundantWire {
                sheet,
                net,
                at,
                len,
            },
            Fault::DeadEnd => Violation::DeadEnd { sheet, net, at },
            Fault::UTurn => Violation::UTurn { sheet, net, at },
            Fault::UnusedJunction { arms } => Violation::UnusedJunction {
                sheet,
                net,
                at,
                arms,
            },
            Fault::MissingJunction { arms } => Violation::MissingJunction {
                sheet,
                net,
                at,
                arms,
            },
            Fault::HugsPart { part } => Violation::WireHugsSymbol {
                sheet,
                net,
                symbol: design.parts[part].refdes.clone(),
                at,
            },
            Fault::HugsFlag { net: flag } => Violation::WireHugsSymbol {
                sheet,
                net,
                symbol: format!("the flag of {}", net_name(design, flag)),
                at,
            },
            Fault::AlongPin { part, pad } => Violation::WireAlongPin {
                sheet,
                net,
                pin: format!("{}.{pad}", design.parts[part].refdes),
                at,
            },
        });
    }
}
