//! `shared NAME limit N { MEMBER ... }` (ADR 0071, stage 3): how many of the members may be in
//! use at once, and members that are functions or pins, not only peripherals.
//!
//! `Zmcu` is invented for these tests. It is not a real part and is not in the standard
//! library.

use netsmith_check::*;

/// Four UARTs, an SPI and an I2C master, all on any pin of the application core, and twelve
/// pins. The shared resources are the tests' own.
fn part(shares: &str) -> String {
    format!(
        r#"
part Zmcu {{
    peripheral UARTE0 on APP_GPIO {{ UARTE0_TX:output UARTE0_RX:input }}
    peripheral UARTE1 on APP_GPIO {{ UARTE1_TX:output UARTE1_RX:input }}
    peripheral UARTE2 on APP_GPIO {{ UARTE2_TX:output UARTE2_RX:input }}
    peripheral UARTE3 on APP_GPIO {{ UARTE3_TX:output UARTE3_RX:input }}
    peripheral SPIM0 on APP_GPIO {{ SPIM0_SCK:output SPIM0_MOSI:output }}
    peripheral TWIM0 on APP_GPIO {{ TWIM0_SCL:bidirectional TWIM0_SDA:bidirectional }}
{shares}
    pins {{
        .p00 bidirectional "P0.00" "1"  fn APP_GPIO
        .p01 bidirectional "P0.01" "2"  fn APP_GPIO
        .p02 bidirectional "P0.02" "3"  fn APP_GPIO
        .p03 bidirectional "P0.03" "4"  fn APP_GPIO
        .p04 bidirectional "P0.04" "5"  fn APP_GPIO
        .p05 bidirectional "P0.05" "6"  fn APP_GPIO
        .p06 bidirectional "P0.06" "7"  fn APP_GPIO
        .p07 bidirectional "P0.07" "8"  fn APP_GPIO
        .p08 bidirectional "P0.08" "9"  fn APP_GPIO
        .p09 bidirectional "P0.09" "10" fn APP_GPIO
        .p10 bidirectional "P0.10" "11" fn APP_GPIO
        .p11 bidirectional "P0.11" "12" fn APP_GPIO
        .q0 bidirectional "Q0" "13" fn NET_GPIO
        .q1 bidirectional "Q1" "14" fn NET_GPIO
    }}
}}

/// Something with a pin for each of the part's, to put a net on.
part Probe {{
    pins {{
        .a00 passive "A00" "1"
        .a01 passive "A01" "2"
        .a02 passive "A02" "3"
        .a03 passive "A03" "4"
        .a04 passive "A04" "5"
        .a05 passive "A05" "6"
        .a06 passive "A06" "7"
        .a07 passive "A07" "8"
        .a08 passive "A08" "9"
        .a09 passive "A09" "10"
        .a10 passive "A10" "11"
        .a11 passive "A11" "12"
        .a12 passive "A12" "13"
        .a13 passive "A13" "14"
    }}
}}
"#
    )
}

const UARTS: &str = "    shared UARTS limit 2 { UARTE0 UARTE1 UARTE2 UARTE3 }";
const REFERENCE: &str = "    shared REFERENCE { .p10 .p11 }";

fn run_with(part: &str, statements: &str) -> Checked {
    check(
        "main.nsm",
        &format!("{part}\nboard B {{\n    .u: Zmcu\n    .probe: Probe\n{statements}\n}}\n"),
        &Loader::std(),
    )
}

fn run(shares: &str, statements: &str) -> Checked {
    run_with(&part(shares), statements)
}

