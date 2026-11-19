//! A part's `.variants` table (issue 1311): an order code in quotes to a row, and the names of
//! its features after it.

use netsmith_lang::ast::*;
use netsmith_lang::{format, parse};

const MODEM: &str = "\
part Modem {
    .variants {
        \"MDM-100-G\" GNSS LTE
        \"MDM-100\" LTE
        \"MDM-100-B\"
    }
}
";

#[test]
fn a_row_is_a_code_and_its_features() {
    let file = parse(MODEM).unwrap();
    let Item::Variants(v) = &file.definitions[0].items[0] else {
        panic!("a variants table")
    };
    let rows: Vec<(&str, Vec<&str>)> = v
        .rows
        .iter()
        .map(|r| {
            (
                r.code.as_str(),
                r.features.iter().map(|f| f.name.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            ("MDM-100-G", vec!["GNSS", "LTE"]),
            ("MDM-100", vec!["LTE"]),
            ("MDM-100-B", vec![]),
        ]
    );
}

#[test]
fn it_prints_back_and_formats_one_row_to_a_line() {
    let printed = parse(MODEM).unwrap().to_source();
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
    assert_eq!(format(MODEM).as_deref(), Ok(MODEM));
    let joined = "part Modem {\n    .variants { \"A\" X; \"B\" }\n}\n";
    assert_eq!(
        format(joined).as_deref(),
        Ok("part Modem {\n    .variants {\n        \"A\" X\n        \"B\"\n    }\n}\n")
    );
}

#[test]
fn a_row_needs_a_code_in_quotes() {
    let e = parse("part P {\n    .variants {\n        GNSS\n    }\n}\n").unwrap_err();
    assert!(e.message.contains("an order code in quotes"), "{}", e.message);
    let e = parse("part P {\n    .variants {\n    }\n}\n").unwrap_err();
    assert!(e.message.contains("`.variants` has no rows"), "{}", e.message);
    let e = parse("part P {\n    .variants {\n        \"\" A\n    }\n}\n").unwrap_err();
    assert!(e.message.contains("cannot be empty"), "{}", e.message);
}

#[test]
fn variants_is_still_an_ordinary_name() {
    let file = parse("board B {\n    .variants: Length\n    .variants &= 1mm\n}\n").unwrap();
    assert_eq!(file.definitions[0].items.len(), 2);
}
