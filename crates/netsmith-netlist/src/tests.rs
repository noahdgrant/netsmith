//! The comparison on hand-written netlists, which need no KiCad.

use super::*;

/// A netlist from `("R1", "10k", "R_0402")` components and `("VCC", ["R1.1", "J1.1"])` nets.
fn nl(components: &[(&str, &str, &str)], nets: &[(&str, &[&str])]) -> Netlist {
    Netlist::from_parts(
        components
            .iter()
            .map(|(r, v, f)| (r.to_string(), v.to_string(), f.to_string()))
            .collect(),
        nets.iter()
            .map(|(n, pins)| {
                (
                    n.to_string(),
                    pins.iter()
                        .map(|p| {
                            let (r, pin) = p.split_once('.').unwrap();
                            (r.to_string(), pin.to_string())
                        })
                        .collect(),
                )
            })
            .collect(),
    )
    .unwrap()
}

const R: &str = "Resistor_SMD:R_0402_1005Metric";
const LED: &str = "LED_SMD:LED_0603_1608Metric";
const HDR: &str = "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical";

fn led() -> Netlist {
    nl(
        &[("J1", "Conn", HDR), ("R1", "330", R), ("D1", "LED", LED)],
        &[
            ("VCC", &["J1.1", "R1.1"]),
            ("Net-(D1-A)", &["R1.2", "D1.1"]),
            ("GND", &["D1.2", "J1.2"]),
        ],
    )
}

/// The same circuit with other designators and other net names.
fn led_renumbered() -> Netlist {
    nl(
        &[("D7", "LED", LED), ("J9", "Conn", HDR), ("R4", "330", R)],
        &[
            ("N$2", &["D7.2", "J9.2"]),
            ("+5V", &["R4.1", "J9.1"]),
            ("N$1", &["D7.1", "R4.2"]),
        ],
    )
}

fn compare_default(a: &Netlist, b: &Netlist) -> Outcome {
    compare(a, b, &Options::default()).unwrap()
}

fn differences(o: Outcome) -> Vec<Difference> {
    match o {
        Outcome::Different(d) => d,
        other => panic!("expected a difference, got {other:?}"),
    }
}

#[test]
fn a_netlist_matches_itself() {
    let Outcome::Match(c) = compare_default(&led(), &led()) else {
        panic!("a netlist must match itself");
    };
    assert_eq!(c.components.len(), 3);
    assert_eq!(c.nets.len(), 3);
    assert!(c.components.iter().all(|(a, b)| a == b));
    assert_eq!(c.interchangeable, 0);
    assert!(report(&Outcome::Match(c)).contains("same designator"));
}

#[test]
fn other_designators_and_net_names_still_match_and_the_correspondence_is_reported() {
    let Outcome::Match(c) = compare_default(&led(), &led_renumbered()) else {
        panic!("renumbering must not matter");
    };
    assert!(c.components.contains(&("R1".into(), "R4".into())));
    assert!(c.components.contains(&("D1".into(), "D7".into())));
    assert!(c.nets.contains(&("VCC".into(), "+5V".into())));
    assert!(c.nets.contains(&("GND".into(), "N$2".into())));
    let text = report(&Outcome::Match(c));
    assert!(
        text.contains("R1 -> R4") && text.contains("`VCC` -> `+5V`"),
        "{text}"
    );
}

#[test]
fn a_miswired_pin_is_named_with_its_nets_and_both_designators() {
    // R4 (R1 in A) is wired to the ground net instead of the supply.
    let b = nl(
        &[("D7", "LED", LED), ("J9", "Conn", HDR), ("R4", "330", R)],
        &[
            ("N$2", &["D7.2", "J9.2", "R4.1"]),
            ("+5V", &["J9.1"]),
            ("N$1", &["D7.1", "R4.2"]),
        ],
    );
    let d = differences(compare_default(&led(), &b));
    assert_eq!(d.len(), 2, "{d:?}");
    let text = report(&Outcome::Different(d));
    assert!(
        text.contains("net `VCC` (A) and `+5V` (B)")
            && text.contains("only in A: R1.1 (in B, R4.1 is on net `N$2`)"),
        "{text}"
    );
    assert!(
        text.contains("net `GND` (A) and `N$2` (B)")
            && text.contains("only in B: R4.1 (in A, R1.1 is on net `VCC`)"),
        "{text}"
    );
}

