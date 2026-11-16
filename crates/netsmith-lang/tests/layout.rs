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
    assert!(e.contains("a guideline statement (`near`, `away`, `loop`, `keepout`, `edge`, `vias`, `route`) or `}` in a `layout` block"), "{e}");
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
    let Rule::Keepout { x, height, forbid, .. } = rules[2] else {
        panic!("{:?}", rules[2])
    };
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
