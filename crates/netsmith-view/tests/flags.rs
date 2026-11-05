//! Nets that leave a sheet end in a flag, at the end of the stub of the pin, whichever way the
//! pin leaves (a flag of a pin that leaves up or down is written upward), and no flag may run
//! through a part, along a wire of another net, or over where another flag stands.
//! Each case is a connector with all its pins on one side whose nets all go to another sheet.

mod common;

use common::rowcol::setup;
use netsmith_core::Design;
use netsmith_view::check::check;
use netsmith_view::layout_sheets;

/// `J1` (a row of `n` pins on top) and `U1` (`n` pins down its left), pin `k` of one on a
/// net with pin `k` of the other, each part in a module of its own.
fn board(n: u32) -> (Design, Vec<netsmith_view::Group>) {
    let nets: Vec<(String, Vec<String>)> = (1..=n)
        .map(|k| (format!("N{k}"), vec![format!("J1.{k}"), format!("U1.{k}")]))
        .collect();
    let refs: Vec<(&str, Vec<&str>)> = nets
        .iter()
        .map(|(name, pins)| (name.as_str(), pins.iter().map(String::as_str).collect()))
        .collect();
    let borrowed: Vec<(&str, &[&str])> = refs.iter().map(|(n, p)| (*n, p.as_slice())).collect();
    // Enough resistors on no net to make the board too big for one drawing.
    let fillers: Vec<String> = (1..=16).map(|k| format!("R{k}")).collect();
    let mut parts = vec![
        ("J1", "Row", "Row", Some("P")),
        ("U1", "Col", "Col", Some("P")),
    ];
    parts.extend(
        fillers
            .iter()
            .map(|r| (r.as_str(), "Resistor", "1k", Some("0402"))),
    );
    let design = common::design("Flags", &parts, &borrowed);
    let group = |name: &str, parts: Vec<usize>| netsmith_view::Group {
        name: name.into(),
        detail: String::new(),
        parts,
    };
    let groups = vec![
        group("one", vec![0]),
        group("two", vec![1]),
        group("rest", (2..design.parts.len()).collect()),
    ];
    (design, groups)
}

#[test]
fn the_wire_to_a_flag_does_not_run_where_a_flag_below_it_stands() {
    // Flags of neighbouring pins stand one pin pitch apart. A wire to one used to run along
    // the row of the next, through the place where that flag was then drawn, and shut its way
    // in: two nets ended in one piece of wire.
    for n in [3, 4, 6, 8] {
        let (_dir, mapping, lib) = setup(n);
        let (design, groups) = board(n);
        let l = layout_sheets(&design, &groups, &mapping, &lib).unwrap();
        let found = check(&design, &l);
        assert!(found.is_empty(), "{n} pins: {found:?}");
    }
}

#[test]
fn a_column_of_flags_taller_than_the_margin_of_the_grid_is_still_reached() {
    // Twenty flags are more than the margin the grid had round the parts: the last ones were
    // outside it, and their wires were laid straight through the connector.
    for n in [16, 20] {
        let (_dir, mapping, lib) = setup(n);
        let (design, groups) = board(n);
        let l = layout_sheets(&design, &groups, &mapping, &lib).unwrap();
        let found = check(&design, &l);
        assert!(found.is_empty(), "{n} pins: {found:?}");
        assert_eq!(
            l.ports.len(),
            2 * n as usize,
            "{n} pins: a flag for every net, on each sheet"
        );
    }
}
