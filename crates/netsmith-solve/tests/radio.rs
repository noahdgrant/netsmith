//! A radio's certification against the antenna the board connects (issue #1278):
//! `std::radio::Radio` and `Antenna`, the antenna's gain held against the grant through the RF
//! path, and the other conditions listed for a person. The parts are invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, radio::verify_lines, solve};

/// The parts: a radio whose grant allows 2 dBi in 2.4 GHz and 5 dBi in 5 GHz, a two-pin link,
/// a shunt part to ground, an antenna of `gain` dBi in 2.4 GHz, and a coax connector.
fn parts(gain: &str) -> String {
    format!(
        "import std::radio::{{Antenna, AntennaBand, AntennaType, Radio, RadioBand}}
import std::sources::Rail
import std::parts::connector::Coax

part Module {{
    .vdd: Rail
    .radio: Radio {{
        .grant &= \"FCC ID 2ABCD-EXAMPLE\"
        .antenna_type &= AntennaType::Chip
        .exposure_distance &= 20mm
        .colocation &= \"no other transmitter within 20 cm\"
        .low: RadioBand {{ .band &= 2.4GHz..2.4835GHz; .max_antenna_gain &= 2 }}
        .high: RadioBand {{ .band &= 5.15GHz..5.85GHz; .max_antenna_gain &= 5 }}
    }}

    pins {{
        .vdd.pos  power   \"VDD\" \"1\"
        .vdd.neg  power   \"GND\" \"2\"
        .radio.rf passive \"RF\"  \"3\"
    }}
}}

part Link {{
    .designator &= \"R\"
    pins {{
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }}
}}

part ChipAntenna {{
    .designator &= \"AE\"
    .antenna: Antenna {{
        .type &= AntennaType::Chip
        .low: AntennaBand {{ .band &= 2.4GHz..2.5GHz; .gain &= {gain} }}
    }}
    pins {{
        .antenna.feed passive \"FEED\" \"1\"
        .gnd          passive \"GND\"  \"2\"
    }}
}}
"
    )
}

fn solved(src: &str) -> Solution {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn checked(src: &str) -> netsmith_check::Model {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    c.boards[0].model.clone()
}

fn radio_errors(s: &Solution) -> Vec<String> {
    s.diagnostics
        .iter()
        .map(|d| d.message.clone())
        .filter(|m| m.contains("radio"))
        .collect()
}

fn radio_notes(s: &Solution) -> Vec<String> {
    s.not_evaluated
        .iter()
        .map(|n| n.message.clone())
        .filter(|m| m.contains("radio"))
        .collect()
}

/// The board: the radio's RF pin through a link, a shunt to ground, and on to the antenna.
fn board(gain: &str) -> String {
    format!(
        "{}
board B {{
    .supply: Rail {{ .voltage &= 3.3V +/- 5%; .pos.net &= \"VDD\" }}
    .u1: Module
    .link: Link
    .shunt: Link
    .ae1: ChipAntenna
    .supply ~ .u1.vdd
    .u1.radio.rf ~ .link.a
    .link.b ~ .ae1.antenna.feed
    .shunt.a ~ .link.b
    .shunt.b ~ .u1.vdd.neg
    .ae1.gnd ~ .u1.vdd.neg
}}
",
        parts(gain)
    )
}

#[test]
fn an_antenna_within_the_grant_passes() {
    let s = solved(&board("1.5"));
    assert!(radio_errors(&s).is_empty(), "{:?}", radio_errors(&s));
    // The antenna states nothing in 5 GHz, so that band is not judged.
    let notes = radio_notes(&s);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].contains("`ae1.antenna` on the RF path of the radio `u1.radio` is not judged against `u1.radio.high`"),
        "{notes:?}"
    );
}

#[test]
fn an_antenna_over_the_grant_is_an_error_that_names_both_figures() {
    let s = solved(&board("3.2"));
    let errors = radio_errors(&s);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0],
        "the antenna `ae1.antenna` on the RF path of the radio `u1.radio` has a gain of 3.2 dBi in 2.4GHz to 2.4835GHz (`ae1.antenna.low.gain`), above the 2 dBi the radio's grant allows there (`u1.radio.low.max_antenna_gain`), through `link`: use an antenna with no more gain, or a radio whose grant allows it"
    );
}

