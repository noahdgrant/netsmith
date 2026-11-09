//! The guideline about a bus role (ADR 0064): a `bidirectional` row on a line whose role gives
//! a direction (of an interface, or of a function of a peripheral that states no kind) is
//! accepted, and the part is asked to say `input`, `output` or `tri_state`, as its datasheet
//! does; and an interface that is joined to another end of its bus and states no role is asked
//! to state it (stage 5). Both are findings of the kind a project may make blocking
//! (`guidelines = "error"`, `--strict`), because they are advice: the part may be right.

use netsmith_check::{InstId, Model};
use netsmith_design::guidelines::Guideline;
use netsmith_solve::Solution;

/// One finding for each `bidirectional` row of a part that states its role and has a line the
/// role gives a direction, and one for each joined interface that states no role. For one board
/// of a system, only those inside it.
pub(crate) fn findings(solution: &Solution, board: Option<InstId>) -> Vec<Guideline> {
    let scoped;
    let model: &Model = match board {
        Some(b) => {
            scoped = solution.model.scoped_to(b);
            &scoped
        }
        None => &solution.model,
    };
    netsmith_check::bidirectional_rows(model)
        .into_iter()
        .chain(netsmith_check::joined_without_role(model))
        .map(|(part, message)| Guideline {
            rule: "bus role",
            message,
            at: Some(part),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_check::{Loader, check};
    use netsmith_solve::{Catalogs, solve};

    const BOARD: &str = r#"
import std::bus::{Spi, SpiRole}
import std::sources::Rail

part Master {
    .vdd: Rail
    .spi: Spi
    .spi.role &= SpiRole::Master
    pins {
        .vdd.pos  power  "VDD"  "1"
        .vdd.neg  power  "VSS"  "2"
        .spi.sclk output "SCK"  "3"
        .spi.mosi output "MOSI" "4"
        .spi.miso input  "MISO" "5"
        .spi.cs   output "CS"   "6"
    }
}

/// A sensor from before roles: no role, every pin `bidirectional`.
part Old {
    .vdd: Rail
    .spi: Spi
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .spi.sclk bidirectional "SCL" "3"
        .spi.mosi bidirectional "SDI" "4"
        .spi.miso bidirectional "SDO" "5"
        .spi.cs   bidirectional "CS"  "6"
    }
}

board B {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .cpu: Master
    .old: Old
    .supply ~ .cpu.vdd
    .supply ~ .old.vdd
    .cpu.spi ~ .old.spi
}
"#;

    fn found(src: &str) -> Vec<Guideline> {
        let checked = check("main.nsm", src, &Loader::std());
        assert!(checked.is_ok(), "{}", checked.render());
        let solution = solve(checked.board("B").unwrap(), &Catalogs::empty());
        findings(&solution, None)
    }

    #[test]
    fn a_joined_interface_with_no_role_is_a_bus_role_finding_at_its_part() {
        let found = found(BOARD);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].rule, "bus role");
        assert!(
            found[0].message.starts_with(
                "`old.spi` is joined to another end of a `Spi` bus and states no role"
            ),
            "{}",
            found[0].message
        );
        assert!(found[0].at.is_some());
    }

    #[test]
    fn a_part_that_states_its_role_has_no_finding() {
        let stated = BOARD.replace(
            ".spi: Spi\n    pins {\n        .vdd.pos  power         \"VDD\" \"1\"",
            ".spi: Spi\n    .spi.role &= SpiRole::Slave\n    pins {\n        .vdd.pos  power         \"VDD\" \"1\"",
        );
        let found = found(&stated);
        // The role is stated, so the only advice left is the `bidirectional` rows.
        assert_eq!(found.len(), 4, "{found:#?}");
        assert!(
            found.iter().all(|g| g
                .message
                .contains("is `bidirectional` in the table of `Old`")),
            "{found:#?}"
        );
    }
}
