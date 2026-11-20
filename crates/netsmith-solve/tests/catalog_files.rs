//! Catalog files a project adds (ADR 0022): reading them, picking from them, and what is
//! said when a file is wrong. Every part and file here is invented; `example.invalid` is not
//! a real address.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, SoldOut, Solution, Value, solve};

/// A regulator with a part number as its key, in one package.
const BOARD: &str = r#"
import std::sources::Rail

part Zz9 {
    .vin:  Rail
    .vout: Rail
    .vin.neg ~ .vout.neg

    .dropout:     Voltage
    .max_current: Current
    .vin.voltage >= .vout.voltage + .dropout
    .vin.current == -.vout.current
    -.vout.current <= .max_current

    pins "SOT-23-5" {
        .vin.pos       power       "IN"     "1"
        .vin.neg       power       "GND"    "2"
        .en            input       "EN"     "3"
        .nc            no_connect  "NC"     "4"
        .vout.pos      power       "OUT"    "5"
    }
    pins "DFN-6" {
        .vin.pos       power       "IN"     "1"
        .vin.neg       power       "GND"    "2" "6"
        .en            input       "EN"     "3"
        .nc            no_connect  "NC"     "4"
        .vout.pos      power       "OUT"    "5"
    }

    .package: Package
    catalog zz9 by .mpn { .mpn, .vout.voltage, .dropout, .max_current, .package }
}

board B {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .u: Zz9 { PICK }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.en ~ .u.vin.pos
}
"#;

const HEADER: &str =
    "mpn,package,vout.voltage,dropout,max_current,price,stock,date,source\n";

/// Two invented regulators, the smaller first.
fn file() -> String {
    format!(
        "{HEADER}\
ZZ9-33-S,SOT-23-5,3.3V +/- 2%,0.3V,0.5A,0.10,1000,2026-10-31,https://example.invalid/zz9-33-s.pdf\n\
ZZ9-33-D,DFN-6,3.3V +/- 2%,0.2V,1A,0.20,500,2026-10-31,\"https://example.invalid/zz9, rev B\"\n"
    )
}

fn q(name: &str, d: netsmith_quantity::Dimension) -> (String, netsmith_check::FieldKind) {
    (name.to_string(), netsmith_check::FieldKind::Quantity(d))
}

fn t(name: &str) -> (String, netsmith_check::FieldKind) {
    (name.to_string(), netsmith_check::FieldKind::Text)
}

fn catalogs(files: &[(&str, &str)]) -> Result<Catalogs, String> {
    let mut c = Catalogs::empty();
    for (name, text) in files {
        c.add_file(name, &format!("catalogs/{name}.csv"), text)?;
    }
    Ok(c)
}

