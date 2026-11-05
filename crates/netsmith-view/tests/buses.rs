//! Interfaces drawn as buses: the members of one interface instance that run between the same
//! two main parts, or leave a sheet from one, are short entries that meet one thick line.
//! The line joins nothing; each entry joins, by name, the net of that name at the other end.

mod common;

use common::bus::{board, rails, setup};
use netsmith_core::Design;
use netsmith_view::check::{Violation, check};
use netsmith_view::geom::Pt;
use netsmith_view::layout::Wire;
use netsmith_view::metrics::measure;
use netsmith_view::{Group, Layout, layout_sheets_rails, render_svg_by_module_with};

struct Drawn {
    design: Design,
    groups: Vec<Group>,
    layout: Layout,
    mapping: netsmith_kicad::Mapping,
    lib: netsmith_kicad::SymbolLib,
    _dir: netsmith_kicad::testlib::TempDir,
}

fn draw(lines: usize, caps: usize) -> Drawn {
    let (dir, mapping, lib) = setup();
    let (design, groups) = board(lines, caps);
    let layout = layout_sheets_rails(&design, &groups, &rails(), &mapping, &lib).unwrap();
    Drawn {
        design,
        groups,
        layout,
        mapping,
        lib,
        _dir: dir,
    }
}

fn kinds(found: &[Violation]) -> Vec<&'static str> {
    found.iter().map(Violation::kind).collect()
}

fn net(d: &Design, name: &str) -> usize {
    d.nets.iter().position(|n| n.name == name).unwrap()
}

#[test]
fn the_members_between_two_parts_are_one_bus() {
    let d = draw(4, 0);
    assert!(d.layout.sheets.is_empty(), "two parts are one drawing");
    assert_eq!(d.layout.buses.len(), 1, "{:?}", d.layout.buses);
    let bus = &d.layout.buses[0];
    // Named by the instance that has the first end's part in it, not the modem's.
    assert_eq!(bus.path, "mcu.uart0");
    assert_eq!(bus.name, "uart0");
    assert_eq!(bus.type_name, "Uart");
    assert_eq!(bus.members.len(), 4);
    assert_eq!(bus.ends.len(), 2);
    assert!(bus.flag.is_none());
    // Members in the order of the pins at the first end, down the page: TX RX RTS CTS.
    let names: Vec<&str> = bus.members.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["tx", "rx", "rts", "cts"]);
    // Each member has an entry at each end, and the free pin is not a member.
    assert_eq!(bus.entries.len(), 8);
    for end in 0..2 {
        let at_end: Vec<usize> = bus
            .entries
            .iter()
            .filter(|e| e.end == end)
            .map(|e| e.net)
            .collect();
        assert_eq!(at_end.len(), 4);
        assert!(!at_end.contains(&net(&d.design, "WAKE")));
    }
    // What the modem calls the same lines is written once, at the end.
    let other = bus.other.as_ref().expect("the modem names its lines apart");
    assert!(
        other.text.starts_with("uart: {rx, tx, cts, rts}"),
        "{}",
        other.text
    );
    assert_eq!(bus.list.text, "{tx, rx, rts, cts}");
    assert_eq!(bus.count.text, "4");
    // No flag and no label is drawn for a member.
    for m in &bus.members {
        assert!(d.layout.ports.iter().all(|p| p.net != m.net));
        assert!(d.layout.labels.iter().all(|l| l.net != m.net));
    }
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_drawing_is_the_netlist_with_a_bus() {
    // No wire joins the two parts: the entries join by name. Taking the entries away leaves
    // the nets open and the bus without members.
    let d = draw(4, 0);
    let mut l = d.layout.clone();
    for bus in &mut l.buses {
        bus.entries.clear();
    }
    let found = check(&d.design, &l);
    assert!(kinds(&found).contains(&"open"), "{found:?}");
    assert!(kinds(&found).contains(&"bus-member"), "{found:?}");
}

#[test]
fn a_bus_counts_once_in_the_numbers() {
    let d = draw(4, 0);
    let m = measure(&d.layout);
    assert_eq!(m.total.buses, 1);
    assert_eq!(m.total.bus_members, 4);
    assert_eq!(m.total.bus_flags, 0);
    assert_eq!(m.total.diagonal_segments, 0);
    // The members' entries end on the line and are not contacts or crossings.
    assert_eq!(m.total.contacts, 0, "{}", m.table());
    assert_eq!(m.total.collinear, 0, "{}", m.table());
    assert_eq!(m.total.flags, 0, "{}", m.table());
}

