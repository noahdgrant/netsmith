//! The board exactly as the layout file has it, with nothing placed (issue #1002):
//! [`Board::as_laid_out`], which `netsmith layout fit --as-laid-out` queries.
//!
//! The placer is what takes the time of a build; a question about one part (`layout fit`) needs
//! only the other parts where they are. When every part has a place in the layout file (or a
//! requirement of the physical file pins it) and the physical file gives the outline's size, the
//! board is made of those places alone, in milliseconds, and the placement check and the fit
//! query run on it as they run on a placed board.
//!
//! A board that is short of a place or of a size is refused, with what it lacks: the board
//! cannot be known without placing it, and a guess would be an answer about a board nobody has.
//!
//! [`Board::laid_out_entries`] is the board of what the layout file decides, whatever it lacks
//! (issue #1224): the parts it places, each where it says, and no other. `netsmith layout check`
//! runs the placement check `fab` runs ([`Board::placement_faults`]) on it, so that a fault among
//! those parts, which no seed of the placer changes, is found before anything is placed.

use netsmith_core::Design;
use netsmith_kicad::Mapping;

use crate::board::{Board, outline_box};
use crate::footprint::FootprintLibrary;
use crate::layout::{Layout, apply_layout};
use crate::placement::{ORIGIN, PlaceOptions};

/// How many parts a refusal names before it says how many more.
const NAMED: usize = 6;

impl Board {
    /// The board of `design` with each part where `layout` (the layout file and its name for
    /// the messages) puts it, or where a requirement of the physical file pins it, and the
    /// outline the design states: nothing is placed. `options` are the placer's
    /// ([`place_options`](crate::place_options)): they tell which requirements pin a part.
    ///
    /// Refused, with the reason: a design that does not make a board ([`Board::unplaced`]), an
    /// outline whose size the design does not state exactly (a width or height left open or
    /// given as a range is only known by placing), a layout entry that is wrong, and a part
    /// that has no place: no entry in the layout file and no requirement that pins it.
    pub fn as_laid_out(
        design: &Design,
        mapping: &Mapping,
        library: &FootprintLibrary,
        options: &PlaceOptions,
        layout: Option<&(Layout, String)>,
    ) -> Result<Board, String> {
        let (rect, exact, ranged) = outline_box(design);
        if !exact || ranged {
            return Err(format!(
                "`{}` does not state its outline's size exactly (a width or a height that is open or a range is known only by placing the board)",
                design.name
            ));
        }
        let mut u =
            Board::unplaced(design, mapping, library, options).map_err(|e| e.to_string())?;
        let lacking = put_decided(&mut u, design, layout);
        if !lacking.is_empty() {
            let shown: Vec<String> = lacking
                .iter()
                .take(NAMED)
                .map(|p| format!("`{p}`"))
                .collect();
            let more = match lacking.len().saturating_sub(NAMED) {
                0 => String::new(),
                n => format!(" and {n} more"),
            };
            return Err(format!(
                "the layout file has no place for {}{more}, and no requirement pins {}: the board as the layout file has it is not known",
                shown.join(", "),
                if lacking.len() == 1 { "it" } else { "them" }
            ));
        }
        let corner_radius = design
            .features
            .outline
            .as_ref()
            .map_or(0.0, |o| o.corner_radius);
        let mut board = u.into_board(design, rect, corner_radius);
        if let Some((layout, file)) = layout {
            apply_layout(&mut board, layout, file).map_err(|e| e.to_string())?;
        }
        Ok(board)
    }

    /// The board of what `layout` decides: each part it places (or a requirement of the physical
    /// file pins) where it says, and the parts the placer places left out, on the outline the
    /// design states. Nothing is placed, and a part with no place is no refusal: it is not on
    /// this board. An outline whose size is a range is as big as the range allows, and a size
    /// the design leaves open is a metre, so that only an edge every size has (the left and the
    /// top, and the others of a size that is stated) is measured.
    ///
    /// The placement check ([`Board::placement_faults`]) on this board finds what it finds on
    /// the placed board about these parts among themselves and against the outline, the
    /// keepouts and the cutouts: `fab` puts every part with an entry where the entry says,
    /// whatever the seed, so these are the faults the layout file makes (issue #1224).
    ///
    /// Refused, with the reason: a design that does not make a board ([`Board::unplaced`]) and a
    /// layout entry that is wrong.
    pub fn laid_out_entries(
        design: &Design,
        mapping: &Mapping,
        library: &FootprintLibrary,
        options: &PlaceOptions,
        layout: Option<&(Layout, String)>,
    ) -> Result<Board, String> {
        let (rect, _, _) = outline_box(design);
        let open = |v: f64| {
            if v.is_finite() {
                v
            } else {
                ORIGIN.x + OPEN_SIZE
            }
        };
        let rect = crate::geometry::Rect::new(
            rect.min,
            crate::geometry::Pt::new(open(rect.max.x), open(rect.max.y)),
        );
        let mut u =
            Board::unplaced(design, mapping, library, options).map_err(|e| e.to_string())?;
        let lacking = put_decided(&mut u, design, layout);
        u.parts.retain(|p| !lacking.contains(&p.path));
        let corner_radius = design
            .features
            .outline
            .as_ref()
            .map_or(0.0, |o| o.corner_radius);
        let mut board = u.into_board(design, rect, corner_radius);
        if let Some((layout, file)) = layout {
            apply_layout(&mut board, layout, file).map_err(|e| e.to_string())?;
        }
        Ok(board)
    }
}

