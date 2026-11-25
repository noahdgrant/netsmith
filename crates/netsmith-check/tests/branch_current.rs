//! The current of a branch (issue #1113): every pin has a `.current`, the most its branch
//! carries, stated on the pin of a part; a pin of a module, a board or an interface of one is
//! refused, where it is written.

use netsmith_check::*;

const PART: &str = r#"
import std::parts::connector::Header1x2
import std::sources::Rail

/// An invented converter with a bias pin.
part Conv {
    .designator &= "U"
    .vin: Rail
    pins {
        .vin.pos power   "VIN"  "1"
        .vin.neg power   "GND"  "2"
        .bias    passive "BIAS" "3"
    }
}
"#;

fn run(rest: &str) -> Checked {
    check("main.nsm", &format!("{PART}\n{rest}"), &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_pin_of_a_part_states_the_current_of_its_branch_from_the_part_or_the_board() {
    let c = run(
        "board B {
    .supply: Rail { .pos.net &= \"VIN\" }
    .u: Conv { .bias.current &= 1mA }
    .supply ~ .u.vin
    .u.bias ~ .supply.pos
    .u.vin.pos.current &= 0A..2A
}",
    );
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    let range = |path: &str| match model.domain_at(path) {
        Some(Domain::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("`{path}` is {other:?}"),
    };
    assert_eq!(range("u.bias.current"), (0.001, 0.001));
    assert_eq!(range("u.vin.pos.current"), (0.0, 2.0));
    // `~` joins pins and shares no current: the rail's pin keeps none.
    assert!(matches!(model.domain_at("supply.pos.current"), Some(Domain::Unset)));
}

#[test]
fn a_pin_of_a_module_or_a_board_is_refused_with_the_pin_of_a_part_to_state_it_on() {
    let c = run(
        "module Stage {
    .vin: Rail
    .u: Conv
    .vin ~ .u.vin
    .vin.pos.current &= 1mA
}
board B {
    .supply: Rail { .pos.net &= \"VIN\"; .pos.current &= 1mA }
    .stage: Stage
    .supply ~ .stage.vin
}",
    );
    assert_eq!(
        messages(&c),
        [
            "`.vin.pos` is a pin of a module, not of a part: the current of a branch is stated on the pin of the part the branch ends at, such as `.ldo.bias.current &= 1mA`, since a module's pin stands for every pin joined to it",
            "`.pos` is a pin of a board, not of a part: the current of a branch is stated on the pin of the part the branch ends at, such as `.ldo.bias.current &= 1mA`, since a board's pin stands for every pin joined to it",
        ],
        "{}",
        c.render()
    );
}

#[test]
fn a_pin_states_the_rms_current_of_its_branch_where_it_states_its_current() {
    // Issue #1690: beside the peak, the heating (rms) current of the branch, on the pin of a
    // part; on a pin of a module it is refused as `.current` is.
    let c = run(
        "board B {
    .supply: Rail { .pos.net &= \"VIN\" }
    .u: Conv { .bias.current &= 1mA; .bias.rms_current &= 0.2mA }
    .supply ~ .u.vin
    .u.bias ~ .supply.pos
}",
    );
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    match model.domain_at("u.bias.rms_current") {
        Some(Domain::Quantity(q)) => assert_eq!((q.lo(), q.hi()), (0.0002, 0.0002)),
        other => panic!("{other:?}"),
    }
    let c = run(
        "module Stage {
    .vin: Rail
    .u: Conv
    .vin ~ .u.vin
    .vin.pos.rms_current &= 1mA
}
board B {
    .supply: Rail { .pos.net &= \"VIN\" }
    .stage: Stage
    .supply ~ .stage.vin
}",
    );
    assert_eq!(
        messages(&c),
        [
            "`.vin.pos` is a pin of a module, not of a part: the current of a branch is stated on the pin of the part the branch ends at, such as `.ldo.bias.rms_current &= 1mA`, since a module's pin stands for every pin joined to it",
        ],
        "{}",
        c.render()
    );
}
