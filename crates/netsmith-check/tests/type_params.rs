//! A module that takes a part as a type parameter, and the `support` section of a part (ADR
//! 0115). Every part, number and name here is invented.

use netsmith_check::*;

/// A buck IC of this test, with what it alone needs in its support section.
const ZBK2: &str = "\
import std::parts::regulator::BuckIc
import std::parts::passive::Capacitor

part Zbk2 provides BuckIc {
    .vref &= 0.8V +/- 1.5%
    .fsw &= 1.2MHz +/- 15%
    .min_input_voltage &= 4.5V
    .max_input_voltage &= 6V
    .max_output_current &= 2A
    .efficiency &= 0.85..0.95
    .min_inductance &= 1.5uH
    .min_input_capacitance &= 2uF
    .min_output_capacitance &= 10uF
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)

    pins {
        .vin.pos  power    \"VIN\"  \"1\"
        .vin.neg  power    \"GND\"  \"2\"
        .sw.pos   power    \"SW\"   \"3\"
        .fb       input    \"FB\"   \"4\"
        .en       input    \"EN\"   \"5\"
        .bst      passive  \"BST\"  \"6\"
        .pg       output   \"PG\"   \"7\"
    }

    support {
        .boost: Capacitor { .capacitance &= 100nF +/- 20% }
        .bst ~> .boost ~> .sw.pos
        .en ~ .vin.pos
    }
}
";

/// The same IC with another name and range.
const ZBK9: &str = "\
import std::parts::regulator::BuckIc

part Zbk9 provides BuckIc {
    .vref &= 0.8V +/- 1.5%
    .fsw &= 1.2MHz +/- 15%
    .min_input_voltage &= 4.5V
    .max_input_voltage &= 60V
    .max_output_current &= 3.5A
    .efficiency &= 0.85..0.95
    .min_inductance &= 1.5uH
    .min_input_capacitance &= 2uF
    .min_output_capacitance &= 10uF
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)

    pins {
        .vin.pos  power  \"VIN\"  \"1\"
        .vin.neg  power  \"GND\"  \"2\"
        .sw.pos   power  \"SW\"   \"3\"
        .fb       input  \"FB\"   \"4\"
    }
}
";

const CONVERTER: &str = "\
import std::parts::regulator::{Buck, BuckIc}
import std::sources::Rail

generic BuckConverter {
    .vin: Rail
    .vout: Rail
}

module Converter<Ic: BuckIc> provides BuckConverter {
    .buck: Buck<Ic>
    .buck.vin ~ .vin
    .buck.vout ~ .vout
}
";

/// A module that provides a generic and holds its IC directly, for a pick through the generic.
const BARE: &str = "\
import std::parts::regulator::BuckIc
import std::sources::Rail
import src::converter::BuckConverter

module Bare<Ic: BuckIc> provides BuckConverter {
    .ic: Ic
    .ic.vin ~ .vin
    .ic.sw ~ .vout
}
";

fn loader(files: &[(&str, &str)]) -> Loader {
    let mut loader = Loader::std();
    for (module, text) in files {
        loader.insert(module, &format!("{}.nsm", module.replace("::", "/")), text);
    }
    loader
}

