//! Requirements on where parts go (ADR 0045), from the source to the package: the placer
//! keeps to them, the layout file keeps the solution, the check measures them on the board
//! file with every number, and a violation stops `fab` unless the project says `warn`.
//! The numbers are an invented case drawing.

use netsmith_kicad::testlib::TempDir;
use std::fs;

use netsmith::Libraries;
use netsmith::fab::{FabOptions, Fabricated, fab_with_options};
use netsmith::project::Manifest;
use netsmith_check::Loader;
use netsmith_fab::KicadCliExe;
use netsmith_solve::Catalogs;

/// A 40 by 24 mm board whose supply header must sit on the left edge, turned, centred along
/// it, and whose LED must stay inside the right of the board.
const BOARD: &str = "\
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Widget {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .led: Led {
        .current &= 5mA..15mA
    }
    .r1: Resistor {}
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
";

/// The case drawing of `BOARD`, in its physical file (ADR 0048).
const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Widget {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }

    // Case drawing, slot A: the header's body is 1.5 mm in from the left edge, centred.
    .power_in.place.left &= 1.5mm +/- 0.1mm
    .power_in.place.y >= .outline.height / 2 - 0.1mm
    .power_in.place.y <= .outline.height / 2 + 0.1mm
    .power_in.place.rotation &= 90
    .led.place.right <= 38mm
    .led.place.top >= 3mm
}
";

fn out(name: &str) -> TempDir {
    TempDir::unused(&format!("constraints-test-{name}"))
}

/// `fab` of `BOARD` with `physical` as its physical file and `layout` as the layout file
/// (none if empty).
fn fab(
    name: &str,
    physical: &str,
    layout: &str,
    catalogs: &Catalogs,
) -> (TempDir, Result<Vec<Fabricated>, String>) {
    fab_waiving(name, physical, layout, catalogs, false)
}

/// [`fab`], with `--allow-drc-violations` as `allow`.
fn fab_waiving(
    name: &str,
    physical: &str,
    layout: &str,
    catalogs: &Catalogs,
    allow: bool,
) -> (TempDir, Result<Vec<Fabricated>, String>) {
    let dir = out(name);
    let file = dir.join("source").join("widget.pcb.toml");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    if !layout.is_empty() {
        fs::write(&file, layout).unwrap();
    }
    let made = fab_with_options(
        "widget.nsm",
        BOARD,
        &Loader::std().with_physical("widget.nsm", "widget.physical.nsm", physical),
        &dir.join("fab"),
        catalogs,
        &Libraries::standard(),
        None,
        &FabOptions {
            layout: Some(file),
            allow_drc_violations: allow,
            ..FabOptions::default()
        },
    );
    (dir, made)
}

fn have_libraries() -> bool {
    KicadCliExe::locate_runnable().is_some()
}

#[test]
fn the_placer_meets_the_requirements_and_the_package_says_so() {
    if !have_libraries() {
        return;
    }
    let (dir, made) = fab("met", PHYSICAL, "", &Catalogs::jlcpcb());
    let made = made.unwrap_or_else(|e| panic!("{e}"));
    assert!(
        made[0].notes.contains("design rules check: passed")
            && made[0].notes.contains("constraints: 5 of 5 satisfied"),
        "{}",
        made[0].notes
    );
    let pcb = fs::read_to_string(dir.join("fab/Widget/Widget.kicad_pcb")).unwrap();
    // The header is turned 90 degrees, and its origin is on the left of the board: the
    // body's left edge is 1.5 mm in.
    let header = pcb
        .split("(footprint ")
        .find(|f| f.contains("PinHeader_1x02"))
        .expect("the header");
    let at = header
        .lines()
        .find_map(|l| l.trim().strip_prefix("(at "))
        .expect("its place");
    assert!(at.trim_end_matches(')').ends_with(" 90"), "{at}");
}

#[test]
fn the_same_source_gives_the_same_board() {
    if !have_libraries() {
        return;
    }
    let read = |name: &str| {
        let (dir, made) = fab(name, PHYSICAL, "", &Catalogs::jlcpcb());
        made.unwrap_or_else(|e| panic!("{e}"));
        fs::read_to_string(dir.join("fab/Widget/Widget.kicad_pcb")).unwrap()
    };
    assert_eq!(read("same-a"), read("same-b"));
}

/// A layout file that moves the header by hand, without locking it.
const MOVED: &str = "version = 1\n[Widget.parts.power_in]\nat = [30.0, 5.0]\n";

#[test]
fn a_part_moved_by_hand_out_of_its_window_stops_fab_with_every_number() {
    if !have_libraries() {
        return;
    }
    let (dir, made) = fab("moved", PHYSICAL, MOVED, &Catalogs::jlcpcb());
    let e = made.expect_err("the header is nowhere near its window");
    assert!(
        e.contains("error [constraint] constraint power_in.place.left (widget.physical.nsm:")
            && e.contains("the left edge of the body of J1 (F.Fab) is 28.73 mm from the left edge of the outline, and it allows 1.4 to 1.6 mm (27.13 mm over)")
            && e.contains("the centre of the body of J1 (F.Fab) is 6.27 mm from the top edge of the outline, and it allows 11.9 to 12.1 mm (5.63 mm over)")
            && e.contains("J1 is turned 0 degrees, and it allows 90 degrees (90 degrees over)"),
        "{e}"
    );
    assert!(
        !dir.join("fab/Widget/Widget-gerbers.zip").exists(),
        "nothing is packaged"
    );
}

