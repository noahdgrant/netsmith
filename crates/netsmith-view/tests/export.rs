//! The schematic as a document (`netsmith_view::export`, issue #1091): its sheets are the
//! drawing's module boxes, one sheet's SVG is the drawing framed on it, and a sheet the drawing
//! has not is an error that lists the ones it has.

use netsmith_view::export::{self, ExportError, Heading, SHEET_MARGIN};

const BY_MODULE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" class="schematic" viewBox="0 0 200 100" data-board="Big" data-bounds="0 0 200 100">
<rect class="sheet" x="0" y="0" width="200" height="100" fill="#fbfaf3"/>
<g class="sheets">
<g class="module-sheet" data-module="power" data-bounds="10 10 80 60"><rect class="module-frame" x="10" y="10" width="80" height="60"/><text class="module-title" font-size="3.6" x="12" y="16">power</text></g>
<g class="module-sheet" data-module="stage (1/2)" data-bounds="110 10 80 60"><rect class="module-frame" x="110" y="10" width="80" height="60"/><text class="module-title" font-size="3.6" x="112" y="16">stage (1/2)</text></g>
</g>
<text class="ref" font-size="1.27" x="30" y="40">U1</text>
<text class="ref" font-size="1.27" x="130" y="40">R7</text>
</svg>"##;

fn heading() -> Heading {
    Heading {
        project: Some("p".to_string()),
        release: "netsmith test".to_string(),
    }
}

#[test]
fn the_sheets_are_the_module_boxes_with_room_round_them() {
    let sheets = export::sheets(BY_MODULE).unwrap();
    let names: Vec<&str> = sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["power", "stage (1/2)"]);
    assert_eq!(sheets[0].region.min.x, 10.0 - SHEET_MARGIN);
    assert_eq!(sheets[0].region.width(), 80.0 + 2.0 * SHEET_MARGIN);
    // A board drawn as one sheet is that sheet, named for the board.
    let small = BY_MODULE.replace("module-sheet", "other");
    let one = export::sheets(&small).unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].name, "Big");
}

#[test]
fn one_sheet_is_the_drawing_framed_on_it_and_an_unknown_one_lists_the_sheets() {
    let svg = export::svg_of(BY_MODULE, Some("power")).unwrap();
    assert!(svg.contains(r#"viewBox="8 8 84 64""#), "{svg}");
    assert_eq!(export::svg_of(BY_MODULE, None).unwrap(), BY_MODULE);
    let pdf = export::pdf_sheet("t", BY_MODULE, "power", &heading()).unwrap();
    assert_eq!(String::from_utf8_lossy(&pdf).matches("/Type /Page ").count(), 1);
    let all = export::pdf("t", &[BY_MODULE], &heading()).unwrap();
    assert_eq!(String::from_utf8_lossy(&all).matches("/Type /Page ").count(), 2);
    assert_eq!(all, export::pdf("t", &[BY_MODULE], &heading()).unwrap());
    match export::png(BY_MODULE, Some("stage")) {
        Err(ExportError::NoSheet { asked, sheets }) => {
            assert_eq!(asked, "stage");
            assert_eq!(sheets, ["power", "stage (1/2)"]);
        }
        other => panic!("{other:?}"),
    }
    let png = export::png(BY_MODULE, Some("power")).unwrap();
    // 84 by 64 millimetres at 10 pixels a millimetre.
    assert_eq!(&png[16..24], &[0, 0, 3, 72, 0, 0, 2, 128]);
}
