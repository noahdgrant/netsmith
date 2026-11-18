//! The nets a board's routing made long, and the nets with the most vias (issue #1194): what a
//! person reads to find a line that must be short (a crystal's, a clock's, a boot capacitor's)
//! and came out long, without reading the board.
//!
//! Read from the board file, as the scorecard is ([`crate::Scorecard`]), so a board netsmith
//! routed and one a person routed in KiCad are measured by the same code. Only the nets that no
//! zone pours count: a poured net's tracks are not its connection, and its vias are mostly
//! stitching, put there on purpose.
//!
//! - A net's **routed length** is the length of all its tracks (an arc by the chords the
//!   scorecard measures). Its **straight length** is the shortest tree of straight lines between
//!   the centres of its pads, the scorecard's spanning length of the one net: no routing is
//!   shorter, give or take a junction off a pad. Their ratio is how much longer than the least
//!   the routing made it.
//! - A net is **long** when its routed length is more than [`LONG_RATIO`] times its straight
//!   length and at least [`LONG_EXCESS`] over it: the second keeps out a net of a millimetre
//!   that a short jog doubles. The long nets are listed longest ratio first.
//! - The nets with the **most vias** are the [`MOST_VIAS`] with the most, of those with at
//!   least [`VIAS_LEAST`] (a signal net that changes layer once has one), most first.
//!
//! Nothing here is a check: no board is refused for it. `fab-notes.txt`, `board drc` (as notes)
//! and the scorecard list them, with `--format json` as data. A net that must be short says
//! how short in a part's `layout` block (`route NAME .pin length LENGTH`), and that is checked.

use netsmith_geom::Artwork;

/// A net is long when its routed length is more than this many times its straight length.
pub const LONG_RATIO: f64 = 2.0;
/// ... and at least this much over it, in mm.
pub const LONG_EXCESS: f64 = 2.0;
/// The most nets the list of vias holds.
pub const MOST_VIAS: usize = 5;
/// The fewest vias of a net that the list of vias holds.
pub const VIAS_LEAST: usize = 2;
/// Points closer than this are one point, in mm.
const SAME: f64 = 1e-3;

/// One routed net's numbers.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NetRoute {
    pub net: String,
    /// The length of all the net's tracks, in mm.
    pub length: f64,
    /// The shortest tree of straight lines between the centres of the net's pads, in mm.
    pub straight: f64,
    /// `length` over `straight`; 0 for a net whose pads are on one point.
    pub ratio: f64,
    pub vias: usize,
}

/// The long nets and the nets with the most vias of a board ([`NetRoutes::of`]).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct NetRoutes {
    /// Each net routed more than [`LONG_RATIO`] times its straight length and [`LONG_EXCESS`]
    /// over it, the longest ratio first.
    pub long: Vec<NetRoute>,
    /// The [`MOST_VIAS`] nets with the most vias, of [`VIAS_LEAST`] or more, most first.
    pub most_vias: Vec<NetRoute>,
}

impl NetRoutes {
    /// The numbers of the board file's routed nets: the nets no zone pours, with a track or a
    /// via.
    pub fn of(art: &Artwork) -> NetRoutes {
        let mut nets: Vec<NetRoute> = crate::net_table::rows(art)
            .into_iter()
            .map(|row| NetRoute {
                net: row.net,
                length: row.length,
                straight: row.straight,
                ratio: row.ratio,
                vias: row.vias,
            })
            .collect();
        let mut long: Vec<NetRoute> = nets
            .iter()
            .filter(|n| {
                n.straight > SAME
                    && n.length > LONG_RATIO * n.straight
                    && n.length - n.straight >= LONG_EXCESS
            })
            .cloned()
            .collect();
        long.sort_by(|a, b| b.ratio.total_cmp(&a.ratio).then(a.net.cmp(&b.net)));
        nets.retain(|n| n.vias >= VIAS_LEAST);
        nets.sort_by(|a, b| b.vias.cmp(&a.vias).then(a.net.cmp(&b.net)));
        nets.truncate(MOST_VIAS);
        NetRoutes {
            long,
            most_vias: nets,
        }
    }

    /// Whether there is nothing to list.
    pub fn is_empty(&self) -> bool {
        self.long.is_empty() && self.most_vias.is_empty()
    }

    /// The lines of `fab-notes.txt` and of `board drc`: a heading for the long nets and a line
    /// for each, and one line of the nets with the most vias. None for a board with neither.
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.long.is_empty() {
            out.push(format!(
                "routed long: {} net{} routed more than {}x the straight length between {} pads and {} mm over it; look at each that must be short (a crystal's, a clock's, a switching node's)",
                self.long.len(),
                if self.long.len() == 1 { "" } else { "s" },
                number(LONG_RATIO),
                if self.long.len() == 1 { "its" } else { "their" },
                number(LONG_EXCESS),
            ));
            out.extend(self.long.iter().map(|n| {
                format!(
                    "  {}: {} mm routed, straight {} mm ({}x), {} via{}",
                    n.net,
                    number(n.length),
                    number(n.straight),
                    number(n.ratio),
                    n.vias,
                    if n.vias == 1 { "" } else { "s" },
                )
            }));
        }
        if !self.most_vias.is_empty() {
            let nets: Vec<String> = self
                .most_vias
                .iter()
                .map(|n| {
                    format!(
                        "{} {} vias ({} mm routed, straight {} mm)",
                        n.net,
                        n.vias,
                        number(n.length),
                        number(n.straight)
                    )
                })
                .collect();
            out.push(format!(
                "most vias (nets no zone pours, {VIAS_LEAST} or more): {}",
                nets.join("; ")
            ));
        }
        out
    }
}

/// A number with at most two decimals and no trailing zeros: `3`, `2.45`.
fn number(x: f64) -> String {
    let text = format!("{x:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}
