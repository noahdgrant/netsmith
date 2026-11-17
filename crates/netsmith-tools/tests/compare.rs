//! `netsmith board compare` on the example boards, with no KiCad: a schematic is read from its files
//! (#855). The tests that hold that reading to KiCad's own (`kicad-cli sch export netlist`) skip
//! when it is not installed, or is Windows KiCad that this shell cannot start (no `cmd.exe`, as
//! under `systemd-run --user`).

use netsmith_testkit::TempDir;
use std::fs;
use std::path::{Path, PathBuf};

use netsmith_tools::compare::{Request, Verdict, compare_files, netlist_of_design};
use netsmith_pipeline::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_fab::{CliError, KicadCli, KicadCliExe};
use netsmith_netlist::{Identity, Netlist, Options, Outcome, compare, report};
use netsmith_pcb::kicad_pcb;
use netsmith_router::route;
use netsmith_solve::Catalogs;

const LED: &str = include_str!("../../../examples/led.nsm");
const REGULATOR: &str = include_str!("../../../examples/linear_regulator.nsm");

/// KiCad, or `None` (the test skips) when it is not installed or is Windows KiCad that this
/// process cannot start (see [`KicadCliExe::interop`]). Any other fault stays a failure.
fn kicad() -> Option<KicadCliExe> {
    let cli = KicadCliExe::locate_runnable()?;
    skip_on_no_windows(cli.interop(), cli)
}

/// `cli` if `interop` is fine; `None`, said on stderr, if Windows programs cannot be started
/// from here; a panic for any other fault.
fn skip_on_no_windows(interop: Result<(), CliError>, cli: KicadCliExe) -> Option<KicadCliExe> {
    match interop {
        Ok(()) => Some(cli),
        Err(CliError::NoWindows(why)) => {
            eprintln!("skipping: Windows KiCad cannot be run from this shell: {why}");
            None
        }
        Err(other) => panic!("{other}"),
    }
}

fn scratch(name: &str) -> TempDir {
    TempDir::new(&format!("compare-test-{name}"))
}

/// Build with our own small KiCad libraries, so that no KiCad is needed to make the files.
fn build(name: &str, src: &str, catalogs: &Catalogs) -> Result<Vec<netsmith_pipeline::Built>, String> {
    build_with(name, src, &Loader::std(), catalogs, &Libraries::sample())
}

/// The schematic and the routed board of the only board of `source`, written to `dir`.
fn write_board(source: &str, dir: &Path) -> (PathBuf, PathBuf) {
    let built = build("t.nsm", source, &Catalogs::jlcpcb()).unwrap_or_else(|e| panic!("{e}"));
    let b = &built[0];
    let sch = dir.join(format!("{}.kicad_sch", b.design.name));
    fs::write(&sch, &b.schematic).unwrap();
    for (file, text) in &b.sheets {
        fs::write(dir.join(file), text).unwrap();
    }
    let libraries = Libraries::sample();
    let mut board =
        netsmith_placer::board_from_design(&b.design, &libraries.mapping, &libraries.footprints)
            .unwrap();
    let unrouted = route(&mut board);
    assert!(unrouted.is_empty(), "{:?}", unrouted.lines);
    let pcb = dir.join(format!("{}.kicad_pcb", b.design.name));
    fs::write(&pcb, kicad_pcb(&board)).unwrap();
    (sch, pcb)
}

fn same(a: &Path, b: &Path) -> Compared {
    let request = Request {
        a: a.to_str().unwrap(),
        b: b.to_str().unwrap(),
        ..Request::default()
    };
    let done = compare_files(&request).unwrap_or_else(|e| panic!("{e}"));
    Compared(done.verdict, done.text)
}

struct Compared(Verdict, String);

#[test]
fn the_generated_schematic_matches_its_own_board() {
    for (name, source) in [("led", LED), ("regulator", REGULATOR)] {
        let dir = scratch(name);
        let (sch, pcb) = write_board(source, &dir);
        let Compared(verdict, text) = same(&sch, &pcb);
        assert_eq!(verdict, Verdict::Match, "{name}: {text}");
        assert!(text.starts_with("A: "), "{text}");
    }
}

#[test]
fn the_source_matches_its_schematic_and_its_board() {
    let dir = scratch("source");
    let (sch, pcb) = write_board(LED, &dir);
    let nsm = dir.join("led.nsm");
    fs::write(&nsm, LED).unwrap();
    for other in [&sch, &pcb] {
        let Compared(verdict, text) = same(&nsm, other);
        assert_eq!(verdict, Verdict::Match, "{text}");
    }
}

#[test]
fn a_schematic_compared_with_itself_matches() {
    let dir = scratch("self");
    let (sch, _) = write_board(REGULATOR, &dir);
    let Compared(verdict, text) = same(&sch, &sch);
    assert_eq!(verdict, Verdict::Match, "{text}");
    assert!(
        text.contains("same designator") && text.contains("same name"),
        "{text}"
    );
}