#[test]
fn with_constraints_warn_the_same_board_is_packaged_and_the_notes_list_them() {
    if !have_libraries() {
        return;
    }
    let catalogs = Catalogs::jlcpcb().with_constraints_warn(true);
    let (_dir, made) = fab("warned", PHYSICAL, MOVED, &catalogs);
    let made = made.unwrap_or_else(|e| panic!("{e}"));
    let notes = &made[0].notes;
    assert!(
        notes.contains("constraints: 2 of 5 satisfied")
            && notes.contains("warning [constraint] constraint power_in.place.left")
            && notes.contains("27.13 mm over"),
        "{notes}"
    );
}

#[test]
fn a_part_locked_in_the_layout_file_against_a_requirement_is_refused_before_placing() {
    if !have_libraries() {
        return;
    }
    let (_dir, made) = fab(
        "locked",
        PHYSICAL,
        &format!("{MOVED}lock = true\n"),
        &Catalogs::jlcpcb(),
    );
    let e = made.expect_err("the lock breaks the requirement");
    assert!(
        e.contains("the requirements on where parts go cannot all be met")
            && e.contains("J1 (`power_in`) is locked in the layout file at (30, 5) mm")
            && e.contains("allows 1.4 to 1.6 mm, and its left is 28.73 mm")
            && e.contains("allows 90 degrees, and it is turned 0")
            && e.contains("unlock it and let the placer move it, or change the requirement"),
        "{e}"
    );
}

#[test]
fn requirements_that_cannot_be_met_together_say_which_and_with_what() {
    if !have_libraries() {
        return;
    }
    // The header's body is wider than the strip the second line leaves.
    let physical = PHYSICAL.replace(
        "    .led.place.top >= 3mm\n",
        "    .led.place.left >= 10mm\n    .led.place.right <= 10.5mm\n",
    );
    let (_dir, made) = fab("squeezed", &physical, "", &Catalogs::jlcpcb());
    let e = made.expect_err("the LED cannot fit between 10 and 10.5 mm");
    assert!(
        e.contains("D1 (`led`) cannot meet its requirements")
            && e.contains("led.place.left")
            && e.contains("at least 10 mm")
            && e.contains("at most 10.5 mm"),
        "{e}"
    );
}

#[test]
fn the_manifest_key_is_warn_or_error_and_reaches_the_catalogs() {
    let with = |word: &str| {
        Manifest::parse(&format!(
            "[project]\nname = \"a\"\nconstraints = \"{word}\"\n"
        ))
    };
    assert_eq!(with("warn").unwrap().constraints.as_deref(), Some("warn"));
    assert_eq!(with("error").unwrap().constraints.as_deref(), Some("error"));
    let e = with("ignore").unwrap_err();
    assert!(
        e.contains("constraints = \"ignore\" is not one of \"warn\" or \"error\""),
        "{e}"
    );
    assert_eq!(
        Manifest::parse("[project]\nname = \"a\"\n")
            .unwrap()
            .constraints,
        None
    );
    assert!(!Manifest::new("a").render().contains("constraints"));
    let m = Manifest {
        constraints: Some("warn".into()),
        ..Manifest::new("a")
    };
    assert_eq!(Manifest::parse(&m.render()), Ok(m));
    // `guidelines` does not govern it.
    let g = Manifest::parse("[project]\nname = \"a\"\nguidelines = \"error\"\n").unwrap();
    assert_ne!(g.constraints.as_deref(), Some("warn"));
    assert!(
        !Catalogs::jlcpcb().constraints_warn(),
        "an error by default"
    );
}

#[test]
fn a_point_pins_the_part_and_a_keep_in_is_two_windows() {
    if !have_libraries() {
        return;
    }
    let physical = PHYSICAL.replace(
        "    .led.place.right <= 38mm\n    .led.place.top >= 3mm\n",
        "    .led.place.x &= 20mm\n    .led.place.y &= 12mm\n    .led.place.rotation &= 0\n    .r1.place.left >= 24mm\n    .r1.place.right <= 34mm\n",
    );
    let (dir, made) = fab("pinned", &physical, "", &Catalogs::jlcpcb());
    let made = made.unwrap_or_else(|e| panic!("{e}"));
    assert!(
        made[0].notes.contains("constraints: 8 of 8 satisfied"),
        "{}",
        made[0].notes
    );
    // The layout file's own entries are not needed: the source holds the requirement and the
    // board file the solution.
    assert!(!dir.join("source/widget.pcb.toml").exists());
}

#[test]
fn allow_drc_violations_does_not_waive_a_requirement_only_warn_does() {
    if !have_libraries() {
        return;
    }
    let (dir, made) = fab_waiving("waived", PHYSICAL, MOVED, &Catalogs::jlcpcb(), true);
    let e = made.expect_err("a requirement is not a design rule");
    assert!(
        e.contains("3 requirement(s) of the design on where parts go are not met")
            && e.contains("--allow-drc-violations does not waive them")
            && e.contains("`constraints = \"warn\"`")
            && e.contains("error [constraint] constraint power_in.place.left"),
        "{e}"
    );
    assert!(
        !dir.join("fab/Widget/Widget-gerbers.zip").exists(),
        "nothing is packaged"
    );
    // With `warn` the same run goes through, and the notes say what is not met.
    let catalogs = Catalogs::jlcpcb().with_constraints_warn(true);
    let (_dir2, made) = fab_waiving("waived-warn", PHYSICAL, MOVED, &catalogs, true);
    let notes = made.unwrap_or_else(|e| panic!("{e}"))[0].notes.clone();
    assert!(
        notes.contains("constraints: 2 of 5 satisfied; 3 not met, listed as warnings because the project or this run asked for `constraints = \"warn\"`"),
        "{notes}"
    );
}