fn solved(pick: &str, catalogs: &Catalogs) -> Solution {
    let src = BOARD.replace("PICK", pick);
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_design_names_a_part_of_a_file_by_its_key_and_gets_that_row() {
    let c = catalogs(&[("zz9", &file())]).unwrap();
    let s = solved(r#".mpn &= "ZZ9-33-D""#, &c);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks.len(), 1);
    assert_eq!(s.picks[0].candidate, "ZZ9-33-D");
    assert_eq!(s.picks[0].mpn.as_deref(), Some("ZZ9-33-D"));
    // No supplier offers the part (ADR 0143): it is bought by no number and sorted in no library.
    assert_eq!(s.picks[0].library, None);
    assert_eq!(s.picks[0].lcsc, None);
    // The package is the row's, so the pinout of DFN-6 is the one used.
    match s.model.domain_at("u.package") {
        Some(netsmith_check::Domain::Text(t)) => assert_eq!(t, "DFN-6"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn without_a_name_the_message_lists_the_rows_of_the_file() {
    let c = catalogs(&[("zz9", &file())]).unwrap();
    let s = solved("", &c);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("name the part with .mpn: ZZ9-33-S, ZZ9-33-D"),
        "{}",
        msgs[0]
    );
}

#[test]
fn a_keyed_file_with_one_row_needs_no_name() {
    let one = format!(
        "{HEADER}\
ZZ9-33-S,SOT-23-5,3.3V +/- 2%,0.3V,0.5A,0.10,1000,2026-10-31,https://example.invalid/zz9-33-s.pdf\n"
    );
    let c = catalogs(&[("zz9", &one)]).unwrap();
    let s = solved("", &c);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks.len(), 1);
    assert_eq!(s.picks[0].candidate, "ZZ9-33-S");
    assert_eq!(s.picks[0].mpn.as_deref(), Some("ZZ9-33-S"));
    match s.model.domain_at("u.mpn") {
        Some(netsmith_check::Domain::Text(t)) => assert_eq!(t, "ZZ9-33-S"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_narrowing_that_leaves_one_row_of_a_keyed_file_picks_it() {
    let c = catalogs(&[("zz9", &file())]).unwrap();
    let s = solved(r#".package &= "DFN-6""#, &c);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks.len(), 1);
    assert_eq!(s.picks[0].candidate, "ZZ9-33-D");
    // A narrowing that leaves both rows still asks for a name, listing both.
    let s = solved(r#".vout.voltage &= 3.3V +/- 5%"#, &c);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("name the part with .mpn: ZZ9-33-S, ZZ9-33-D"),
        "{}",
        msgs[0]
    );
}

#[test]
fn a_keyed_file_with_one_row_that_does_not_fit_is_not_picked() {
    let one = format!(
        "{HEADER}\
ZZ9-33-S,SOT-23-5,3.3V +/- 2%,0.3V,0.5A,0.10,1000,2026-10-31,https://example.invalid/zz9-33-s.pdf\n"
    );
    let c = catalogs(&[("zz9", &one)]).unwrap();
    let s = solved(r#".package &= "DFN-6""#, &c);
    assert!(s.picks.is_empty());
    assert!(!messages(&s).is_empty());
}

#[test]
fn the_order_of_the_rows_is_the_order_they_are_tried_in() {
    // Not keyed, the first row that fits is taken. Narrow nothing but the package.
    let swapped = file()
        .replace("ZZ9-33-S", "ZZ9-A")
        .replace("ZZ9-33-D", "ZZ9-B");
    let c = catalogs(&[("zz9", &swapped)]).unwrap();
    let s = solved(r#".mpn &= "ZZ9-B""#, &c);
    assert_eq!(s.picks[0].candidate, "ZZ9-B");
}

#[test]
fn a_row_is_offered_whatever_its_stock_cell_says() {
    // Stock is a fact about a day, not about a part: the solver does not read it (ADR 0047),
    // so a sold-out row is picked and only the step that orders the part passes it over.
    let sold_out = file().replacen(",1000,", ",0,", 1);
    let c = catalogs(&[("zz9", &sold_out)]).unwrap();
    let s = solved(r#".mpn &= "ZZ9-33-S""#, &c);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZ9-33-S");
    let msgs = messages(&solved("", &c));
    assert!(
        msgs[0].contains("name the part with .mpn: ZZ9-33-S, ZZ9-33-D"),
        "{}",
        msgs[0]
    );
    // An empty cell is picked as before.
    let unknown = file().replacen(",1000,", ",,", 1);
    let c = catalogs(&[("zz9", &unknown)]).unwrap();
    let s = solved(r#".mpn &= "ZZ9-33-S""#, &c);
    assert_eq!(s.picks[0].candidate, "ZZ9-33-S");
    // Its cells are still read: a mistake in a row is reported.
    let wrong = sold_out.replacen("0.3V", "0.3A", 1);
    let c = catalogs(&[("zz9", &wrong)]).unwrap();
    let s = solved("", &c);
    assert!(
        messages(&s)
            .iter()
            .any(|m| m.contains("catalogs/zz9.csv:2")),
        "{:?}",
        messages(&s)
    );
}

#[test]
fn a_price_breaks_column_is_allowed_and_ignored() {
    let text = file()
        .replacen("source\n", "source,price_breaks\n", 1)
        .replace("pdf\n", "pdf,1:0.10 100:0.08\n")
        .replace("rev B\"\n", "rev B\",1:0.20\n");
    let c = catalogs(&[("zz9", &text)]).unwrap();
    let s = solved(r#".mpn &= "ZZ9-33-D""#, &c);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZ9-33-D");
}

#[test]
fn a_checked_column_is_allowed_and_ignored() {
    let text = file()
        .replacen("source\n", "source,checked\n", 1)
        .replace("pdf\n", "pdf,2026-11-02\n")
        .replace("rev B\"\n", "rev B\",\n");
    let c = catalogs(&[("zz9", &text)]).unwrap();
    let s = solved(r#".mpn &= "ZZ9-33-D""#, &c);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZ9-33-D");
}

#[test]
fn a_file_named_like_a_catalog_adds_its_rows_after_the_ones_already_there() {
    let mut c = Catalogs::jlcpcb();
    let before = resistors(&c);
    let row = "mpn,resistance,max_power,package,price,stock,date,source\n\
               ZZR-1,4.99kohm +/- 1%,62.5mW,0402,0.01,10,2026-10-31,https://example.invalid/zzr\n";
    c.add_file("resistor", "catalogs/resistor.csv", row)
        .unwrap();
    let after = resistors(&c);
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(after[..before.len()], before[..]);
    assert_eq!(after.last().unwrap(), "ZZR-1");
}

fn resistors(c: &Catalogs) -> Vec<String> {
    use netsmith_quantity::Dimension;
    let columns = [
        q("resistance", Dimension::RESISTANCE),
        q("max_power", Dimension::POWER),
        t("package"),
        q("temperature_coefficient", Dimension::NONE),
    ];
    c.resolve("resistor", &columns, &[], &[])
        .unwrap()
        .unwrap()
        .candidates
        .iter()
        .map(|c| c.name.clone())
        .collect()
}

// ---- what is said when a file is wrong ---------------------------------------------

fn read_error(text: &str) -> String {
    catalogs(&[("zz9", text)]).unwrap_err()
}

#[test]
fn a_row_with_a_part_number_and_no_source_is_refused_with_its_line() {
    let e = read_error(&format!("{HEADER}ZZ9-1,SOT-23-5,3.3V,0.3V,0.5A,,,,\n"));
    assert!(
        e.starts_with("catalogs/zz9.csv:2:") && e.contains("mpn ZZ9-1") && e.contains("no source"),
        "{e}"
    );
}

#[test]
fn a_row_without_a_part_number_needs_a_name_but_no_source() {
    assert!(catalogs(&[("zz9", "name,package\nmade up,A\n")]).is_ok());
    let e = read_error("package\nA\n");
    assert!(
        e.starts_with("catalogs/zz9.csv:2:") && e.contains("no mpn or name"),
        "{e}"
    );
}

#[test]
fn a_part_twice_is_refused_naming_both_lines_in_one_file_and_across_files() {
    let e = read_error(&format!(
        "{HEADER}{}",
        file().lines().nth(1).unwrap().to_string() + "\n" + file().lines().nth(1).unwrap() + "\n"
    ));
    assert!(
        e.starts_with("catalogs/zz9.csv:3:")
            && e.contains("mpn `ZZ9-33-S`")
            && e.contains("catalogs/zz9.csv:2"),
        "{e}"
    );
    // The project's file repeats a part of the standard one.
    let mut c = Catalogs::jlcpcb();
    let dup = "mpn,source\n0402WGF3300TCE,https://example.invalid/again\n";
    let e = c
        .add_file("resistor", "catalogs/resistor.csv", dup)
        .unwrap_err();
    assert!(
        e.starts_with("catalogs/resistor.csv:2:")
            && e.contains("mpn `0402WGF3300TCE`")
            && e.contains("data/jlcpcb/catalog/resistor.csv:"),
        "{e}"
    );
}

const MAKERS: &str = "manufacturer,mpn,package,vout.voltage,dropout,max_current,source\n";

/// A row of the `zz9` catalog with a `manufacturer` cell.
fn made_by(maker: &str, mpn: &str, package: &str) -> String {
    format!("{maker},{mpn},{package},3.3V +/- 2%,0.3V,0.5A,https://example.invalid/{mpn}.pdf\n")
}

#[test]
fn two_rows_of_one_mpn_from_two_manufacturers_are_two_parts() {
    // A part is its manufacturer and its part number (ADR 0143): two makers may use one number.
    let text = format!(
        "{MAKERS}{}{}",
        made_by("Acme Analog", "ZZ9-33", "SOT-23-5"),
        made_by("Zenith Semi", "ZZ9-33", "DFN-6")
    );
    let c = catalogs(&[("zz9", &text)]).unwrap();
    let s = solved(r#".mpn &= "ZZ9-33"
    .package &= "DFN-6""#, &c);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].mpn.as_deref(), Some("ZZ9-33"));
    assert_eq!(s.picks[0].manufacturer.as_deref(), Some("Zenith Semi"));
    // Across files too: a project's row of the same number from another maker is another part.
    let other = format!("{MAKERS}{}", made_by("Zenith Semi", "ZZ9-33", "DFN-6"));
    let first = format!("{MAKERS}{}", made_by("Acme Analog", "ZZ9-33", "SOT-23-5"));
    let mut c = catalogs(&[("zz9", &first)]).unwrap();
    c.add_file("zz9", "catalogs/more/zz9.csv", &other).unwrap();
}

#[test]
fn the_same_manufacturer_and_mpn_twice_is_refused_naming_both_places() {
    let row = made_by("Acme Analog", "ZZ9-33", "SOT-23-5");
    let e = read_error(&format!("{MAKERS}{row}{row}"));
    assert_eq!(
        e,
        "catalogs/zz9.csv:3: the manufacturer `Acme Analog` and the mpn `ZZ9-33` is already a row at catalogs/zz9.csv:2; a part is in a catalog once"
    );
    // Across two files, the second one's line is the one refused.
    let mut c = catalogs(&[("zz9", &format!("{MAKERS}{row}"))]).unwrap();
    let e = c
        .add_file("zz9", "catalogs/more/zz9.csv", &format!("{MAKERS}{row}"))
        .unwrap_err();
    assert!(
        e.starts_with("catalogs/more/zz9.csv:2: the manufacturer `Acme Analog` and the mpn `ZZ9-33`")
            && e.contains("already a row at catalogs/zz9.csv:2"),
        "{e}"
    );
}

#[test]
fn a_row_that_names_no_manufacturer_is_its_mpn_whoever_makes_it() {
    // An empty `manufacturer` is any maker, so it is the same part as a row of the number that
    // names one, in either order; the message says which row names none.
    for (first, second, blank) in [
        (made_by("", "ZZ9-33", "SOT-23-5"), made_by("Acme Analog", "ZZ9-33", "DFN-6"), 2),
        (made_by("Acme Analog", "ZZ9-33", "SOT-23-5"), made_by("", "ZZ9-33", "DFN-6"), 3),
    ] {
        let e = read_error(&format!("{MAKERS}{first}{second}"));
        assert_eq!(
            e,
            format!(
                "catalogs/zz9.csv:3: the mpn `ZZ9-33` (of `Acme Analog`; the row at catalogs/zz9.csv:{blank} names no manufacturer, which is that mpn whoever makes it: give it its manufacturer to make the two rows two parts) is already a row at catalogs/zz9.csv:2; a part is in a catalog once"
            )
        );
    }
    // A catalog with no `manufacturer` column reads as before: its mpn alone is the identity.
    let e = read_error(&format!(
        "{HEADER}{}",
        file().lines().nth(1).unwrap().to_string() + "\n" + file().lines().nth(1).unwrap() + "\n"
    ));
    assert!(e.contains("the mpn `ZZ9-33-S` is already a row at catalogs/zz9.csv:2"), "{e}");
}

#[test]
fn a_manufacturer_with_no_mpn_is_refused_with_its_line() {
    let e = read_error("manufacturer,name,package,source\nAcme Analog,made up,A,https://example.invalid\n");
    assert!(
        e.starts_with("catalogs/zz9.csv:2: the row names the manufacturer `Acme Analog` and no `mpn`"),
        "{e}"
    );
}

#[test]
fn the_fixed_columns_are_checked_with_their_line() {
    for (bad, what) in [
        ("name,price\na,cheap\n", "price `cheap`"),
        ("name,price\na,-1\n", "price `-1`"),
        ("name,stock\na,1.5\n", "stock `1.5`"),
        ("name,date\na,31/10/2026\n", "date `31/10/2026`"),
        ("name,checked\na,yesterday\n", "checked `yesterday`"),
        (
            "name,assembly\na,solder\n",
            "assembly `solder` is not fab, hand, none or consigned",
        ),
    ] {
        let e = read_error(bad);
        assert!(
            e.starts_with("catalogs/zz9.csv:") && e.contains(what),
            "{e}"
        );
    }
}

#[test]
fn a_ragged_row_a_repeated_column_an_empty_file_and_an_open_quote_are_refused() {
    let e = read_error("name,package\na\n");
    assert!(
        e.starts_with("catalogs/zz9.csv:2:")
            && e.contains("1 fields but the header (line 1) has 2"),
        "{e}"
    );
    let e = read_error("name,name\na,b\n");
    assert!(
        e.starts_with("catalogs/zz9.csv:1:") && e.contains("`name` is there twice"),
        "{e}"
    );
    let e = read_error("");
    assert!(e.contains("is empty"), "{e}");
    let e = read_error("name\na\n\"never closed\n");
    assert!(
        e.starts_with("catalogs/zz9.csv:3") && e.contains("never closed"),
        "{e}"
    );
}

#[test]
fn a_column_the_part_does_not_list_is_named_with_the_header_line() {
    let text = file().replace("max_current", "colour");
    let c = catalogs(&[("zz9", &text)]).unwrap();
    let s = solved(r#".mpn &= "ZZ9-33-S""#, &c);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("catalogs/zz9.csv:1: `colour` is not a column of `catalog zz9`")
            && msgs[0].contains(".max_current"),
        "{}",
        msgs[0]
    );
    assert_eq!(s.picks.len(), 0);
}

#[test]
fn a_column_the_part_lists_and_the_file_lacks_is_named() {
    let text = "mpn,package,vout.voltage,dropout,source\nZZ9-1,SOT-23-5,3.3V +/- 2%,0.3V,https://example.invalid/a\n";
    let c = catalogs(&[("zz9", text)]).unwrap();
    let s = solved(r#".mpn &= "ZZ9-1""#, &c);
    let msgs = messages(&s);
    assert!(
        msgs.len() == 1 && msgs[0].contains("catalogs/zz9.csv:1: there is no `max_current` column"),
        "{msgs:#?}"
    );
}

#[test]
fn a_value_that_cannot_be_read_names_the_line_the_column_and_the_cell() {
    for (cell, why) in [
        ("0.3 volts", "volts"),
        ("0.3V +/-", "found"),
        ("fast", "is a name"),
        ("\"\"\"0.3V\"\"\"", "not a quoted string"),
        ("0.3A", "`0.3A` is in A, but this column holds V"),
        ("0.3", "`0.3` is a plain number, but this column holds V"),
    ] {
        let text = file().replace("0.3V", cell);
        let c = catalogs(&[("zz9", &text)]).unwrap();
        let s = solved(r#".mpn &= "ZZ9-33-S""#, &c);
        let msgs = messages(&s);
        assert_eq!(msgs.len(), 1, "{cell}: {msgs:#?}");
        assert!(
            msgs[0].contains("catalogs/zz9.csv:2: `dropout` of `catalog zz9`:")
                && msgs[0].contains(why),
            "{cell}: {}",
            msgs[0]
        );
    }
}

#[test]
fn text_is_written_as_it_is_without_quotes() {
    let text = file().replace("SOT-23-5", "\"\"\"SOT-23-5\"\"\"");
    let c = catalogs(&[("zz9", &text)]).unwrap();
    let s = solved(r#".mpn &= "ZZ9-33-S""#, &c);
    let msgs = messages(&s);
    assert!(
        msgs.len() == 1
            && msgs[0].contains("catalogs/zz9.csv:2: `package` of `catalog zz9`")
            && msgs[0].contains("write SOT-23-5"),
        "{msgs:#?}"
    );
}

#[test]
fn a_cell_with_a_value_a_part_cannot_take_is_caught_only_when_a_part_uses_the_catalog() {
    // The file alone is fine: what a cell means depends on the part that lists the column.
    assert!(catalogs(&[("zz9", &file().replace("0.3V", "0.3A"))]).is_ok());
}

#[test]
fn the_picked_row_keeps_its_columns_as_values() {
    use netsmith_quantity::Dimension;
    let c = catalogs(&[("zz9", &file())]).unwrap();
    let columns = [
        t("mpn"),
        t("package"),
        q("vout.voltage", Dimension::VOLTAGE),
        q("dropout", Dimension::VOLTAGE),
        q("max_current", Dimension::CURRENT),
    ];
    let cat = c.resolve("zz9", &columns, &[], &[]).unwrap().unwrap();
    assert_eq!(
        cat.candidates[0].column("mpn"),
        Some(&Value::Text("ZZ9-33-S".into()))
    );
    assert!(
        matches!(cat.candidates[1].column("max_current"), Some(Value::Quantity(q)) if q.mid() == 1.0)
    );
}

// ---- the derate form ------------------------------------------------------------

fn derate_error(cell: &str) -> String {
    use netsmith_quantity::Dimension;
    let text = format!(
        "mpn,capacitance,effective_capacitance,source\nZZC-1,10uF +/- 20%,\"{cell}\",https://example.invalid/c\n"
    );
    let c = catalogs(&[("zzcap", &text)]).unwrap();
    let columns = [
        q("capacitance", Dimension::CAPACITANCE),
        q("effective_capacitance", Dimension::CAPACITANCE),
    ];
    c.resolve("zzcap", &columns, &[], &[]).unwrap().unwrap_err()
}

#[test]
fn a_derate_cell_is_checked_for_its_form_and_its_curve() {
    for (cell, why) in [
        ("derate capacitance", "write `derate <column> by <field>"),
        ("derate capacitance of bias: 0:1", "write `derate"),
        ("derate nothing by bias: 0:1", "`nothing` is not a column"),
        ("derate capacitance by bias: 1:1 5:0.5", "must start at 0:1"),
        ("derate capacitance by bias: 0:1 5:1.5", "out of range"),
        (
            "derate capacitance by bias: 0:1 5:0.5 4:0.4",
            "out of order",
        ),
        ("derate capacitance by bias: 0:1 x", "not volts:fraction"),
    ] {
        let e = derate_error(cell);
        assert!(
            e.starts_with("catalogs/zzcap.csv:2: `effective_capacitance` of `catalog zzcap`:")
                && e.contains(why),
            "{cell}: {e}"
        );
    }
}

#[test]
fn a_derate_cell_with_no_points_is_a_part_with_no_published_curve() {
    use netsmith_quantity::Dimension;
    let text = "mpn,capacitance,effective_capacitance,source\nZZC-1,10uF +/- 20%,derate capacitance by bias:,https://example.invalid/c\n";
    let c = catalogs(&[("zzcap", text)]).unwrap();
    let columns = [
        q("capacitance", Dimension::CAPACITANCE),
        q("effective_capacitance", Dimension::CAPACITANCE),
    ];
    let cat = c.resolve("zzcap", &columns, &[], &[]).unwrap().unwrap();
    match cat.candidates[0].column("effective_capacitance") {
        Some(Value::Derated(d)) => assert!(d.curve.is_empty() && d.against == "bias"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_part_number_gives_its_record_from_any_file() {
    // The standard files ship no price, stock or date: the record says so and keeps the rest.
    let mut catalogs = Catalogs::jlcpcb();
    let part = catalogs
        .supplier("C1525")
        .expect("C1525 is in the standard files");
    assert_eq!(part.mpn, "CL05B104KO5NNNC");
    assert_eq!(part.library, Some(netsmith_solve::Library::Basic));
    assert_eq!(
        (part.stock, part.unit_price_usd, part.date),
        (None, None, None)
    );
    assert!(
        part.at.starts_with("data/jlcpcb/catalog/capacitor.csv:"),
        "{}",
        part.at
    );
    assert_eq!(catalogs.supplier("C0"), None);

    crate::offered::add(
        &mut catalogs,
        "resistor",
        "catalogs/own.csv",
        "lcsc,mpn,resistance,max_power,package,price,source\nC9999998,ZZ-1,317ohm +/- 1%,62.5mW,0402,0.5,https://example.invalid\n",
    )
    .unwrap();
    let own = catalogs.supplier("C9999998").unwrap();
    assert_eq!(
        (own.unit_price_usd, own.stock, own.library),
        (Some(0.5), None, None)
    );
    assert_eq!(own.at, "catalogs/own.csv:2");
}

#[test]
fn a_row_may_state_who_assembles_its_part_and_the_pick_carries_it() {
    let text = file()
        .replacen("mpn,", "assembly,mpn,", 1)
        .replace("ZZ9-33-S,", "hand,ZZ9-33-S,")
        .replace("ZZ9-33-D,", ",ZZ9-33-D,");
    let c = catalogs(&[("zz9", &text)]).unwrap();
    let hand = solved(r#".mpn &= "ZZ9-33-S""#, &c);
    assert!(hand.diagnostics.is_empty(), "{:?}", messages(&hand));
    assert_eq!(hand.picks[0].assembly.as_deref(), Some("hand"));
    // An empty cell is a row with nothing to say.
    let silent = solved(r#".mpn &= "ZZ9-33-D""#, &c);
    assert_eq!(silent.picks[0].assembly, None);
}

/// The invented regulators with a `height` cell: 1.45 mm for the small one, and nothing for the
/// other, which is a row that gives none.
fn file_with_heights() -> String {
    file()
        .replacen("mpn,", "height,mpn,", 1)
        .replace("ZZ9-33-S,", "1.45mm,ZZ9-33-S,")
        .replace("ZZ9-33-D,", ",ZZ9-33-D,")
}

#[test]
fn a_row_may_state_its_height_and_the_pick_carries_it_in_mm() {
    let c = catalogs(&[("zz9", &file_with_heights())]).unwrap();
    let known = solved(r#".mpn &= "ZZ9-33-S""#, &c);
    assert!(known.diagnostics.is_empty(), "{:?}", messages(&known));
    assert_eq!(known.picks[0].height, Some(1.45));
    // An empty cell is an unknown height, which is not zero.
    let unknown = solved(r#".mpn &= "ZZ9-33-D""#, &c);
    assert!(unknown.diagnostics.is_empty(), "{:?}", messages(&unknown));
    assert_eq!(unknown.picks[0].height, None);
    // A file with no `height` column has none either, and is read as before.
    let plain = catalogs(&[("zz9", &file())]).unwrap();
    assert_eq!(
        solved(r#".mpn &= "ZZ9-33-S""#, &plain).picks[0].height,
        None
    );
}

#[test]
fn a_height_is_not_a_column_of_the_part_so_it_never_picks_the_row() {
    // The design states a height of 5 mm, the row says 1.45 mm: the row is still the one
    // that is named, and the pick carries the row's number for the build to set aside.
    let c = catalogs(&[("zz9", &file_with_heights())]).unwrap();
    let s = solved(
        r#".mpn &= "ZZ9-33-S"
        .height &= 5mm"#,
        &c,
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZ9-33-S");
    assert_eq!(s.picks[0].height, Some(1.45));
}

#[test]
fn a_height_that_is_not_one_length_is_refused_with_its_file_and_line() {
    for (cell, why) in [
        ("1mm..2mm", "a range, and a height is one value"),
        ("1.5mm +/- 0.1mm", "a range, and a height is one value"),
        ("-1mm", "negative"),
        ("3V", "a height is a length"),
        ("tall", ""),
    ] {
        let text = file()
            .replacen("mpn,", "height,mpn,", 1)
            .replace("ZZ9-33-S,", &format!("{cell},ZZ9-33-S,"))
            .replace("ZZ9-33-D,", ",ZZ9-33-D,");
        let c = catalogs(&[("zz9", &text)]).unwrap();
        let s = solved(r#".mpn &= "ZZ9-33-S""#, &c);
        let said = messages(&s).join("\n");
        assert!(
            said.contains("catalogs/zz9.csv:2: `height`") && said.contains(why),
            "{cell}: {said}"
        );
    }
}

#[test]
fn a_part_that_lists_height_as_its_own_column_reads_it_as_that_column() {
    let board = BOARD.replace(
        "catalog zz9 by .mpn { .mpn, .vout.voltage, .dropout, .max_current, .package }",
        "catalog zz9 by .mpn { .mpn, .vout.voltage, .dropout, .max_current, .package, .height optional }",
    );
    let c = catalogs(&[("zz9", &file_with_heights())]).unwrap();
    let src = board.replace("PICK", r#".mpn &= "ZZ9-33-S""#);
    let checked = check("test.nsm", &src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let s = solve(&checked.boards[0], &c);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    // The part's own column sets the field; the fixed meaning is not applied twice.
    assert_eq!(s.picks[0].height, None);
    match s.model.domain_at("u.height") {
        Some(netsmith_check::Domain::Quantity(q)) => assert!((q.lo() - 1.45e-3).abs() < 1e-9),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_row_may_be_consigned_and_needs_no_offer_for_it() {
    let text = file()
        .replacen("mpn,", "assembly,mpn,", 1)
        .replace("ZZ9-33-S,", "consigned,ZZ9-33-S,")
        .replace("ZZ9-33-D,", ",ZZ9-33-D,");
    let c = catalogs(&[("zz9", &text)]).unwrap();
    let consigned = solved(r#".mpn &= "ZZ9-33-S""#, &c);
    assert!(
        consigned.diagnostics.is_empty(),
        "{:?}",
        messages(&consigned)
    );
    assert_eq!(consigned.picks[0].assembly.as_deref(), Some("consigned"));
}

/// A resistor row of a project's own file, with the cells the test varies: `lcsc`, `stock`.
fn resistor_rows(lcsc: &str, stock: &str) -> String {
    format!(
        "lcsc,mpn,resistance,max_power,package,library,stock,source\n\
         {lcsc},ZZR-1,45.9kohm +/- 0.1%,100mW,0603,Extended,{stock},https://example.invalid/zzr-1\n"
    )
}

fn project_resistors(lcsc: &str, stock: &str) -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    crate::offered::add(
        &mut c,
        "resistor",
        "catalogs/resistor.csv",
        &resistor_rows(lcsc, stock),
    )
    .unwrap();
    c
}

#[test]
fn a_row_that_states_a_stock_of_zero_is_sold_out_and_its_file_and_line_are_named() {
    // The row is the first line after the header of the project's file.
    let c = project_resistors("C900001", "0");
    assert_eq!(
        c.sold_out("C900001"),
        Some(SoldOut::Stated {
            at: "catalogs/resistor.csv:2".to_string()
        })
    );
}

#[test]
fn an_empty_or_a_positive_stock_states_nothing_to_note() {
    for stock in ["", "1", "25000"] {
        let c = project_resistors("C900001", stock);
        assert_eq!(c.sold_out("C900001"), None, "stock `{stock}`");
    }
    // A number no file lists, and a standard row (which ships no stock), are not sold out.
    let c = project_resistors("C900001", "0");
    assert_eq!(c.sold_out("C404040"), None);
    assert_eq!(c.sold_out("C25104"), None);
}

#[test]
fn a_part_that_considered_csv_records_as_out_of_stock_is_sold_out_unless_the_row_says_otherwise() {
    // C1723147 is in data/jlcpcb/considered.csv as out of stock when it was read.
    let c = project_resistors("C1723147", "");
    assert_eq!(c.sold_out("C1723147"), Some(SoldOut::Considered));
    // The row's own number is later knowledge: stock 0 says so itself, a positive stock wins.
    let zero = project_resistors("C1723147", "0");
    assert!(matches!(
        zero.sold_out("C1723147"),
        Some(SoldOut::Stated { .. })
    ));
    let some = project_resistors("C1723147", "300");
    assert_eq!(some.sold_out("C1723147"), None);
    // A part considered for another reason (the AMS1117) is not a part out of stock.
    let other = project_resistors("C6186", "");
    assert_eq!(other.sold_out("C6186"), None);
}

/// Two kinds of part that name the `zz9` catalog: the regulator of `BOARD` and a diode with
/// fewer columns.
const TWO_KINDS: &str = r#"
part Zy1 {
    .vf: Voltage
    .package: Package
    pins "SOD-123" {
        .a  passive  "A"  "1"
        .k  passive  "K"  "2"
    }
    catalog zz9 { .vf, .package }
}

board Two {
    .u: Zz9 {}
    .d1: Zy1 {}
    .d2: Zy1 {}
}
"#;

fn two_kinds() -> String {
    let regulator = BOARD.split("board B {").next().unwrap();
    format!("{regulator}{TWO_KINDS}")
}

#[test]
fn a_catalog_name_used_by_two_kinds_with_other_columns_is_refused_naming_both() {
    let c = catalogs(&[("zz9", &file())]).unwrap();
    let src = two_kinds();
    let checked = check("test.nsm", &src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let s = solve(&checked.boards[0], &c);
    let msgs = messages(&s);
    // Told once for the pair of kinds, not once for each diode.
    let shared: Vec<&String> = msgs.iter().filter(|m| m.contains("also uses")).collect();
    assert_eq!(shared.len(), 1, "{msgs:#?}");
    let m = shared[0];
    // The parts are taken in path order, so the diodes (`d1`) are the first use and the
    // regulator (`u`) is the part that is told.
    assert!(m.contains("`u` is a `Zz9`"), "{m}");
    assert!(m.contains("`Zy1` also uses the `zz9` catalog"), "{m}");
    assert!(m.contains("; only `Zy1` lists .vf)"), "{m}");
    assert!(
        m.contains("only `Zz9` lists .dropout, .max_current, .mpn, .vout.voltage"),
        "{m}"
    );
    // The part that is refused is not picked.
    let picked: Vec<String> = s.picks.iter().map(|p| s.model.path_of(p.part)).collect();
    assert!(!picked.contains(&"u".to_string()), "{picked:?}");
}

#[test]
fn two_kinds_that_list_the_same_columns_share_a_catalog_name() {
    let regulator = BOARD.split("board B {").next().unwrap();
    let src = format!(
        "{regulator}{}",
        r#"
part Zz9b {
    .vin:  Rail
    .vout: Rail
    .dropout:     Voltage
    .max_current: Current
    pins "SOT-23-5" {
        .vin.pos       power       "IN"     "1"
        .vin.neg       power       "GND"    "2"
        .vout.pos      power       "OUT"    "5"
    }
    .package: Package
    catalog zz9 by .mpn { .mpn, .vout.voltage, .dropout, .max_current, .package }
}

board Same {
    .u: Zz9 {}
    .v: Zz9b {}
}
"#
    );
    let checked = check("test.nsm", &src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let s = solve(&checked.boards[0], &catalogs(&[("zz9", &file())]).unwrap());
    assert!(
        !messages(&s).iter().any(|m| m.contains("also uses")),
        "{:?}",
        messages(&s)
    );
}
