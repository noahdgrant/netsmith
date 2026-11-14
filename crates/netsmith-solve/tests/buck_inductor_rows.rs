//! A buck of about 1 A from 12 V finds an inductor in the standard catalog (issue #703).
//!
//! Before the molded 1210 rows, no standard inductor was above 5.6 uH and over 1 A (the largest
//! of the four rows above 4.7 uH is the Murata 10 uH at 0.75 A), so a buck IC that asks for 6 uH
//! or more could not be given an inductor at any useful load. The IC here is invented (`Zbk2`,
//! its figures are those of a 28 V, 2 A, 500 kHz part such as the ones #620 adds), and the
//! catalogs are the standard ones, so the pick is the standard catalog's.
//!
//! The worst case the inductor is judged at: a 16 V input, 3.3 V +/- 4 % out (3.432 V), 425 kHz
//! (500 kHz less 15 %) and the lowest inductance of a 10 uH +/- 20 % part, 8 uH. The ripple is
//! `3.432 * (1 - 3.432 / 16) / (8e-6 * 425e3) = 0.793 A`, and the inductor must hold the 1 A load
//! plus half of it, 1.40 A, without saturating.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn board() -> String {
    r#"import std::parts::regulator::{Buck, BuckIc}
import std::sources::Rail

part Zbk2 provides BuckIc {
    .vin.voltage >= 4.5V
    .vin.voltage <= 28V
    .vref &= 0.596V +/- 1.5%
    .min_input_voltage &= 4.5V
    .max_input_voltage &= 28V
    .max_output_current &= 2A
    .efficiency &= 0.85..0.95
    .min_input_capacitance &= 4.7uF
    .min_output_capacitance &= 10uF
    .fsw &= 500kHz +/- 15%
    .min_inductance &= 6uH
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)
    dc switching_regulator .vin .sw .efficiency
    pins {
        .vin.pos       power   "VIN"  "1"
        .vin.neg       power   "GND"  "2"
        .sw.pos        power   "SW"   "3"
        .fb            input   "FB"   "4"
    }
    .package: Package
    catalog zbk2 { .mpn, .package }
}

board Converter {
    .input: Rail {
        .voltage &= 9V..16V
        .peak &= 24V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..1A
    }
    .stage: Buck<Zbk2> {
        .vout.voltage &= 3.3V +/- 4%
    }
    .input ~ .stage.vin
    .stage.vout ~ .output
}
"#
    .to_string()
}

const IC: &str = "mpn,name,label,package,source
ZBK2-A,ZBK2-A invented buck IC,ZBK2,SOT-23-6,invented for a test
";

fn run() -> Solution {
    let c = check("buck_inductor.nsm", &board(), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file("zbk2", "catalogs/zbk2.csv", IC)
        .unwrap();
    solve(&c.boards[0], &catalogs)
}

fn range(s: &Solution, path: &str) -> (f64, f64) {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("`{path}` is {other:?}"),
    }
}

/// The LCSC numbers of the rows that the inductor catalog gained with this issue, above 5.6 uH and
/// over 1 A.
const MOLDED_ABOVE_6UH: [&str; 5] = ["C17701263", "C17701264", "C39846661", "C48945791", "C48945862"];

/// The buck is solved with no finding, and its inductor is one of the new rows: it is at least the
/// IC's 6 uH and holds the load and half the ripple, which no row of the catalog before did.
#[test]
fn a_buck_of_1a_from_12v_that_asks_for_6uh_picks_a_molded_inductor() {
    let s = run();
    let messages: Vec<_> = s.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert!(messages.is_empty(), "{messages:#?}");
    let pick = s
        .picks
        .iter()
        .find(|p| p.catalog == "inductor")
        .expect("the buck's inductor is picked");
    let lcsc = pick.lcsc.as_deref().expect("a real part");
    assert!(
        MOLDED_ABOVE_6UH.contains(&lcsc),
        "picked {lcsc} ({:?}), not one of the rows added for #703",
        pick.mpn
    );
    // The part's own figures: at least the IC's 6 uH at the low end of its tolerance, and an
    // Isat over the 1.40 A of the load and half the worst ripple.
    let l = range(&s, "stage.inductor.inductance");
    assert!(l.0 >= 6e-6, "{l:?}");
    let isat = range(&s, "stage.inductor.saturation_current");
    assert!(isat.0 >= 1.40 && isat.1 > 1.0, "{isat:?}");
    let ripple = range(&s, "stage.ripple");
    assert!(ripple.1 < 1.0, "{ripple:?}");
}
