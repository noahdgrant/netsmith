//! A board's `.corner` (issue #1508): how the tracks of every net of it turn, a
//! `std::sources::TrackCorner` the board declares, `Rounded` when it states none.

use netsmith_check::{Loader, check};
use netsmith_core::TrackCorner;
use netsmith_design::{Designed, design_board};
use netsmith_solve::{Catalogs, solve};

fn designed(lines: &str) -> Result<Designed, Vec<String>> {
    let source = format!(
        "import std::mfg::jlcpcb::TwoLayer\nimport std::parts::connector::Header1x2\nimport std::sources::TrackCorner\n\nboard B {{\n    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}\n    .j: Header1x2\n{lines}}}\n"
    );
    let c = check("t.nsm", &source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    design_board(&c.boards[0], &s)
}

fn corner(lines: &str) -> TrackCorner {
    designed(lines)
        .unwrap_or_else(|e| panic!("{e:#?}"))
        .design
        .features
        .corner
}

#[test]
fn a_board_that_states_no_corner_is_rounded() {
    assert_eq!(corner(""), TrackCorner::Rounded);
    assert_eq!(corner("    .corner: TrackCorner\n"), TrackCorner::Rounded);
}

#[test]
fn a_board_states_its_corner_with_a_variant_of_track_corner() {
    assert_eq!(
        corner("    .corner: TrackCorner\n    .corner &= TrackCorner::Mitered\n"),
        TrackCorner::Mitered
    );
    assert_eq!(
        corner("    .corner: TrackCorner\n    .corner &= TrackCorner::Rounded\n"),
        TrackCorner::Rounded
    );
}

#[test]
fn a_corner_of_another_type_says_what_to_write() {
    let Err(errors) = designed("    .corner: String\n    .corner &= \"mitered\"\n") else {
        panic!("a `.corner` of text is refused");
    };
    assert!(
        errors.iter().any(|e| e.contains("`corner` is not a `TrackCorner`")
            && e.contains(".corner &= TrackCorner::Mitered")),
        "{errors:#?}"
    );
}