fn run(root: &str) -> Checked {
    let files = [
        ("src::zbk2", ZBK2),
        ("src::zbk9", ZBK9),
        ("src::converter", CONVERTER),
        ("src::bare", BARE),
    ];
    check("test.nsm", root, &loader(&files))
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn ok(c: &Checked) {
    assert!(c.is_ok(), "{}", c.render());
}

const RAILS: &str = "\
    .input: Rail { .voltage &= 5V +/- 5% }
    .output: Rail { .current &= 0mA..1A }
";

#[test]
fn a_board_names_the_part_and_the_module_holds_it() {
    let board = format!(
        "import src::zbk2::Zbk2\nimport std::parts::regulator::Buck\nimport std::sources::Rail\n\nboard B {{\n{RAILS}    .reg: Buck<Zbk2> {{ .vout.voltage &= 1.8V +/- 3% }}\n    .input ~ .reg.vin\n    .reg.vout ~ .output\n}}\n"
    );
    let c = run(&board);
    ok(&c);
    let model = &c.board("B").unwrap().model;
    let reg = model.instance(model.inst_at("reg").unwrap());
    assert_eq!(reg.kind, InstKind::Module);
    assert_eq!(reg.type_name, "Buck<Zbk2>");
    let ic = model.instance(model.inst_at("reg.ic").unwrap());
    assert_eq!(ic.kind, InstKind::Part);
    assert_eq!(ic.type_name, "Zbk2");
    // Its support part is a part of its own, below it.
    let boost = model.instance(model.inst_at("reg.ic.boost").unwrap());
    assert_eq!(boost.kind, InstKind::Part);
    assert_eq!(boost.type_name, "Capacitor");
    // Naming the part picks nothing.
    assert!(
        model
            .uses
            .iter()
            .all(|u| !u.path.starts_with("reg") || u.path.starts_with("reg."))
    );
}

#[test]
fn a_parametric_provider_stands_for_one_candidate_per_part_of_its_bound() {
    let board = format!(
        "import src::converter::BuckConverter\nimport src::zbk2::Zbk2\nimport src::zbk9::Zbk9\nimport std::sources::Rail\n\nboard B {{\n{RAILS}    .reg: BuckConverter {{ .vout.voltage &= 1.8V +/- 3% }}\n    .input ~ .reg.vin\n    .reg.vout ~ .output\n}}\n"
    );
    let c = run(&board);
    ok(&c);
    let model = &c.board("B").unwrap().model;
    let u = model.uses.iter().find(|u| u.path == "reg").unwrap();
    let names: Vec<&str> = u.candidates.iter().map(|p| p.name.as_str()).collect();
    // The standard library's provider of the bound first, then the project's, by module path.
    assert_eq!(
        names,
        ["Converter<Lt8641>", "Converter<Zbk2>", "Converter<Zbk9>"]
    );
    assert_eq!(
        u.candidates[1].id,
        "src::converter::Converter<src::zbk2::Zbk2>"
    );
    assert!(u.candidates.iter().all(|p| p.kind == InstKind::Module));
    // An assignment names a candidate by its id.
    let mut assign = Assignment::new();
    assign.insert(
        "B".to_string(),
        [(
            "reg".to_string(),
            "src::converter::Converter<src::zbk9::Zbk9>".to_string(),
        )]
        .into(),
    );
    let files = [
        ("src::zbk2", ZBK2),
        ("src::zbk9", ZBK9),
        ("src::converter", CONVERTER),
    ];
    let c = check_with("test.nsm", &board, &loader(&files), &assign);
    ok(&c);
    let model = &c.board("B").unwrap().model;
    let reg = model.instance(model.inst_at("reg").unwrap());
    assert_eq!(reg.type_name, "Converter<Zbk9>");
    let ic = model.instance(model.inst_at("reg.buck.ic").unwrap());
    assert_eq!(ic.type_name, "Zbk9");
}

#[test]
fn a_module_reaches_only_what_the_bound_declares() {
    let module = "import std::parts::regulator::BuckIc\nimport std::sources::Rail\n\nmodule Enabled<Ic: BuckIc> {\n    .vin: Rail\n    .ic: Ic\n    .ic.vin ~ .vin\n    .ic.en ~ .vin.pos\n}\n";
    let c = run(module);
    assert_eq!(
        messages(&c),
        [
            "`en` is not declared by `BuckIc`, the bound of the type parameter `Ic`; a module reaches only the members of its parameter's bound, so that it works with every part that provides it (declare it in `BuckIc`, or wire it in the `support` section of the parts that have it)"
        ]
    );
}

#[test]
fn an_argument_is_a_part_that_provides_the_bound() {
    let board = "import src::converter::Converter\nimport src::zbk2::Zbk2\nimport std::parts::passive::Resistor\nimport std::parts::regulator::BuckIc\nimport std::sources::Rail\n\nboard B {\n    .a: Converter<Resistor>\n    .b: Converter\n    .c: Converter<Zbk2, Zbk2>\n    .d: Rail<Zbk2>\n    .e: Converter<BuckIc>\n}\n";
    let c = run(board);
    let m = messages(&c);
    assert!(m.contains(&"`Resistor` is not an argument for `Ic` of `Converter`, which is a part that provides `BuckIc`: it does not provide `BuckIc`; name a part that does".to_string()), "{m:#?}");
    assert!(m.contains(&"`Converter` takes a part for `Ic` (a part that provides `BuckIc`): name it, `Converter<...>`, or let the solver pick one by using a generic that `Converter` provides".to_string()), "{m:#?}");
    assert!(m.contains(&"`Converter` takes 1 type argument, for `Ic` (a part that provides `BuckIc`), and this gives 2".to_string()), "{m:#?}");
    assert!(m.contains(&"`Rail` takes no type arguments; only a module declared with parameters does (`module Buck<Ic: BuckIc>`)".to_string()), "{m:#?}");
    assert!(
        m.iter().any(
            |m| m.starts_with("`BuckIc` is not an argument for `Ic` of `Converter`")
                && m.contains("it is the bound itself, which stands for no part")
        ),
        "{m:#?}"
    );
    assert_eq!(m.len(), 5, "{m:#?}");
}

#[test]
fn a_bound_is_a_generic_that_parts_provide() {
    let module = "import std::sources::Rail\nimport src::converter::BuckConverter\n\nmodule A<R: Rail> {\n    .r: R\n}\n\nmodule B<C: BuckConverter> {\n    .c: C\n}\n";
    let m = messages(&run(module));
    assert_eq!(
        m,
        [
            "the bound of a type parameter is a generic that parts provide, and `Rail` is an interface; a module takes a part as a parameter",
            "the bound of a type parameter is a generic that parts provide, and `BuckConverter` is a generic that modules provide; a module takes a part as a parameter",
        ],
        "{m:#?}"
    );
}

#[test]
fn a_parameter_passes_on_to_a_module_with_the_same_bound() {
    // `Converter<Ic>` gives its own parameter to `Buck<Ic>`; checked on its own, the bound
    // stands for it.
    let c = run(CONVERTER);
    ok(&c);
    let other = "import std::parts::regulator::{Buck, LinearRegulator}\n\nmodule Wrong<L: LinearRegulator> {\n    .buck: Buck<L>\n}\n";
    let m = messages(&run(other));
    assert_eq!(
        m,
        [
            "`L` is bounded by `LinearRegulator`, and `Ic` of `Buck` by `BuckIc`; a parameter is an argument only where the bounds are the same generic"
        ],
        "{m:#?}"
    );
}

#[test]
fn a_part_holds_parts_only_in_its_support_section() {
    let src = "import std::parts::passive::Capacitor\nimport std::sources::Rail\n\npart Chip {\n    .c: Capacitor\n    pins {\n        .a passive \"A\" \"1\"\n    }\n}\n\nmodule M {\n    .v: Rail\n    support {\n        .x: Capacitor\n    }\n}\n\npart Other {\n    pins {\n        .a passive \"A\" \"1\"\n    }\n    support {\n        .level: Voltage\n        pins {\n            .b passive \"B\" \"2\"\n        }\n    }\n}\n";
    let m = messages(&run(src));
    assert!(m.contains(&"a part holds other parts only in its `support` section, the circuit its datasheet puts around it: move `.c: Capacitor` into `support { ... }`".to_string()), "{m:#?}");
    assert!(m.contains(&"`support` can only appear directly inside a part, for the circuit its datasheet puts around it, and this is a module; a module or a board places such parts as members".to_string()), "{m:#?}");
    assert!(m.contains(&"a `support` section holds the parts of the part's application circuit, and `.level: Voltage` is not a part; declare it in the part itself".to_string()), "{m:#?}");
    assert!(m.contains(&"a `support` section holds parts and the statements that wire them to the part; this belongs in the part itself".to_string()), "{m:#?}");
}

#[test]
fn a_support_part_is_placed_with_its_part_wherever_it_is() {
    let board = "import src::zbk2::Zbk2\nimport std::sources::Rail\n\nboard B {\n    .supply: Rail { .voltage &= 5V }\n    .u: Zbk2\n    .supply ~ .u.vin\n}\n";
    let c = run(board);
    let model = &c.board("B").unwrap().model;
    assert!(model.inst_at("u.boost").is_some());
    // Its pin is on the net of the part's own pin.
    let bst = model.inst_at("u.bst").unwrap();
    let boost_a = model.inst_at("u.boost.a").unwrap();
    assert!(
        model
            .nets()
            .iter()
            .any(|n| n.pins.contains(&bst) && n.pins.contains(&boost_a))
    );
}

// ---- a join to the pins of a named IC (ADR 0120) ----

/// A board around `Buck<Zbk2>` with `lines` added.
fn named(lines: &str) -> String {
    format!(
        "import src::zbk2::Zbk2\nimport std::parts::mechanical::TestPoint\nimport std::parts::passive::Resistor\nimport std::parts::regulator::Buck\nimport std::sources::Rail\n\nboard B {{\n{RAILS}    .reg: Buck<Zbk2> {{ .vout.voltage &= 1.8V +/- 3% }}\n    .tp: TestPoint\n    .input ~ .reg.vin\n    .reg.vout ~ .output\n{lines}}}\n"
    )
}

#[test]
fn a_board_that_names_the_ic_joins_a_pin_the_bound_does_not_declare() {
    // `BuckIc` has no `.pg`; `Zbk2` has. A `~` and a `~>` chain both reach it.
    let c = run(&named(
        "    .pullup: Resistor { .resistance &= 100kohm +/- 5% }\n    .reg.ic.pg ~ .tp.p1\n    .output.pos ~> .pullup ~> .reg.ic.pg\n",
    ));
    ok(&c);
    let model = &c.board("B").unwrap().model;
    let pg = model.inst_at("reg.ic.pg").unwrap();
    let tp = model.inst_at("tp.p1").unwrap();
    assert!(
        model
            .nets()
            .iter()
            .any(|n| n.pins.contains(&pg) && n.pins.contains(&tp))
    );
}

#[test]
fn a_pin_the_named_ic_lacks_is_an_error_at_its_line() {
    let board = named("    .reg.ic.pgood ~ .tp.p1\n");
    let c = run(&board);
    // The test point stays unjoined, which is said too.
    let d: Vec<_> = c.diagnostics.iter().filter(|d| !d.message.starts_with("`tp.p1`")).collect();
    assert_eq!(d.len(), 1, "{}", c.render());
    assert_eq!(d[0].message, "`Zbk2` has no member `pgood`");
    assert_eq!(&board[d[0].span.start..d[0].span.end], "pgood");
}

#[test]
fn a_join_through_the_generic_is_refused() {
    let board = format!(
        "import src::bare::Bare\nimport src::converter::BuckConverter\nimport src::zbk2::Zbk2\nimport std::parts::mechanical::TestPoint\nimport std::sources::Rail\n\nboard B {{\n{RAILS}    .reg: BuckConverter\n    .tp: TestPoint\n    .input ~ .reg.vin\n    .reg.vout ~ .output\n    .reg.ic.pg ~ .tp.p1\n}}\n"
    );
    let c = run(&board);
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.starts_with("`ic` is not declared by `BuckConverter`: this use picks its IC through the generic, so the IC and its pins are not known here, and a join reaches the pins of a module's IC only where the IC is named, as in `Bare<")),
        "{m:#?}"
    );
}

