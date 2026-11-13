//! What the placer's unit tests build boards from: the same as `netsmith-pcb`'s own fixtures
//! (`board::tests` and `rules` there), which are compiled only into that crate's tests.

use netsmith_core::{Design, Net, Part, PinRef, Process};
use netsmith_pcb::FootprintLibrary;

pub fn fixtures() -> FootprintLibrary {
    FootprintLibrary::at(netsmith_testkit::sample_footprints())
}

fn part(refdes: &str, kind: &str, value: &str, package: Option<&str>) -> Part {
    Part {
        // What the design stage records for a standard connector (ADR 0068).
        provides: if netsmith_testkit::std_class(kind) == Some("connector") {
            vec!["Connector".to_string()]
        } else {
            Vec::new()
        },
        class: netsmith_testkit::std_class(kind).and_then(netsmith_core::PartClass::from_word),
        refdes: refdes.into(),
        kind: kind.into(),
        value: value.into(),
        package: package.map(str::to_string),
        path: String::new(),
        pins: Vec::new(),
    }
}

fn net(name: &str, pins: &[(&str, &str)]) -> Net {
    Net {
        name: name.into(),
        pins: pins
            .iter()
            .map(|(r, p)| PinRef {
                refdes: (*r).into(),
                pin: (*p).into(),
            })
            .collect(),
    }
}

/// The `TwoLayer` process at 1.6 mm, as `netsmith-pcb`'s tests have it (`rules::test_process`).
pub fn test_process() -> Process {
    process_of(
        "import std::mfg::jlcpcb::TwoLayer\nimport std::parts::passive::Resistor\nboard T {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .r: Resistor\n}\n",
    )
}

/// The same circuit as `examples/led.nsm`.
pub fn led_design() -> Design {
    Design {
        modules: Vec::new(),
        name: "LedExample".into(),
        process: Some(test_process()),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts: vec![
            part("D1", "Led", "LED", None),
            part("J1", "Header1x2", "Conn_01x02", None),
            part("R1", "Resistor", "300", Some("0402")),
        ],
        nets: vec![
            net("GND", &[("D1", "1"), ("J1", "2")]),
            net("VCC", &[("J1", "1"), ("R1", "1")]),
            net("net-led.anode", &[("D1", "2"), ("R1", "2")]),
        ],
    }
}

/// The process of the one board of `source`, as the check, the solver and the design stage
/// resolve it: what `netsmith_pipeline::build_with` gives, without the netsmith crate.
fn process_of(source: &str) -> Process {
    let checked = netsmith_check::check("t.nsm", source, &netsmith_check::Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = &checked.boards[0];
    let solution = netsmith_solve::solve(board, &netsmith_solve::Catalogs::jlcpcb());
    assert!(
        solution.diagnostics.is_empty(),
        "{:?}",
        solution.diagnostics
    );
    netsmith_design::design_board(board, &solution)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .design
        .process
        .expect("the board has a process")
}

/// The board with the inner layers `body` says, over a resistor that ties
/// GND to 3V3, as `netsmith-pcb`'s `rules::four_layer_process` and its siblings resolve it.
fn layered_process(module: &str, name: &str, body: &str) -> Process {
    process_of(&format!(
        "import std::mfg::{module}::{name}\nimport std::parts::passive::Resistor\nboard T {{\n    .process: {name} {{\n{body}    }}\n    .r: Resistor {{\n        .a.net &= \"GND\"\n        .b.net &= \"3V3\"\n    }}\n}}\n"
    ))
}

/// The LED board on four layers: ground and 3.3 V planes, and the supply net called `3V3`.
pub fn four_layer_design() -> Design {
    let mut design = led_design();
    design.process = Some(layered_process(
        "jlcpcb",
        "FourLayer",
        "        .inner1.net &= \"GND\"\n        .inner2.net &= \"3V3\"\n",
    ));
    design.nets[1].name = "3V3".into();
    design
}

/// The LED board on six layers: GND on the first inner layer, 3V3 on the last, two signal
/// layers between them.
pub fn six_layer_design() -> Design {
    let mut design = led_design();
    design.process = Some(layered_process(
        "jlcpcb",
        "SixLayer",
        "        .inner1.net &= \"GND\"\n        .inner2.use &= \"signal\"\n        .inner3.use &= \"signal\"\n        .inner4.net &= \"3V3\"\n",
    ));
    design.nets[1].name = "3V3".into();
    design
}

/// The LED board on ACME's ten layers: ground on the first inner layer, 3.3 V on the last.
pub fn ten_layer_design() -> Design {
    let mut design = led_design();
    let body: String = (1..=8)
        .map(|n| match n {
            1 => format!("        .inner{n}.net &= \"GND\"\n"),
            8 => format!("        .inner{n}.net &= \"3V3\"\n"),
            _ => format!("        .inner{n}.use &= \"signal\"\n"),
        })
        .collect();
    design.process = Some(layered_process("acme", "TenLayer", &body));
    design.nets[1].name = "3V3".into();
    design
}
