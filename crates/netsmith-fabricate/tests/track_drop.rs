//! The `track drop` guideline (issue #652): `fab` measures the voltage a power net's routed
//! tracks drop at its current and warns, or stops with strict guidelines, when it is more than
//! 1 % of the rail's voltage.

use netsmith_testkit::TempDir;
use std::fs;

use netsmith_fabricate::fab::{FabOptions, Fabricated, fab_with_options};
use netsmith_pipeline::Libraries;
use netsmith_solve::Catalogs;

/// 2 A from a 3.3 V supply's header to a load's header, the tracks sized for heat (0.98 mm).
const FEED: &str = "\
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Feed {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .load: Rail {
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
        .current &= 0A..2A
    }
    .source: Rail {
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
        .voltage &= 3.3V
        .current &= -2A..0A
    }
    .supply: Header1x2
    .out: Header1x2
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}
";

const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Feed {
    case {
        .outline: Outline {
            .width &= 80mm
            .height &= 20mm
        }
    }
}
";

/// `fab` of [`FEED`] with the two headers `apart` mm apart.
fn run(name: &str, apart: f64, strict: bool) -> (TempDir, Result<Vec<Fabricated>, String>) {
    let dir = TempDir::unused(&format!("track-drop-{name}"));
    let layout = dir.join("source").join("feed.pcb.toml");
    fs::create_dir_all(layout.parent().unwrap()).unwrap();
    fs::write(
        &layout,
        format!(
            "version = 1\n\n[Feed.parts.supply]\nat = [10.0, 10.0]\n\n[Feed.parts.out]\nat = [{}, 10.0]\n",
            10.0 + apart
        ),
    )
    .unwrap();
    let made = fab_with_options(
        "feed.nsm",
        FEED,
        &netsmith_check::Loader::std().with_physical("feed.nsm", "feed.physical.nsm", PHYSICAL),
        &dir.join("fab"),
        &Catalogs::jlcpcb().with_strict_guidelines(strict),
        &Libraries::standard(),
        None,
        &FabOptions {
            layout: Some(layout),
            ..FabOptions::default()
        },
    );
    (dir, made)
}

#[test]
fn a_long_power_track_is_a_warning_in_the_fab_notes() {
    // 2 A through at least 55 mm of 0.98 mm in 1 oz drops at least 0.055 V, 1.7 % of 3.3 V.
    let (_dir, made) = run("far", 55.0, false);
    let made = made.expect("a guideline does not stop fab");
    let notes = &made[0].notes;
    let line = notes
        .lines()
        .find(|l| l.contains("track drop guideline"))
        .unwrap_or_else(|| panic!("no finding in:\n{notes}"));
    assert!(
        line.starts_with("warning: Feed: track drop guideline: net `3V3` drops up to ")
            && line.contains("at 2 A (")
            && line.contains("of its 3.3 V; the budget is 1%)")
            && line.contains("0.98 mm at its narrowest, in 0.035 mm copper")
            && line.contains("give a rail on the net `.min_track &= "),
        "{line}"
    );
    // A ground has no voltage of its own, so its drop is not judged.
    assert!(!notes.contains("net `GND` drops"), "{notes}");
}

#[test]
fn a_short_power_track_has_no_finding() {
    // About 10 mm drops about 0.01 V, 0.3 % of 3.3 V.
    let (_dir, made) = run("near", 10.0, false);
    let made = made.expect("fab succeeds");
    assert!(
        !made[0].notes.contains("track drop"),
        "{}",
        made[0].notes
    );
}

#[test]
fn with_strict_guidelines_a_long_power_track_stops_fab() {
    let (dir, made) = run("strict", 55.0, true);
    let err = made.expect_err("strict guidelines refuse the drop");
    assert!(
        err.contains("error: Feed: track drop guideline: net `3V3` drops up to ")
            && err.contains("this is an error because guidelines are strict"),
        "{err}"
    );
    assert!(
        !dir.join("fab").join("Feed").join("fab-notes.txt").exists(),
        "no package is written"
    );
}
