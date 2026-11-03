//! `netsmith compare` on the example boards. The tests that read a schematic run real
//! KiCad (`kicad-cli sch export netlist`), so they skip when it is not installed; the
//! rest need no KiCad.

use netsmith_kicad::testlib::TempDir;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use netsmith::compare::{Request, Verdict, compare_files, netlist_of_design};
use netsmith::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_fab::{KicadCli, KicadCliExe};
use netsmith_netlist::{Netlist, Options, Outcome, compare, report};
use netsmith_pcb::{Board, kicad_pcb, route};
use netsmith_solve::Catalogs;

const LED: &str = include_str!("../../../examples/led.nsm");
const REGULATOR: &str = include_str!("../../../examples/linear_regulator.nsm");

fn scratch(name: &str) -> TempDir {
    TempDir::new(&format!("compare-test-{name}"))
}

/// Build with our own small KiCad libraries, so that no KiCad is needed to make the files.
fn build(name: &str, src: &str, catalogs: &Catalogs) -> Result<Vec<netsmith::Built>, String> {
    build_with(name, src, &Loader::std(), catalogs, &Libraries::sample())
}

/// The schematic and the routed board of the only board of `source`, written to `dir`.
fn write_board(source: &str, dir: &Path) -> (PathBuf, PathBuf) {
    let built = build("t.nsm", source, &Catalogs::jlcpcb()).unwrap_or_else(|e| panic!("{e}"));
    let b = &built[0];
    let sch = dir.join(format!("{}.kicad_sch", b.design.name));
    fs::write(&sch, &b.schematic).unwrap();
    let libraries = Libraries::sample();
    let mut board =
        Board::from_design(&b.design, &libraries.mapping, &libraries.footprints).unwrap();
    let unrouted = route(&mut board);
    assert!(unrouted.is_empty(), "{:?}", unrouted.lines);
    let pcb = dir.join(format!("{}.kicad_pcb", b.design.name));
    fs::write(&pcb, kicad_pcb(&board)).unwrap();
    (sch, pcb)
}

fn same(a: &Path, b: &Path, kicad: &dyn KicadCli) -> Compared {
    let request = Request {
        a: a.to_str().unwrap(),
        b: b.to_str().unwrap(),
        ..Request::default()
    };
    let done = compare_files(&request, Some(kicad)).unwrap_or_else(|e| panic!("{e}"));
    Compared(done.verdict, done.text)
}

struct Compared(Verdict, String);

#[test]
fn the_generated_schematic_matches_its_own_board() {
    let Some(kicad) = KicadCliExe::locate() else {
        return;
    };
    for (name, source) in [("led", LED), ("regulator", REGULATOR)] {
        let dir = scratch(name);
        let (sch, pcb) = write_board(source, &dir);
        let Compared(verdict, text) = same(&sch, &pcb, &kicad);
        assert_eq!(verdict, Verdict::Match, "{name}: {text}");
        assert!(text.starts_with("A: "), "{text}");
    }
}

#[test]
fn the_source_matches_its_schematic_and_its_board() {
    let Some(kicad) = KicadCliExe::locate() else {
        return;
    };
    let dir = scratch("source");
    let (sch, pcb) = write_board(LED, &dir);
    let nsm = dir.join("led.nsm");
    fs::write(&nsm, LED).unwrap();
    for other in [&sch, &pcb] {
        let Compared(verdict, text) = same(&nsm, other, &kicad);
        assert_eq!(verdict, Verdict::Match, "{text}");
    }
}

#[test]
fn a_schematic_compared_with_itself_matches() {
    let Some(kicad) = KicadCliExe::locate() else {
        return;
    };
    let dir = scratch("self");
    let (sch, _) = write_board(REGULATOR, &dir);
    let Compared(verdict, text) = same(&sch, &sch, &kicad);
    assert_eq!(verdict, Verdict::Match, "{text}");
    assert!(
        text.contains("same designator") && text.contains("same name"),
        "{text}"
    );
}

#[test]
fn renumbered_designators_and_other_net_names_still_match() {
    let Some(kicad) = KicadCliExe::locate() else {
        return;
    };
    let dir = scratch("renumber");
    let (sch, _) = write_board(LED, &dir);
    let text = fs::read_to_string(&sch).unwrap();
    let renamed = text
        .replace("\"R1\"", "\"R7\"")
        .replace("\"D1\"", "\"D4\"")
        .replace("\"VCC\"", "\"SUPPLY\"")
        .replace("\"GND\"", "\"RETURN\"");
    assert_ne!(renamed, text);
    let other = dir.join("renamed.kicad_sch");
    fs::write(&other, renamed).unwrap();
    let Compared(verdict, out) = same(&sch, &other, &kicad);
    assert_eq!(verdict, Verdict::Match, "{out}");
    assert!(
        out.contains("R1 -> R7") && out.contains("D1 -> D4"),
        "{out}"
    );
    assert!(
        out.contains("-> `SUPPLY`") || out.contains("-> `/SUPPLY`"),
        "{out}"
    );

    // With designators as identity the same two files differ.
    let request = Request {
        a: sch.to_str().unwrap(),
        b: other.to_str().unwrap(),
        by_refdes: true,
        ..Request::default()
    };
    let done = compare_files(&request, Some(&kicad)).unwrap();
    assert_eq!(done.verdict, Verdict::Different);
    assert!(
        done.text.contains("component only in A: R1"),
        "{}",
        done.text
    );
}

