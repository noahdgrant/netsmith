//! The guide's `gen:LGA` row (chapter 26, issue 1423) makes a footprint that its own `table`
//! measures and agrees with, with no KiCad: the ring, the grid inside it, the pitch and the body
//! the generator draws on `F.Fab` are what the row's datasheet numbers say. A table that states
//! the ring alone, as a row that forgot the grid would, is refused naming both counts. The test is
//! here, beside the guide's runner, because a change to the guide runs this crate's tests.

use netsmith_kicad::{FootprintSpec, FootprintTable, Mapped};
use netsmith_pcb::{Footprint, Subject, generate_footprint_spec, validate_table};

/// The `footprint` and `table` of the guide's row that asks for `gen:LGA(...)`.
fn guide_row() -> (String, String) {
    let guide = netsmith_testkit::guide_text();
    let value = |key: &str| {
        guide
            .lines()
            .filter_map(|l| l.strip_prefix(&format!("{key} = \"")))
            .map(|l| l.trim_end_matches('"').to_string())
            .collect::<Vec<_>>()
    };
    let footprint = value("footprint")
        .into_iter()
        .find(|f| f.starts_with("gen:LGA("))
        .expect("the guide has a row with a gen:LGA footprint");
    let start = guide.find(&footprint).unwrap();
    let table = guide[start..]
        .lines()
        .nth(1)
        .and_then(|l| l.strip_prefix("table = \""))
        .map(|l| l.trim_end_matches('"').to_string())
        .expect("the gen:LGA row states a table");
    (footprint, table)
}

fn check(footprint: &Footprint, id: &str, table: &str) -> Vec<String> {
    let mapped = Mapped {
        lib_id: None,
        footprint: id.to_string(),
        origin: "kicad.toml line 5".into(),
        pin_names: Default::default(),
        table: Some(FootprintTable::parse(table).unwrap()),
    };
    let subject = Subject {
        refdes: "U1",
        kind: "Sensor",
        package: Some("LGA-20"),
    };
    validate_table(&subject, &mapped, footprint)
}

#[test]
fn the_guides_lga_agrees_with_the_table_of_its_row() {
    let (footprint, table) = guide_row();
    let spec = FootprintSpec::parse(&footprint).unwrap();
    let fp = generate_footprint_spec(&spec).unwrap_or_else(|e| panic!("{footprint}: {e}"));
    assert_eq!(check(&fp, &spec.id(), &table), Vec::<String>::new());
    // Sixteen pads in the ring and four in the grid.
    assert_eq!(fp.pad_numbers().len(), 20);
}

#[test]
fn a_table_that_leaves_out_the_grid_is_refused_with_both_counts() {
    let (footprint, _) = guide_row();
    let spec = FootprintSpec::parse(&footprint).unwrap();
    let fp = generate_footprint_spec(&spec).unwrap();
    let problems = check(&fp, &spec.id(), "pads=16, pitch=0.5mm, body=3x3mm");
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(
        problems[0].contains("has 20 pads") && problems[0].contains("says 16"),
        "{}",
        problems[0]
    );
    // A body of the next size is told apart.
    let problems = check(&fp, &spec.id(), "body=3.5x3.5mm");
    assert!(
        problems.len() == 1 && problems[0].contains("draws a body 3x3mm"),
        "{problems:#?}"
    );
}