#[test]
fn a_resistor_of_another_value_is_one_difference() {
    let b = nl(
        &[("J1", "Conn", HDR), ("R1", "470", R), ("D1", "LED", LED)],
        &[
            ("VCC", &["J1.1", "R1.1"]),
            ("X", &["R1.2", "D1.1"]),
            ("GND", &["D1.2", "J1.2"]),
        ],
    );
    let d = differences(compare_default(&led(), &b));
    assert_eq!(d.len(), 1, "{d:?}");
    let text = report(&Outcome::Different(d));
    assert!(
        text.contains("R1 (A) and R1 (B) correspond but differ: A is 330 in")
            && text.contains("B is 470 in"),
        "{text}"
    );
}

#[test]
fn another_footprint_is_a_difference_but_another_library_is_not() {
    let other_library = nl(
        &[
            ("J1", "Conn", HDR),
            ("R1", "330", "R_0402_1005Metric"),
            ("D1", "LED", LED),
        ],
        &[
            ("VCC", &["J1.1", "R1.1"]),
            ("X", &["R1.2", "D1.1"]),
            ("GND", &["D1.2", "J1.2"]),
        ],
    );
    assert!(matches!(
        compare_default(&led(), &other_library),
        Outcome::Match(_)
    ));
    let bigger = nl(
        &[
            ("J1", "Conn", HDR),
            ("R1", "330", "Resistor_SMD:R_0805_2012Metric"),
            ("D1", "LED", LED),
        ],
        &[
            ("VCC", &["J1.1", "R1.1"]),
            ("X", &["R1.2", "D1.1"]),
            ("GND", &["D1.2", "J1.2"]),
        ],
    );
    let text = report(&compare_default(&led(), &bigger));
    assert!(
        text.contains("R_0402_1005Metric") && text.contains("R_0805_2012Metric"),
        "{text}"
    );
}

#[test]
fn a_missing_and_an_extra_component_are_each_listed() {
    // B has no resistor, and has a capacitor across the supply.
    let b = nl(
        &[
            ("D7", "LED", LED),
            ("J9", "Conn", HDR),
            ("C1", "100n", "C_0402"),
        ],
        &[
            ("N$2", &["D7.2", "J9.2", "C1.2"]),
            ("+5V", &["J9.1", "C1.1", "D7.1"]),
        ],
    );
    let text = report(&compare_default(&led(), &b));
    assert!(text.contains("component only in A: R1 (330"), "{text}");
    assert!(text.contains("component only in B: C1 (100n"), "{text}");
}

/// `n` resistors in parallel between two nets.
fn parallel(n: usize, names: bool) -> Netlist {
    let comps: Vec<(String, String, String)> = (1..=n)
        .map(|i| (format!("R{i}"), "10k".to_string(), R.to_string()))
        .collect();
    let (hi, lo) = if names { ("A", "B") } else { ("X", "Y") };
    Netlist::from_parts(
        comps,
        vec![
            (
                hi.into(),
                (1..=n)
                    .map(|i| (format!("R{i}"), "1".to_string()))
                    .collect(),
            ),
            (
                lo.into(),
                (1..=n)
                    .map(|i| (format!("R{i}"), "2".to_string()))
                    .collect(),
            ),
        ],
    )
    .unwrap()
}

#[test]
fn identical_resistors_in_parallel_match_by_individualization() {
    let Outcome::Match(c) = compare_default(&parallel(2, true), &parallel(2, false)) else {
        panic!("two parallel resistors are the same circuit");
    };
    assert_eq!(c.components.len(), 2);
    assert!(c.interchangeable >= 1, "the pairing was a choice");
    assert!(report(&Outcome::Match(c)).contains("interchangeable"));
}

#[test]
fn two_parallel_resistors_do_not_match_three() {
    let d = differences(compare_default(&parallel(2, true), &parallel(3, true)));
    let text = report(&Outcome::Different(d));
    assert!(text.contains("component only in B: R3"), "{text}");
}

/// `n` rows of an LED and its resistor from the supply to ground.
fn led_array(n: usize, offset: usize) -> Netlist {
    let mut comps = Vec::new();
    let mut nets: Vec<(String, Vec<(String, String)>)> =
        vec![("VCC".into(), Vec::new()), ("GND".into(), Vec::new())];
    for i in 0..n {
        let (r, d) = (format!("R{}", i + offset), format!("D{}", i + offset));
        comps.push((r.clone(), "330".to_string(), R.to_string()));
        comps.push((d.clone(), "LED".to_string(), LED.to_string()));
        nets[0].1.push((r.clone(), "1".into()));
        nets[1].1.push((d.clone(), "2".into()));
        nets.push((format!("row{i}"), vec![(r, "2".into()), (d, "1".into())]));
    }
    Netlist::from_parts(comps, nets).unwrap()
}

