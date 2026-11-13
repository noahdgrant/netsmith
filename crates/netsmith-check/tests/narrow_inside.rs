//! A module's user narrows a member inside it by its path (ADR 0116), here into the `support`
//! section of the IC that a parametric module holds (ADR 0115).

use netsmith_check::{Domain, Loader, check};

fn board(lines: &str) -> String {
    format!(
        "import std::parts::regulator::{{Buck, Lt8641}}\nimport std::sources::Rail\n\n\
         board B {{\n    .vin: Rail {{ .voltage &= 12V +/- 5%; .pos.net &= \"VIN\"; .neg.net &= \"GND\" }}\n    \
         .vout: Rail {{ .pos.net &= \"5V\"; .neg.net &= \"GND\" }}\n{lines}    .reg.vin ~ .vin\n    .reg.vout ~ .vout\n}}\n"
    )
}

#[test]
fn a_board_narrows_a_support_part_of_the_ic_a_parametric_module_holds() {
    let c = check(
        "t.nsm",
        &board("    .reg: Buck<Lt8641>\n    .reg.ic.soft_start.capacitance &= 22nF +/- 10%\n"),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    let Some(Domain::Quantity(q)) = c.boards[0].model.domain_at("reg.ic.soft_start.capacitance")
    else {
        panic!("no capacitance");
    };
    // The board's narrowing is a statement of the field, so the support's `?= 10nF` gives way.
    assert!((q.lo() - 19.8e-9).abs() < 1e-12 && (q.hi() - 24.2e-9).abs() < 1e-12, "{q}");
}

#[test]
fn a_join_does_not_reach_the_ic_of_a_parametric_module() {
    // Only a narrowing reaches inside; joining a pin of the named IC is issue #666.
    let c = check(
        "t.nsm",
        &board("    .reg: Buck<Lt8641>\n    .reg.ic.soft_start.a ~ .vin.neg\n"),
        &Loader::std(),
    );
    assert!(!c.is_ok());
    let text = c.render();
    assert!(
        text.contains("`ic` is inside `Buck<Lt8641>`")
            && text.contains("only narrowing (`&=`) reaches inside a module"),
        "{text}"
    );
}
