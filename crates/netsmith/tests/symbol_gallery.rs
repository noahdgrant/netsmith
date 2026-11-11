//! The symbol gallery (`netsmith::symbol_gallery`, the example `symbol_gallery`): its page is
//! well formed, has one section for each symbol netsmith draws and for each IC, and is written
//! with no KiCad installed. No test looks at a pixel: how it looks is for a person to review.

use std::collections::BTreeSet;
use std::path::Path;

use netsmith::symbol_gallery::{EXAMPLE_ICS, example_ics, render};
use netsmith_kicad::{SymbolLib, generate, passives, testlib};

fn examples() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Tags that have no end tag.
const VOID: [&str; 4] = ["meta", "link", "br", "hr"];

/// A balanced-tag check: every tag opens and closes in order, and a `<` that is no tag is an
/// error. Comments and the doctype are skipped; a script or a style is read as text.
fn assert_well_formed(page: &str) {
    assert!(page.starts_with("<!doctype html>"));
    let mut stack: Vec<String> = Vec::new();
    let mut rest = page;
    while let Some(start) = rest.find('<') {
        rest = &rest[start..];
        if let Some(body) = rest.strip_prefix("<!doctype") {
            rest = &body[body.find('>').expect("an unclosed doctype")..];
            continue;
        }
        let end = rest.find('>').expect("a tag that never closes");
        let tag = &rest[1..end];
        assert!(!tag.contains('<'), "a `<` inside a tag: {tag}");
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap();
        assert!(
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric()),
            "not a tag: <{tag}>"
        );
        rest = &rest[end + 1..];
        if closing {
            assert_eq!(
                stack.pop().as_deref(),
                Some(name),
                "</{name}> closes nothing"
            );
        } else if tag.ends_with('/') || VOID.contains(&name) {
            // Self-closing.
        } else {
            stack.push(name.to_string());
            if name == "style" {
                let at = rest.find("</style>").expect("a style never ends");
                assert!(!rest[..at].contains("</"), "a tag inside the style");
                rest = &rest[at..];
            }
        }
        // Text between tags has no bare `<`: the next one is the next tag.
    }
    assert!(stack.is_empty(), "never closed: {stack:?}");
}

/// What a page says it has a section for: `data-symbol` and `data-ic` values, in order.
fn attribute_values(page: &str, attribute: &str) -> Vec<String> {
    let key = format!(" {attribute}=\"");
    page.match_indices(&key)
        .map(|(at, _)| {
            let from = at + key.len();
            page[from..from + page[from..].find('"').unwrap()].to_string()
        })
        .collect()
}

#[test]
fn the_page_has_one_section_for_each_symbol_passives_draws_and_for_each_ic() {
    let ics = example_ics(&examples()).unwrap_or_else(|e| panic!("{e}"));
    let page = render(None, &ics);
    assert_well_formed(&page);
    // The list of the gallery is `passives::ids()`: a symbol added there and not drawn here
    // (or the other way) fails this.
    let sections = attribute_values(&page, "data-symbol");
    let ids: Vec<String> = passives::ids().map(str::to_string).collect();
    assert_eq!(sections, ids, "one section for each id, in its order");
    assert_eq!(
        sections.iter().collect::<BTreeSet<_>>().len(),
        sections.len(),
        "an id is listed twice"
    );
    // Each of the four ICs is there, with its box and its pins.
    let titles: Vec<String> = attribute_values(&page, "data-ic");
    let expected: Vec<String> = EXAMPLE_ICS.iter().map(|ic| ic.title.to_string()).collect();
    assert_eq!(titles, expected);
    for ic in &ics {
        assert!(ic.ours.pins().len() >= 3, "{}", ic.title);
    }
    // Every drawn side has an SVG, and the groups of the page lose no symbol: 33 sections are
    // 33 `netsmith` sides.
    assert_eq!(
        page.matches("data-side=\"netsmith\"").count(),
        ids.len() + ics.len()
    );
}

