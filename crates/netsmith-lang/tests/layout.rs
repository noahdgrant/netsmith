//! `layout { source "..." near ... }` in a part: the parse and the printed form (ADR 0030,
//! issues #829 and #994).

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const PART: &str = r#"part Reg {
    layout {
        source "SNVSA24, 11.1 Layout Guidelines, p.17"
        /// The input bypass capacitor Cin must be placed close to the VIN pin.
        near input_cap bridging .vin.pos .vin.neg to .vin.pos
        near inductor on .sw.pos kind inductor to .sw.pos within 3mm
    }
}
"#;

fn layout(src: &str) -> Layout {
    let file = parse(src).unwrap();
    match &file.definitions[0].items[0] {
        Item::Layout(l) => l.clone(),
        other => panic!("not a layout block: {other:?}"),
    }
}

fn error(body: &str) -> String {
    parse(&format!("part P {{\n    layout {{\n{body}\n    }}\n}}\n"))
        .unwrap_err()
        .message
}

#[test]
fn the_statements_of_a_block() {
    let l = layout(PART);
    let [
        LayoutStatement::Source { text, .. },
        LayoutStatement::Guideline(cap),
        LayoutStatement::Guideline(coil),
    ] = l.statements.as_slice()
    else {
        panic!("{:?}", l.statements)
    };
    let Rule::Near {
        select: cap_select,
        to: cap_to,
        within: cap_within,
    } = &cap.rule
    else {
        panic!("{:?}", cap.rule)
    };
    let Rule::Near {
        select: coil_select,
        within: coil_within,
        ..
    } = &coil.rule
    else {
        panic!("{:?}", coil.rule)
    };
    assert_eq!(text, "SNVSA24, 11.1 Layout Guidelines, p.17");
    assert_eq!(cap.name.name, "input_cap");
    assert_eq!(
        cap.docs,
        ["The input bypass capacitor Cin must be placed close to the VIN pin."]
    );
    let Select::Bridging(a, b) = cap_select else {
        panic!("{cap_select:?}")
    };
    assert_eq!((a.to_source(), b.to_source()), (".vin.pos".into(), ".vin.neg".into()));
    assert_eq!(cap_to.to_source(), ".vin.pos");
    assert!(cap_within.is_none());
    let Select::On { pin, kind } = coil_select else {
        panic!("{coil_select:?}")
    };
    assert_eq!(pin.to_source(), ".sw.pos");
    assert_eq!(kind.as_ref().map(|k| k.name.as_str()), Some("inductor"));
    assert_eq!(coil_within.as_ref().map(Expr::to_source).as_deref(), Some("3mm"));
}

#[test]
fn a_block_prints_back_to_its_source() {
    let file = parse(PART).unwrap();
    assert_eq!(file.to_source(), PART);
    assert_eq!(parse(&file.to_source()).unwrap(), file);
}

#[test]
fn layout_stays_a_name_outside_the_start_of_a_block() {
    let file = parse("part P {\n    .layout: Pin\n}\n").unwrap();
    assert!(matches!(file.definitions[0].items[0], Item::Decl(_)));
}

#[test]
fn a_near_without_a_selector_says_which_are_possible() {
    let e = error("        near input_cap to .vin.pos");
    assert!(e.contains("`bridging .a .b` or `on .pin` after `near input_cap`"), "{e}");
}

#[test]
fn a_near_without_to_says_so() {
    let e = error("        near input_cap bridging .vin.pos .vin.neg");
    assert!(e.contains("`to` and the pin the parts go next to"), "{e}");
}

#[test]
fn a_source_needs_its_text_in_quotes() {
    let e = error("        source SNVSA24");
    assert!(e.contains("the document and section in quotes after `source`"), "{e}");
}

#[test]
fn a_block_holds_only_its_statements() {
    let e = error("        .cap: Capacitor");
    assert!(e.contains("a guideline statement (`near`, `away`, `loop`, `keepout`, `edge`, `vias`, `route`, `kelvin`) or `}` in a `layout` block"), "{e}");
}

#[test]
fn a_doc_comment_above_the_source_is_refused() {
    let e = error("        /// Layout.\n        source \"X\"");
    assert!(e.contains("not above `source`"), "{e}");
}

/// Every statement of issue #994, in a block for one package.
const EVERY: &str = r#"part Reg {
    layout "SOT-223" {
        source "Datasheet X, 9 Layout, p.20"
        /// Keep the switching inductor away from the feedback pin.
        away coil on .sw kind inductor from .fb beyond 5mm
        loop hot bridging .vin.pos .vin.neg within 12mm
        keepout under at -1mm, -2.5mm size 2mm, 5mm forbid tracks vias
        keepout clear at 0mm, 0mm size 1mm, 1mm
        edge feed .ant within 1mm
        edge tab .tab
        vias thermal .tab 4
        route sw .sw width 0.5mm length 10mm
        route fb .fb length 5mm
    }
}
"#;

#[test]
fn every_statement_and_a_package_print_back_to_their_source() {
    let file = parse(EVERY).unwrap();
    assert_eq!(file.to_source(), EVERY);
    assert_eq!(parse(&file.to_source()).unwrap(), file);
}

