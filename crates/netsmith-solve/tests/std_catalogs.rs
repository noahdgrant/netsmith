//! What the solver does with the catalogs of the standard generics that have rows, a series, or
//! a column that a file may leave out (ADR 0068 stage 3). The four kinds that have synthesized
//! parts (`Resistor`, `Capacitor`, `Inductor`, `Led`) are solved with no series, with E24 and
//! with E96, and each result is recorded: the effort, the picks, the generic parts with what
//! `resolve` finds for them, the findings and the guideline findings of ADR 0065. The text
//! below was printed by the tree before stage 3 changed any of the code that reads these
//! tables, and the tree after it prints the same, except that the nearest rows listed for a
//! generic part that no row stands for are ranked by how near they come to the need that fails
//! (issue #182). Every part and number here is invented or stated by the design itself.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Series, Solution, alternatives, resolve, solve};

const LED: &str = include_str!("../../../examples/led.nsm");
const STATUS: &str = include_str!("../../../examples/status_indicator.nsm");
const LDO: &str = include_str!("../../../examples/linear_regulator.nsm");

/// A capacitor of 130 nF (E24 has it, no standard row does), an inductor of 3.3 uH, and a red
/// LED whose forward voltage is lower than any row's.
const MIXED: &str = "\
import std::parts::passive::{Capacitor, Inductor, Led}
import std::sources::Rail

board Mixed {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .c: Capacitor {
        .capacitance &= 130nF +/- 10%
        .max_voltage >= .bias * 1.5
    }
    .l: Inductor {
        .inductance &= 3.3uH +/- 20%
    }
    .led: Led {
        .color &= Red
        .forward_voltage &= 1.5V..1.7V
        .current &= 5mA..15mA
    }
    .supply.pos ~ .c.a
    .supply.neg ~ .c.b
    .supply.pos ~> .l ~> .led ~> .supply.neg
}
";

/// A resistor of a value no row has, and one that E96 has and E24 does not.
const VALUES: &str = "\
import std::parts::passive::Resistor

board Values {
    .a: Resistor { .resistance &= 390ohm +/- 1% }
    .b: Resistor { .resistance &= 2.21kohm +/- 1% }
    .c: Resistor { .resistance &= 335ohm..345ohm }
}
";

/// A value the file has, and one only the file's first row serves.
const PLAIN: &str = "\
import std::parts::passive::Resistor

board Plain {
    .a: Resistor { .resistance &= 330ohm +/- 1% }
    .b: Resistor { .resistance &= 360ohm +/- 1% }
}
";

/// Nothing limits the value of any of them: the solver takes the first row that fits.
const UNLIMITED: &str = "\
import std::parts::passive::{Capacitor, Inductor, Resistor}

board Unlimited {
    .r: Resistor {}
    .c: Capacitor {}
    .l: Inductor {}
}
";

/// A column that a catalog may leave empty, asked for.
const ASKED: &str = "\
import std::parts::passive::{Capacitor, Dielectric, Inductor, Resistor}

board Asked {
    .r: Resistor {
        .resistance &= 10kohm +/- 1%
        .temperature_coefficient <= 0.0001
    }
    .c: Capacitor {
        .capacitance &= 100nF +/- 10%
        .dielectric &= Dielectric::X7R
    }
    .l: Inductor {
        .inductance &= 2.2uH +/- 20%
        .rms_current >= 2A
    }
}
";

/// A capacitor and an inductor whose rating no catalog row has.
const RATED: &str = "\
import std::parts::passive::{Capacitor, Inductor}

board Rated {
    .c: Capacitor {
        .capacitance &= 4.7uF +/- 20%
        .max_voltage >= 500V
    }
    .l: Inductor {
        .inductance &= 10uH +/- 20%
        .saturation_current >= 40A
    }
}
";

/// A part of the project's that lists the `resistor` catalog, which is read by name.
const TAP: &str = "\
part Tap {
    .designator &= \"R\"
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    .resistance: Resistance
    .max_power: Power
    .package: Package
    catalog resistor { .resistance, .max_power, .package }
}

