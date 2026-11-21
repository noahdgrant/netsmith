//! Curve files (issue #1260): a row's column read off points over the part's current and the
//! board's temperature, beside the catalog. The row and its curve are invented: no real part's
//! curve has a recorded source yet (`data/jlcpcb/README.md`).

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// One invented Schottky row: 0.6 V at its rated 1 A is the single figure.
const ROW: &str = "mpn,forward_voltage,max_reverse_voltage,max_forward_current,package,source
ZZD-CURVE,0.6V,40V,1A,SOD-323,https://example.invalid/zzd
";

/// Its forward voltage at three temperatures; the 125 C line has currents of its own.
const CURVE: &str = "temperature,current,forward_voltage
-40degC,1mA,0.35V
-40degC,1A,0.62V
25degC,1mA,0.30V
25degC,100mA,0.42V
25degC,1A,0.55V
125degC,1mA,0.18V
125degC,1A,0.45V
";

const FILE: &str = "catalogs/curves/schottky/ZZD-CURVE.csv";

fn catalogs(curve: Option<&str>) -> Catalogs {
    let mut c = Catalogs::empty();
    crate::offered::add(&mut c, "schottky", "catalogs/schottky.csv", ROW).unwrap();
    if let Some(text) = curve {
        c.add_curve("schottky", "ZZD-CURVE", FILE, text).unwrap();
    }
    c
}

/// A board with one Schottky diode, its statements `diode`, and `rest` after it.
fn board(diode: &str, rest: &str) -> String {
    format!(
        "import std::parts::diode::Schottky
import std::env::Environment

board B {{
    .d: Schottky {{ {diode} }}
{rest}
}}
"
    )
}

