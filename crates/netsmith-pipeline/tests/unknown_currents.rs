//! A net with two or more unknown currents stays unsolved, and the build says so (issue #796).
//! Every value here is invented.

use netsmith_check::Loader;
use netsmith_pipeline::{Built, Libraries, build_with};
use netsmith_solve::Catalogs;

/// A supply and a load on one net, with `extra` in the board.
fn pair(extra: &str) -> String {
    format!(
        r#"import std::mfg::jlcpcb::TwoLayer
import std::sources::Rail

board Pair {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .load: Rail {{
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .supply ~ .load{extra}
}}
"#
    )
}

fn built(src: &str) -> Built {
    build_with(
        "t.nsm",
        src,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0)
}

fn unknown(b: &Built) -> Vec<&String> {
    b.notes
        .iter()
        .filter(|n| n.contains("unknown currents"))
        .collect()
}

#[test]
fn two_unknown_currents_on_a_net_are_named_in_a_note() {
    let b = built(&pair(""));
    assert_eq!(
        unknown(&b),
        ["net `VIN` has 2 unknown currents, `load.current`, `supply.current`, so none is worked \
          out from the net: state all but one of them"],
        "{:#?}",
        b.notes
    );
}

#[test]
fn stating_a_current_leaves_one_unknown_and_the_note_goes() {
    let b = built(&pair("\n    .load.current &= 0A..10mA"));
    assert!(unknown(&b).is_empty(), "{:#?}", b.notes);
}