/// The size of an outline the design leaves open, for [`Board::laid_out_entries`], in mm.
const OPEN_SIZE: f64 = 1000.0;

/// Put each part of `u` that the layout file locks, or a requirement pins, where it says (the
/// other entries are put on the board afterwards, as for a placed board), and give the paths of
/// the parts that have no place: no entry and no requirement that pins them.
fn put_decided(
    u: &mut crate::board::Unplaced,
    design: &Design,
    layout: Option<&(Layout, String)>,
) -> Vec<String> {
    let entries = layout.and_then(|(l, _)| l.boards.get(&design.name));
    let mut lacking = Vec::new();
    for part in &mut u.parts {
        if let Some(f) = u.placement.fixed.get(&part.refdes) {
            part.at = ORIGIN + f.at;
            part.rotation = f.rotation;
            part.side = f.side;
            part.reference_side = f.text;
        } else if !entries.is_some_and(|e| e.parts.contains_key(&part.path)) {
            lacking.push(part.path.clone());
        }
    }
    lacking
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::tests::{fixtures, led_design};
    use crate::part_fit::FitNear;
    use crate::place_options;

    /// The LED board with the paths of its parts their designators in lower case, as a layout
    /// names them, and a 40 x 30 mm outline.
    fn design() -> Design {
        let mut design = led_design();
        for p in &mut design.parts {
            p.path = p.refdes.to_lowercase();
        }
        design.features.outline = Some(netsmith_core::Outline {
            width: (40.0, 40.0),
            height: (30.0, 30.0),
            corner_radius: 0.0,
            polygon: None,
        });
        design
    }

    fn layout(text: &str) -> (Layout, String) {
        (
            Layout::parse(text, "led.pcb.toml").unwrap_or_else(|e| panic!("{e}")),
            "led.pcb.toml".to_string(),
        )
    }

    fn made(design: &Design, layout: Option<&(Layout, String)>) -> Result<Board, String> {
        let options = place_options(design, layout.map(|l| &l.0));
        Board::as_laid_out(design, &Mapping::standard(), &fixtures(), &options, layout)
    }

    const ALL: &str = "version = 1\n[LedExample.parts.d1]\nat = [6.0, 8.0]\n[LedExample.parts.j1]\nat = [30.0, 15.0]\nlock = true\n[LedExample.parts.r1]\nat = [14.0, 8.0]\nrotation = 90.0\nside = \"bottom\"\n";

    #[test]
    fn every_part_is_where_the_layout_file_puts_it_and_nothing_is_placed() {
        let design = design();
        let board = made(&design, Some(&layout(ALL))).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            (board.outline.width(), board.outline.height()),
            (40.0, 30.0)
        );
        let at = |refdes: &str| {
            let p = board.part(refdes).unwrap();
            let a = p.at - board.outline.min;
            ((a.x, a.y), p.rotation, p.side)
        };
        assert_eq!(at("D1"), ((6.0, 8.0), 0.0, crate::Side::Front));
        assert_eq!(at("J1"), ((30.0, 15.0), 0.0, crate::Side::Front));
        assert_eq!(at("R1"), ((14.0, 8.0), 90.0, crate::Side::Back));
    }

    #[test]
    fn a_fit_query_on_it_agrees_with_the_board_it_was_made_of() {
        let design = design();
        let board = made(&design, Some(&layout(ALL))).unwrap_or_else(|e| panic!("{e}"));
        let fit = board
            .fit_part(&design, "d1", &FitNear::Itself, 0.5, None)
            .unwrap();
        assert!(fit.now.fits, "{}", fit.text());
        assert_eq!(fit.now.at, [6.0, 8.0]);
        // The same answer on the board with the part moved away and back: the query only reads.
        assert_eq!(
            fit,
            board
                .clone()
                .fit_part(&design, "d1", &FitNear::Itself, 0.5, None)
                .unwrap()
        );
    }

    #[test]
    fn a_part_with_no_place_is_named_and_the_board_is_refused() {
        let design = design();
        let text = "version = 1\n[LedExample.parts.d1]\nat = [6.0, 8.0]\n";
        let e = made(&design, Some(&layout(text))).unwrap_err();
        assert!(
            e.contains("no place for `j1`, `r1`") && e.contains("no requirement pins them"),
            "{e}"
        );
        let e = made(&design, None).unwrap_err();
        assert!(e.contains("`d1`, `j1`, `r1`"), "{e}");
    }

    #[test]
    fn an_outline_that_is_not_stated_exactly_is_refused() {
        let mut design = design();
        design.features.outline = None;
        let e = made(&design, Some(&layout(ALL))).unwrap_err();
        assert!(
            e.contains("does not state its outline's size exactly"),
            "{e}"
        );
        let mut design = self::design();
        design.features.outline.as_mut().unwrap().width = (30.0, 40.0);
        let e = made(&design, Some(&layout(ALL))).unwrap_err();
        assert!(
            e.contains("does not state its outline's size exactly"),
            "{e}"
        );
    }

    #[test]
    fn a_layout_entry_for_a_part_the_board_lacks_is_the_layout_errors() {
        let design = design();
        let text = format!("{ALL}[LedExample.parts.u9]\nat = [1.0, 1.0]\n");
        let e = made(&design, Some(&layout(&text))).unwrap_err();
        assert!(e.contains("u9"), "{e}");
    }
}