#[test]
fn a_resistor_of_another_value_is_found_by_kicad_too() {
    let Some(kicad) = KicadCliExe::locate() else {
        return;
    };
    let dir = scratch("value");
    let (sch, _) = write_board(LED, &dir);
    let text = fs::read_to_string(&sch).unwrap();
    let other = dir.join("other.kicad_sch");
    fs::write(&other, text.replace("\"330\"", "\"470\"")).unwrap();
    let Compared(verdict, out) = same(&sch, &other, &kicad);
    assert_eq!(verdict, Verdict::Different, "{out}");
    assert!(
        out.contains("1 difference") && out.contains("A is 330") && out.contains("B is 470"),
        "{out}"
    );
}

/// The netlist of the LED example as `netsmith build` sees it, which needs no KiCad.
fn led_netlist() -> Netlist {
    let built = build("led.nsm", LED, &Catalogs::jlcpcb()).unwrap();
    netlist_of_design(&built[0].design, &Libraries::sample().mapping).unwrap()
}

#[test]
fn a_deliberately_miswired_copy_is_named_by_nets_and_pins() {
    let a = led_netlist();
    let mut b = a.clone();
    // The resistor's supply side goes to ground instead.
    let r1 = b.find("R1").unwrap();
    let gnd = b.nets.iter().position(|n| n.name == "GND").unwrap();
    let vcc = b.nets.iter().position(|n| n.name == "VCC").unwrap();
    b.nets[vcc].pins.retain(|(c, _)| *c != r1);
    b.nets[gnd].pins.push((r1, "1".into()));
    for (pin, net) in &mut b.components[r1].pins {
        if pin == "1" {
            *net = gnd;
        }
    }
    let outcome = compare(&a, &b, &Options::default()).unwrap();
    let Outcome::Different(d) = &outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(d.len(), 2, "{d:?}");
    let text = report(&outcome);
    assert!(
        text.contains("net `VCC` (A) and `VCC` (B)")
            && text.contains("only in A: R1.1 (in B, R1.1 is on net `GND`)"),
        "{text}"
    );
    assert!(
        text.contains("net `GND` (A) and `GND` (B)")
            && text.contains("only in B: R1.1 (in A, R1.1 is on net `VCC`)"),
        "{text}"
    );
}

#[test]
fn the_command_exits_0_for_a_match_1_for_a_difference_and_2_for_a_mistake() {
    let dir = scratch("exit");
    let xml = |value: &str| {
        format!(
            r#"<export version="E"><components>
              <comp ref="R1"><value>{value}</value><footprint>Resistor_SMD:R_0402_1005Metric</footprint></comp>
              <comp ref="R2"><value>1k</value><footprint>Resistor_SMD:R_0402_1005Metric</footprint></comp>
            </components><nets>
              <net code="1" name="A"><node ref="R1" pin="1"/><node ref="R2" pin="1"/></net>
              <net code="2" name="unconnected-(R1-Pad2)"><node ref="R1" pin="2"/></net>
            </nets></export>"#
        )
    };
    let (a, b, c) = (dir.join("a.xml"), dir.join("b.xml"), dir.join("c.xml"));
    fs::write(&a, xml("4k7")).unwrap();
    fs::write(&b, xml("4k7")).unwrap();
    fs::write(&c, xml("4.7k")).unwrap();
    let run = |args: &[&Path]| {
        let out = Command::new(env!("CARGO_BIN_EXE_netsmith"))
            .arg("compare")
            .args(args)
            .output()
            .unwrap();
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let (code, out, _) = run(&[&a, &b]);
    assert_eq!(code, Some(0), "{out}");
    assert!(
        out.contains("match: 2 components and 1 nets correspond"),
        "{out}"
    );

    let (code, out, _) = run(&[&a, &c]);
    assert_eq!(code, Some(1), "{out}");
    assert!(out.contains("different: 1 difference"), "{out}");
    let (code, _, _) = run(&[&a, &c, Path::new("--loose-values")]);
    assert_eq!(code, Some(0));

    let (code, _, err) = run(&[&a]);
    assert_eq!(code, Some(2));
    assert!(err.contains("two files"), "{err}");
    let (code, _, err) = run(&[&a, &dir.join("missing.xml")]);
    assert_eq!(code, Some(2));
    assert!(err.contains("missing.xml"), "{err}");

    // --map names components that are not there.
    let map = dir.join("map.txt");
    fs::write(&map, "R9 R1\n").unwrap();
    let (code, _, err) = run(&[&a, &b, Path::new("--map"), &map]);
    assert_eq!(code, Some(2));
    assert!(err.contains("R9"), "{err}");
}

#[test]
fn a_board_is_chosen_by_name_when_a_file_has_several() {
    let two = include_str!("../../../examples/two_boards.nsm");
    let dir = scratch("boards");
    let file = dir.join("two.nsm");
    fs::write(&file, two).unwrap();
    let f = file.to_str().unwrap();
    let request = |board| Request {
        a: f,
        b: f,
        board,
        ..Request::default()
    };
    let e = compare_files(&request(None), None).unwrap_err();
    assert!(e.contains("2 boards") && e.contains("--board"), "{e}");
    let e = compare_files(&request(Some("Nope")), None).unwrap_err();
    assert!(e.contains("`Nope`") && e.contains("TwoBoards/power"), "{e}");
    let done = compare_files(&request(Some("TwoBoards/power")), None).unwrap();
    assert_eq!(done.verdict, Verdict::Match);
}
