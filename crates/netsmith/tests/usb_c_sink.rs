//! `std::parts::connector::UsbCSink`: a USB Type-C receptacle for a device, with the Rd of the
//! Type-C specification (5.1 kohm on each of CC1 and CC2) built in and `.vbus`, the host's
//! supply, a source for the checks downstream. The boards are small fixtures; no KiCad.

use netsmith::{Built, Libraries, Outcome, build_with, outcome_with};
use netsmith_check::{Loader, check};
use netsmith_core::{Net, PinKind};
use netsmith_kicad::install::Search;
use netsmith_kicad::testlib::TempDir;
use netsmith_solve::{Catalogs, solve};

/// A device board with one receptacle: 5 V +/- 5 % from the host and a load of 100 mA to 300 mA.
fn board(jack: &str, extra: &str) -> String {
    format!(
        "import std::parts::connector::UsbCSink
import std::sources::Rail

board Device {{
    .jack: UsbCSink {{ {jack} }}
    .jack.vbus.voltage &= 5V +/- 5%
    .jack.vbus.pos.net &= \"VBUS\"
    .jack.vbus.neg.net &= \"GND\"
    .load: Rail {{ .current &= 100mA..300mA }}
    .jack.vbus ~ .load
{extra}
}}
"
    )
}

/// A machine with no KiCad install and no project: the standard mapping and libraries only.
fn libraries() -> Libraries {
    let nowhere = TempDir::new("usb-c-sink-no-kicad");
    Libraries::for_project_by(None, &Search::under(nowhere.path()))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn build(src: &str, catalogs: &Catalogs) -> Result<Built, String> {
    build_with("sink.nsm", src, &Loader::std(), catalogs, &libraries()).map(|mut b| b.remove(0))
}

fn built(jack: &str) -> Built {
    build(&board(jack, ""), &Catalogs::jlcpcb()).unwrap_or_else(|e| panic!("{e}"))
}

fn kind<'a>(built: &'a Built, kind: &str) -> Vec<&'a netsmith_core::Part> {
    built
        .design
        .parts
        .iter()
        .filter(|p| p.kind == kind)
        .collect()
}

/// The net that carries pad `pad` of part `refdes`.
fn net_of<'a>(built: &'a Built, refdes: &str, pad: &str) -> &'a Net {
    built
        .design
        .nets
        .iter()
        .find(|n| n.pins.iter().any(|p| p.refdes == refdes && p.pin == pad))
        .unwrap_or_else(|| panic!("no net carries {refdes}.{pad}"))
}

fn on_net(net: &Net, refdes: &str) -> Vec<String> {
    net.pins
        .iter()
        .filter(|p| p.refdes == refdes)
        .map(|p| p.pin.clone())
        .collect()
}

#[test]
fn the_receptacle_has_one_rd_resistor_on_each_cc_contact_to_ground() {
    let built = built(".rd_package &= \"0402\"");
    let jacks = kind(&built, "UsbCReceptacle16P");
    assert_eq!(jacks.len(), 1);
    let jack = &jacks[0].refdes;
    let resistors = kind(&built, "Resistor");
    assert_eq!(resistors.len(), 2, "two Rd resistors and nothing else");
    for r in &resistors {
        assert_eq!(r.package.as_deref(), Some("0402"), "{}", r.refdes);
        assert!(r.refdes.starts_with('R'), "{}", r.refdes);
    }
    let ground = net_of(&built, jack, "A1");
    // CC1 is A5 and CC2 is B5: each net is the contact and one pad of one resistor.
    let mut seen = Vec::new();
    for pad in ["A5", "B5"] {
        let cc = net_of(&built, jack, pad);
        assert_ne!(cc.name, ground.name, "{pad} is not ground");
        assert_eq!(cc.pins.len(), 2, "{pad}: {:?}", cc.pins);
        let other = cc.pins.iter().find(|p| &p.refdes != jack).unwrap();
        assert!(resistors.iter().any(|r| r.refdes == other.refdes), "{pad}");
        seen.push(other.refdes.clone());
        // The resistor's other pad is on ground.
        let far = if other.pin == "1" { "2" } else { "1" };
        assert_eq!(
            net_of(&built, &other.refdes, far).name,
            ground.name,
            "{pad}"
        );
    }
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 2, "CC1 and CC2 have a resistor each");
    // Ground is the four GND contacts, the shell and the two resistors.
    let mut contacts = on_net(ground, jack);
    contacts.sort();
    assert_eq!(contacts, ["A1", "A12", "B1", "B12", "S1"]);
    assert_eq!(ground.pins.len(), 5 + 2);
    // VBUS is the four contacts and the load: no resistor.
    let vbus = net_of(&built, jack, "A4");
    let mut contacts = on_net(vbus, jack);
    contacts.sort();
    assert_eq!(contacts, ["A4", "A9", "B4", "B9"]);
    assert_eq!(vbus.name, "VBUS");
}