#[test]
fn with_no_kicad_every_stock_side_says_so_and_the_page_still_builds() {
    let ics = example_ics(&examples()).unwrap_or_else(|e| panic!("{e}"));
    let page = render(None, &ics);
    assert_eq!(
        page.matches("data-side=\"kicad-absent\"").count(),
        passives::ids().count() + ics.len()
    );
    assert_eq!(page.matches("data-side=\"kicad\"").count(), 0);
    assert_eq!(
        page.matches("KiCad library not found: no stock counterpart drawn")
            .count(),
        passives::ids().count() + ics.len()
    );
    // The same inputs give the same page.
    assert_eq!(page, render(None, &ics));
}

#[test]
fn a_library_that_has_the_symbol_is_drawn_beside_netsmiths_and_one_that_has_not_says_so() {
    // The sample library of the tests is a few symbols of our own, not KiCad's.
    let lib = SymbolLib::at(testlib::sample_symbols());
    let page = render(Some(&lib), &[]);
    assert_well_formed(&page);
    let drawn = page.matches("data-side=\"kicad\"").count();
    let absent = page.matches("data-side=\"kicad-absent\"").count();
    assert!(drawn > 0, "the sample library has `Device:R`");
    assert_eq!(drawn + absent, passives::ids().count());
    assert!(
        page.contains("KiCad&#39;s library has no symbol")
            || page.contains("KiCad's library has no symbol")
    );
}

#[test]
fn the_committed_page_has_a_section_for_every_symbol_so_it_is_written_again_when_one_is_added() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/symbol-gallery.html");
    let page = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    assert_well_formed(&page);
    let ids: Vec<String> = passives::ids().map(str::to_string).collect();
    assert_eq!(
        attribute_values(&page, "data-symbol"),
        ids,
        "docs/symbol-gallery.html is out of date: `cargo run -p netsmith --example symbol_gallery -- --out docs --file symbol-gallery.html`"
    );
    let titles: Vec<String> = EXAMPLE_ICS.iter().map(|ic| ic.title.to_string()).collect();
    assert_eq!(attribute_values(&page, "data-ic"), titles);
}

/// The text of a symbol's field.
fn field(symbol: &netsmith_kicad::Symbol, name: &str) -> String {
    symbol
        .definition()
        .find_all("property")
        .find(|p| p.args().first().and_then(|a| a.text()) == Some(name))
        .and_then(|p| p.args().get(1)?.text().map(str::to_string))
        .unwrap_or_default()
}

#[test]
fn no_pin_name_of_an_ic_box_is_over_another_and_a_value_is_not_the_internal_name() {
    let ics = example_ics(&examples()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(ics.len(), 4);
    let page = render(None, &ics);
    for ic in &ics {
        assert_eq!(
            generate::overlapping_names(&ic.ours),
            Vec::<(String, String)>::new(),
            "{}",
            ic.title
        );
        // The library's `Value` is the part's kind and the name (the key, with the package) is
        // longer; a sheet shows the part's own value, which the page draws.
        let name = ic
            .ours
            .lib_id()
            .strip_prefix("netsmith:")
            .unwrap()
            .to_string();
        let library_value = field(&ic.ours, "Value");
        assert!(
            name.starts_with(&library_value) && name != library_value,
            "{name}"
        );
        assert!(
            !ic.value.is_empty() && !ic.value.contains("_SOT"),
            "{}",
            ic.value
        );
        assert!(
            page.contains(&format!(">{}</text>", ic.value)),
            "the page does not show the value `{}` of {}",
            ic.value,
            ic.title
        );
        assert!(
            !page.contains(&format!(">{name}</text>")),
            "the page shows the internal name `{name}`"
        );
    }
}

#[test]
fn a_fet_s_fields_start_clear_of_its_body_and_it_draws_its_body_diode() {
    for id in ["Transistor_FET:Q_NMOS_GSD", "Transistor_FET:Q_PMOS_GSD"] {
        let fet = passives::symbol(id).unwrap();
        for name in ["Reference", "Value"] {
            assert_eq!(fet.field_justify(name), ["left"], "{id} {name}");
            let (x, _, _) = fet.field_offset(name).unwrap();
            // The circle ends at 1.651 + 2.794 = 4.445 mm.
            assert!(x > 4.445, "{id} {name} starts at {x}, over the body");
        }
        // The diode's rail between the drain's and the source's leads.
        assert!(
            fet.definition()
                .to_kicad_string()
                .contains("(xy 3.302 1.778)"),
            "{id} has no body diode"
        );
    }
}
