//! Interfaces drawn as buses: the members of one interface instance that run between the same
//! two main parts, or leave a sheet from one, are short entries that meet one thick line.
//! The line joins nothing; each entry joins, by name, the net of that name at the other end.

use crate::common;

use common::bus::{board, board_directed, board_nested, board_shared, rails, setup};
use netsmith_core::Design;
use netsmith_view::check::{Violation, check};
use netsmith_view::geom::Pt;
use netsmith_view::layout::{MemberFlow, Wire};
use netsmith_view::metrics::measure;
use netsmith_view::{Group, Layout, layout_sheets_rails, render_svg_by_module_with};

struct Drawn {
    design: Design,
    groups: Vec<Group>,
    layout: Layout,
    mapping: netsmith_kicad::Mapping,
    lib: netsmith_kicad::SymbolLib,
    _dir: netsmith_testkit::TempDir,
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
            provides: Vec::new(),
            class: netsmith_testkit::std_class("Resistor").and_then(netsmith_core::PartClass::from_word),
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

#[test]
fn the_svg_says_a_buss_type_and_members_for_a_page_that_follows_it() {
    let d = draw(4, 0);
    let svg =
        render_svg_by_module_with(&d.design, &d.groups, &rails(), &d.mapping, &d.lib).unwrap();
    assert!(svg.contains("data-type=\"Uart\""), "{svg}");
    // The members in the order of the pins, as JSON, with the quotes escaped for the attribute.
    assert!(
        svg.contains(
            "data-members=\"[[&quot;tx&quot;,&quot;UART_TX&quot;],[&quot;rx&quot;,&quot;UART_RX&quot;],[&quot;rts&quot;,&quot;UART_RTS&quot;],[&quot;cts&quot;,&quot;UART_CTS&quot;]]\""
        ),
        "{svg}"
    );
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

#[test]
fn a_bus_flag_says_what_the_sheet_it_goes_to_calls_the_bus() {
    // The two sheets name the same lines by their own instances (`uart0`, `uart`) and call
    // the members by their own names (the modem's `tx` is the processor's `rx`): each flag is
    // followed by the other instance's path and member list, so the reader finds the other
    // end by what it is called there, and a page follows it by the path.
    let d = draw(4, 8);
    let by_path = |p: &str| d.layout.buses.iter().find(|b| b.path == p).unwrap();
    let mcu = by_path("mcu.uart0");
    let other = mcu.other.as_ref().expect("the flag says where it goes");
    assert_eq!(other.text, "\u{2192} modem.uart: {rx, tx, cts, rts}");
    assert_eq!(mcu.other_path.as_deref(), Some("modem.uart"));
    let modem = by_path("modem.uart");
    let other = modem.other.as_ref().expect("the flag says where it goes");
    // The processor's names for the modem's members, in the order the modem's bus lists them.
    let theirs: Vec<&str> = modem
        .members
        .iter()
        .map(|m| {
            let net = &d.design.nets[m.net].name;
            let inst = d.design.interfaces.iter().find(|i| i.path == "mcu.uart0");
            &inst
                .unwrap()
                .members
                .iter()
                .find(|x| &x.net == net)
                .unwrap()
                .name[..]
        })
        .collect();
    assert_eq!(
        other.text,
        format!("\u{2192} mcu.uart0: {{{}}}", theirs.join(", "))
    );
    assert_eq!(modem.other_path.as_deref(), Some("mcu.uart0"));
    // The drawing is still the netlist and the text lies on nothing.
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
    // The page reads the path from the flag.
    let svg = netsmith_view::render_layout(&d.design, &d.layout);
    assert!(svg.contains("data-other=\"modem.uart\""), "{svg}");
    assert!(svg.contains("data-other=\"mcu.uart0\""));
    // A bus between two parts on one sheet says its other name as it did, with no arrow, and
    // the page is told nothing new.
    let one = draw(4, 0);
    assert!(
        one.layout.buses[0]
            .other
            .as_ref()
            .unwrap()
            .text
            .starts_with("uart: ")
    );
    let svg = netsmith_view::render_layout(&one.design, &one.layout);
    assert!(!svg.contains("data-other"));
}

fn draw_directed(lines: usize, caps: usize) -> Drawn {
    let (dir, mapping, lib) = setup();
    let (design, groups) = board_directed(lines, caps);
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

fn flow_at(d: &Drawn, net_name: &str, end: usize) -> Option<MemberFlow> {
    let net = net(&d.design, net_name);
    let e = d.layout.buses[0]
        .entries
        .iter()
        .find(|e| e.net == net && e.end == end)
        .unwrap();
    e.flow
}

#[test]
fn an_entry_shows_which_way_its_signal_goes_from_the_pin_table() {
    use MemberFlow::{Gives, Takes};
    let d = draw_directed(4, 0);
    assert_eq!(d.layout.buses.len(), 1);
    // The processor is the first end: its `tx` and `rts` are outputs, `rx` and `cts` inputs.
    let first = [
        ("UART_TX", Gives),
        ("UART_RX", Takes),
        ("UART_RTS", Gives),
        ("UART_CTS", Takes),
    ];
    for (name, flow) in first {
        assert_eq!(flow_at(&d, name, 0), Some(flow), "{name} at the processor");
        let other = if flow == Gives { Takes } else { Gives };
        assert_eq!(flow_at(&d, name, 1), Some(other), "{name} at the modem");
    }
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_arrowhead_lies_on_its_entry_and_points_the_way_the_signal_goes() {
    use MemberFlow::{Both, Gives, Takes};
    let mut d = draw_directed(4, 0);
    // A bidirectional pin gets both ways, a passive one none (the drawing is made again).
    d.design.parts[0].pins[0].kind = netsmith_core::PinKind::Bidirectional;
    d.design.parts[0].pins[1].kind = netsmith_core::PinKind::Passive;
    d.layout = layout_sheets_rails(&d.design, &d.groups, &rails(), &d.mapping, &d.lib).unwrap();
    let bus = &d.layout.buses[0];
    let mut heads = 0;
    for e in &bus.entries {
        let name = &d.design.nets[e.net].name;
        let n = e.arrows().len();
        match (name.as_str(), e.end) {
            ("UART_TX", 0) => {
                assert_eq!((e.flow, n), (Some(Both), 2));
            }
            ("UART_RX", 0) => {
                assert_eq!((e.flow, n), (None, 0), "a passive pin has no arrow");
            }
            (_, 0) | (_, 1) => assert_eq!(n, 1, "{name}"),
            _ => unreachable!(),
        }
        // The wire runs from the pin to `at` along `out`; each head's three points are within
        // half a head's width of that line and between the two ends.
        let (dx, dy) = e.out.vector();
        let from = Pt::new(e.at.x - dx * e.length, e.at.y - dy * e.length);
        for tri in e.arrows() {
            heads += 1;
            for p in tri {
                let across = (p.x - from.x) * dy - (p.y - from.y) * dx;
                let along = (p.x - from.x) * dx + (p.y - from.y) * dy;
                assert!(across.abs() <= 0.5 + 1e-6, "{name}: {across}");
                assert!(
                    along >= -1e-6 && along <= e.length + 1e-6,
                    "{name}: {along}"
                );
            }
            // The tip is the point that is not level with the other two.
            let tip_along = (tri[0].x - from.x) * dx + (tri[0].y - from.y) * dy;
            let base_along = (tri[1].x - from.x) * dx + (tri[1].y - from.y) * dy;
            let pointing_out = tip_along > base_along;
            match e.flow.unwrap() {
                Gives => assert!(pointing_out, "{name}: an output points away from its part"),
                Takes => assert!(!pointing_out, "{name}: an input points at its part"),
                Both => {}
            }
        }
    }
    assert_eq!(
        heads,
        8 - 1 + 1,
        "one head a pin, two for the bidirectional one, none passive"
    );
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
    // The SVG has the polygons, and the look only when there is one.
    let svg = netsmith_view::render_layout(&d.design, &d.layout);
    assert_eq!(svg.matches("class=\"bus-arrow ").count(), heads);
    assert!(svg.contains(".schematic .bus-arrow{"));
    let plain = draw(4, 0);
    let svg = netsmith_view::render_layout(&plain.design, &plain.layout);
    assert!(!svg.contains("bus-arrow"));
}

#[test]
fn the_direction_of_the_members_changes_nothing_else_in_the_drawing() {
    // The same board with its pin tables and without: the same wires, bends, crossings, sizes.
    let with = draw_directed(4, 0);
    let without = draw(4, 0);
    assert_eq!(
        measure(&with.layout).table(),
        measure(&without.layout).table()
    );
    assert_eq!(with.layout.bounds, without.layout.bounds);
    assert_eq!(with.layout.wires, without.layout.wires);
    let (a, b) = (&with.layout.buses[0], &without.layout.buses[0]);
    assert_eq!(a.lines, b.lines);
    assert_eq!(a.list, b.list);
}

fn draw_nested(caps: usize) -> Drawn {
    let (dir, mapping, lib) = setup();
    let (design, groups) = board_nested(caps);
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

#[test]
fn an_interface_that_nests_another_is_one_bus_with_dotted_member_names() {
    // `link` has `irq` and, inside it, `data` with three lines: four lines, one bus, named by
    // the outer interface; the members of `data` carry its name before theirs. (Drawn as it
    // was, `data` was a bus of three and `irq` a net of its own.)
    let d = draw_nested(0);
    assert_eq!(d.layout.buses.len(), 1, "{:?}", d.layout.buses.len());
    let bus = &d.layout.buses[0];
    assert_eq!(bus.path, "mcu.link");
    assert_eq!(bus.name, "link");
    assert_eq!(bus.type_name, "Link");
    assert_eq!(bus.members.len(), 4);
    assert_eq!(bus.count.text, "4");
    // In the order of the processor's pins: tx, rx, rts, cts.
    assert_eq!(bus.list.text, "{data.sclk, data.mosi, irq, data.miso}");
    // The nested rail is power: it is no member, and no wire or flag of the line is lost.
    assert!(bus.members.iter().all(|m| !m.name.contains("power")));
    for m in &bus.members {
        assert!(d.layout.ports.iter().all(|p| p.net != m.net));
        assert!(d.layout.labels.iter().all(|l| l.net != m.net));
    }
    assert!(d.layout.buses.iter().all(|b| b.path != "mcu.link.data"));
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
    let m = measure(&d.layout);
    assert_eq!((m.total.buses, m.total.bus_members), (1, 4));
}

#[test]
fn a_nested_interface_is_one_bus_on_each_sheet() {
    let d = draw_nested(8);
    assert_eq!(d.layout.sheets.len(), 2);
    let mut paths: Vec<&str> = d.layout.buses.iter().map(|b| b.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, ["mcu.link", "modem.link"]);
    for bus in &d.layout.buses {
        assert_eq!(bus.members.len(), 4, "{}", bus.path);
        assert!(bus.flag.is_some());
        // The other sheet's instance names the same lines the same way, and is told by path.
        let other = bus.other.as_ref().expect("the flag says where it goes");
        assert!(other.text.starts_with("\u{2192} "), "{}", other.text);
        assert!(other.text.contains("data.sclk"), "{}", other.text);
    }
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
}

fn draw_shared(devices: usize, lines: usize, caps: usize) -> Drawn {
    let (dir, mapping, lib) = setup();
    let (design, groups) = board_shared(devices, lines, caps);
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

#[test]
fn the_parts_on_one_shared_line_are_one_bus_with_a_branch_to_each() {
    // Four parts, three lines that have a pin on each: one bus, one thick line with a spine
    // and an entry at each part. (Drawn as it was, each of the three was a net of four pins.)
    let d = draw_shared(3, 3, 0);
    assert!(d.layout.sheets.is_empty());
    assert_eq!(d.layout.buses.len(), 1, "{:?}", d.layout.buses.len());
    let bus = &d.layout.buses[0];
    assert_eq!(bus.path, "mcu.spi");
    assert_eq!(bus.name, "spi");
    assert_eq!(bus.members.len(), 3);
    assert_eq!(bus.ends.len(), 4);
    assert!(bus.flag.is_none());
    assert_eq!(bus.count.text, "3");
    assert_eq!(bus.list.text, "{sclk, mosi, miso}");
    // The processor, the first end, is the part in the instance's own module.
    assert_eq!(d.design.parts[bus.ends[0]].refdes, "U1");
    // Three entries at each of the four parts, one for each member.
    assert_eq!(bus.entries.len(), 12);
    for end in 0..4 {
        let mut nets: Vec<usize> = bus
            .entries
            .iter()
            .filter(|e| e.end == end)
            .map(|e| e.net)
            .collect();
        nets.sort_unstable();
        let mut want: Vec<usize> = bus.members.iter().map(|m| m.net).collect();
        want.sort_unstable();
        assert_eq!(nets, want, "end {end}");
    }
    // One spine for each part and a run to each of the three that are not the first: the
    // thick line is one tree.
    assert!(bus.lines.len() >= 4 + 3, "{}", bus.lines.len());
    // No member has a flag, a label or a wire between the parts of its own.
    for m in &bus.members {
        assert!(d.layout.ports.iter().all(|p| p.net != m.net));
        assert!(d.layout.labels.iter().all(|l| l.net != m.net));
        let wires = d.layout.wires.iter().filter(|w| w.net == m.net).count();
        assert_eq!(
            wires, 4,
            "only the four entries of {}",
            d.design.nets[m.net].name
        );
    }
    // The select line of the first sensor is not a member and is a net as before.
    let cs = net(&d.design, "CS1");
    assert!(bus.members.iter().all(|m| m.net != cs));
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
    let m = measure(&d.layout);
    assert_eq!(
        (m.total.buses, m.total.bus_members, m.total.bus_flags),
        (1, 3, 0)
    );
    assert_eq!(m.total.diagonal_segments, 0);
    assert_eq!(m.total.collinear, 0, "{}", m.table());
}

#[test]
fn two_lines_shared_by_three_parts_are_a_bus_and_by_two_are_not() {
    // An I2C-sized bus: the threshold is three members between two places, two among three.
    let shared = draw_shared(2, 2, 0);
    assert_eq!(shared.layout.buses.len(), 1);
    assert_eq!(shared.layout.buses[0].members.len(), 2);
    assert_eq!(shared.layout.buses[0].ends.len(), 3);
    let found = check(&shared.design, &shared.layout);
    assert!(found.is_empty(), "{found:?}");
    let pair = draw_shared(1, 2, 0);
    assert!(pair.layout.buses.is_empty(), "two lines between two parts");
}

#[test]
fn a_shared_bus_is_one_bus_on_each_sheet_with_a_flag_and_the_parts_of_that_sheet() {
    let d = draw_shared(4, 3, 14);
    assert_eq!(d.layout.sheets.len(), 2);
    assert_eq!(d.layout.buses.len(), 2, "one bus on each sheet");
    let by_path = |p: &str| d.layout.buses.iter().find(|b| b.path == p).unwrap();
    // The processor's sheet has the processor and two sensors at its ends, the other sheet
    // the other two sensors; both go on, in one flag each.
    let first = by_path("mcu.spi");
    assert_eq!(first.ends.len(), 3);
    assert!(first.flag.is_some());
    let second = d
        .layout
        .buses
        .iter()
        .find(|b| b.path != "mcu.spi")
        .expect("a bus on the second sheet");
    assert_eq!(second.ends.len(), 2);
    assert!(second.flag.is_some());
    for bus in [first, second] {
        assert_eq!(bus.members.len(), 3, "{}", bus.path);
        assert_eq!(bus.entries.len(), 3 * bus.ends.len());
        for m in &bus.members {
            assert!(d.layout.ports.iter().all(|p| p.net != m.net));
        }
    }
    // Each flag says which instance of the other sheet it goes to, never one of its own.
    assert!(
        first
            .other_path
            .as_deref()
            .is_some_and(|p| p.starts_with("bank.")),
        "{:?}",
        first.other_path
    );
    assert_eq!(second.other_path.as_deref(), Some("mcu.spi"));
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
    let m = measure(&d.layout);
    assert_eq!(
        (m.total.buses, m.total.bus_members, m.total.bus_flags),
        (2, 6, 2)
    );
}

#[test]
fn a_net_with_a_part_beside_a_pin_is_still_not_a_member_of_a_shared_bus() {
    // A pull-up on one line: the net has a part that is not a main part, so it stays a net
    // (the rule of the two-end bus), and the other two lines are a bus of two on four parts.
    let (dir, mapping, lib) = setup();
    let (mut design, groups) = board_shared(3, 3, 0);
    design.parts.push(netsmith_core::Part {
        provides: Vec::new(),
        class: netsmith_testkit::std_class("Resistor").and_then(netsmith_core::PartClass::from_word),
        refdes: "R1".into(),
        kind: "Resistor".into(),
        value: "4.7k".into(),
        package: Some("0402".into()),
        path: "mcu.r1".into(),
        pins: Vec::new(),
    });
    let sclk = net(&design, "SCLK");
    design.nets[sclk].pins.push(netsmith_core::PinRef {
        refdes: "R1".into(),
        pin: "1".into(),
    });
    let vcc = net(&design, "VCC");
    design.nets[vcc].pins.push(netsmith_core::PinRef {
        refdes: "R1".into(),
        pin: "2".into(),
    });
    let layout = layout_sheets_rails(&design, &groups, &rails(), &mapping, &lib).unwrap();
    let found = check(&design, &layout);
    assert!(found.is_empty(), "{found:?}");
    for bus in &layout.buses {
        assert!(bus.members.iter().all(|m| m.net != sclk));
    }
    drop(dir);
}
