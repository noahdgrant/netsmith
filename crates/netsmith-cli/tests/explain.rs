//! `netsmith explain <path> [file]` prints, as text, what the solve left at a part or member
//! and why each part has its row (issue 1008).

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

/// The converter of issue 744: a buck with a 5 mA to 50 mA load.
const BUCK: &str = r#"import std::parts::regulator::{Buck, Lmr16006y}
import std::sources::Rail

board Repro {
    .input: Rail {
        .voltage &= 9V..16V
        .peak &= 24V
        .pos.net &= "VIN"
    }
    .reg: Buck<Lmr16006y> {
        .vout.pos.net &= "V3V3"
        .vout.voltage &= 3.3V +/- 5%
    }
    .input ~ .reg.vin
    .load: Rail { .current &= 5mA..50mA }
    .reg.vout ~ .load
}
"#;

fn explain(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .arg("explain")
        .args(args)
        .current_dir(cwd)
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap()
}

#[test]
fn a_part_says_its_row_and_the_rows_passed_over_and_its_members_their_ranges() {
    let dir = TempDir::new("explain-part");
    fs::write(dir.join("buck.nsm"), BUCK).unwrap();
    let run = explain(&dir, &["reg.inductor", "buck.nsm", "--all-rows"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(said.contains("board Repro\n"), "{said}");
    assert!(
        said.contains("  reg.inductor: ") && said.contains(" from the `inductor` catalog"),
        "{said}"
    );
    // A row before it was refused by the converter's least inductance, with where that is.
    assert!(
        said.contains("refused (at std/parts/regulator.nsm:") && said.contains("min_inductance"),
        "{said}"
    );
    // Its members, with the range of current it carries.
    let current = said
        .lines()
        .find(|l| l.trim_start().starts_with("reg.inductor.current "))
        .unwrap_or_else(|| panic!("no current in {said}"));
    assert!(current.contains("..50"), "{current}");
    // Nothing of another part is there.
    assert!(!said.contains("  reg.ic.shdn_pullup:"), "{said}");
}

#[test]
fn a_module_says_every_part_inside_it_and_the_rows_listed_are_capped() {
    let dir = TempDir::new("explain-module");
    fs::write(dir.join("buck.nsm"), BUCK).unwrap();
    let run = explain(&dir, &["reg", "buck.nsm"]);
    assert!(run.status.success());
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(said.contains("  reg.inductor: ") && said.contains("  reg.ic.shdn_pullup: "), "{said}");
    // `reg` is the module, not every name that begins with the letters.
    let none = explain(&dir, &["re", "buck.nsm"]);
    assert_eq!(none.status.code(), Some(1));
    let said = String::from_utf8_lossy(&none.stderr);
    assert!(
        said.contains("no member and no part with a pick at or inside `re`")
            && said.contains("`reg.inductor`"),
        "{said}"
    );
    let board = explain(&dir, &["reg", "buck.nsm", "--board", "Other"]);
    assert_eq!(board.status.code(), Some(1));
}

#[test]
fn a_failed_group_is_explained_after_its_error() {
    let dir = TempDir::new("explain-failed");
    fs::write(
        dir.join("failing.nsm"),
        "import std::parts::passive::Resistor

board Failing {
    .r: Resistor
    .r.resistance >= 1000Gohm
}
",
    )
    .unwrap();
    let run = explain(&dir, &["r", "failing.nsm"]);
    assert_eq!(run.status.code(), Some(1));
    let error = String::from_utf8_lossy(&run.stderr);
    assert!(error.contains("no `resistor` candidate satisfies `r`"), "{error}");
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(said.contains("board Failing (failed)\n"), "{said}");
    assert!(
        said.contains("  r: no row of the `resistor` catalog could be chosen"),
        "{said}"
    );
    // The rows are capped at 20, and the rest are counted.
    assert!(said.contains("... and ") && said.contains("--all-rows"), "{said}");
    assert_eq!(said.matches(": refused (at failing.nsm:5): ").count(), 20, "{said}");
}
