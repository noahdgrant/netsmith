//! `layout { source "..." near ... }` in a part: the parse and the printed form (ADR 0030,
//! issue #829).

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
        LayoutStatement::Near(cap),
        LayoutStatement::Near(coil),
    ] = l.statements.as_slice()
    else {
        panic!("{:?}", l.statements)
    };
    assert_eq!(text, "SNVSA24, 11.1 Layout Guidelines, p.17");
    assert_eq!(cap.name.name, "input_cap");
    assert_eq!(
        cap.docs,
        ["The input bypass capacitor Cin must be placed close to the VIN pin."]
    );
    let Select::Bridging(a, b) = &cap.select else {
        panic!("{:?}", cap.select)
    };
    assert_eq!((a.to_source(), b.to_source()), (".vin.pos".into(), ".vin.neg".into()));
    assert_eq!(cap.to.to_source(), ".vin.pos");
    assert!(cap.within.is_none());
    let Select::On { pin, kind } = &coil.select else {
        panic!("{:?}", coil.select)
    };
    assert_eq!(pin.to_source(), ".sw.pos");
    assert_eq!(kind.as_ref().map(|k| k.name.as_str()), Some("inductor"));
    assert_eq!(coil.within.as_ref().map(Expr::to_source).as_deref(), Some("3mm"));
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
    assert!(e.contains("a `near` statement or `}` in a `layout` block"), "{e}");
}

#[test]
fn a_doc_comment_above_the_source_is_refused() {
    let e = error("        /// Layout.\n        source \"X\"");
    assert!(e.contains("not above `source`"), "{e}");
}