#[test]
fn members_that_go_on_to_another_sheet_end_in_one_flag() {
    let d = draw(4, 8);
    assert_eq!(d.layout.sheets.len(), 2);
    assert_eq!(d.layout.buses.len(), 2, "{:?}", d.layout.buses.len());
    let mut paths: Vec<&str> = d.layout.buses.iter().map(|b| b.path.as_str()).collect();
    paths.sort();
    // Each sheet names the bus by its own module's instance.
    assert_eq!(paths, ["mcu.uart0", "modem.uart"]);
    for bus in &d.layout.buses {
        assert_eq!(bus.ends.len(), 1);
        let flag = bus.flag.as_ref().expect("a flag ends the bus");
        assert_eq!(flag.label.text, bus.name);
        assert_eq!(bus.entries.len(), 4);
        for m in &bus.members {
            // One flag for the bus, none for each member.
            assert!(d.layout.ports.iter().all(|p| p.net != m.net));
        }
    }
    // Only the free line, which has a pin on each sheet, has flags of its own.
    let wake = net(&d.design, "WAKE");
    assert_eq!(d.layout.ports.len(), 2);
    assert!(d.layout.ports.iter().all(|p| p.net == wake));
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
    let m = measure(&d.layout);
    assert_eq!(m.total.buses, 2);
    assert_eq!(m.total.bus_flags, 2);
    assert_eq!(m.total.bus_members, 8);
}

#[test]
fn every_member_of_a_bundle_flag_has_its_entry_on_the_other_sheet() {
    let d = draw(4, 8);
    let nets = |i: usize| -> Vec<usize> {
        let mut v: Vec<usize> = d.layout.buses[i].members.iter().map(|m| m.net).collect();
        v.sort_unstable();
        v
    };
    // The two sheets' buses carry the same nets, under the names of their own modules.
    assert_eq!(nets(0), nets(1));
    assert_ne!(d.layout.buses[0].list.text, d.layout.buses[1].list.text);
}

#[test]
fn a_member_whose_pin_is_on_another_side_of_its_part_stays_out() {
    // The modem's `cts` line is moved to a pin on its other side: three of four are left.
    let (dir, mapping, lib) = setup();
    let (mut design, _) = board(4, 0);
    let cts = net(&design, "UART_CTS");
    let u2 = |p: &str| netsmith_core::PinRef {
        refdes: "U2".into(),
        pin: p.into(),
    };
    design.nets[cts].pins.retain(|p| p.refdes != "U2");
    design.nets[cts].pins.push(u2("6"));
    let vcc = net(&design, "VCC");
    design.nets[vcc]
        .pins
        .retain(|p| !(p.refdes == "U2" && p.pin == "6"));
    let layout = layout_sheets_rails(&design, &[], &rails(), &mapping, &lib).unwrap();
    assert_eq!(layout.buses.len(), 1);
    let names: Vec<&str> = layout.buses[0]
        .members
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(names, ["tx", "rx", "rts"]);
    let found = check(&design, &layout);
    assert!(found.is_empty(), "{found:?}");
    drop(dir);
}

#[test]
fn an_interface_with_fewer_than_three_members_stays_as_it_was() {
    let with = draw(2, 8);
    assert!(with.layout.buses.is_empty());
    // The same board with no interfaces told is the same drawing, byte for byte.
    let mut bare = with.design.clone();
    bare.interfaces.clear();
    let again =
        layout_sheets_rails(&bare, &with.groups, &rails(), &with.mapping, &with.lib).unwrap();
    assert_eq!(with.layout, again);
    // Its members are flags, as before.
    assert!(
        with.layout
            .ports
            .iter()
            .any(|p| with.design.nets[p.net].name == "UART_TX")
    );
    // The same on one sheet: wires or names.
    let one = draw(2, 0);
    assert!(one.layout.buses.is_empty());
    assert!(check(&one.design, &one.layout).is_empty());
}

#[test]
fn a_net_with_a_part_on_it_is_not_a_member() {
    // A resistor in series on two of the four lines puts a second part on their nets: the other
    // two are too few for a bus, and the board is drawn as before.
    let (dir, mapping, lib) = setup();
    let (mut design, _) = board(4, 0);
    for line in ["UART_TX", "UART_RX"] {
        let n = net(&design, line);
        let refdes = format!("R{}", design.parts.len());
        design.parts.push(netsmith_core::Part {
            refdes: refdes.clone(),
            kind: "Resistor".into(),
            value: "33".into(),
            package: Some("0402".into()),
            path: "mcu.r".into(),
            pins: Vec::new(),
        });
        design.nets[n].pins.push(netsmith_core::PinRef {
            refdes,
            pin: "1".into(),
        });
    }
    let layout = layout_sheets_rails(&design, &[], &rails(), &mapping, &lib).unwrap();
    assert!(layout.buses.is_empty(), "{}", layout.buses.len());
    drop(dir);
}