#[test]
fn an_led_array_matches_however_it_is_numbered() {
    let Outcome::Match(c) = compare_default(&led_array(8, 1), &led_array(8, 20)) else {
        panic!("rows of an array are interchangeable");
    };
    assert_eq!(c.components.len(), 16);
    assert_eq!(c.nets.len(), 10);
    // Each LED is paired with the LED of the row of its own resistor.
    let a = led_array(8, 1);
    let b = led_array(8, 20);
    let map: std::collections::HashMap<_, _> = c.components.iter().cloned().collect();
    for i in 0..8 {
        let r = map[&format!("R{}", i + 1)].clone();
        let d = map[&format!("D{}", i + 1)].clone();
        assert_eq!(
            r[1..].parse::<usize>().unwrap(),
            d[1..].parse::<usize>().unwrap()
        );
    }
    assert!(a.components.len() == b.components.len());
}

#[test]
fn an_array_with_one_row_miswired_is_not_the_same() {
    let a = led_array(4, 1);
    let mut b = led_array(4, 1);
    // Row 2's LED is turned round.
    let i = b.find("D2").unwrap();
    b.components[i].pins = vec![
        ("1".into(), b.components[i].pins[1].1),
        ("2".into(), b.components[i].pins[0].1),
    ];
    for net in &mut b.nets {
        for (c, pin) in &mut net.pins {
            if *c == i {
                *pin = if pin == "1" { "2".into() } else { "1".into() };
            }
        }
    }
    let d = differences(compare_default(&a, &b));
    let text = report(&Outcome::Different(d));
    assert!(text.contains("D2.1") || text.contains("D2.2"), "{text}");
    assert!(!text.contains("D1.") && !text.contains("D3."), "{text}");
}

#[test]
fn a_ring_of_six_is_not_two_rings_of_three() {
    let ring = |sizes: &[usize]| {
        let mut comps = Vec::new();
        let mut nets = Vec::new();
        let mut next = 1;
        for &size in sizes {
            let first = next;
            for k in 0..size {
                let this = first + k;
                let following = first + (k + 1) % size;
                comps.push((format!("R{this}"), "1k".to_string(), R.to_string()));
                nets.push((
                    format!("n{this}"),
                    vec![
                        (format!("R{this}"), "2".to_string()),
                        (format!("R{following}"), "1".to_string()),
                    ],
                ));
            }
            next += size;
        }
        Netlist::from_parts(comps, nets).unwrap()
    };
    let outcome = compare_default(&ring(&[6]), &ring(&[3, 3]));
    assert!(matches!(outcome, Outcome::Different(_)), "{outcome:?}");
    assert!(matches!(
        compare_default(&ring(&[3, 3]), &ring(&[3, 3])),
        Outcome::Match(_)
    ));
}

#[test]
fn a_search_past_its_bound_says_where_it_stopped() {
    let options = Options {
        min_refinements: 0,
        refinements_per_component: 0,
        ..Options::default()
    };
    let o = compare(&parallel(3, true), &parallel(3, false), &options).unwrap();
    let Outcome::Undecided(stop) = &o else {
        panic!("expected undecided, got {o:?}");
    };
    assert_eq!(stop.refinements, 1);
    assert_eq!(stop.limit, 0);
    assert_eq!(stop.ambiguous.len(), 1, "{stop:?}");
    assert_eq!(stop.ambiguous[0].a, ["R1", "R2", "R3"]);
    let why = report(&o);
    assert!(
        why.contains("could not decide")
            && why.contains("3 components of 10k")
            && why.contains("R1, R2, R3")
            && why.contains("after 1 refinement")
            && why.contains("limit of 0 refinements"),
        "{why}"
    );
}

#[test]
fn the_bound_grows_with_the_board() {
    let options = Options::default();
    assert_eq!(options.refinement_bound(100), 20_000);
    assert_eq!(options.refinement_bound(1_000), 20_000);
    assert_eq!(options.refinement_bound(5_000), 100_000);
}