#[test]
fn renumbered_designators_and_other_net_names_still_match() {
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
    let Compared(verdict, out) = same(&sch, &other);
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
    let done = compare_files(&request).unwrap();
    assert_eq!(done.verdict, Verdict::Different);
    assert!(
        done.text.contains("component only in A: R1"),
        "{}",
        done.text
    );
}

#[test]
fn a_resistor_of_another_value_is_found() {
    let dir = scratch("value");
    let (sch, _) = write_board(LED, &dir);
    let text = fs::read_to_string(&sch).unwrap();
    let other = dir.join("other.kicad_sch");
    fs::write(&other, text.replace("\"330\"", "\"470\"")).unwrap();
    let Compared(verdict, out) = same(&sch, &other);
    assert_eq!(verdict, Verdict::Different, "{out}");
    assert!(
        out.contains("1 difference") && out.contains("A is 330") && out.contains("B is 470"),
        "{out}"
    );
}

/// The netlist of the LED example as `netsmith board build` sees it, which needs no KiCad.
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
    let e = compare_files(&request(None)).unwrap_err();
    assert!(e.contains("2 boards") && e.contains("--board"), "{e}");
    let e = compare_files(&request(Some("Nope"))).unwrap_err();
    assert!(e.contains("`Nope`") && e.contains("TwoBoards/power"), "{e}");
    let done = compare_files(&request(Some("TwoBoards/power"))).unwrap();
    assert_eq!(done.verdict, Verdict::Match);
}

#[test]
fn windows_kicad_that_cannot_be_started_skips_the_test_and_other_faults_do_not() {
    let cli = || KicadCliExe::new("/x/kicad-cli.exe");
    assert!(skip_on_no_windows(Ok(()), cli()).is_some());
    let none = CliError::NoWindows("cannot ask Windows for its temp directory".into());
    assert!(skip_on_no_windows(Err(none), cli()).is_none());
    let real = CliError::Failed {
        command: "cmd.exe".into(),
        output: "boom".into(),
    };
    let panicked = std::panic::catch_unwind(|| skip_on_no_windows(Err(real), cli()));
    assert!(panicked.is_err(), "a real fault must fail, not skip");
}

#[test]
fn a_linux_kicad_needs_no_windows() {
    assert!(KicadCliExe::new("/usr/bin/kicad-cli").interop().is_ok());
}

/// KiCad's netlist of a schematic, read from the XML `kicad-cli` exports.
fn kicads_netlist(kicad: &KicadCliExe, sch: &Path) -> Netlist {
    let xml = kicad
        .netlist(sch)
        .unwrap_or_else(|e| panic!("{}: {e}", sch.display()));
    Netlist::from_kicad_xml(&xml).unwrap_or_else(|e| panic!("{}: {e}", sch.display()))
}

/// The netlist read from a schematic's files is KiCad's: the same parts with the same values and
/// footprints, the same pins on each net, and the same names of the nets a label or a power
/// symbol names (a net only pins name, `Net-(R1-Pad1)`, may be named after another of its pins).
pub(crate) fn read_as_kicad_reads(kicad: &KicadCliExe, sch: &Path) {
    let ours = Netlist::from_kicad_schematic(sch).unwrap_or_else(|e| panic!("{e}"));
    let theirs = kicads_netlist(kicad, sch);
    let options = Options {
        identity: Identity::ByRefdes,
        ..Options::default()
    };
    let outcome = compare(&ours, &theirs, &options).unwrap();
    let Outcome::Match(pairs) = &outcome else {
        panic!(
            "{}: the netlist read from the files is not KiCad's: {}",
            sch.display(),
            report(&outcome)
        );
    };
    let renamed: Vec<_> = pairs
        .nets
        .iter()
        .filter(|(a, b)| a != b && !b.starts_with("Net-("))
        .collect();
    assert!(
        renamed.is_empty(),
        "{}: nets named otherwise than KiCad names them (ours, KiCad's): {renamed:?}",
        sch.display()
    );
}

#[test]
fn a_schematic_is_read_as_kicad_reads_it() {
    let Some(kicad) = kicad() else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // The ports' originals in KiCad 7's format and netsmith's own schematics. The other ports'
    // originals are in KiCad 10's format, which KiCad 9 does not open; netsmith-netlist's
    // `kicad_schematic` tests hold them to their boards.
    for sch in [
        "examples/orbit/reference/ORBIT.kicad_sch",
        "crates/netsmith-kicad/testdata/flat/led.kicad_sch",
        "crates/netsmith-kicad/testdata/flat/regulator.kicad_sch",
        "crates/netsmith-kicad/testdata/flat/two_drawn_regulators.kicad_sch",
    ] {
        read_as_kicad_reads(&kicad, &root.join(sch));
    }
    for (name, source) in [("led", LED), ("regulator", REGULATOR)] {
        let dir = scratch(name);
        let (sch, _) = write_board(source, &dir);
        read_as_kicad_reads(&kicad, &sch);
    }
}
