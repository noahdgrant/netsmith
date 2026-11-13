//! Source text in, a package JLCPCB can take out. KiCad is not needed to make one; the tests
//! that cross-check netsmith's files and design rules check against KiCad's skip when it is
//! not installed.

use netsmith_kicad::testlib::TempDir;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use netsmith::Libraries;
use netsmith::fab::{FabOptions, fab, fab_with_options};
use netsmith_fab::{KicadCli, KicadCliExe};
use netsmith_solve::Catalogs;

const LED: &str = include_str!("../../../examples/led.nsm");
const REGULATOR: &str = include_str!("../../../examples/linear_regulator.nsm");
const FOUR_LAYER: &str = include_str!("../../../examples/four_layer.nsm");
const SIX_LAYER: &str = include_str!("../../../examples/six_layer.nsm");
const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");
const OUTLINE_AND_POURS: &str = include_str!("../../../examples/outline_and_pours.nsm");
const OUTLINE_AND_POURS_PHYSICAL: &str =
    include_str!("../../../examples/outline_and_pours.physical.nsm");
const OUTLINE_AND_POURS_LAYOUT: &str = include_str!("../../../examples/outline_and_pours.pcb.toml");

fn kicad() -> Option<KicadCliExe> {
    KicadCliExe::locate_runnable()
}

fn out(name: &str) -> TempDir {
    TempDir::unused(&format!("fab-test-{name}"))
}

/// A file of the board's Gerber zip.
fn from_zip(dir: &std::path::Path, board: &str, file: &str) -> String {
    use std::io::Read as _;
    let zip = fs::File::open(dir.join(board).join(format!("{board}-gerbers.zip"))).unwrap();
    let mut archive = zip::ZipArchive::new(zip).unwrap();
    let mut text = String::new();
    archive
        .by_name(&format!("gerber/{file}"))
        .unwrap_or_else(|e| panic!("{file}: {e}"))
        .read_to_string(&mut text)
        .unwrap();
    text
}

