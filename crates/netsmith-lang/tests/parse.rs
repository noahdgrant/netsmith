use netsmith_lang::ast::*;
use netsmith_lang::parse;

const LED: &str = include_str!("../../../examples/led.nsm");

fn names(idents: &[Ident]) -> Vec<&str> {
    idents.iter().map(|i| i.name.as_str()).collect()
}

/// Parse `board T { .x &= <expr> }` and return the right-hand side.
fn expr(src: &str) -> Expr {
    let file = parse(&format!("board T {{\n    .x &= {src}\n}}\n")).unwrap();
    match file.definitions[0].items[0].clone() {
        Item::Constraint(c) => c.rhs,
        other => panic!("expected a constraint, got {other:?}"),
    }
}

/// Canonical text of an expression after a parse.
fn canon(src: &str) -> String {
    expr(src).to_source()
}

fn err(src: &str) -> (String, usize, usize) {
    let e = parse(src).unwrap_err();
    let (line, col) = e.line_col(src);
    (e.message, line, col)
}

#[test]
fn parses_the_led_example() {
    let file = parse(LED).unwrap();

    assert_eq!(file.imports.len(), 4);
    assert_eq!(
        names(&file.imports[0].path),
        ["std", "parts", "connector", "Header1x2"]
    );
    assert!(file.imports[0].names.is_none());
    assert_eq!(names(&file.imports[1].path), ["std", "parts", "passive"]);
    assert_eq!(
        names(file.imports[1].names.as_ref().unwrap()),
        ["Resistor", "Led"]
    );
    assert_eq!(names(&file.imports[2].path), ["std", "sources", "Rail"]);
    assert!(file.imports[2].names.is_none());
    assert_eq!(
        names(&file.imports[3].path),
        ["std", "mfg", "jlcpcb", "TwoLayer"]
    );

    let design = &file.definitions[0];
    assert_eq!(design.name.name, "LedExample");
    assert_eq!(design.items.len(), 9);

    let Item::Decl(process) = &design.items[0] else {
        panic!("process")
    };
    assert_eq!(process.name.to_source(), ".process");
    assert_eq!(names(&process.ty.segments), ["TwoLayer"]);

    let Item::Decl(supply) = &design.items[1] else {
        panic!("supply")
    };
    assert_eq!(supply.name.to_source(), ".supply");
    assert_eq!(names(&supply.ty.segments), ["Rail"]);
    assert_eq!(supply.body.as_ref().unwrap().len(), 3);

    let Item::Decl(r1) = &design.items[3] else {
        panic!("r1")
    };
    assert_eq!(
        r1.body.as_ref().map(Vec::len),
        Some(0),
        "`{{}}` is an empty body"
    );

    let Item::Constraint(eq) = &design.items[7] else {
        panic!("equation")
    };
    assert_eq!(eq.op, CmpOp::EqEq);
    assert_eq!(eq.lhs.to_source(), ".led.current");

    let Item::Connect(chain) = &design.items[8] else {
        panic!("chain")
    };
    assert_eq!(chain.first.to_source(), ".supply.pos");
    assert_eq!(chain.rest.len(), 3);
    assert!(chain.rest.iter().all(|(op, _)| *op == ConnectOp::Series));
}

#[test]
fn canonical_form_of_the_example_is_exact_and_stable() {
    let once = parse(LED).unwrap().to_source();
    assert_eq!(
        once,
        "\
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board LedExample {
    /// Made by JLCPCB's two-layer process, on a board 1.6 mm thick.
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .led: Led {
        .current &= 5mA..15mA
    }
    .r1: Resistor {}
    /// The supply comes in through this header.
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
"
    );
    let twice = parse(&once).unwrap().to_source();
    assert_eq!(once, twice);
}

#[test]
fn units_tolerances_and_ranges() {
    assert_eq!(canon("5V +/- 5%"), "5V +/- 5%");
    assert_eq!(canon("5V ± 5%"), "5V +/- 5%");
    assert_eq!(canon("1.8V..2.2V"), "1.8V..2.2V");
    assert_eq!(canon("1kohm +/- 1%"), "1kohm +/- 1%");
    assert_eq!(canon("0x1A"), "0x1A");
}