board Bench {
    .t: Tap { .resistance &= 390ohm +/- 1% }
    .u: Tap {}
}
";

/// FNV-1a, so that a long text is one short word of the record.
fn hash(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn series_name(series: Option<Series>) -> &'static str {
    series.map_or("none", Series::name)
}

/// One line per board for one series, the whole of what the solver says.
fn record(name: &str, source: &str, catalogs: &Catalogs) -> Vec<String> {
    let checked = check("main.nsm", source, &Loader::std());
    assert!(checked.is_ok(), "{name}: {}", checked.render());
    let series = series_name(catalogs.series());
    checked
        .standalone_boards()
        .map(|board| {
            let s: Solution = solve(board, catalogs);
            let mut picks: Vec<String> = s
                .picks
                .iter()
                .map(|p| {
                    let found = p.generic.as_ref().map(|g| {
                        let order = match resolve(g, catalogs, &|_| None) {
                            Ok(o) => format!("ok {}", o.name),
                            Err(e) => format!("err {}", hash(&e)),
                        };
                        let near: Vec<String> = alternatives(g, catalogs)
                            .iter()
                            .map(|a| a.line.clone())
                            .collect();
                        format!(" [{}; {}; {}]", g.describe(), order, near.join(" / "))
                    });
                    format!(
                        "{}={}x{} label {} rejected {}{}",
                        s.model.path_of(p.part),
                        p.candidate,
                        p.count,
                        p.label,
                        p.rejected,
                        found.unwrap_or_default()
                    )
                })
                .collect();
            picks.sort();
            let messages: Vec<&str> = s.diagnostics.iter().map(|d| d.message.as_str()).collect();
            let mut unresolved: Vec<String> = s
                .unresolved
                .iter()
                .map(|u| format!("{}: {}", s.model.path_of(u.part), u.reason))
                .collect();
            unresolved.sort();
            let mut loose: Vec<String> = s
                .unconstrained
                .iter()
                .map(|u| {
                    format!(
                        "{}.{} took {}",
                        s.model.path_of(u.part),
                        u.column,
                        u.row
                    )
                })
                .collect();
            loose.sort();
            format!(
                "{name}/{series}/{}: trials {} picks [{}] findings {} {} unresolved [{}] deferred {} not-evaluated {} unconstrained [{}] unstated {}",
                board.name,
                s.trials,
                picks.join(", "),
                messages.len(),
                hash(&messages.join("\n")),
                unresolved.join("; "),
                s.deferred.len(),
                s.not_evaluated.len(),
                loose.join("; "),
                s.unstated.len(),
            )
        })
        .collect()
}

const ROWS: &str = "lcsc,mpn,name,label,resistance,max_power,package,library,source\n\
C900002,ZZR-330-E,330Ω Extended,330,330ohm +/- 1%,62.5mW,0402,Extended,https://example.invalid/a\n\
C900001,ZZR-330-B,330Ω Basic,330,330ohm +/- 1%,62.5mW,0402,Basic,https://example.invalid/b\n\
C900003,ZZR-360-B,360Ω Basic,360,360ohm +/- 1%,62.5mW,0402,Basic,https://example.invalid/c\n";

fn all() -> Vec<String> {
    let boards = [
        ("led", LED),
        ("status", STATUS),
        ("ldo", LDO),
        ("mixed", MIXED),
        ("values", VALUES),
        ("plain", PLAIN),
        ("unlimited", UNLIMITED),
        ("asked", ASKED),
        ("rated", RATED),
        ("tap", TAP),
    ];
    let mut lines = Vec::new();
    for series in [None, Some(Series::E24), Some(Series::E96)] {
        let catalogs = Catalogs::jlcpcb().with_series(series);
        for (name, source) in boards {
            lines.extend(record(name, source, &catalogs));
        }
    }
    // A project's own file of the `resistor` catalog, which has none of the columns that a file
    // may leave out.
    for series in [None, Some(Series::E24)] {
        let mut project = Catalogs::empty().with_series(series);
        project
            .add_file("resistor", "catalogs/resistor.csv", ROWS)
            .unwrap();
        for (name, source) in [
            ("values", VALUES),
            ("plain", PLAIN),
            ("asked", ASKED),
            ("tap", TAP),
        ] {
            lines.extend(record(&format!("file-{name}"), source, &project));
        }
    }
    lines
}