#[test]
fn the_rules_of_every_statement() {
    let l = layout(EVERY);
    assert_eq!(l.package.as_ref().map(|(p, _)| p.as_str()), Some("SOT-223"));
    let rules: Vec<&Rule> = l
        .statements
        .iter()
        .filter_map(|s| match s {
            LayoutStatement::Guideline(g) => Some(&g.rule),
            LayoutStatement::Source { .. } => None,
        })
        .collect();
    let words: Vec<&str> = rules.iter().map(|r| r.word()).collect();
    assert_eq!(
        words,
        ["away", "loop", "keepout", "keepout", "edge", "edge", "vias", "route", "route"]
    );
    let Rule::Keepout(area) = rules[2] else {
        panic!("{:?}", rules[2])
    };
    let KeepoutArea {
        x, height, forbid, ..
    } = &**area;
    assert_eq!((x.to_source(), height.to_source()), ("-1mm".into(), "5mm".into()));
    let forbid: Vec<&str> = forbid.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(forbid, ["tracks", "vias"]);
    let Rule::Route { width, length, .. } = rules[8] else {
        panic!("{:?}", rules[8])
    };
    assert!(width.is_none());
    assert_eq!(length.as_ref().map(Expr::to_source).as_deref(), Some("5mm"));
    let pins: Vec<String> = l.statements[1].pins().iter().map(|p| p.to_source()).collect();
    assert_eq!(pins, [".sw", ".fb"]);
}

#[test]
fn an_away_without_beyond_says_so() {
    let e = error("        away coil on .sw from .fb");
    assert!(e.contains("`beyond` and the least distance from the pin"), "{e}");
}

#[test]
fn a_loop_names_its_pins_with_bridging() {
    let e = error("        loop hot .vin.pos .vin.neg");
    assert!(e.contains("`bridging .a .b` after `loop hot`"), "{e}");
}

#[test]
fn a_keepout_needs_a_comma_between_its_numbers() {
    let e = error("        keepout k at 1mm 2mm size 1mm, 1mm");
    assert!(e.contains("`,` between the two numbers of `at`"), "{e}");
}

#[test]
fn a_route_says_what_it_limits() {
    let e = error("        route sw .sw");
    assert!(e.contains("`width LENGTH`, `length LENGTH` or both"), "{e}");
}

#[test]
fn a_package_is_followed_by_its_block() {
    let e = parse("part P {\n    layout \"SOT-223\" source\n}\n")
        .unwrap_err()
        .message;
    assert!(e.contains("`{` after the package of `layout \"...\"`"), "{e}");
}

const KEYED: &str = r#"layout "XC6206P332MR-G" for LinearRegulator {
    source "XC6206 series, 9 Layout, p.9"
    /// "Keep the input capacitor within 2 mm of VIN."
    near input_cap bridging .vin.pos .vin.neg to .vin.pos within 2mm
}

layout "OTHER-ROW" for LinearRegulator {
    source "OTHER, 3 Layout, p.4"
    vias ground .vin.neg 2
}
"#;

#[test]
fn a_block_at_the_top_of_a_file_is_for_one_row_of_a_generic() {
    let file = parse(KEYED).unwrap();
    assert!(file.definitions.is_empty());
    let [first, second] = file.layouts.as_slice() else {
        panic!("{:?}", file.layouts)
    };
    let keyed = first.keyed.as_ref().unwrap();
    assert_eq!(keyed.mpn.0, "XC6206P332MR-G");
    assert_eq!(keyed.generic.name, "LinearRegulator");
    assert!(first.package.is_none());
    assert_eq!(first.statements.len(), 2);
    assert_eq!(second.keyed.as_ref().unwrap().mpn.0, "OTHER-ROW");
    assert_eq!(file.to_source(), KEYED);
    assert_eq!(parse(&file.to_source()).unwrap(), file);
}

#[test]
fn a_block_at_the_top_of_a_file_names_its_generic() {
    let e = parse("layout \"X\" {\n    source \"x\"\n}\n").unwrap_err().message;
    assert!(
        e.contains("`for` and the generic whose catalog has the row"),
        "{e}"
    );
    let e = parse("layout \"X\" for {\n}\n").unwrap_err().message;
    assert!(e.contains("the name of the generic whose catalog has the row"), "{e}");
}

#[test]
fn a_block_in_a_part_does_not_take_for() {
    assert!(parse("part P {\n    layout \"X\" for G {\n    }\n}\n").is_err());
}

/// A `kelvin` statement (issue #1148), with each selector.
const KELVIN: &str = r#"part Amp {
    layout {
        source "Datasheet Y, 8 Layout, p.14"
        /// Connect IN+ and IN- to the shunt's pads with Kelvin connections.
        kelvin sense_p bridging .in_p .in_n to .in_p
        kelvin remote on .sense kind resistor to .sense
    }
}
"#;

#[test]
fn a_kelvin_statement_prints_back_to_its_source() {
    let file = parse(KELVIN).unwrap();
    assert_eq!(file.to_source(), KELVIN);
    assert_eq!(parse(&file.to_source()).unwrap(), file);
    let l = layout(KELVIN);
    let LayoutStatement::Guideline(g) = &l.statements[1] else {
        panic!("{:?}", l.statements)
    };
    assert_eq!(g.rule.word(), "kelvin");
    let pins: Vec<String> = g.rule.pins().iter().map(|p| p.to_source()).collect();
    assert_eq!(pins, [".in_p", ".in_n", ".in_p"]);
}

#[test]
fn a_kelvin_statement_names_its_sense_pin_after_to() {
    let e = error("        kelvin sense_p bridging .in_p .in_n");
    assert!(e.contains("`to` and the sense pin"), "{e}");
}
