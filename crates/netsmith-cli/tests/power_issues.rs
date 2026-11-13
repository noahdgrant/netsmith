//! What the power model finds wrong (issue #187) is reported where the other findings are:
//! by `netsmith build` (an overdrawn supply stops it, the rest are notes), by the analysis the
//! editor serves, and in the power tree the page draws. The page does not work any of it out.
//!
//! Each kind has an invented small board: a rail that can draw more than its supply can give, a
//! rail nothing supplies, a rail with a load whose current the solver did not settle, and a
//! converter whose efficiency it did not settle.

use netsmith_testkit::TempDir;
use std::{fs, path::Path, process::Command};

/// The parts of the boards: a load, and a converter that makes `5V` of what it is given. The
/// converter says what it can give (`max_current`); `Converter` also states that its output is
/// at most that, which the solver checks, and `Loose` does not, as a part that only says what
/// it can give may not.
fn parts() -> String {
    let converter = |name: &str, relation: &str| {
        format!(
            "part {name} {{
    .vin: Rail
    .vout: Rail
    .max_current: Current
    .efficiency: Number
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current * .vout.voltage / (.vin.voltage * .efficiency)
{relation}
    pins {{
        .vin.pos  power \"VIN\"  \"1\"
        .vin.neg  power \"GND\"  \"2\"
        .vout.pos power \"VOUT\" \"3\"
    }}
}}
"
        )
    };
    format!(
        "import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

part Load {{
    .power: Rail

    pins {{
        .power.pos power \"VCC\" \"1\"
        .power.neg power \"GND\" \"2\"
    }}
}}

{}
{}",
        converter("Converter", "    -.vout.current <= .max_current"),
        converter("Loose", "")
    )
}

/// A converter (`kind`) of at most 100 mA from 12 V to 5 V, and a load on its output that draws
/// `load`. `more` is more of the converter's body: its efficiency, say.
fn supplied(kind: &str, load: &str, more: &str) -> String {
    format!(
        "{}
board Bench {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .input: Rail {{
        .voltage &= 12V
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }}
    .stage: {kind} {{
        .vout.voltage &= 5V
        .max_current &= 100mA
        {more}
        .vout.pos.net &= \"V5\"
        .vout.neg.net &= \"GND\"
    }}
    .load: Load {{ .power.current &= {load} }}
    .input ~ .stage.vin
    .stage.vout ~ .load.power
}}
",
        parts()
    )
}

const SETTLED: &str = ".efficiency &= 0.8..0.9";

/// A load that nothing supplies, on a connector, so that it builds into a schematic.
fn orphan() -> String {
    "import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Bench {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .load: Rail {
        .pos.net &= \"V5\"
        .neg.net &= \"GND\"
        .current &= 0mA..10mA
        .min_track &= 1mm..1m
    }
    .out: Header1x2
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}
"
    .to_string()
}

// ---- the command line ----------------------------------------------------------------

fn project(name: &str, src: &str) -> TempDir {
    let root = TempDir::new(&format!("power-issues-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"bench\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), src).unwrap();
    root
}

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_KICAD_SYMBOLS", netsmith_testkit::sample_symbols())
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_testkit::sample_footprints(),
        )
        .output()
        .unwrap()
}

#[test]
fn build_fails_on_an_overdrawn_supply_and_says_where_and_how_much() {
    let root = project("over", &supplied("Loose", "0mA..200mA", SETTLED));
    for args in [&["build"][..], &["build", "--strict"][..]] {
        let run = netsmith(&root, args);
        let said = String::from_utf8_lossy(&run.stderr);
        assert!(!run.status.success(), "{args:?}: {said}");
        assert!(
            said.contains(
                "error: Bench: V5 can draw up to 200 mA, more than the 100 mA its supply can give"
            ) && said.contains("--> src/main.nsm:"),
            "{said}"
        );
    }
}

#[test]
fn build_prints_the_notes_and_still_succeeds_even_with_strict() {
    let root = project("note", &orphan());
    for args in [&["build"][..], &["build", "--strict"][..]] {
        let run = netsmith(&root, args);
        let said = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{args:?}: {said}");
        assert!(
            said.contains("note: Bench: V5 has no source: nothing supplies it"),
            "{said}"
        );
    }
}

// ---- the analysis the editor serves ------------------------------------------------------