#[test]
fn precedence() {
    // `+` binds looser than `*`.
    let Expr::Binary {
        op: BinOp::Add,
        rhs,
        ..
    } = expr("a + b * c")
    else {
        panic!("add")
    };
    assert!(matches!(*rhs, Expr::Binary { op: BinOp::Mul, .. }));
    // The tolerance applies to the whole sum.
    let Expr::Tolerance { center, .. } = expr("a + b +/- 1%") else {
        panic!("tolerance")
    };
    assert!(matches!(*center, Expr::Binary { op: BinOp::Add, .. }));
    // A range is the loosest of all.
    let Expr::Range { lo, .. } = expr("1V +/- 5%..2V") else {
        panic!("range")
    };
    assert!(matches!(*lo, Expr::Tolerance { .. }));
    assert!(matches!(expr("-x"), Expr::Neg(..)));
    // Subtraction is left-associative: (a - b) - c.
    let Expr::Binary { lhs, .. } = expr("a - b - c") else {
        panic!("sub")
    };
    assert!(matches!(*lhs, Expr::Binary { op: BinOp::Sub, .. }));
}

#[test]
fn names_with_and_without_a_leading_dot() {
    let Expr::Path(local) = expr(".supply.voltage") else {
        panic!()
    };
    let Expr::Path(outer) = expr("supply.voltage") else {
        panic!()
    };
    assert!(local.leading_dot && !outer.leading_dot);
    assert_eq!(names(&local.segments), names(&outer.segments));
}

#[test]
fn quoted_names_for_pins() {
    let src = "board T {\n    .u1.'5' ~ .r1.'1'\n}\n";
    let file = parse(src).unwrap();
    let Item::Connect(c) = &file.definitions[0].items[0] else {
        panic!()
    };
    assert!(c.first.segments[1].quoted);
    assert_eq!(file.to_source(), src);
}

#[test]
fn statements_wrap_after_operators_and_inside_parentheses() {
    let wrapped =
        "board T {\n    .a ~>\n        .b ~>\n        .c\n    .x == (1V -\n        2V)\n}\n";
    let flat = "board T {\n    .a ~> .b ~> .c\n    .x == (1V - 2V)\n}\n";
    assert_eq!(
        parse(wrapped).unwrap().to_source(),
        parse(flat).unwrap().to_source()
    );
}

#[test]
fn semicolons_separate_statements_on_one_line() {
    let file = parse("board T { .a: R; .b: R }").unwrap();
    assert_eq!(file.definitions[0].items.len(), 2);
}

#[test]
fn comments_are_ignored() {
    let with = "// top\nboard T { // here\n    .a: R // there\n    // alone\n    .b: R\n}\n";
    let without = "board T {\n    .a: R\n    .b: R\n}\n";
    assert_eq!(
        parse(with).unwrap().to_source(),
        parse(without).unwrap().to_source()
    );
}

#[test]
fn comparison_operators() {
    let file =
        parse("board T {\n    .a >= 1V\n    .b <= 2V\n    .c == 3V\n    .e &= 5V\n}\n").unwrap();
    let ops: Vec<_> = file.definitions[0]
        .items
        .iter()
        .map(|i| match i {
            Item::Constraint(c) => c.op,
            _ => panic!(),
        })
        .collect();
    assert_eq!(ops, [CmpOp::Ge, CmpOp::Le, CmpOp::EqEq, CmpOp::Narrow]);
}

#[test]
fn a_default_is_its_own_operator_and_prints_back_as_written() {
    let src = "module M {\n    .fsw: Frequency\n    .name: String\n    .fsw ?= 1.2MHz +/- 5%\n    .name ?= \"x\"\n    .a ?= .b * 2\n    .c ?= 1V..2V\n}\n";
    let file = parse(src).unwrap();
    let ops: Vec<_> = file.definitions[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Constraint(c) => Some(c.op),
            _ => None,
        })
        .collect();
    assert_eq!(ops, [CmpOp::Default; 4]);
    assert_eq!(file.to_source(), src);
    // `?=` and `&=` are different statements: neither prints as the other.
    assert!(!file.to_source().contains("&="));
}

#[test]
fn a_default_needs_a_value_and_the_statement_error_lists_the_operator() {
    let err = parse("board T {\n    .a ?=\n}\n").unwrap_err();
    assert!(err.message.contains("expected"), "{}", err.message);
    // The message for a statement that is not one lists `?=` among the operators.
    let err = parse("board T {\n    .a 5\n}\n").unwrap_err();
    assert!(err.message.contains("`?=`"), "{}", err.message);
}