/// `n` groups of two equal resistors in parallel, each group between nets of its own: each
/// group is one choice, so the search runs about one refinement per group.
fn parallel_groups(n: usize, offset: usize) -> Netlist {
    let mut comps = Vec::new();
    let mut nets = Vec::new();
    for g in 0..n {
        let (x, y) = (
            format!("R{}", offset + 2 * g),
            format!("R{}", offset + 2 * g + 1),
        );
        comps.push((x.clone(), "10k".to_string(), R.to_string()));
        comps.push((y.clone(), "10k".to_string(), R.to_string()));
        nets.push((
            format!("hi{g}"),
            vec![(x.clone(), "1".to_string()), (y.clone(), "1".to_string())],
        ));
        nets.push((
            format!("lo{g}"),
            vec![(x, "2".to_string()), (y, "2".to_string())],
        ));
    }
    Netlist::from_parts(comps, nets).unwrap()
}

#[test]
fn a_board_past_a_fixed_bound_decides_with_the_bound_scaled_by_its_components() {
    let (a, b) = (parallel_groups(40, 1), parallel_groups(40, 500));
    let Outcome::Match(c) = compare_default(&a, &b) else {
        panic!("the groups are the same circuit");
    };
    let needed = c.refinements;
    assert!(needed > 40, "{needed}");
    assert!(needed < 20 * 160, "{needed}");
    // A fixed bound just short of what the board needs cannot decide it (the last
    // refinement makes no choice, so it is not held to the bound)...
    let fixed = Options {
        min_refinements: needed - 2,
        refinements_per_component: 0,
        ..Options::default()
    };
    assert!(matches!(
        compare(&a, &b, &fixed).unwrap(),
        Outcome::Undecided(_)
    ));
    // ...and the same floor grown by the board's 160 components does.
    let scaled = Options {
        min_refinements: needed - 2,
        ..Options::default()
    };
    assert!(matches!(
        compare(&a, &b, &scaled).unwrap(),
        Outcome::Match(_)
    ));
    // Past the scaled bound, the answer says how far it got and what is still alike.
    let short = Options {
        min_refinements: needed / 4,
        refinements_per_component: 0,
        ..Options::default()
    };
    let Outcome::Undecided(stop) = compare(&a, &b, &short).unwrap() else {
        panic!("past its bound");
    };
    assert_eq!(stop.refinements, needed / 4 + 1);
    assert!(!stop.ambiguous.is_empty());
    assert!(
        stop.ambiguous
            .iter()
            .all(|g| g.value == "10k" && g.a.len() == g.b.len()),
        "{stop:?}"
    );
    let text = report(&Outcome::Undecided(stop));
    assert!(text.contains("still alike"), "{text}");
}

/// The LED board of `led()` with R1's two pads the other way round.
fn led_with_r1_turned(footprint: &str) -> Netlist {
    nl(
        &[
            ("J1", "Conn", HDR),
            ("R1", "330", footprint),
            ("D1", "LED", LED),
        ],
        &[
            ("VCC", &["J1.1", "R1.2"]),
            ("Net-(D1-A)", &["R1.1", "D1.1"]),
            ("GND", &["D1.2", "J1.2"]),
        ],
    )
}

#[test]
fn a_resistor_with_its_pads_swapped_matches_and_the_swap_is_a_note() {
    let Outcome::Match(c) = compare_default(&led(), &led_renumbered_with_r_turned()) else {
        panic!("a resistor has no polarity");
    };
    assert_eq!(c.swapped, [("R1".to_string(), "R4".to_string())]);
    let text = report(&Outcome::Match(c));
    assert!(
        text.contains("pads swapped") && text.contains("R1 -> R4"),
        "{text}"
    );
    // Two-pad parts of every family without polarity: capacitors, inductors and ferrite beads,
    // fuses, whatever the value.
    for footprint in [
        "Capacitor_SMD:C_0402_1005Metric",
        "Inductor_SMD:L_0603_1608Metric",
        "Fuse:Fuse_1206_3216Metric",
        "R_Axial_DIN0207_L6.3mm_D2.5mm_P10.16mm_Horizontal",
    ] {
        let straight = nl(
            &[
                ("J1", "Conn", HDR),
                ("R1", "330", footprint),
                ("D1", "LED", LED),
            ],
            &[
                ("VCC", &["J1.1", "R1.1"]),
                ("Net-(D1-A)", &["R1.2", "D1.1"]),
                ("GND", &["D1.2", "J1.2"]),
            ],
        );
        let Outcome::Match(c) = compare_default(&straight, &led_with_r1_turned(footprint)) else {
            panic!("{footprint} has no polarity");
        };
        assert_eq!(
            c.swapped,
            [("R1".to_string(), "R1".to_string())],
            "{footprint}"
        );
    }
}