fn solved(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn errors(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn forward_voltage(s: &Solution) -> (f64, f64) {
    match s.model.domain_at("d.forward_voltage") {
        Some(Domain::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("{other:?}"),
    }
}

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

#[test]
fn the_curve_is_read_at_the_current_and_the_environment() {
    let s = solved(
        &board(
            ".current &= 100mA",
            "    .environment: Environment { .temperature &= 25degC }",
        ),
        &catalogs(Some(CURVE)),
    );
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert_eq!(s.picks[0].candidate, "ZZD-CURVE");
    assert!(close(forward_voltage(&s), (0.42, 0.42)), "{:?}", forward_voltage(&s));
}

#[test]
fn without_an_environment_the_curve_is_read_over_all_its_temperatures() {
    let s = solved(&board(".current &= 100mA", ""), &catalogs(Some(CURVE)));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    // At 100 mA: 0.42 V at 25 C, and on the 125 C line 0.18 V + 0.27 V * 99/999.
    let hot = 0.18 + 0.27 * 0.099 / 0.999;
    assert!(close(forward_voltage(&s), (hot, 0.42)), "{:?}", forward_voltage(&s));
}

#[test]
fn a_range_of_current_and_temperature_gives_the_lowest_to_the_highest() {
    let s = solved(
        &board(
            ".current &= 1mA..1A",
            "    .environment: Environment { .temperature &= -40degC..125degC }",
        ),
        &catalogs(Some(CURVE)),
    );
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert!(close(forward_voltage(&s), (0.18, 0.62)), "{:?}", forward_voltage(&s));
}

#[test]
fn without_a_current_the_row_keeps_its_single_figure() {
    let s = solved(&board("", ""), &catalogs(Some(CURVE)));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert!(close(forward_voltage(&s), (0.6, 0.6)), "{:?}", forward_voltage(&s));
}

#[test]
fn a_drop_the_figure_fails_is_met_at_the_stated_current() {
    let src = board(
        ".current &= 100mA",
        "    .environment: Environment { .temperature &= 25degC }\n    .d.drop <= 0.5V",
    );
    let s = solved(&src, &catalogs(Some(CURVE)));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    // The same row with no curve has only its 0.6 V.
    let s = solved(&src, &catalogs(None));
    assert!(!errors(&s).is_empty(), "the figure alone does not meet 0.5 V");
}

#[test]
fn a_current_past_the_points_is_refused_and_names_the_file() {
    let s = solved(&board(".current &= 0.5mA", ""), &catalogs(Some(CURVE)));
    let all = errors(&s).join("\n");
    assert!(all.contains(FILE), "{all}");
    assert!(all.contains("its `current` is 500uA"), "{all}");
    assert!(all.contains("the curve covers 1mA..1A"), "{all}");
}

#[test]
fn a_cold_environment_past_the_points_is_refused() {
    let s = solved(
        &board(
            ".current &= 100mA",
            "    .environment: Environment { .temperature &= -55degC..25degC }",
        ),
        &catalogs(Some(CURVE)),
    );
    let all = errors(&s).join("\n");
    assert!(all.contains("its `temperature`"), "{all}");
}

fn resolve_error(curve: &str, row: &str, catalog_rows: &str) -> String {
    let mut c = Catalogs::empty();
    crate::offered::add(&mut c, "schottky", "catalogs/schottky.csv", catalog_rows)
        .unwrap();
    c.add_curve("schottky", row, FILE, curve).unwrap();
    let s = solved(&board(".current &= 100mA", ""), &c);
    errors(&s).join("\n")
}

#[test]
fn a_curve_that_fits_no_row_or_column_is_an_error_naming_its_file() {
    let e = resolve_error(CURVE, "ZZD-OTHER", ROW);
    assert!(
        e.contains(FILE) && e.contains("a curve file is named by the mpn of the row it is for"),
        "{e}"
    );
    let e = resolve_error("current,max_power\n1mA,1W\n", "ZZD-CURVE", ROW);
    assert!(e.contains("is not a quantity column of `catalog schottky`"), "{e}");
    let e = resolve_error("current,max_forward_current\n1mA,1V\n", "ZZD-CURVE", ROW);
    assert!(e.contains("the points of `max_forward_current` are in"), "{e}");
    let blank = ROW.replace("ZZD-CURVE,0.6V", "ZZD-CURVE,");
    let e = resolve_error(CURVE, "ZZD-CURVE", &blank);
    assert!(e.contains("keeps its single figure"), "{e}");
}

#[test]
fn a_second_curve_of_the_same_column_and_a_curve_of_no_catalog_are_errors() {
    let mut c = catalogs(Some(CURVE));
    let e = c
        .add_curve("schottky", "ZZD-CURVE", "other.csv", CURVE)
        .unwrap_err();
    assert!(e.contains("already has a curve of `forward_voltage`") && e.contains(FILE), "{e}");
    let e = c
        .add_curve("zener", "ZZD-CURVE", "z.csv", CURVE)
        .unwrap_err();
    assert!(e.contains("there is no catalog `zener`"), "{e}");
}

#[test]
fn a_curve_named_by_the_mpn_of_two_manufacturers_parts_is_refused_naming_both_rows() {
    // Two rows of one mpn from two makers are two parts (ADR 0143), and a file named by the mpn
    // cannot say which one its points were read for (issue #1464): it attaches to neither.
    let rows = "manufacturer,mpn,forward_voltage,max_reverse_voltage,max_forward_current,package,source
Acme Analog,ZZD-CURVE,0.6V,40V,1A,SOD-323,https://example.invalid/acme
Zenith Semi,ZZD-CURVE,0.5V,40V,1A,SOD-123,https://example.invalid/zenith
";
    let e = resolve_error(CURVE, "ZZD-CURVE", rows);
    assert!(
        e.contains(&format!(
            "{FILE}: the rows at catalogs/schottky.csv:2 (`Acme Analog`) and catalogs/schottky.csv:3 (`Zenith Semi`) both have the mpn `ZZD-CURVE`"
        )) && e.contains("a curve file is for one part"),
        "{e}"
    );
    // One maker's row alone takes the file.
    let one = rows.lines().take(2).collect::<Vec<_>>().join("\n") + "\n";
    let mut c = Catalogs::empty();
    crate::offered::add(&mut c, "schottky", "catalogs/schottky.csv", &one).unwrap();
    c.add_curve("schottky", "ZZD-CURVE", FILE, CURVE).unwrap();
    let s = solved(
        &board(
            ".current &= 100mA",
            "    .environment: Environment { .temperature &= 25degC }",
        ),
        &c,
    );
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert!(close(forward_voltage(&s), (0.42, 0.42)), "{:?}", forward_voltage(&s));
}
