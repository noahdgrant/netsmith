//! A coin cell with solder tabs, lying flat on the board, through hole
//! (`netsmith:BatteryCell_Coin_D20.0mm_Tabs_P15.00mm`): a package of `std::sources::Cell`, a
//! 2032-size rechargeable cell (a LIR2032, say) sold with two tabs welded on (issue 1399).
//!
//! **netsmith's own land pattern, not a maker's drawing.** KiCad has no footprint of this name,
//! and the land is not read from any part's drawing: the owner decided on issue 1399 that netsmith
//! draws it. It is a file of netsmith's own library (`std/libs/netsmith.pretty/`), which
//! [`file`] writes and a test holds the committed file to, not a generated name of a KiCad
//! library, so a board opened in KiCad finds it in the library beside the board. The name gives the cell (20 mm across) and the pitch (15 mm) between the two tab
//! pins. Pad 1, the positive tab, is at the origin with square (rounded) corners, pad 2, the
//! negative one, the pitch to its right, round; each is a 1.0 mm hole in a 2.0 mm pad, which takes
//! a tab pin up to about 0.8 mm across. The cell is centred between the two pins, so both stand
//! under it, as a tab bent down from the rim does. The courtyard is the cell's circle, 0.25 mm
//! out; the fabrication layer draws the cell and a plus sign at the positive pin, the silkscreen
//! the cell outside its outline. A real part's tabs may stand elsewhere (makers differ in where
//! the positive tab comes down and how far apart the pins are): compare its drawing with this land
//! before a board is ordered, and give a part whose pins differ a footprint of the project's own
//! (`kicad.toml`).

use super::draw::{Draw, Drawing, Kind, PadSpec, Shape};
use crate::footprint::Mount;
use crate::geometry::Pt;

const NAME: &str = "BatteryCell_Coin_D20.0mm_Tabs_P15.00mm";
const DIAMETER: f64 = 20.0;
const PITCH: f64 = 15.0;
const PAD: f64 = 2.0;
const DRILL: f64 = 1.0;

/// The file of netsmith's own library: its name and the text of its `.kicad_mod`.
pub(super) fn file() -> (String, String) {
    let (d, below) = drawing();
    (NAME.to_string(), d.text(-below, below))
}

fn drawing() -> (Drawing, f64) {
    let mut d = Drawing::new(netsmith_kicad::STDLIB_NAME, NAME, Mount::ThroughHole);
    // No 3D model: KiCad has none of this name.
    d.without_model();
    d.describe(
        "Coin cell with solder tabs, 20 mm across, 15 mm between the tab pins, lying flat; \
         netsmith's own land pattern, not from a maker's drawing",
        "battery cell coin tabs THT",
    );
    for (number, shape, x) in [(1, Shape::RoundRect, 0.0), (2, Shape::Circle, PITCH)] {
        d.pad(&PadSpec::new(
            number,
            Kind::Through(DRILL),
            shape,
            Pt::new(x, 0.0),
            (PAD, PAD),
        ));
    }
    let centre = Pt::new(PITCH / 2.0, 0.0);
    let radius = DIAMETER / 2.0;
    d.circle_at(Draw::Courtyard, centre, radius + 0.25);
    d.circle_at(Draw::Fab, centre, radius);
    d.circle_at(Draw::Silk, centre, radius + 0.12);
    // A plus sign on the fabrication layer, beside the positive pin, inside the cell.
    let plus = Pt::new(PAD / 2.0 + 0.8, -PAD / 2.0 - 0.6);
    let arm = 0.5;
    d.line(
        Draw::Fab,
        Pt::new(plus.x - arm, plus.y),
        Pt::new(plus.x + arm, plus.y),
    );
    d.line(
        Draw::Fab,
        Pt::new(plus.x, plus.y - arm),
        Pt::new(plus.x, plus.y + arm),
    );
    d.fab_reference_at(centre, 1.0);
    (d, radius + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::footprint::{Footprint, PadKind};

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn the_coin_cell_has_its_two_tab_pins_at_the_pitch_under_the_cell() {
        let (name, text) = file();
        let fp = Footprint::parse(&format!("netsmith:{name}"), &text).unwrap();
        assert_eq!(fp.pad_numbers(), ["1", "2"]);
        assert!(near(fp.pads[0].at.x, 0.0) && near(fp.pads[1].at.x, PITCH));
        for pad in &fp.pads {
            assert_eq!(pad.kind, PadKind::ThruHole);
            assert!(near(pad.drill.unwrap().width, DRILL));
            assert!(near(pad.width, PAD));
        }
        let r = DIAMETER / 2.0 + 0.25;
        assert!(near(fp.courtyard.min.x, PITCH / 2.0 - r));
        assert!(near(fp.courtyard.max.x, PITCH / 2.0 + r));
        assert!(near(fp.courtyard.max.y, r));
    }
}