/// `led_renumbered()` with R4's pads the other way round.
fn led_renumbered_with_r_turned() -> Netlist {
    nl(
        &[("D7", "LED", LED), ("J9", "Conn", HDR), ("R4", "330", R)],
        &[
            ("N$2", &["D7.2", "J9.2"]),
            ("+5V", &["R4.2", "J9.1"]),
            ("N$1", &["D7.1", "R4.1"]),
        ],
    )
}

#[test]
fn a_diode_with_its_pads_swapped_is_still_a_difference() {
    // D1's anode and cathode exchanged: the LED is in backwards.
    let b = nl(
        &[("J1", "Conn", HDR), ("R1", "330", R), ("D1", "LED", LED)],
        &[
            ("VCC", &["J1.1", "R1.1"]),
            ("Net-(D1-A)", &["R1.2", "D1.2"]),
            ("GND", &["D1.1", "J1.2"]),
        ],
    );
    let text = report(&compare_default(&led(), &b));
    assert!(text.starts_with("different"), "{text}");
    assert!(text.contains("D1.1") && text.contains("D1.2"), "{text}");
    // So is every other part whose footprint has a polarity, or that has none to read: an
    // electrolytic or tantalum capacitor, a diode, and a footprint the netlist does not name.
    for footprint in [
        "Capacitor_THT:CP_Radial_D5.0mm_P2.00mm",
        "Capacitor_Tantalum_SMD:CP_EIA-3216-18_Kemet-A",
        "Diode_SMD:D_SOD-123",
        "",
    ] {
        let straight = nl(
            &[
                ("J1", "Conn", HDR),
                ("R1", "330", footprint),
                ("D1", "LED", LED),
            ],
            &[
                ("VCC", &["J1.1", "R1.1"]),
                ("Net-(D1-A)", &["R1.2", "D1.1"]),
                ("GND", &["D1.2", "J1.2"]),
            ],
        );
        let o = compare_default(&straight, &led_with_r1_turned(footprint));
        assert!(matches!(o, Outcome::Different(_)), "{footprint}: {o:?}");
    }
    // And a resistor too, when the option is off.
    let strict = Options {
        two_pad_swaps: false,
        ..Options::default()
    };
    assert!(matches!(
        compare(&led(), &led_renumbered_with_r_turned(), &strict).unwrap(),
        Outcome::Different(_)
    ));
}

#[test]
fn a_swapped_resistor_beside_a_real_difference_is_not_listed_as_one() {
    // R4 is turned, and the LED is wired to the supply instead of to R4.
    let b = nl(
        &[("D7", "LED", LED), ("J9", "Conn", HDR), ("R4", "330", R)],
        &[
            ("N$2", &["D7.2", "J9.2"]),
            ("+5V", &["R4.2", "J9.1", "D7.1"]),
            ("N$1", &["R4.1"]),
        ],
    );
    let text = report(&compare_default(&led(), &b));
    assert!(text.starts_with("different"), "{text}");
    assert!(text.contains("D7.1"), "{text}");
    assert!(!text.contains("R4.2") && !text.contains("R1.1"), "{text}");
}

#[test]
fn a_mapped_resistor_may_be_swapped_too() {
    let options = Options {
        identity: Identity::Map(vec![("R1".into(), "R4".into())]),
        ..Options::default()
    };
    let Outcome::Match(c) = compare(&led(), &led_renumbered_with_r_turned(), &options).unwrap()
    else {
        panic!("a mapped resistor has no polarity either");
    };
    assert_eq!(c.swapped, [("R1".to_string(), "R4".to_string())]);
}

#[test]
fn loose_values_are_needed_to_equate_4k7_and_4_7k() {
    let make = |v: &str| {
        nl(
            &[("R1", v, R), ("R2", "1k", R)],
            &[("A", &["R1.1", "R2.1"]), ("B", &["R1.2"])],
        )
    };
    let (a, b) = (make("4k7"), make("4.7k"));
    assert!(matches!(compare_default(&a, &b), Outcome::Different(_)));
    let loose = Options {
        loose_values: true,
        ..Options::default()
    };
    assert!(matches!(
        compare(&a, &b, &loose).unwrap(),
        Outcome::Match(_)
    ));
}

