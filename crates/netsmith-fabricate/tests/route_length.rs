//! The longest a net may be routed, stated in a part's `layout` block (`route NAME .pin length
//! LENGTH`, issue #1194), end to end: `fab` of a board whose oscillator's datasheet keeps each
//! crystal line under 15 mm. With the load capacitor of XOUT placed 25 mm off by the layout
//! file, that line is longer than the limit and the other is not: the warning names the line,
//! its length and the limit, and the line within it has none. The parts, their datasheet and
//! their footprints (netsmith's generated SOT-23, so no KiCad is needed) are invented.

use std::fs;

use netsmith_fabricate::fab::{FabOptions, Fabricated, fab_with_options};
use netsmith_kicad::Search;
use netsmith_pipeline::Libraries;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const CLOCK: &str = r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Capacitor
import std::sources::Rail

/// An invented oscillator whose datasheet keeps the lines to its crystal short.
part Osc9 {
    .designator &= "U"
    .supply: Rail
    pins {
        .xin        passive "XIN"  "1"
        .supply.neg power   "GND"  "2"
        .xout       passive "XOUT" "3"
        .enable     passive "EN"   "4"
        .supply.pos power   "VDD"  "5"
    }
    layout {
        source "OSC9 datasheet, 8.2 Layout, p.14"
        /// "Keep each crystal line under 15 mm, its load capacitor included."
        route xin .xin length 15mm
        route xout .xout length 15mm
    }
}

/// An invented crystal, its case on a third pad.
part Xt9 {
    .designator &= "Y"
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
        .case passive "GND" "3"
    }
}

board Clock {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .source: Rail {
        .voltage &= 3.3V
        .pos.net &= "VDD"
        .neg.net &= "GND"
    }
    .feed: Header1x2
    .osc: Osc9 {
        .assembly &= AssemblyMode::Hand
    }
    .xtal: Xt9 {
        .assembly &= AssemblyMode::Hand
    }
    .c1: Capacitor { .capacitance &= 1uF +/- 20% }
    .c2: Capacitor { .capacitance &= 1uF +/- 20% }
    .feed.p1 ~ .source.pos
    .feed.p2 ~ .source.neg
    .source ~ .osc.supply
    .osc.xin ~ .xtal.a
    .osc.xout ~ .xtal.b
    .osc.xin ~> .c1 ~> .osc.supply.neg
    .osc.xout ~> .c2 ~> .osc.supply.neg
}
"#;

const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Clock {
    case {
        .outline: Outline {
            .width &= 50mm
            .height &= 25mm
        }
    }
}
";

/// The footprints of the invented parts, generated: the project's own rows for their kinds.
const KICAD: &str = "\
[[part]]
kind = \"Osc9\"
package = \"SOT-23-5\"
default = true
footprint = \"gen:SOT23(pins=5)\"

[[part]]
kind = \"Xt9\"
package = \"SOT-23\"
default = true
footprint = \"gen:SOT23(pins=3)\"
";

/// `fab` of [`CLOCK`] with the oscillator, the crystal and C1 close together and C2 at `c2`.
fn run(name: &str, c2: (f64, f64)) -> (TempDir, Fabricated) {
    let dir = TempDir::unused(&format!("route-length-{name}"));
    let project = dir.join("source");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("netsmith.toml"),
        "[project]\nname = \"clock\"\ntop_level = [\"clock.nsm\"]\n",
    )
    .unwrap();
    fs::write(project.join("kicad.toml"), KICAD).unwrap();
    let layout = project.join("clock.pcb.toml");
    let at = |part: &str, (x, y): (f64, f64)| {
        format!("[Clock.parts.{part}]\nat = [{x:.1}, {y:.1}]\nlock = true\n\n")
    };
    fs::write(
        &layout,
        format!(
            "version = 1\n\n{}{}{}{}{}",
            at("feed", (4.0, 20.0)),
            at("osc", (12.0, 12.0)),
            at("xtal", (17.0, 12.0)),
            at("c1", (17.0, 16.0)),
            at("c2", c2),
        ),
    )
    .unwrap();
    let empty = dir.join("no-kicad");
    fs::create_dir_all(&empty).unwrap();
    let libraries =
        Libraries::for_project_by(Some(&project), &Search::under(&empty).with_no_kicad()).unwrap();
    let made = fab_with_options(
        "clock.nsm",
        CLOCK,
        &netsmith_check::Loader::std().with_physical("clock.nsm", "clock.physical.nsm", PHYSICAL),
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &libraries,
        None,
        &FabOptions {
            layout: Some(layout),
            ..FabOptions::default()
        },
    )
    .unwrap_or_else(|e| panic!("a guideline does not stop fab: {e}"));
    (dir, made.into_iter().next().unwrap())
}

/// The lines of the notes that are the `route` guideline's.
fn route_lines(notes: &str) -> Vec<&str> {
    notes
        .lines()
        .filter(|l| l.contains("layout guideline") && l.contains("the tracks of the net"))
        .collect()
}

#[test]
fn a_crystal_line_longer_than_the_datasheet_allows_is_a_warning_that_names_it() {
    let (_dir, made) = run("far", (42.0, 20.0));
    let lines = route_lines(&made.notes);
    assert_eq!(lines.len(), 1, "{}", made.notes);
    let l = lines[0];
    assert!(
        l.starts_with("warning: Clock: layout guideline: `osc` (U1) asks for `xout`: the tracks of the net of its pin `.xout` (pads 3) at most 15 mm long in all, and the tracks of `")
            && l.contains("(OSC9 datasheet, 8.2 Layout, p.14; clock.nsm:")
            && l.ends_with(": move the parts on the net nearer U1, or route it shorter"),
        "{l}"
    );
    // C2 is 25 mm from the crystal: the line is at least that long, and the warning says so.
    let length: f64 = l
        .split("` are ")
        .nth(1)
        .and_then(|r| r.split(" mm").next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("no length in {l}"));
    assert!(length > 25.0, "{l}");
    // The fab notes and the data of the run list the same nets.
    assert_eq!(
        made.notes.contains("\nrouted long:"),
        !made.routes.long.is_empty(),
        "{}",
        made.notes
    );
}

#[test]
fn crystal_lines_within_the_limit_have_no_warning() {
    let (_dir, made) = run("near", (17.0, 8.0));
    assert!(route_lines(&made.notes).is_empty(), "{}", made.notes);
}