#[test]
fn the_drawing_is_the_same_every_time_and_keyed_by_path() {
    let a = draw(4, 8);
    let b = draw(4, 8);
    let svg = |d: &Drawn| {
        render_svg_by_module_with(&d.design, &d.groups, &rails(), &d.mapping, &d.lib).unwrap()
    };
    assert_eq!(svg(&a), svg(&b));
    // The order the interfaces are listed in does not change the drawing.
    let mut reversed = a.design.clone();
    reversed.interfaces.reverse();
    let again = layout_sheets_rails(&reversed, &a.groups, &rails(), &a.mapping, &a.lib).unwrap();
    assert_eq!(a.layout, again);
}

#[test]
fn the_svg_has_the_classes_of_a_bus() {
    let d = draw(4, 0);
    let svg =
        render_svg_by_module_with(&d.design, &d.groups, &rails(), &d.mapping, &d.lib).unwrap();
    for needle in [
        "class=\"bus\"",
        "class=\"wire bus-entry\"",
        "class=\"bus-label\"",
        "class=\"bus-count\"",
        "data-bus=\"mcu.uart0\"",
        "data-net=\"UART_TX\"",
    ] {
        assert!(svg.contains(needle), "missing {needle}");
    }
    // The members of a bus are not drawn as flags or labels.
    assert!(!svg.contains("class=\"port\" data-net=\"UART_TX\""));
}

// --- The rule `bus-member` catches what it is for -------------------------------------

fn bus_found(d: &Drawn, l: &Layout) -> Vec<String> {
    check(&d.design, l)
        .iter()
        .filter(|v| v.kind() == "bus-member")
        .map(ToString::to_string)
        .collect()
}

#[test]
fn an_entry_of_a_net_that_is_not_a_member_is_found() {
    let d = draw(4, 0);
    let mut l = d.layout.clone();
    l.buses[0].entries[0].net = net(&d.design, "WAKE");
    let found = bus_found(&d, &l);
    assert!(
        found
            .iter()
            .any(|f| f.contains("WAKE") && f.contains("not a member")),
        "{found:?}"
    );
}

#[test]
fn a_member_with_no_entry_at_one_end_is_found() {
    let d = draw(4, 0);
    let mut l = d.layout.clone();
    let gone = l.buses[0]
        .entries
        .iter()
        .position(|e| e.end == 1 && d.design.nets[e.net].name == "UART_RTS")
        .unwrap();
    l.buses[0].entries.remove(gone);
    let found = bus_found(&d, &l);
    assert!(
        found
            .iter()
            .any(|f| f.contains("UART_RTS") && f.contains("0 entries")),
        "{found:?}"
    );
}

#[test]
fn two_members_at_one_place_of_the_line_are_found() {
    let d = draw(4, 0);
    let mut l = d.layout.clone();
    let first = l.buses[0].entries[0].at;
    let other = l.buses[0]
        .entries
        .iter()
        .position(|e| e.end == 0 && e.at != first)
        .unwrap();
    l.buses[0].entries[other].at = first;
    let found = bus_found(&d, &l);
    assert!(
        found.iter().any(|f| f.contains("joined at one place")),
        "{found:?}"
    );
}

#[test]
fn an_entry_off_the_line_is_found() {
    let d = draw(4, 0);
    let mut l = d.layout.clone();
    let at = l.buses[0].entries[0].at;
    l.buses[0].entries[0].at = Pt::new(at.x + 400.0, at.y + 400.0);
    let found = bus_found(&d, &l);
    assert!(
        found.iter().any(|f| f.contains("not on the thick line")),
        "{found:?}"
    );
}

#[test]
fn a_wire_that_touches_the_line_is_found() {
    let d = draw(4, 0);
    let mut l = d.layout.clone();
    let (a, b) = l.buses[0].runs().into_iter().last().unwrap();
    // A wire of a net that is not a member ends on the middle of the trunk.
    let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    l.wires.push(Wire {
        net: net(&d.design, "WAKE"),
        points: vec![Pt::new(mid.x, mid.y - 20.0), mid],
    });
    let found = bus_found(&d, &l);
    assert!(
        found
            .iter()
            .any(|f| f.contains("WAKE") && f.contains("touches the thick line")),
        "{found:?}"
    );
}