#[test]
fn the_rd_resistors_are_5_1_kohm() {
    let built = built(".rd_package &= \"0402\"");
    for r in kind(&built, "Resistor") {
        assert!(r.value.starts_with("5.1k"), "{}: `{}`", r.refdes, r.value);
    }
    // Nothing is left to a pick the solver could not make: 5.1 kohm at 1 % is in the catalog.
    assert!(
        built
            .sourcing
            .iter()
            .zip(&built.design.parts)
            .filter(|(_, p)| p.kind == "Resistor")
            .all(|(s, _)| s.lcsc.is_some()),
        "every Rd has an LCSC number"
    );
}

#[test]
fn the_vbus_rail_is_a_source_that_supplies_what_the_loads_draw() {
    let src = board("", "");
    let checked = check("test.nsm", &src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let solution = solve(checked.board("Device").unwrap(), &Catalogs::jlcpcb());
    assert!(
        solution.diagnostics.is_empty(),
        "{:?}",
        solution.diagnostics
    );
    // The current of the receptacle's rail follows from the loads (ADR 0007): it is negative.
    let model = &checked.board("Device").unwrap().model;
    let vbus = model.inst_at("jack.vbus").unwrap();
    assert!(model.member(vbus, "current").is_some());
    // A budget on the supply is a bound the board writes, and it is judged with the numbers.
    let over = board("", "    .jack.vbus.current >= -200mA");
    let checked = check("test.nsm", &over, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let solution = solve(checked.board("Device").unwrap(), &Catalogs::jlcpcb());
    let messages: Vec<&str> = solution
        .diagnostics
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(messages.len(), 1, "{messages:#?}");
    assert!(
        messages[0].contains("jack.vbus.current must always be at least -200mA")
            && messages[0].contains("-300mA..-100mA"),
        "{}",
        messages[0]
    );
}

#[test]
fn it_is_a_connector_for_the_placer_and_the_schematic() {
    let built = built("");
    let jack = &kind(&built, "UsbCReceptacle16P")[0];
    assert!(jack.is_connector(), "the receptacle provides Connector");
    assert!(jack.refdes.starts_with('J'), "{}", jack.refdes);
    // The Rd resistors are not connectors.
    assert!(kind(&built, "Resistor").iter().all(|r| !r.is_connector()));
}

#[test]
fn a_board_that_uses_it_has_no_guideline_finding() {
    let strict = Catalogs::jlcpcb().with_strict_guidelines(true);
    let built = build(&board(".rd_package &= \"0402\"", ""), &strict)
        .unwrap_or_else(|e| panic!("a strict build must pass: {e}"));
    assert!(
        built.guidelines.is_empty(),
        "{:?}",
        built.guideline_warnings()
    );
}

#[test]
fn the_package_picks_the_footprint_and_the_default_is_the_gct_one() {
    let libraries = libraries();
    for (jack, package, footprint) in [
        (
            "",
            "GCT_USB4105_16P",
            "netsmith:USB_C_Receptacle_GCT_USB4105_16P_TopMnt_Horizontal",
        ),
        (
            ".package &= \"HRO_TYPE-C-31-M-12\"",
            "HRO_TYPE-C-31-M-12",
            "netsmith:USB_C_Receptacle_HRO_TYPE-C-31-M-12",
        ),
    ] {
        let built = built(jack);
        let part = &kind(&built, "UsbCReceptacle16P")[0];
        let mapped = libraries
            .mapping
            .map(&part.kind, part.package.as_deref())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(mapped.footprint, footprint, "{package}");
        // Every pad of the table is a pad of the footprint, and the other way round.
        let pads: std::collections::BTreeSet<String> = part
            .pins
            .iter()
            .flat_map(|p| p.pads.iter().cloned())
            .collect();
        let fp = libraries
            .footprints
            .load(&mapped.footprint)
            .unwrap_or_else(|e| panic!("{e}"));
        let fp_pads: std::collections::BTreeSet<String> = fp
            .pad_numbers()
            .into_iter()
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .collect();
        assert_eq!(pads, fp_pads, "{package}");
    }
}

#[test]
fn the_receptacle_has_the_pads_of_the_standard_usb_2_receptacle() {
    // `UsbC2` and the sink's receptacle are the same sixteen contacts, so a footprint is one
    // for both and a board can swap one for the other.
    let pads = |src: &str, kind: &str| -> std::collections::BTreeSet<String> {
        let built = build(src, &Catalogs::jlcpcb()).unwrap_or_else(|e| panic!("{e}"));
        kind_pads(&built, kind)
    };
    let usb2 = pads(
        "import std::parts::connector::UsbC2\nimport std::sources::Rail\n\
         board B {\n .jack: UsbC2\n .supply: Rail { .voltage &= 5V +/- 5% }\n .jack.usb.vbus ~ .supply\n}\n",
        "UsbC2",
    );
    let sink = pads(&board("", ""), "UsbCReceptacle16P");
    assert_eq!(usb2, sink);
    assert_eq!(sink.len(), 17);
}

fn kind_pads(built: &Built, name: &str) -> std::collections::BTreeSet<String> {
    kind(built, name)[0]
        .pins
        .iter()
        .flat_map(|p| p.pads.iter().cloned())
        .collect()
}

#[test]
fn the_pins_are_the_supply_the_data_pair_the_cc_pair_and_the_shell() {
    let built = built("");
    let jack = kind(&built, "UsbCReceptacle16P")[0];
    let row = |name: &str| -> (PinKind, Vec<String>) {
        let p = jack.pins.iter().find(|p| p.name == name).unwrap();
        (p.kind, p.pads.clone())
    };
    assert_eq!(row("vbus.pos").0, PinKind::Power);
    assert_eq!(row("vbus.pos").1, ["A4", "B4", "A9", "B9"]);
    assert_eq!(row("vbus.neg").1, ["A1", "B1", "A12", "B12"]);
    assert_eq!(row("dp").1, ["A6", "B6"]);
    assert_eq!(row("dm").1, ["A7", "B7"]);
    assert_eq!(row("cc1").1, ["A5"]);
    assert_eq!(row("cc2").1, ["B5"]);
    assert_eq!(row("sbu1").1, ["A8"]);
    assert_eq!(row("sbu2").1, ["B8"]);
    assert_eq!(row("shell").1, ["S1"]);
}

/// A receptacle of a project's own, as the Easyduino has one: a part that provides the generic,
/// with the maker's pin table for its own package and a one-row catalog. Invented.
const OWN_RECEPTACLE: &str = "\
import std::parts::connector::{Connector, UsbCSinkReceptacle}

part AcmeReceptacle provides UsbCSinkReceptacle, Connector {
    pins \"SMD\" {
        .vbus.pos power         \"VBUS\"   \"A4\" \"A9\" \"B4\" \"B9\"
        .vbus.neg power         \"GND\"    \"A1\" \"A12\" \"B1\" \"B12\"
        .cc1      passive       \"CC1\"    \"A5\"
        .cc2      passive       \"CC2\"    \"B5\"
        .dp       bidirectional \"D+\"     \"A6\" \"B6\"
        .dm       bidirectional \"D-\"     \"A7\" \"B7\"
        .sbu1     passive       \"SBU1\"   \"A8\"
        .sbu2     passive       \"SBU2\"   \"B8\"
        .shield   passive       \"SHIELD\" \"S1\"
    }
    .shell ~ .shield
    catalog acme_receptacle by .mpn { .mpn, .package }
}
";

const ACME_ROWS: &str = "\
lcsc,mpn,package,library,source
C0000001,ACME-USBC-16,SMD,Extended,https://example.invalid/acme-usbc-16
";

/// The board of `board`, with the project's receptacle in scope (a file the board imports).
fn with_own_receptacle(jack: &str) -> Result<Outcome, String> {
    let mut loader = Loader::std();
    loader.insert("src::acme", "src/acme.nsm", OWN_RECEPTACLE);
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file("acme_receptacle", "catalogs/acme_receptacle.csv", ACME_ROWS)
        .unwrap();
    let src = format!(
        "import src::acme::AcmeReceptacle   // brings the provider in scope\n{}",
        board(jack, "")
    );
    outcome_with("sink.nsm", &src, &loader, &catalogs, None).map_err(|problems| {
        problems
            .iter()
            .map(|p| p.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn parts_of(outcome: &Outcome, kind: &str) -> usize {
    outcome.boards[0]
        .design
        .parts
        .iter()
        .filter(|p| p.kind == kind)
        .count()
}

#[test]
fn a_provider_the_project_adds_is_chosen_for_its_package_and_number() {
    let outcome = with_own_receptacle(
        ".package &= \"SMD\"\n.receptacle_mpn &= \"ACME-USBC-16\"\n.rd_package &= \"0402\"",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(parts_of(&outcome, "AcmeReceptacle"), 1);
    assert_eq!(
        parts_of(&outcome, "UsbCReceptacle16P"),
        0,
        "not the standard one"
    );
    let built = &outcome.boards[0];
    let jack = built
        .design
        .parts
        .iter()
        .find(|p| p.kind == "AcmeReceptacle")
        .unwrap();
    assert!(jack.is_connector());
    // It is the same module: the Rd resistors, the shell on ground, the same nets.
    let ground = net_of(built, &jack.refdes, "A1");
    assert!(on_net(ground, &jack.refdes).contains(&"S1".to_string()));
    assert_eq!(ground.pins.len(), 5 + 2);
    let lcsc = built
        .sourcing
        .iter()
        .find(|s| s.refdes == jack.refdes)
        .and_then(|s| s.lcsc.as_deref());
    assert_eq!(lcsc, Some("C0000001"));
    assert!(outcome.boards.iter().all(|b| {
        b.design
            .parts
            .iter()
            .filter(|p| p.kind == "Resistor")
            .count()
            == 2
    }));
}

#[test]
fn the_standard_receptacle_stays_the_default_when_a_project_adds_a_provider() {
    // The project's receptacle is in scope, and the board states nothing about it: the standard
    // provider comes first and serves, with its footprint rows as before.
    let outcome = with_own_receptacle("").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(parts_of(&outcome, "UsbCReceptacle16P"), 1);
    assert_eq!(parts_of(&outcome, "AcmeReceptacle"), 0);
    // The standard package that the standard receptacle has a table for still picks it.
    let outcome =
        with_own_receptacle(".package &= \"HRO_TYPE-C-31-M-12\"").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(parts_of(&outcome, "UsbCReceptacle16P"), 1);
}

#[test]
fn a_package_that_no_provider_has_is_refused_with_the_reason_for_each() {
    let Err(err) = with_own_receptacle(".package &= \"QFN\"") else {
        panic!("a package that no provider has must not build");
    };
    assert!(
        err.contains("no UsbCSinkReceptacle provider satisfies"),
        "{err}"
    );
}

#[test]
fn the_standard_receptacle_is_a_provider_of_the_generic_and_a_connector() {
    // Placed by its own name it is the same part, with the same pins as the generic's use.
    let built = built(".package &= \"HRO_TYPE-C-31-M-12\"");
    let jack = &kind(&built, "UsbCReceptacle16P")[0];
    assert!(jack.provides.iter().any(|g| g == "UsbCSinkReceptacle"));
    assert!(jack.is_connector());
}