#[test]
fn the_standard_catalogs_solve_and_resolve_as_they_did() {
    let now = all().join("\n");
    if std::env::var("STD_CATALOGS_PRINT").is_ok() {
        println!("{now}");
    }
    assert_eq!(now.trim(), BEFORE.trim());
}

const BEFORE: &str = "\
led/none/LedExample: trials 3 picks [led=KT-0603R RED LED 0603 C2286x1 label KT-0603R rejected 0, power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, r1=330Ω 1% 0402 C25104x1 label 330 rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 0 not-evaluated 0 unconstrained [] unstated 0\n\
status/none/IndicatorExample: trials 5 picks [enable_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, status_indicator.bypass=100nF 10% 16V X7R 0402 C1525x1 label 100n rejected 0, status_indicator.current_limit=1kΩ 1% 0402 C11702x1 label 1k rejected 0, status_indicator.led=LTST-C190KGKT GREEN LED 0603 C125094x1 label LTST-C190KGKT rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 3 not-evaluated 0 unconstrained [] unstated 0\n\
ldo/none/RegulatorExample: trials 62 picks [power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, power_out=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, stage.input_cap=10uF 20% 10V X5R 0402 C315248x1 label 10u rejected 30, stage.output_cap=22uF 20% 6.3V X5R 0603 C59461x1 label 22u rejected 27, stage.regulator=XC6206P332MR-G 3.3V 100mA LDO SOT-23 C5446x1 label XC6206P332MR-G rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 0 not-evaluated 0 unconstrained [] unstated 0\n\
mixed/none/Mixed: trials 0 picks [] findings 2 03f15ee7e23d7b4f unresolved [c: no candidate satisfies its requirements; l: no combination of candidates satisfies these parts together; led: no combination of candidates satisfies these parts together] deferred 3 not-evaluated 0 unconstrained [] unstated 0\n\
values/none/Values: trials 1 picks [a=390Ω 1% 0603 C23151x1 label 390 rejected 0] findings 2 efd86bd0db97d867 unresolved [b: no candidate satisfies its requirements; c: no candidate satisfies its requirements] deferred 6 not-evaluated 0 unconstrained [] unstated 0\n\
plain/none/Plain: trials 2 picks [a=330Ω 1% 0402 C25104x1 label 330 rejected 0, b=360Ω 1% 0402 C270607x1 label 360 rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 4 not-evaluated 0 unconstrained [] unstated 0\n\
unlimited/none/Unlimited: trials 3 picks [c=18pF 5% 50V C0G 0402 C1549x1 label 18p rejected 0, l=MHCD322512A-2R2M-A8S 2.2uH 3.6A inductor 1210 C285758x1 label MHCD322512A-2R2M-A8S rejected 0, r=330Ω 1% 0402 C25104x1 label 330 rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 4 not-evaluated 0 unconstrained [c.capacitance took 18pF 5% 50V C0G 0402 C1549; l.inductance took MHCD322512A-2R2M-A8S 2.2uH 3.6A inductor 1210 C285758; r.resistance took 330Ω 1% 0402 C25104] unstated 0\n\
asked/none/Asked: trials 1 picks [c=100nF 10% 16V X7R 0402 C1525x1 label 100n rejected 0] findings 2 dcf3398acbe50c81 unresolved [l: no candidate satisfies its requirements; r: no candidate satisfies its requirements] deferred 6 not-evaluated 0 unconstrained [] unstated 0\n\
rated/none/Rated: trials 3 picks [] findings 2 870b51dfd149ca82 unresolved [c: no candidate satisfies its requirements; l: no candidate satisfies its requirements] deferred 4 not-evaluated 0 unconstrained [] unstated 0\n\
tap/none/Bench: trials 0 picks [] findings 2 864ed8ba0bce66a8 unresolved [t: the `resistor` catalog cannot be read; u: the `resistor` catalog cannot be read] deferred 0 not-evaluated 0 unconstrained [] unstated 0\n\
led/E24/LedExample: trials 3 picks [led=KT-0603R RED LED 0603 C2286x1 label KT-0603R rejected 0, power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, r1=330Ω 1% 0402 C25104x1 label 330 rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 0 not-evaluated 0 unconstrained [] unstated 0\n\
status/E24/IndicatorExample: trials 5 picks [enable_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, status_indicator.bypass=100nF 10% 16V X7R 0402 C1525x1 label 100n rejected 0, status_indicator.current_limit=1kΩ 1% 0402 C11702x1 label 1k rejected 0, status_indicator.led=LTST-C190KGKT GREEN LED 0603 C125094x1 label LTST-C190KGKT rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 3 not-evaluated 0 unconstrained [] unstated 0\n\
ldo/E24/RegulatorExample: trials 62 picks [power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, power_out=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, stage.input_cap=10uF 20% 10V X5R 0402 C315248x1 label 10u rejected 30, stage.output_cap=22uF 20% 6.3V X5R 0603 C59461x1 label 22u rejected 27, stage.regulator=XC6206P332MR-G 3.3V 100mA LDO SOT-23 C5446x1 label XC6206P332MR-G rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 0 not-evaluated 0 unconstrained [] unstated 0\n\
mixed/E24/Mixed: trials 3 picks [c=generic 130 nF 10% 0402 (E24, needs a recorded DC-bias curve)x1 label 130n rejected 0 [130 nF 10% 0402, rated voltage at least 7.875 V (E24, needs a recorded DC-bias curve); err f777404ac38a7748; 100nF 10% 16V X7R 0402 C1525 (-23.1%, Basic) / 100nF 10% 50V X7R 0402 C307331 (-23.1%, Basic) / 220nF 10% 16V X7R 0402 C16772 (+69.2%, Basic)], l=generic 3.3 uH 20% 0402 (E24)x1 label 3.3u rejected 0 [3.3 uH 20% 0402, saturation current at least 15 mA (E24); err fe5681ce08537a47; LQH32PN3R3NN0L 3.3uH 1.25A inductor 1210 C337905 (+0.0%, Extended) / LQH32PN4R7NN0L 4.7uH 1A inductor 1210 C98021 (+42.4%, Extended) / MHCD322512A-2R2M-A8S 2.2uH 3.6A inductor 1210 C285758 (-33.3%, Extended)], led=generic LED 0402x1 label LED rejected 0 [Red LED 0402, forward voltage 1.5 V..1.7 V; err 30f4cd10085b7820; XL-1005SURC RED LED 0402 C25503345 (Extended) / XL-1608SURC-06 RED LED 0603 C965799 (Extended) / KT-0603R RED LED 0603 C2286 (Basic)]] findings 0 cbf29ce484222325 unresolved [] deferred 1 not-evaluated 0 unconstrained [] unstated 0\n\
values/E24/Values: trials 1 picks [a=390Ω 1% 0603 C23151x1 label 390 rejected 0] findings 2 efd86bd0db97d867 unresolved [b: no candidate satisfies its requirements; c: no candidate satisfies its requirements] deferred 6 not-evaluated 0 unconstrained [] unstated 0\n\
plain/E24/Plain: trials 2 picks [a=330Ω 1% 0402 C25104x1 label 330 rejected 0, b=360Ω 1% 0402 C270607x1 label 360 rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 4 not-evaluated 0 unconstrained [] unstated 0\n\
unlimited/E24/Unlimited: trials 3 picks [c=18pF 5% 50V C0G 0402 C1549x1 label 18p rejected 0, l=MHCD322512A-2R2M-A8S 2.2uH 3.6A inductor 1210 C285758x1 label MHCD322512A-2R2M-A8S rejected 0, r=330Ω 1% 0402 C25104x1 label 330 rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 4 not-evaluated 0 unconstrained [c.capacitance took 18pF 5% 50V C0G 0402 C1549; l.inductance took MHCD322512A-2R2M-A8S 2.2uH 3.6A inductor 1210 C285758; r.resistance took 330Ω 1% 0402 C25104] unstated 0\n\
asked/E24/Asked: trials 1 picks [c=100nF 10% 16V X7R 0402 C1525x1 label 100n rejected 0] findings 2 3038b506209b6fcb unresolved [l: no candidate satisfies its requirements; r: no candidate satisfies its requirements] deferred 6 not-evaluated 0 unconstrained [] unstated 0\n\
rated/E24/Rated: trials 5 picks [c=generic 4.7 uF 20% 0402 (E24, needs a recorded DC-bias curve)x1 label 4.7u rejected 2 [4.7 uF 20% 0402, rated voltage at least 500 V (E24, needs a recorded DC-bias curve); err 16d2248a16798f4c; 2.2uF 10% 100V X7R 1210 C55151 (-53.2%, Extended) / 2.2uF 10% 100V X7R 1206 C170101 (-53.2%, Extended) / 1uF 10% 100V X7R 1206 C13832 (-78.7%, Extended)], l=generic 10 uH 20% 0402 (E24)x1 label 10u rejected 1 [10 uH 20% 0402, saturation current at least 40 A (E24); err 0f0a2f50c0b3c4ce; MHCD322512A-2R2M-A8S 2.2uH 3.6A inductor 1210 C285758 (-78.0%, Extended) / LQH32PN3R3NN0L 3.3uH 1.25A inductor 1210 C337905 (-67.0%, Extended) / LQH32PN4R7NN0L 4.7uH 1A inductor 1210 C98021 (-53.0%, Extended)]] findings 0 cbf29ce484222325 unresolved [] deferred 2 not-evaluated 0 unconstrained [] unstated 0\n\
tap/E24/Bench: trials 0 picks [] findings 2 864ed8ba0bce66a8 unresolved [t: the `resistor` catalog cannot be read; u: the `resistor` catalog cannot be read] deferred 0 not-evaluated 0 unconstrained [] unstated 0\n\
led/E96/LedExample: trials 3 picks [led=KT-0603R RED LED 0603 C2286x1 label KT-0603R rejected 0, power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, r1=330Ω 1% 0402 C25104x1 label 330 rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 0 not-evaluated 0 unconstrained [] unstated 0\n\
status/E96/IndicatorExample: trials 5 picks [enable_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, status_indicator.bypass=100nF 10% 16V X7R 0402 C1525x1 label 100n rejected 0, status_indicator.current_limit=1kΩ 1% 0402 C11702x1 label 1k rejected 0, status_indicator.led=LTST-C190KGKT GREEN LED 0603 C125094x1 label LTST-C190KGKT rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 3 not-evaluated 0 unconstrained [] unstated 0\n\
ldo/E96/RegulatorExample: trials 62 picks [power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, power_out=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1 label Conn_01x02 rejected 0, stage.input_cap=10uF 20% 10V X5R 0402 C315248x1 label 10u rejected 30, stage.output_cap=22uF 20% 6.3V X5R 0603 C59461x1 label 22u rejected 27, stage.regulator=XC6206P332MR-G 3.3V 100mA LDO SOT-23 C5446x1 label XC6206P332MR-G rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 0 not-evaluated 0 unconstrained [] unstated 0\n\
mixed/E96/Mixed: trials 3 picks [c=generic 130 nF 10% 0402 (E24, needs a recorded DC-bias curve)x1 label 130n rejected 0 [130 nF 10% 0402, rated voltage at least 7.875 V (E24, needs a recorded DC-bias curve); err f777404ac38a7748; 100nF 10% 16V X7R 0402 C1525 (-23.1%, Basic) / 100nF 10% 50V X7R 0402 C307331 (-23.1%, Basic) / 220nF 10% 16V X7R 0402 C16772 (+69.2%, Basic)], l=generic 3.3 uH 20% 0402 (E24)x1 label 3.3u rejected 0 [3.3 uH 20% 0402, saturation current at least 15 mA (E24); err fe5681ce08537a47; LQH32PN3R3NN0L 3.3uH 1.25A inductor 1210 C337905 (+0.0%, Extended) / LQH32PN4R7NN0L 4.7uH 1A inductor 1210 C98021 (+42.4%, Extended) / MHCD322512A-2R2M-A8S 2.2uH 3.6A inductor 1210 C285758 (-33.3%, Extended)], led=generic LED 0402x1 label LED rejected 0 [Red LED 0402, forward voltage 1.5 V..1.7 V; err 30f4cd10085b7820; XL-1005SURC RED LED 0402 C25503345 (Extended) / XL-1608SURC-06 RED LED 0603 C965799 (Extended) / KT-0603R RED LED 0603 C2286 (Basic)]] findings 0 cbf29ce484222325 unresolved [] deferred 1 not-evaluated 0 unconstrained [] unstated 0\n\
values/E96/Values: trials 3 picks [a=390Ω 1% 0603 C23151x1 label 390 rejected 0, b=generic 2210 ohm 1% 0402 62.5 mW (E96)x1 label 2.21k rejected 0 [2210 ohm 1% 0402 62.5 mW (E96); err 90fff57232eade75; 2.2kΩ 1% 0402 C25879 (-0.5%, Basic) / 2.4kΩ 1% 0402 C25882 (+8.6%, Extended) / 2kΩ 1% 0402 C4109 (-9.5%, Basic)], c=generic 340 ohm 1% 0402 62.5 mW (E96)x1 label 340 rejected 0 [340 ohm 1% 0402 62.5 mW (E96); err 4752a04a6f6eaeb9; 330Ω 1% 0402 C25104 (-2.9%, Basic) / 360Ω 1% 0402 C270607 (+5.9%, Extended) / 300Ω 1% 0402 C25102 (-11.8%, Extended)]] findings 0 cbf29ce484222325 unresolved [] deferred 6 not-evaluated 0 unconstrained [] unstated 0\n\
plain/E96/Plain: trials 2 picks [a=330Ω 1% 0402 C25104x1 label 330 rejected 0, b=360Ω 1% 0402 C270607x1 label 360 rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 4 not-evaluated 0 unconstrained [] unstated 0\n\
unlimited/E96/Unlimited: trials 3 picks [c=18pF 5% 50V C0G 0402 C1549x1 label 18p rejected 0, l=MHCD322512A-2R2M-A8S 2.2uH 3.6A inductor 1210 C285758x1 label MHCD322512A-2R2M-A8S rejected 0, r=330Ω 1% 0402 C25104x1 label 330 rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 4 not-evaluated 0 unconstrained [c.capacitance took 18pF 5% 50V C0G 0402 C1549; l.inductance took MHCD322512A-2R2M-A8S 2.2uH 3.6A inductor 1210 C285758; r.resistance took 330Ω 1% 0402 C25104] unstated 0\n\
asked/E96/Asked: trials 1 picks [c=100nF 10% 16V X7R 0402 C1525x1 label 100n rejected 0] findings 2 d2dfbf28ec862d57 unresolved [l: no candidate satisfies its requirements; r: no candidate satisfies its requirements] deferred 6 not-evaluated 0 unconstrained [] unstated 0\n\
rated/E96/Rated: trials 5 picks [c=generic 4.7 uF 20% 0402 (E24, needs a recorded DC-bias curve)x1 label 4.7u rejected 2 [4.7 uF 20% 0402, rated voltage at least 500 V (E24, needs a recorded DC-bias curve); err 16d2248a16798f4c; 2.2uF 10% 100V X7R 1210 C55151 (-53.2%, Extended) / 2.2uF 10% 100V X7R 1206 C170101 (-53.2%, Extended) / 1uF 10% 100V X7R 1206 C13832 (-78.7%, Extended)], l=generic 10 uH 20% 0402 (E24)x1 label 10u rejected 1 [10 uH 20% 0402, saturation current at least 40 A (E24); err 0f0a2f50c0b3c4ce; MHCD322512A-2R2M-A8S 2.2uH 3.6A inductor 1210 C285758 (-78.0%, Extended) / LQH32PN3R3NN0L 3.3uH 1.25A inductor 1210 C337905 (-67.0%, Extended) / LQH32PN4R7NN0L 4.7uH 1A inductor 1210 C98021 (-53.0%, Extended)]] findings 0 cbf29ce484222325 unresolved [] deferred 2 not-evaluated 0 unconstrained [] unstated 0\n\
tap/E96/Bench: trials 0 picks [] findings 2 864ed8ba0bce66a8 unresolved [t: the `resistor` catalog cannot be read; u: the `resistor` catalog cannot be read] deferred 0 not-evaluated 0 unconstrained [] unstated 0\n\
file-values/none/Values: trials 0 picks [] findings 3 cc4054b3dfee51ff unresolved [a: no candidate satisfies its requirements; b: no candidate satisfies its requirements; c: no candidate satisfies its requirements] deferred 6 not-evaluated 0 unconstrained [] unstated 0\n\
file-plain/none/Plain: trials 2 picks [a=330Ω Extendedx1 label 330 rejected 0, b=360Ω Basicx1 label 360 rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 4 not-evaluated 0 unconstrained [] unstated 0\n\
file-asked/none/Asked: trials 0 picks [] findings 1 21ec1a3db2847667 unresolved [c: there is no `capacitor` catalog to choose from; l: there is no `inductor` catalog to choose from; r: no candidate satisfies its requirements] deferred 6 not-evaluated 0 unconstrained [] unstated 0\n\
file-tap/none/Bench: trials 1 picks [u=330Ω Extendedx1 label 330 rejected 0] findings 1 a61c365262d9b284 unresolved [t: no candidate satisfies its requirements] deferred 0 not-evaluated 0 unconstrained [u.resistance took 330Ω Extended] unstated 0\n\
file-values/E24/Values: trials 1 picks [a=generic 390 ohm 1% 0402 62.5 mW (E24)x1 label 390 rejected 0 [390 ohm 1% 0402 62.5 mW (E24); err ae7749bf0c3f343e; 360Ω Basic (-7.7%, Basic) / 330Ω Basic (-15.4%, Basic) / 330Ω Extended (-15.4%, Extended)]] findings 2 efd86bd0db97d867 unresolved [b: no candidate satisfies its requirements; c: no candidate satisfies its requirements] deferred 6 not-evaluated 0 unconstrained [] unstated 0\n\
file-plain/E24/Plain: trials 2 picks [a=330Ω Extendedx1 label 330 rejected 0, b=360Ω Basicx1 label 360 rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 4 not-evaluated 0 unconstrained [] unstated 0\n\
file-asked/E24/Asked: trials 0 picks [] findings 1 b7cb18421d5f0ee2 unresolved [c: there is no `capacitor` catalog to choose from; l: there is no `inductor` catalog to choose from; r: no candidate satisfies its requirements] deferred 6 not-evaluated 0 unconstrained [] unstated 0\n\
file-tap/E24/Bench: trials 2 picks [t=generic 390 ohm 1% 0402 62.5 mW (E24)x1 label 390 rejected 0 [390 ohm 1% 0402 62.5 mW (E24); err ae7749bf0c3f343e; 360Ω Basic (-7.7%, Basic) / 330Ω Basic (-15.4%, Basic) / 330Ω Extended (-15.4%, Extended)], u=330Ω Extendedx1 label 330 rejected 0] findings 0 cbf29ce484222325 unresolved [] deferred 0 not-evaluated 0 unconstrained [u.resistance took 330Ω Extended] unstated 0\n\
";