#[test]
fn nested_decls_and_qualified_types() {
    let src = "board T {\n    .a: std::Thing {\n        .b: Inner {\n            .c &= 1V\n        }\n    }\n}\n";
    assert_eq!(parse(src).unwrap().to_source(), src);
}

#[test]
fn errors_say_where_and_what() {
    let (msg, line, col) = err("board T {\n    .a: R\n");
    assert!(msg.contains("never closed"), "{msg}");
    assert_eq!((line, col), (1, 9));

    let (msg, line, col) = err("board T {\n    .x &=\n}\n");
    assert!(msg.contains("expected a value, found `}`"), "{msg}");
    assert_eq!((line, col), (3, 1));

    let (msg, line, _) = err("board T {\n    5V\n}\n");
    assert!(msg.contains("expected `:`"), "{msg}");
    assert_eq!(line, 2);

    let (msg, ..) = err("board T { .a: R .b: R }");
    assert!(msg.contains("end of line or `;`"), "{msg}");

    let (msg, ..) = err("board T {\n    .x == \"abc\n}\n");
    assert!(msg.contains("string is never closed"), "{msg}");

    let (msg, ..) = err("fn main() {}");
    assert!(msg.contains("expected `import`, `board`"), "{msg}");

    let (msg, ..) = err("board T {\n    (.a) : R\n}\n");
    assert!(msg.contains("expected a name before `:`"), "{msg}");
}

#[test]
fn rendered_errors_underline_the_source() {
    let src = "board T {\n    .x &=\n}\n";
    let e = parse(src).unwrap_err();
    let text = e.render("led.nsm", src);
    assert!(text.contains("--> led.nsm:3:1"), "{text}");
    assert!(text.contains("3 | }"), "{text}");
    assert!(text.contains("^"), "{text}");
}

#[test]
fn an_empty_file_has_nothing_in_it() {
    let file = parse("  \n// only a comment\n").unwrap();
    assert!(file.imports.is_empty() && file.definitions.is_empty());
}

#[test]
fn a_dot_after_a_space_starts_a_new_name_instead_of_continuing_the_path() {
    // `.a .b` is two names, not the path `a.b`.
    let (msg, ..) = err("board T {\n    .x == .a .b\n}\n");
    assert!(msg.contains("end of line or `;`"), "{msg}");
    let file = parse("board T {\n    .x == .a.b\n}\n").unwrap();
    let Item::Constraint(c) = &file.definitions[0].items[0] else {
        panic!()
    };
    let Expr::Path(p) = &c.rhs else { panic!() };
    assert_eq!(names(&p.segments), ["a", "b"]);
}

#[test]
fn a_system_holds_boards_and_joins_their_ports() {
    let src =
        "system Demo {\n    .main: MainBoard\n    .io: IoBoard\n    .main.output ~ .io.input\n}\n";
    let file = parse(src).unwrap();
    let def = &file.definitions[0];
    assert_eq!(def.kind, DefKind::System);
    assert_eq!(def.name.name, "Demo");
    assert_eq!(def.items.len(), 3);
    assert!(matches!(&def.items[2], Item::Connect(c) if c.first.to_source() == ".main.output"));
    assert_eq!(file.to_source(), src);
}

#[test]
fn there_is_no_plain_equals() {
    for src in [
        "board T {\n    .x = 5V\n}\n",
        "board T {\n    .x = .a.b * 2\n}\n",
    ] {
        let (msg, line, col) = err(src);
        assert_eq!(
            msg,
            "there is no `=`: use `&=` to narrow a value, or `==` for an equation that must hold",
        );
        assert_eq!((line, col), (2, 8), "{src}");
    }
}

#[test]
fn a_word_after_the_type_of_a_declaration_is_a_parse_error_and_required_is_a_plain_name() {
    // `required` after a type was a marker (ADR 0060) and is gone (ADR 0065): the parser reads
    // it as it reads any other word there, and says nothing special about it.
    let (msg, line, _) = err("module Lamp {\n    .balance: Resistance required\n}\n");
    let (other, _, _) = err("module Lamp {\n    .balance: Resistance banana\n}\n");
    assert_eq!(line, 2);
    assert_eq!(msg, "expected end of line or `;`, found `required`");
    assert_eq!(msg.replace("required", "banana"), other);
    // Elsewhere it is an ordinary name, and a `pins` row keeps its own `required`.
    let src = "module Lamp {\n    .required: Voltage\n    .required &= 1\n}\n";
    assert_eq!(parse(src).unwrap().to_source(), src);
}