/// Pin `n`, connected to the probe and chosen as `function`.
fn choose(n: u32, function: &str) -> String {
    format!("    .u.p{n:02} ~ .probe.a{n:02}\n    .u.p{n:02}.function &= \"{function}\"\n")
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn only(c: &Checked, message: &str) {
    assert_eq!(messages(c), [message], "{}", c.render());
}

fn line_of(c: &Checked, i: usize) -> String {
    let d = &c.diagnostics[i];
    let (line, _) = d.line_col(&c.sources);
    c.sources
        .get(d.source)
        .text
        .lines()
        .nth(line - 1)
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn a_limit_lets_that_many_members_run_at_once() {
    let c = run(
        UARTS,
        &format!("{}{}", choose(0, "UARTE0_TX"), choose(2, "UARTE3_TX")),
    );
    assert!(c.is_ok(), "{}", c.render());
    // Two functions of one peripheral are still one.
    let c = run(
        UARTS,
        &format!(
            "{}{}{}",
            choose(0, "UARTE0_TX"),
            choose(1, "UARTE0_RX"),
            choose(2, "UARTE3_TX")
        ),
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn more_members_than_the_limit_names_every_use_and_the_members() {
    let c = run(
        UARTS,
        &format!(
            "{}{}{}",
            choose(0, "UARTE0_TX"),
            choose(2, "UARTE1_TX"),
            choose(4, "UARTE3_TX")
        ),
    );
    only(
        &c,
        "`u` (`Zmcu`) uses 3 of the peripherals of `UARTS`, which allows 2 at a time: UARTE0 on `u.p00` as UARTE0_TX (main.nsm:52), UARTE1 on `u.p02` as UARTE1_TX (main.nsm:54), UARTE3 on `u.p04` as UARTE3_TX (main.nsm:56); `UARTS` lists UARTE0, UARTE1, UARTE2 and UARTE3",
    );
    // The error is at the third use, in path order.
    assert_eq!(line_of(&c, 0), ".u.p04.function &= \"UARTE3_TX\"");
}

#[test]
fn two_pins_that_cannot_both_be_used_are_named_with_how_each_is() {
    let c = run(
        REFERENCE,
        &format!("{}{}", choose(10, "APP_GPIO"), choose(11, "UARTE0_TX")),
    );
    only(
        &c,
        "`u.p10` and `u.p11` of `Zmcu` are both in use, but they share `REFERENCE`, which allows 1 at a time: `u.p10` as APP_GPIO (main.nsm:52) and `u.p11` as UARTE0_TX (main.nsm:54)",
    );
    assert_eq!(line_of(&c, 0), ".u.p11.function &= \"UARTE0_TX\"");
    // One of the two is fine, and a pin that nothing uses is not in use.
    let c = run(REFERENCE, &choose(10, "APP_GPIO"));
    assert!(c.is_ok(), "{}", c.render());
    let c = run(REFERENCE, "");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_pin_is_in_use_when_it_is_connected_with_no_function_chosen() {
    // `.q0` and `.q1` each offer one function, so nothing needs to be chosen: the wire is the use.
    let shares = "    shared QPAIR { .q0 .q1 }";
    let c = run(shares, "    .u.q0 ~ .probe.a12\n    .u.q1 ~ .probe.a13\n");
    only(
        &c,
        "`u.q0` and `u.q1` of `Zmcu` are both in use, but they share `QPAIR`, which allows 1 at a time: `u.q0` connected to `probe.a12` and `u.q1` connected to `probe.a13`",
    );
    // The error is at the part's declaration: there is no statement of a function to point at.
    assert_eq!(line_of(&c, 0), ".u: Zmcu");
    // One of them wired is fine.
    let c = run(shares, "    .u.q0 ~ .probe.a12\n");
    assert!(c.is_ok(), "{}", c.render());
    // A function chosen on a pin that is not connected is a use too.
    let c = run(
        shares,
        "    .u.q0 ~ .probe.a12\n    .u.q1.function &= \"NET_GPIO\"\n",
    );
    assert_eq!(messages(&c).len(), 1, "{}", c.render());
}

#[test]
fn functions_that_exclude_each_other_are_members_too() {
    // The clock line of the SPI and the clock line of the I2C master are one pad's worth of
    // hardware: two functions, of two peripherals.
    let shares = "    shared CLOCK { TWIM0_SCL SPIM0_SCK }";
    let c = run(
        shares,
        &format!("{}{}", choose(2, "SPIM0_SCK"), choose(4, "TWIM0_SCL")),
    );
    only(
        &c,
        "`u` (`Zmcu`) uses SPIM0_SCK and TWIM0_SCL, which share `CLOCK` and run one at a time: SPIM0_SCK on `u.p02` (main.nsm:52), TWIM0_SCL on `u.p04` (main.nsm:54)",
    );
    // The data lines are not in it.
    let c = run(
        shares,
        &format!("{}{}", choose(2, "SPIM0_SCK"), choose(4, "TWIM0_SDA")),
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_resource_may_mix_a_peripheral_a_function_and_a_pin() {
    let shares = "    shared SLEEPY limit 2 { UARTE3 TWIM0_SCL .p07 }";
    let c = run(
        shares,
        &format!(
            "{}{}{}",
            choose(0, "UARTE3_TX"),
            choose(4, "TWIM0_SCL"),
            choose(7, "APP_GPIO")
        ),
    );
    only(
        &c,
        "`u` (`Zmcu`) uses 3 of the members of `SLEEPY`, which allows 2 at a time: UARTE3 on `u.p00` as UARTE3_TX (main.nsm:52), TWIM0_SCL on `u.p04` (main.nsm:54), `u.p07` as APP_GPIO (main.nsm:56); `SLEEPY` lists UARTE3, TWIM0_SCL and `.p07`",
    );
}

#[test]
fn one_peripheral_in_two_resources_with_different_limits_is_judged_in_each() {
    let shares = format!("{UARTS}\n    shared PAIR {{ UARTE0 UARTE1 }}");
    let c = run(
        &shares,
        &format!("{}{}", choose(0, "UARTE0_TX"), choose(2, "UARTE1_TX")),
    );
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 1, "{}", c.render());
    assert!(
        msgs[0].starts_with(
            "`u` (`Zmcu`) uses UARTE0 and UARTE1, which share `PAIR` and run one at a time: "
        ),
        "{msgs:#?}"
    );
    // The resource with the limit does not list the others of another limit as free.
    assert!(!msgs[0].contains("UARTS"), "{msgs:#?}");
}

#[test]
fn the_order_of_the_statements_does_not_matter() {
    // The resources first, then the table, then the peripherals: the same part.
    let text = part(&format!("{UARTS}\n{REFERENCE}"));
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| *l == "part Zmcu {").unwrap();
    let end = start + lines[start..].iter().position(|l| *l == "}").unwrap();
    let body = &lines[start + 1..end];
    let of = |word: &'static str| {
        body.iter()
            .copied()
            .filter(move |l| l.trim_start().starts_with(word))
            .collect::<Vec<&str>>()
            .join("\n")
    };
    let table = body.iter().position(|l| l.trim() == "pins {").unwrap();
    let mut rows: Vec<&str> = body[table + 1..body.len() - 1].to_vec();
    rows.reverse();
    let moved = format!(
        "part Zmcu {{\n{}\n    pins {{\n{}\n    }}\n{}\n}}\n{}\n",
        of("shared"),
        rows.join("\n"),
        of("peripheral"),
        lines[end + 1..].join("\n"),
    );
    let statements = format!(
        "{}{}{}",
        choose(0, "UARTE0_TX"),
        choose(2, "UARTE1_TX"),
        choose(4, "UARTE3_TX")
    );
    let first = run_with(&text, &statements);
    let second = run_with(&moved, &statements);
    // The lines differ with the order; the rest does not.
    let bare = |c: &Checked| -> Vec<String> {
        messages(c)
            .iter()
            .map(|m| {
                let mut out = String::new();
                let mut rest = m.as_str();
                while let Some(i) = rest.find(" (main.nsm:") {
                    out.push_str(&rest[..i]);
                    rest = &rest[i + 1..];
                    rest = &rest[rest.find(')').unwrap() + 1..];
                }
                out.push_str(rest);
                out
            })
            .collect()
    };
    assert_eq!(messages(&first).len(), 1, "{}", first.render());
    assert_eq!(bare(&first), bare(&second), "{}", second.render());
}

#[test]
fn a_function_a_module_passes_on_counts_with_the_modules_path() {
    let src = format!(
        "{}
module Hub {{
    .a: Pin
    .b: Pin
    .mcu: Zmcu
    .probe: Probe
    .a ~ .mcu.p00
    .b ~ .mcu.p02
    .mcu.p00.function == .a.function
    .mcu.p02.function == .b.function
}}
board C {{
    .hub: Hub
    .hub.a.function &= \"UARTE0_TX\"
    .hub.b.function &= \"UARTE1_TX\"
}}
",
        part("    shared PAIR { UARTE0 UARTE1 }")
    );
    let c = check("main.nsm", &src, &Loader::std());
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 1, "{}", c.render());
    assert!(
        msgs[0].starts_with(
            "`hub.mcu` (`Zmcu`) uses UARTE0 and UARTE1, which share `PAIR` and run one at a time: UARTE0 on `hub.mcu.p00` as UARTE0_TX ("
        ),
        "{msgs:#?}"
    );
    // The line it names is the board's statement, which the board can edit.
    assert!(
        line_of(&c, 0).starts_with(".hub.b.function"),
        "{}",
        c.render()
    );
}

// ---- The mistakes of a definition ----

#[test]
fn a_limit_must_be_fewer_than_the_members() {
    let c = run(
        "    shared UARTS limit 4 { UARTE0 UARTE1 UARTE2 UARTE3 }",
        "",
    );
    only(
        &c,
        "`shared UARTS limit 4` allows every one of its 4 members at once, so it states no limit at all; a limit is at least 1 and fewer than the number of members: lower it, or remove it for one at a time",
    );
    assert_eq!(
        line_of(&c, 0),
        "shared UARTS limit 4 { UARTE0 UARTE1 UARTE2 UARTE3 }"
    );
    let c = run(
        "    shared UARTS limit 3 { UARTE0 UARTE1 UARTE2 UARTE3 }",
        "",
    );
    assert!(c.is_ok(), "{}", c.render());
    // `limit 1` is the same as none, and is allowed.
    let c = run("    shared UARTS limit 1 { UARTE0 UARTE1 }", "");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_member_is_a_peripheral_a_function_or_a_pin_of_the_part() {
    let c = run("    shared S { UARTE0 UARTE9 }", "");
    only(
        &c,
        "`UARTE9` in `shared S` is not a peripheral, a function or a pin of `Zmcu`; did you mean `UARTE0`? the peripherals are UARTE0, UARTE1, UARTE2, UARTE3, SPIM0 and TWIM0",
    );
    let c = run("    shared S { UARTE0 .nope }", "");
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 1, "{}", c.render());
    assert!(msgs[0].contains("nope"), "{msgs:#?}");
}

#[test]
fn a_function_may_not_be_a_member_with_its_own_peripheral() {
    let c = run("    shared S { UARTE0 UARTE0_TX UARTE1 }", "");
    only(
        &c,
        "`UARTE0_TX` is a function of `peripheral UARTE0`, which is also a member of `shared S`; list one of them",
    );
}

#[test]
fn a_member_is_listed_once() {
    let c = run("    shared S { .p10 .p11 .p10 }", "");
    only(&c, "`.p10` is listed twice in `shared S`");
    let c = run("    shared S { TWIM0_SCL SPIM0_SCK TWIM0_SCL }", "");
    only(&c, "`TWIM0_SCL` is listed twice in `shared S`");
}