#[test]
fn the_path_does_not_go_through_ground() {
    // An antenna joined only through the shunt's ground side is not on the path.
    let src = format!(
        "{}
board B {{
    .supply: Rail {{ .voltage &= 3.3V +/- 5%; .pos.net &= \"VDD\" }}
    .u1: Module
    .shunt: Link
    .ae1: ChipAntenna
    .supply ~ .u1.vdd
    .u1.radio.rf ~ .shunt.a
    .shunt.b ~ .u1.vdd.neg
    .ae1.antenna.feed ~ .u1.vdd.neg
    .ae1.gnd ~ .u1.vdd.neg
}}
",
        parts("9")
    );
    let s = solved(&src);
    assert!(radio_errors(&s).is_empty(), "{:?}", radio_errors(&s));
    let notes = radio_notes(&s);
    assert!(
        notes
            .iter()
            .any(|n| n.contains("reaches no antenna part")),
        "{notes:?}"
    );
}

#[test]
fn a_path_to_a_connector_leaves_the_gain_to_a_person() {
    let src = format!(
        "{}
board B {{
    .supply: Rail {{ .voltage &= 3.3V +/- 5%; .pos.net &= \"VDD\" }}
    .u1: Module
    .j1: Coax
    .supply ~ .u1.vdd
    .u1.radio.rf ~ .j1.signal
    .j1.shield ~ .u1.vdd.neg
}}
",
        parts("1")
    );
    let s = solved(&src);
    assert!(radio_errors(&s).is_empty(), "{:?}", radio_errors(&s));
    let notes = radio_notes(&s);
    assert!(
        notes.iter().any(|n| n
            == "the antenna gain of the radio `u1.radio` is not judged: the RF path from `u1.radio.rf` ends at the connector `j1` and reaches no antenna part, so the antenna on its cable is a person's to verify"),
        "{notes:?}"
    );
    let lines = verify_lines(&checked(&src));
    assert!(
        lines.contains(&"verify: radio `u1.radio` (FCC ID 2ABCD-EXAMPLE): a person must verify that the antenna (connected at `j1`) has no more than 2 dBi of gain in 2.4GHz to 2.4835GHz".to_string()),
        "{lines:#?}"
    );
}

#[test]
fn the_notes_list_the_conditions_a_person_must_verify() {
    let lines = verify_lines(&checked(&board("1.5")));
    assert_eq!(
        lines,
        [
            "verify: radio `u1.radio` (FCC ID 2ABCD-EXAMPLE): a person must verify that the antenna is of the type the grant was tested with, Chip (`ae1.antenna` is Chip)",
            "verify: radio `u1.radio` (FCC ID 2ABCD-EXAMPLE): a person must verify that the product keeps a person at least 20mm from the antenna (the grant's exposure condition)",
            "verify: radio `u1.radio` (FCC ID 2ABCD-EXAMPLE): a person must verify the limits on radios beside it (co-location): no other transmitter within 20 cm",
        ]
    );
}

#[test]
fn a_module_with_its_own_antenna_has_only_its_conditions_listed() {
    let src = "import std::radio::{AntennaType, Radio}
import std::sources::Rail

part Module {
    .vdd: Rail
    .radio: Radio {
        .grant &= \"FCC ID 2ABCD-ONBOARD\"
        .antenna_type &= AntennaType::PcbTrace
        .reference_design &= \"keep the copper clear of the antenna's end (datasheet 4.2)\"
    }
    pins {
        .vdd.pos power \"VDD\" \"1\"
        .vdd.neg power \"GND\" \"2\"
    }
}

board B {
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= \"VDD\" }
    .u1: Module
    .supply ~ .u1.vdd
}
";
    let s = solved(src);
    assert!(radio_errors(&s).is_empty(), "{:?}", radio_errors(&s));
    assert!(radio_notes(&s).is_empty(), "{:?}", radio_notes(&s));
    assert_eq!(
        verify_lines(&checked(src)),
        [
            "verify: radio `u1.radio` (FCC ID 2ABCD-ONBOARD): a person must verify that the antenna is of the type the grant was tested with, PcbTrace",
            "verify: radio `u1.radio` (FCC ID 2ABCD-ONBOARD): a person must verify that the trace to the antenna follows the reference design the grant requires: keep the copper clear of the antenna's end (datasheet 4.2)",
        ]
    );
}