fn read(dir: &std::path::Path, board: &str, file: &str) -> String {
    fs::read_to_string(dir.join(board).join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

#[test]
fn the_led_board_becomes_a_complete_package() {
    let Some(cli) = kicad() else { return };
    let dir = out("led");
    let made = fab(
        "led.nsm",
        LED,
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        Some(&cli),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].board, "LedExample");

    for file in [
        "LedExample-gerbers.zip",
        "LedExample.kicad_pcb",
        "LedExample.kicad_pro",
        "LedExample.svg",
        "bom.csv",
        "cpl.csv",
        "fab-notes.txt",
    ] {
        assert!(
            dir.join("LedExample").join(file).is_file(),
            "{file} was not written"
        );
    }

    // Real, sourced parts, in JLCPCB's column names. The header is hand-soldered.
    assert_eq!(
        read(&dir, "LedExample", "bom.csv"),
        "Comment,Designator,Footprint,LCSC Part #\nKT-0603R,D1,LED_0603_1608Metric,C2286\n330,R1,R_0402_1005Metric,C25104\n"
    );
    let cpl = read(&dir, "LedExample", "cpl.csv");
    assert!(
        cpl.starts_with("Designator,Mid X,Mid Y,Layer,Rotation\n"),
        "{cpl}"
    );
    assert!(
        cpl.contains("D1,") && cpl.contains("R1,") && !cpl.contains("J1"),
        "{cpl}"
    );
    let notes = read(&dir, "LedExample", "fab-notes.txt");
    assert!(notes.contains("hand-solder: J1"), "{notes}");
    assert!(notes.contains("design rules check: passed"), "{notes}");
    assert!(
        notes.contains("process: JLCPCB TwoLayer (numbers from https://jlcpcb.com/capabilities/pcb-capabilities)"),
        "{notes}"
    );
    assert!(
        notes.contains("track 0.1 mm") && notes.contains("1.6 mm thick"),
        "{notes}"
    );
    assert!(
        notes.contains("rotation unverified") && notes.contains("D1"),
        "{notes}"
    );

    for layer in [
        "F_Cu",
        "B_Cu",
        "F_Mask",
        "F_Paste",
        "F_Silkscreen",
        "Edge_Cuts",
    ] {
        assert!(
            made[0].gerber_files.iter().any(|f| f.contains(layer)),
            "no {layer} in {:?}",
            made[0].gerber_files
        );
    }
    assert!(made[0].gerber_files.iter().any(|f| f.ends_with("PTH.drl")));
}

#[test]
fn who_assembles_a_part_decides_the_bom_the_placement_file_and_the_notes() {
    let Some(cli) = kicad() else { return };
    // The LED board with a hole, a fiducial and programming pads that nobody fits, and its
    // LED soldered by hand though it is surface mount.
    let source = LED
        .replace(
            "import std::parts::connector::Header1x2\n",
            "import std::parts::connector::{Header1x2, ProgrammingPads6}\n\
             import std::parts::mechanical::{Fiducial, MountingHole}\n",
        )
        .replace(
            "board LedExample",
            "module Mech {\n    .hole: MountingHole\n    .fid: Fiducial\n}\n\nboard LedExample",
        )
        .replace(
            "    .led: Led {\n",
            "    .led: Led {\n        .assembly &= AssemblyMode::Hand\n",
        )
        .replace(
            "    .supply.pos ~> .r1",
            "    .mech: Mech\n    .pads: ProgrammingPads6\n\
             \x20   .pads.p1 ~ .supply.pos\n    .pads.p2 ~ .supply.neg\n    .supply.pos ~> .r1",
        );
    let dir = out("assembly");
    let made = fab(
        "led.nsm",
        &source,
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        Some(&cli),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made.len(), 1);
    // Only the resistor is the fab's.
    assert_eq!(
        read(&dir, "LedExample", "bom.csv"),
        "Comment,Designator,Footprint,LCSC Part #\n330,R1,R_0402_1005Metric,C25104\n"
    );
    let cpl = read(&dir, "LedExample", "cpl.csv");
    assert!(
        cpl.contains("R1,") && !cpl.contains("D1,") && !cpl.contains("U"),
        "{cpl}"
    );
    let notes = read(&dir, "LedExample", "fab-notes.txt");
    assert!(notes.contains("hand-solder: D1, J1\n"), "{notes}");
    assert!(
        notes.contains("not fitted (no BOM line, no placement): FID1, H1, J2\n"),
        "{notes}"
    );
    assert!(notes.contains("design rules check: passed"), "{notes}");
}

/// The LED board with its LED and resistor in a module that offers who assembles them
/// (ADR 0042), and `choice` added where the board uses it.
fn offered_assembly_board(choice: &str) -> String {
    format!(
        "import std::parts::connector::Header1x2\n\
         import std::parts::passive::{{Resistor, Led}}\n\
         import std::sources::Rail\n\
         import std::mfg::jlcpcb::TwoLayer\n\
         module Lamp {{\n\
         \x20   .supply: Rail\n\
         \x20   .assembly_mode: AssemblyMode\n\
         \x20   .led: Led {{ .current &= 5mA..15mA }}\n\
         \x20   .r1: Resistor\n\
         \x20   .led.assembly &= .assembly_mode\n\
         \x20   .r1.assembly &= .assembly_mode\n\
         \x20   .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance\n\
         \x20   .supply.pos ~> .r1 ~> .led ~> .supply.neg\n\
         }}\n\
         board OfferedAssembly {{\n\
         \x20   .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}\n\
         \x20   .supply: Rail {{\n\
         \x20       .voltage &= 5V +/- 5%\n\
         \x20       .pos.net &= \"VCC\"\n\
         \x20       .neg.net &= \"GND\"\n\
         \x20   }}\n\
         \x20   .lamp: Lamp\n\
         \x20   .supply ~ .lamp.supply\n\
         \x20   .power_in: Header1x2\n\
         \x20   .supply.pos ~ .power_in.p1\n\
         \x20   .supply.neg ~ .power_in.p2\n\
         {choice}}}\n"
    )
}

#[test]
fn a_module_that_offers_assembly_lets_the_board_move_its_parts_out_of_the_bom() {
    if netsmith_kicad::testlib::skip_without_kicad_libraries() {
        return;
    }
    // Unset: the module's parts are the board house's, as parts are.
    let dir = out("offered-unset");
    fab(
        "offered.nsm",
        &offered_assembly_board(""),
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        None,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let bom = read(&dir, "OfferedAssembly", "bom.csv");
    assert!(bom.contains(",D1,") && bom.contains(",R1,"), "{bom}");
    let notes = read(&dir, "OfferedAssembly", "fab-notes.txt");
    assert!(notes.contains("hand-solder: J1\n"), "{notes}");

    // The board chooses `hand`: the module's parts are in neither file and in the notes.
    let dir = out("offered-hand");
    fab(
        "offered.nsm",
        &offered_assembly_board("    .lamp.assembly_mode &= AssemblyMode::Hand\n"),
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        None,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let bom = read(&dir, "OfferedAssembly", "bom.csv");
    assert_eq!(bom, "Comment,Designator,Footprint,LCSC Part #\n");
    let cpl = read(&dir, "OfferedAssembly", "cpl.csv");
    assert!(!cpl.contains("D1,") && !cpl.contains("R1,"), "{cpl}");
    let notes = read(&dir, "OfferedAssembly", "fab-notes.txt");
    assert!(notes.contains("hand-solder: D1, J1, R1\n"), "{notes}");
}

#[test]
fn the_regulator_board_becomes_a_complete_package() {
    let Some(cli) = kicad() else { return };
    let dir = out("regulator");
    let made = fab(
        "reg.nsm",
        REGULATOR,
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        Some(&cli),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made[0].board, "RegulatorExample");
    assert_eq!(
        read(&dir, "RegulatorExample", "bom.csv"),
        "Comment,Designator,Footprint,LCSC Part #\n10u,C1,C_0402_1005Metric,C315248\n22u,C2,C_0603_1608Metric,C59461\nXC6206P332MR-G,U1,SOT-23,C5446\n"
    );
    let notes = read(&dir, "RegulatorExample", "fab-notes.txt");
    assert!(notes.contains("hand-solder: J1, J2"), "{notes}");
    assert!(
        notes.contains("rotation unverified") && notes.contains("U1"),
        "{notes}"
    );
}

#[test]
fn the_four_layer_board_becomes_a_package_with_its_inner_layers() {
    let Some(cli) = kicad() else { return };
    let dir = out("four-layer");
    let made = fab(
        "four.nsm",
        FOUR_LAYER,
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        Some(&cli),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made[0].board, "FourLayerExample");
    // The gate passed with the planes filled by KiCad, and the notes say what was made.
    let notes = read(&dir, "FourLayerExample", "fab-notes.txt");
    assert!(notes.contains("design rules check: passed"), "{notes}");
    assert!(
        notes.contains("copper layers: 4 (F.Cu, In1.Cu, In2.Cu, B.Cu)"),
        "{notes}"
    );
    assert!(
        notes.contains("4 layers (In1.Cu is a plane on GND, In2.Cu is a plane on 3V3)"),
        "{notes}"
    );
    for layer in ["F_Cu", "In1_Cu", "In2_Cu", "B_Cu", "Edge_Cuts"] {
        assert!(
            made[0]
                .gerber_files
                .iter()
                .any(|f| f.ends_with(&format!("-{layer}.gbr"))),
            "no {layer} in {:?}",
            made[0].gerber_files
        );
    }
    // The inner Gerber holds the pour, which netsmith fills itself: the plane as dark
    // copper, then what it keeps clear of in clear polarity.
    let gnd = from_zip(&dir, "FourLayerExample", "FourLayerExample-In1_Cu.gbr");
    assert!(
        gnd.contains("G36*") && gnd.contains("%TO.N,GND*%") && gnd.contains("%LPC*%"),
        "{gnd}"
    );
    assert!(
        notes.contains("zone fill: netsmith filled GND on In1.Cu, 3V3 on In2.Cu itself"),
        "{notes}"
    );
    // The board file holds the zones unfilled, as netsmith writes them.
    let pcb = read(&dir, "FourLayerExample", "FourLayerExample.kicad_pcb");
    assert!(pcb.contains("(zone") && !pcb.contains("filled_polygon"));
}

#[test]
fn the_six_layer_board_becomes_a_package_with_no_kicad_and_the_notes_say_what_each_layer_is() {
    let dir = out("six-layer");
    let made = fab(
        "six.nsm",
        SIX_LAYER,
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        None,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made[0].board, "SixLayerExample");
    let notes = read(&dir, "SixLayerExample", "fab-notes.txt");
    assert!(notes.contains("design rules check: passed"), "{notes}");
    assert!(
        notes.contains("copper layers: 6 (F.Cu, In1.Cu, In2.Cu, In3.Cu, In4.Cu, B.Cu)"),
        "{notes}"
    );
    assert!(
        notes.contains(
            "6 layers (In1.Cu is a plane on GND, In2.Cu is a signal layer, with no tracks yet, In3.Cu is a signal layer, with no tracks yet, In4.Cu is a plane on 3V3), 1.6 mm thick, 1 oz copper, build-up JLC06161H-3313"
        ),
        "{notes}"
    );
    for layer in ["F_Cu", "In1_Cu", "In2_Cu", "In3_Cu", "In4_Cu", "B_Cu"] {
        assert!(
            made[0]
                .gerber_files
                .iter()
                .any(|f| f.ends_with(&format!("-{layer}.gbr"))),
            "no {layer} in {:?}",
            made[0].gerber_files
        );
    }
    // The planes hold their pour; a signal layer holds no pour and no track, only the
    // through-hole pads' copper.
    let gnd = from_zip(&dir, "SixLayerExample", "SixLayerExample-In1_Cu.gbr");
    assert!(gnd.contains("G36*") && gnd.contains("%TO.N,GND*%"), "{gnd}");
    let signal = from_zip(&dir, "SixLayerExample", "SixLayerExample-In2_Cu.gbr");
    assert!(
        signal.contains("%TF.FileFunction,Copper,L3,Inr*%") && !signal.contains("G36*"),
        "{signal}"
    );
    let pcb = read(&dir, "SixLayerExample", "SixLayerExample.kicad_pcb");
    assert!(pcb.contains("(10 \"In4.Cu\" power)") && pcb.contains("(6 \"In2.Cu\" signal)"));
}

#[test]
fn the_six_layer_example_agrees_with_kicads_own_plot() {
    cross_checked("six_layer", SIX_LAYER);
}

#[test]
fn a_four_layer_board_that_does_not_name_its_planes_is_refused_before_any_file_is_written() {
    let dir = out("four-layer-unnamed");
    let source = FOUR_LAYER.replace("        .inner1.net &= \"GND\"\n", "");
    let err = fab(
        "four.nsm",
        &source,
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        None,
    )
    .expect_err("an inner layer needs a net");
    assert!(
        err.contains("`process.inner1` is an inner layer of a 4-layer board and has no net"),
        "{err}"
    );
    assert!(!dir.exists(), "nothing is written");
}

#[test]
fn making_a_package_twice_gives_identical_files() {
    let Some(cli) = kicad() else { return };
    let (a, b) = (out("same-a"), out("same-b"));
    for dir in [&a, &b] {
        fab(
            "led.nsm",
            LED,
            dir,
            &Catalogs::jlcpcb(),
            &Libraries::standard(),
            Some(&cli),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    }
    for file in [
        "LedExample-gerbers.zip",
        "bom.csv",
        "cpl.csv",
        "LedExample.kicad_pcb",
    ] {
        let path = |d: &Path| d.join("LedExample").join(file);
        assert_eq!(
            fs::read(path(&a)).unwrap(),
            fs::read(path(&b)).unwrap(),
            "{file} differs"
        );
    }
}

/// Every hole, test point and the fiducial of the standard mapping, made with no KiCad
/// footprint library to read: the footprints are generated, and the board file carries their
/// geometry (ADR 0097, step 3).
#[test]
fn holes_test_points_and_a_fiducial_are_fabricated_with_no_kicad_footprint_library() {
    use netsmith_kicad::install::Search;
    use netsmith_kicad::sexp::Sexp;
    use netsmith_kicad::testlib;
    use netsmith_pcb::{Footprint, FootprintLibrary};

    let nowhere = TempDir::new("no-footprint-library");
    let machine = Search::under(nowhere.path());
    let footprints = FootprintLibrary::installed_by(&machine);
    assert!(
        footprints.load("Resistor_SMD:R_0402_1005Metric").is_ok(),
        "the generators answer without an install"
    );
    assert!(
        footprints
            .load("Connector_USB:USB_C_Receptacle_GCT_USB4085")
            .is_err(),
        "no footprint library of KiCad's is in reach"
    );
    let libraries = Libraries::with_mapping(
        netsmith_kicad::Mapping::standard(),
        netsmith_kicad::SymbolLib::at(testlib::sample_symbols()),
        footprints,
    );
    let source = LED
        .replace(
            "import std::parts::connector::Header1x2\n",
            "import std::parts::connector::Header1x2\n\
             import std::parts::mechanical::{Fiducial, MountingHole, PlatedHole, TestPoint}\n",
        )
        .replace(
            "board LedExample",
            "module Mech {\n    .m3: MountingHole\n    .m2: MountingHole { .package &= \"M2\" }\n\
             \x20   .fid: Fiducial\n}\n\nboard LedExample",
        )
        .replace(
            "    .supply.pos ~> .r1",
            "    .mech: Mech\n\
             \x20   .p3: PlatedHole\n    .p2: PlatedHole { .package &= \"M2\" }\n\
             \x20   .tp10: TestPoint { .package &= \"D1.0mm\" }\n\
             \x20   .tp15: TestPoint\n\
             \x20   .tp20: TestPoint { .package &= \"D2.0mm\" }\n\
             \x20   .p3.p1 ~ .supply.neg\n    .p2.p1 ~ .supply.neg\n\
             \x20   .tp10.p1 ~ .supply.pos\n    .tp15.p1 ~ .supply.pos\n    .tp20.p1 ~ .supply.neg\n\
             \x20   .supply.pos ~> .r1",
        );
    let dir = out("mechanical-no-footprints");
    fab(
        "led.nsm",
        &source,
        &dir,
        &Catalogs::jlcpcb(),
        &libraries,
        None,
    )
    .unwrap_or_else(|e| panic!("{e}"));

    // The board file holds each footprint with the pads, the holes and the courtyard that the
    // generator gives it.
    let pcb = Sexp::parse(&read(&dir, "LedExample", "LedExample.kicad_pcb")).unwrap();
    let ids = [
        "MountingHole:MountingHole_3.2mm_M3",
        "MountingHole:MountingHole_2.2mm_M2",
        "MountingHole:MountingHole_3.2mm_M3_Pad",
        "MountingHole:MountingHole_2.2mm_M2_Pad",
        "Fiducial:Fiducial_1mm_Mask2mm",
        "TestPoint:TestPoint_Pad_D1.0mm",
        "TestPoint:TestPoint_Pad_D1.5mm",
        "TestPoint:TestPoint_Pad_D2.0mm",
    ];
    for id in ids {
        let node = pcb
            .find_all("footprint")
            .find(|f| f.args().first().and_then(|a| a.text()) == Some(id))
            .unwrap_or_else(|| panic!("the board has no {id}"));
        let written = Footprint::parse(id, &node.to_kicad_string()).unwrap();
        let made = netsmith_pcb::generate::generate(id).unwrap();
        assert_eq!(written.pads.len(), made.pads.len(), "{id}");
        for (w, m) in written.pads.iter().zip(&made.pads) {
            assert_eq!(
                (w.kind, w.shape, w.at, w.angle, &w.layers),
                (m.kind, m.shape, m.at, 0.0, &m.layers),
                "{id}"
            );
            assert_eq!(
                (w.width, w.height, w.drill),
                (m.width, m.height, m.drill),
                "{id}"
            );
        }
        assert_eq!(written.courtyard, made.courtyard, "{id}");
    }
}

/// An SMA connector and the programming pads, made with no KiCad footprint library to read: the
/// footprints are netsmith's own, written from the makers' drawings (ADR 0097, step 3), and the
/// board file carries their pads.
#[test]
fn a_coax_and_programming_pads_are_fabricated_with_no_kicad_footprint_library() {
    use netsmith_kicad::install::Search;
    use netsmith_kicad::sexp::Sexp;
    use netsmith_pcb::{Footprint, FootprintLibrary};

    let nowhere = TempDir::new("no-footprint-library-connectors");
    let machine = Search::under(nowhere.path());
    let footprints = FootprintLibrary::installed_by(&machine);
    assert!(
        footprints
            .load("Connector_Coaxial:SMA_Amphenol_132134_Vertical")
            .is_err(),
        "no footprint library of KiCad's is in reach"
    );
    let libraries = Libraries::with_mapping(
        netsmith_kicad::Mapping::standard(),
        netsmith_kicad::SymbolLib::at("/nonexistent").with_generators(),
        footprints,
    );
    let source = LED
        .replace(
            "import std::parts::connector::Header1x2\n",
            "import std::parts::connector::{Coax, Header1x2, ProgrammingPads6}\n",
        )
        .replace(
            "    .supply.pos ~> .r1",
            "    .rf: Coax\n    .pads: ProgrammingPads6\n\
             \x20   .rf.signal ~ .supply.pos\n    .rf.shield ~ .supply.neg\n\
             \x20   .pads.p1 ~ .supply.pos\n    .pads.p2 ~ .supply.neg\n\
             \x20   .supply.pos ~> .r1",
        );
    let dir = out("connectors-no-footprints");
    fab(
        "led.nsm",
        &source,
        &dir,
        &Catalogs::jlcpcb(),
        &libraries,
        None,
    )
    .unwrap_or_else(|e| panic!("{e}"));

    let pcb = Sexp::parse(&read(&dir, "LedExample", "LedExample.kicad_pcb")).unwrap();
    let own = FootprintLibrary::at("/nonexistent");
    for id in [
        "netsmith:SMA_Amphenol_132134_Vertical",
        "netsmith:Tag-Connect_TC2030-IDC-NL_2x03_P1.27mm_Vertical",
    ] {
        let node = pcb
            .find_all("footprint")
            .find(|f| f.args().first().and_then(|a| a.text()) == Some(id))
            .unwrap_or_else(|| panic!("the board has no {id}"));
        let written = Footprint::parse(id, &node.to_kicad_string()).unwrap();
        let made = own.load(id).unwrap();
        assert_eq!(written.pads.len(), made.pads.len(), "{id}");
        for (w, m) in written.pads.iter().zip(&made.pads) {
            // The pad's angle is the footprint's own on the board, the placer's choice.
            assert_eq!(
                (w.number.as_str(), w.kind, w.shape, w.at, &w.layers),
                (m.number.as_str(), m.kind, m.shape, m.at, &m.layers),
                "{id}"
            );
            assert_eq!(
                (w.width, w.height, w.drill),
                (m.width, m.height, m.drill),
                "{id}"
            );
        }
        assert_eq!(written.courtyard, made.courtyard, "{id}");
    }
}

#[test]
fn a_part_with_no_part_number_is_refused_and_nothing_is_written() {
    let Some(cli) = kicad() else { return };
    let dir = out("no-lcsc");
    // The sample catalogs hold resistors with no supplier number.
    let err = fab(
        "led.nsm",
        LED,
        &dir,
        &Catalogs::sample(),
        &Libraries::standard(),
        Some(&cli),
    )
    .expect_err("a part with no LCSC number must stop the package");
    assert!(err.contains("LCSC"), "{err}");
    assert!(
        !dir.join("LedExample")
            .join("LedExample-gerbers.zip")
            .exists()
    );
}

#[test]
fn a_requirement_nothing_can_meet_stops_before_any_file_is_written() {
    let Some(cli) = kicad() else { return };
    let dir = out("tight");
    let tight = LED.replace(".current &= 5mA..15mA", ".current &= 9.9mA..10.1mA");
    let err = fab(
        "led.nsm",
        &tight,
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        Some(&cli),
    )
    .expect_err("the solver cannot satisfy this");
    assert!(
        err.contains("no combination of candidates") || err.contains("no `resistor` candidate"),
        "{err}"
    );
    assert!(
        !dir.exists(),
        "nothing should be written for a board that cannot be built"
    );
}

#[test]
fn a_board_with_no_process_is_refused_and_told_what_to_add() {
    if netsmith_kicad::testlib::skip_without_kicad_libraries() {
        return;
    }
    let dir = out("no-process");
    let source = LED.replace(
        ".process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }",
        "",
    );
    let err = fab(
        "led.nsm",
        &source,
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        None,
    )
    .expect_err("fab needs a process");
    assert!(
        err.contains("`LedExample` does not say how it is made")
            && err.contains("import std::mfg::jlcpcb::TwoLayer")
            && err.contains(".process: TwoLayer"),
        "{err}"
    );
    assert!(!dir.exists(), "nothing is written");
}

#[test]
fn a_board_that_has_not_chosen_a_thickness_is_refused() {
    if netsmith_kicad::testlib::skip_without_kicad_libraries() {
        return;
    }
    let dir = out("no-thickness");
    let source = LED.replace(".stackup.thickness &= 1.6mm", "");
    let err = fab(
        "led.nsm",
        &source,
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        None,
    )
    .expect_err("fab needs a thickness");
    assert!(err.contains(".process.stackup.thickness &= 1.6mm"), "{err}");
}

#[test]
fn each_board_of_a_system_gets_a_package_of_its_own_and_passes_the_design_rules() {
    let Some(cli) = kicad() else { return };
    let dir = out("system");
    let made = fab(
        "two_boards.nsm",
        TWO_BOARDS,
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        Some(&cli),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let labels: Vec<_> = made.iter().map(|m| m.board.as_str()).collect();
    assert_eq!(labels, ["TwoBoards/power", "TwoBoards/led"]);

    for (sub, name) in [("power", "PowerBoard"), ("led", "LedBoard")] {
        let board = dir.join("TwoBoards").join(sub);
        for file in [
            format!("{name}-gerbers.zip"),
            format!("{name}.kicad_pcb"),
            "bom.csv".to_string(),
            "cpl.csv".to_string(),
            "fab-notes.txt".to_string(),
        ] {
            assert!(board.join(&file).is_file(), "{sub}/{file} was not written");
        }
        let notes = fs::read_to_string(board.join("fab-notes.txt")).unwrap();
        assert!(
            notes.contains("design rules check: passed"),
            "{sub}: {notes}"
        );
    }
    let bom = fs::read_to_string(dir.join("TwoBoards/led/bom.csv")).unwrap();
    assert!(bom.contains("C25076") && bom.contains("C2286"), "{bom}");
}

#[test]
fn a_route_timeout_stops_fab_but_writes_the_incomplete_board_and_lists_the_connections_left() {
    if netsmith_kicad::testlib::skip_without_kicad_libraries() {
        return;
    }
    let dir = out("route-timeout");
    // `--allow-drc-violations` makes no difference: connections left are an error either way.
    for allow_drc_violations in [false, true] {
        let err = fab_with_options(
            "led.nsm",
            LED,
            &netsmith_check::Loader::std(),
            &dir,
            &Catalogs::jlcpcb(),
            &Libraries::standard(),
            None,
            &FabOptions {
                route_timeout: Some(Duration::ZERO),
                allow_drc_violations,
                ..FabOptions::default()
            },
        )
        .expect_err("no time to route");
        assert!(
            err.contains("3 connections could not be routed")
                && err.contains("stopped after the route timeout of 0 s")
                && err.contains("--route-timeout"),
            "{err}"
        );
        for line in ["net GND: D1.1 to J1.2", "net VCC: J1.1 to R1.1"] {
            assert!(err.contains(line), "{line} is not named in {err}");
        }
        assert!(
            err.contains("INCOMPLETE") && err.contains("routing-failure.txt"),
            "{err}"
        );
    }
    // The part that is routed can be opened: the board file and the Gerbers are written, and
    // marked, and every connection left is listed with its net and its two pads.
    let board = dir.join("LedExample");
    for file in [
        "LedExample.kicad_pcb",
        "LedExample-gerbers.zip",
        "fab-notes.txt",
        "routing-failure.txt",
    ] {
        assert!(board.join(file).is_file(), "{file} was not written");
    }
    let report = fs::read_to_string(board.join("routing-failure.txt")).unwrap();
    assert!(report.starts_with("INCOMPLETE: "), "{report}");
    for line in ["net GND: D1.1 at (", "net VCC: J1.1 at (", ") to R1.1 at ("] {
        assert!(report.contains(line), "{line} is not in {report}");
    }
    assert_eq!(report.matches("\nnet ").count(), 3, "{report}");
    // With no time, each line is left out of time, and the same is written for a program.
    assert_eq!(
        report
            .matches(": not finished, the time ran out; in its way:")
            .count(),
        3,
        "{report}"
    );
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(board.join("routing-report.json")).unwrap())
            .unwrap();
    assert_eq!(json["why"].as_array().map(Vec::len), Some(3), "{json}");
    assert_eq!(json["why"][0]["stage"], "out-of-time", "{json}");
    assert!(json["crowding"]["counts"].is_array(), "{json}");
    let notes = fs::read_to_string(board.join("fab-notes.txt")).unwrap();
    assert!(
        notes.contains("ERROR: INCOMPLETE: 3 connection(s) have no copper")
            && notes.contains("ERROR: not routed: net GND: D1.1 to J1.2")
            && notes.contains("NOT clean, packaged anyway"),
        "{notes}"
    );
}

static PROGRESS: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[test]
fn fab_says_which_stage_it_is_in_so_that_a_long_run_is_not_silent() {
    if netsmith_kicad::testlib::skip_without_kicad_libraries() {
        return;
    }
    let dir = out("progress");
    let _ = fab_with_options(
        "led.nsm",
        LED,
        &netsmith_check::Loader::std(),
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        None,
        &FabOptions {
            route_timeout: Some(Duration::ZERO),
            progress: Some(|m| PROGRESS.lock().unwrap().push(m.to_string())),
            ..FabOptions::default()
        },
    );
    let said = PROGRESS.lock().unwrap().join("\n");
    assert!(said.contains("LedExample: placing 3 parts"), "{said}");
    assert!(said.contains("placed on a "), "{said}");
    assert!(said.contains("routing 3 connections"), "{said}");
}

/// `fab` of the outline-and-pours example with `layout` as its layout file, in a folder of
/// its own.
fn fab_with_layout(
    name: &str,
    layout: &str,
) -> (TempDir, Result<Vec<netsmith::fab::Fabricated>, String>) {
    fab_with_physical(name, OUTLINE_AND_POURS_PHYSICAL, layout)
}

/// [`fab_with_layout`], with `physical` as the example's physical file.
fn fab_with_physical(
    name: &str,
    physical: &str,
    layout: &str,
) -> (TempDir, Result<Vec<netsmith::fab::Fabricated>, String>) {
    let cli = KicadCliExe::locate_runnable().expect("the caller checked that KiCad is here");
    let dir = out(name);
    let file = dir.join("source").join("outline_and_pours.pcb.toml");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, layout).unwrap();
    let made = fab_with_options(
        "outline_and_pours.nsm",
        OUTLINE_AND_POURS,
        &netsmith_check::Loader::std().with_physical(
            "outline_and_pours.nsm",
            "outline_and_pours.physical.nsm",
            physical,
        ),
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        Some(&cli),
        &FabOptions {
            layout: Some(file),
            ..FabOptions::default()
        },
    );
    (dir, made)
}

#[test]
fn the_outline_and_pours_example_passes_kicads_check_with_its_layout_file() {
    if kicad().is_none() {
        return;
    }
    let (dir, made) = fab_with_layout("outline-pours", OUTLINE_AND_POURS_LAYOUT);
    let made = made.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made.len(), 1);
    assert!(
        made[0].notes.contains("design rules check: passed"),
        "{}",
        made[0].notes
    );
    let pcb = read(
        &dir.join("fab"),
        "OutlineAndPours",
        "OutlineAndPours.kicad_pcb",
    );
    // The outline is 50 by 36 with round corners, and the slot is cut inside it.
    assert_eq!(
        pcb.matches("(gr_arc").count(),
        8,
        "four corners of the outline and of the slot"
    );
    // The keepout is where the physical file says: (18, 2) from the outline's corner at (20, 20).
    assert!(pcb.contains("(xy 38 22)"), "the keepout's corner");
    assert!(pcb.contains("(fp_text user \"REV A\""), "the label");
    // The ground pours are filled copper in the Gerbers, and the board is four layers.
    assert!(made[0].gerber_files.iter().any(|f| f.contains("F_Cu")));
    // KiCad plots the same board, with its outline, slot, keepout, pours and fiducials, and
    // agrees with netsmith's files.
    let report = netsmith_fab::crosscheck::check(
        &dir.join("fab/OutlineAndPours/OutlineAndPours.kicad_pcb"),
        &KicadCliExe::locate_runnable().expect("checked above"),
        &Default::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(report.ok(), "{report}");
    let notes = read(&dir.join("fab"), "OutlineAndPours", "fab-notes.txt");
    assert!(
        notes.contains("F.Cu is a pour on GND, B.Cu is a pour on GND"),
        "{notes}"
    );
}

#[test]
fn a_layout_file_entry_for_a_part_that_is_gone_stops_fab_naming_it() {
    if kicad().is_none() {
        return;
    }
    let (dir, made) = fab_with_layout(
        "stale-layout",
        &format!(
            "{OUTLINE_AND_POURS_LAYOUT}\n[OutlineAndPours.parts.\"stage.old_cap\"]\nat = [10.0, 10.0]\n"
        ),
    );
    let err = made.expect_err("the layout is out of date");
    assert!(
        err.contains("the board `OutlineAndPours` has no part `stage.old_cap` (its parts: ")
            && err.contains("outline_and_pours.pcb.toml"),
        "{err}"
    );
    assert!(!dir.join("fab").exists(), "nothing is written");
}

#[test]
fn an_example_whose_keepout_has_no_place_is_stopped_by_the_placement_check() {
    if kicad().is_none() {
        return;
    }
    let physical =
        OUTLINE_AND_POURS_PHYSICAL.replace("        .x &= 18mm\n        .y &= 2mm\n", "");
    assert_ne!(physical, OUTLINE_AND_POURS_PHYSICAL);
    let (_dir, made) = fab_with_physical("no-place", &physical, "version = 1\n");
    let err = made.expect_err("the keepout has nowhere to be");
    assert!(
        err.contains("the keepout `antenna` has no position"),
        "{err}"
    );
}

#[test]
fn a_part_the_layout_file_puts_on_the_bottom_is_on_the_back_in_the_package() {
    if kicad().is_none() {
        return;
    }
    let (dir, made) = fab_with_layout(
        "bottom-part",
        &format!(
            "{OUTLINE_AND_POURS_LAYOUT}\n[OutlineAndPours.parts.\"stage.input_cap\"]\nat = [25.0, 28.0]\nside = \"bottom\"\n"
        ),
    );
    let made = made.unwrap_or_else(|e| panic!("{e}"));
    assert!(
        made[0].notes.contains("design rules check: passed"),
        "{}",
        made[0].notes
    );
    let cpl = read(&dir.join("fab"), "OutlineAndPours", "cpl.csv");
    let bottom: Vec<&str> = cpl.lines().filter(|l| l.contains(",bottom,")).collect();
    assert_eq!(bottom.len(), 1, "{cpl}");
    let pcb = read(
        &dir.join("fab"),
        "OutlineAndPours",
        "OutlineAndPours.kicad_pcb",
    );
    assert!(pcb.contains("(layer \"B.Cu\")"), "a footprint on the back");
}

#[test]
fn without_kicad_the_check_runs_and_the_files_are_the_same_bytes() {
    if netsmith_kicad::testlib::skip_without_kicad_libraries() {
        return;
    }
    // The same board with and without KiCad: the design rules check passes, and the
    // Gerbers, the drill files, the placement file and the bill of materials are the same
    // bytes, because they are netsmith's own and so is the check (ADR 0037).
    let with = out("no-kicad-with");
    let without = out("no-kicad-without");
    let run = |dir: &Path, kicad: Option<&dyn KicadCli>| {
        fab_with_options(
            "led.nsm",
            LED,
            &netsmith_check::Loader::std(),
            dir,
            &Catalogs::jlcpcb(),
            &Libraries::standard(),
            kicad,
            &FabOptions::default(),
        )
    };
    run(&without, None).unwrap_or_else(|e| panic!("{e}"));
    let notes = read(&without, "LedExample", "fab-notes.txt");
    assert!(notes.contains("design rules check: passed"), "{notes}");
    assert!(!notes.contains("not run"), "{notes}");
    if let Some(cli) = kicad() {
        run(&with, Some(&cli)).unwrap_or_else(|e| panic!("{e}"));
        for file in [
            "LedExample-gerbers.zip",
            "bom.csv",
            "cpl.csv",
            "LedExample.kicad_pcb",
            "fab-notes.txt",
        ] {
            let path = |d: &Path| d.join("LedExample").join(file);
            assert_eq!(
                fs::read(path(&with)).unwrap(),
                fs::read(path(&without)).unwrap(),
                "{file} depends on KiCad"
            );
        }
    }
}

/// Fabricates `source` with the cross-check on: KiCad plots the same board and every layer,
/// drill file and placement must agree with netsmith's own.
fn cross_checked(name: &str, source: &str) {
    let Some(cli) = kicad() else { return };
    let dir = out(&format!("cross-{name}"));
    let made = fab_with_options(
        &format!("{name}.nsm"),
        source,
        &netsmith_check::Loader::std(),
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        Some(&cli),
        &FabOptions {
            cross_check: true,
            ..FabOptions::default()
        },
    )
    .unwrap_or_else(|e| panic!("{e}"));
    for board in &made {
        assert!(
            board
                .notes
                .contains("cross-check against KiCad: the files agree"),
            "{}",
            board.notes
        );
    }
}

#[test]
fn the_led_example_agrees_with_kicads_own_plot() {
    cross_checked("led", LED);
}

#[test]
fn the_regulator_example_agrees_with_kicads_own_plot() {
    cross_checked("linear_regulator", REGULATOR);
}

#[test]
fn the_four_layer_example_agrees_with_kicads_own_plot() {
    cross_checked("four_layer", FOUR_LAYER);
}

#[test]
fn the_two_board_example_agrees_with_kicads_own_plot() {
    cross_checked("two_boards", TWO_BOARDS);
}