#[test]
fn by_refdes_takes_the_designators_as_identity() {
    let by = Options {
        identity: Identity::ByRefdes,
        ..Options::default()
    };
    assert!(matches!(
        compare(&led(), &led(), &by).unwrap(),
        Outcome::Match(_)
    ));
    // The same circuit renumbered is a different design when designators are identity.
    let text = report(&compare(&led(), &led_renumbered(), &by).unwrap());
    assert!(
        text.contains("component only in A: R1") && text.contains("component only in B: R4"),
        "{text}"
    );
    // Swapped designators of different values show as value differences at once.
    let swapped = nl(
        &[("J1", "Conn", HDR), ("R1", "LED", LED), ("D1", "330", R)],
        &[
            ("VCC", &["J1.1", "R1.1"]),
            ("X", &["R1.2", "D1.1"]),
            ("GND", &["D1.2", "J1.2"]),
        ],
    );
    let text = report(&compare(&led(), &swapped, &by).unwrap());
    assert!(
        text.contains("R1 (A) and R1 (B) correspond but differ"),
        "{text}"
    );
}

#[test]
fn a_map_fixes_pairs_and_the_rest_are_matched_freely() {
    let options = Options {
        identity: Identity::Map(vec![("R1".into(), "R4".into())]),
        ..Options::default()
    };
    assert!(matches!(
        compare(&led(), &led_renumbered(), &options).unwrap(),
        Outcome::Match(_)
    ));
    // Parallel resistors: the map decides which is which.
    let a = parallel(2, true);
    let b = parallel(2, false);
    let fixed = Options {
        identity: Identity::Map(vec![("R1".into(), "R2".into())]),
        ..Options::default()
    };
    let Outcome::Match(c) = compare(&a, &b, &fixed).unwrap() else {
        panic!("still the same circuit");
    };
    assert!(c.components.contains(&("R1".into(), "R2".into())));
    assert!(c.components.contains(&("R2".into(), "R1".into())));
}

#[test]
fn a_map_naming_a_missing_or_repeated_component_is_refused() {
    let with = |pairs: &[(&str, &str)]| Options {
        identity: Identity::Map(
            pairs
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
        ),
        ..Options::default()
    };
    let e = compare(&led(), &led(), &with(&[("R9", "R1")])).unwrap_err();
    assert!(e.contains("R9") && e.contains("A"), "{e}");
    let e = compare(&led(), &led(), &with(&[("R1", "R9")])).unwrap_err();
    assert!(e.contains("R9") && e.contains("B"), "{e}");
    let e = compare(&led(), &led(), &with(&[("R1", "R1"), ("R1", "D1")])).unwrap_err();
    assert!(e.contains("R1") && e.contains("twice"), "{e}");
}

#[test]
fn a_mapped_pair_of_different_values_is_a_difference_even_if_wired_alike() {
    let options = Options {
        identity: Identity::Map(vec![("R1".into(), "R1".into())]),
        ..Options::default()
    };
    let b = nl(
        &[("J1", "Conn", HDR), ("R1", "470", R), ("D1", "LED", LED)],
        &[
            ("VCC", &["J1.1", "R1.1"]),
            ("X", &["R1.2", "D1.1"]),
            ("GND", &["D1.2", "J1.2"]),
        ],
    );
    let d = differences(compare(&led(), &b, &options).unwrap());
    assert_eq!(d.len(), 1);
    assert!(matches!(d[0], Difference::ComponentClass { .. }));
}

#[test]
fn ignoring_unconnected_drops_nets_of_one_pin() {
    // A has a pin of a test point wired to the resistor; B leaves the extra pin on a net of
    // its own, which only it has.
    let a = nl(
        &[("R1", "1k", R), ("R2", "1k", R)],
        &[("X", &["R1.1", "R2.1"]), ("Y", &["R1.2"])],
    );
    let b = nl(
        &[("R1", "1k", R), ("R2", "1k", R)],
        &[("X", &["R1.1", "R2.1"]), ("Y", &["R1.2"]), ("Z", &["R2.2"])],
    );
    let text = report(&compare_default(&a, &b));
    assert!(text.contains("net only in B: `Z`"), "{text}");
    let lenient = Options {
        ignore_unconnected: true,
        ..Options::default()
    };
    assert!(matches!(
        compare(&a, &b, &lenient).unwrap(),
        Outcome::Match(_)
    ));
}

#[test]
fn a_large_array_is_decided_quickly() {
    let (a, b) = (led_array(60, 1), led_array(60, 100));
    assert!(matches!(compare_default(&a, &b), Outcome::Match(_)));
    let mut broken = led_array(60, 100);
    let d = broken.find("D130").unwrap();
    broken.components[d].value = "RED".into();
    let text = report(&compare_default(&a, &broken));
    assert!(
        text.contains("1 difference") && text.contains("B is RED"),
        "{text}"
    );
}