#[test]
fn a_join_reaches_the_ic_and_not_its_support_parts() {
    let c = run(&named("    .reg.ic.boost.a ~ .tp.p1\n"));
    let m = messages(&c);
    assert_eq!(
        m[..1],
        ["`boost` is a part of the `support` section of `Zbk2`, inside `Buck<Zbk2>`: a join reaches only the named IC's own pins and interfaces; narrow a value of `boost` with `&=`"],
        "{m:#?}"
    );
}

#[test]
fn a_relation_does_not_reach_the_named_ic() {
    // Only a join (and a narrowing, ADR 0116) reaches the IC from outside.
    let m = messages(&run(&named("    .reg.ic.vref <= 1V\n")));
    assert!(
        m.iter().any(|m| m.contains("only narrowing (`&=`) reaches inside a module")),
        "{m:#?}"
    );
}

#[test]
fn a_join_follows_the_parameter_into_a_module_given_it() {
    // `Converter<Ic>` gives its parameter to `.buck: Buck<Ic>`, so a board that names
    // `Converter<Zbk2>` named the IC at `.reg.buck.ic` too (ADR 0120).
    let board = format!(
        "import src::converter::Converter\nimport src::zbk2::Zbk2\nimport std::parts::mechanical::TestPoint\nimport std::sources::Rail\n\nboard B {{\n{RAILS}    .reg: Converter<Zbk2> {{ .vout.voltage &= 1.8V +/- 3% }}\n    .tp: TestPoint\n    .input ~ .reg.vin\n    .reg.vout ~ .output\n    .reg.buck.ic.pg ~ .tp.p1\n}}\n"
    );
    let c = run(&board);
    ok(&c);
    let model = &c.board("B").unwrap().model;
    let pg = model.inst_at("reg.buck.ic.pg").unwrap();
    let tp = model.inst_at("tp.p1").unwrap();
    assert!(
        model
            .nets()
            .iter()
            .any(|n| n.pins.contains(&pg) && n.pins.contains(&tp))
    );
}

#[test]
fn a_join_through_a_module_given_the_parameter_reaches_only_the_ic() {
    // The module given the parameter is entered only on the way to its IC.
    let board = format!(
        "import src::converter::Converter\nimport src::zbk2::Zbk2\nimport std::parts::mechanical::TestPoint\nimport std::sources::Rail\n\nboard B {{\n{RAILS}    .reg: Converter<Zbk2> {{ .vout.voltage &= 1.8V +/- 3% }}\n    .tp: TestPoint\n    .input ~ .reg.vin\n    .reg.vout ~ .output\n    .reg.buck.divider.rtop.a ~ .tp.p1\n}}\n"
    );
    let m = messages(&run(&board));
    assert!(
        m.iter().any(|m| m.starts_with("`divider` is inside `Buck<Zbk2>` and is not part of its interface: only narrowing (`&=`) reaches inside a module")),
        "{m:#?}"
    );
}

#[test]
fn a_module_that_passes_its_parameter_on_reaches_only_the_bound() {
    // Its own parameter stands for no part it named, so its text does not join the IC's pins.
    let module = "import std::parts::regulator::{Buck, BuckIc}\nimport std::sources::Rail\n\nmodule Watched<Ic: BuckIc> {\n    .good: Rail\n    .stage: Buck<Ic>\n    .stage.ic.pg ~ .good.pos\n}\n";
    let m = messages(&run(module));
    assert_eq!(
        m,
        ["`ic` is inside `Buck<BuckIc>`, and the part it holds was not named here: a join reaches the pins of a module's IC only where its user names the part (`.reg: Buck<Lt8641>`), and a module's own text reaches only its parameter's bound"],
        "{m:#?}"
    );
}

#[test]
fn a_part_named_inside_a_module_is_not_named_by_the_board() {
    // `Supply` named `Zbk2`, so it joins `.stage.ic.pg`; the board that places `Supply` did not.
    let src = format!(
        "import src::zbk2::Zbk2\nimport std::parts::mechanical::TestPoint\nimport std::parts::regulator::Buck\nimport std::sources::Rail\n\nmodule Supply {{\n    .vin: Rail\n    .vout: Rail\n    .stage: Buck<Zbk2>\n    .probe: TestPoint\n    .vin ~ .stage.vin\n    .stage.vout ~ .vout\n    .stage.ic.pg ~ .probe.p1\n}}\n\nboard B {{\n{RAILS}    .s: Supply {{ .vout.voltage &= 1.8V +/- 3% }}\n    .tp: TestPoint\n    .input ~ .s.vin\n    .s.vout ~ .output\n    .s.stage.ic.pg ~ .tp.p1\n}}\n"
    );
    let m = messages(&run(&src));
    assert!(
        m.iter().any(|m| m.starts_with("`stage` is inside `Supply`, and the part it holds was not named here")),
        "{m:#?}"
    );
    assert!(!m.iter().any(|m| m.contains("`ic`")), "{m:#?}");
}
