//! The pure parts of `netsmith render`'s drawing: what its flags parse to and the file it writes.

use netsmith_3d::{DEFAULT_RENDER_SIZE, DEFAULT_RENDER_VIEW, default_render_out, parse_render_section, parse_render_size, Axis, View, RenderCaseMode, RenderRequest};

#[test]
fn a_size_is_two_numbers_within_the_range() {
    assert_eq!(parse_render_size("1600x1000"), Ok((1600, 1000)));
    assert_eq!(parse_render_size(" 64 X 8192 "), Ok((64, 8192)));
    for bad in ["1600", "63x100", "100x9000", "axb", "8192x8192"] {
        let e = parse_render_size(bad).unwrap_err();
        assert!(e.contains("--size needs"), "{bad}: {e}");
    }
}

#[test]
fn a_section_is_an_axis_and_a_finite_position() {
    assert_eq!(parse_render_section("x=12.5"), Ok((Axis::X, 12.5)));
    assert_eq!(parse_render_section("Z = -0.8"), Ok((Axis::Z, -0.8)));
    for bad in ["x", "w=1", "y=", "y=inf", "y=nan"] {
        assert!(parse_render_section(bad).is_err(), "{bad}");
    }
}

#[test]
fn the_default_file_is_named_for_the_board_and_the_view() {
    assert_eq!(
        default_render_out("led", View::Iso).to_str(),
        Some("led-iso.png")
    );
    assert_eq!(
        default_render_out("Sys/member", View::Top).to_str(),
        Some("Sys-member-top.png")
    );
}

#[test]
fn the_default_request_is_what_the_flags_default_to() {
    let r = RenderRequest::default();
    assert_eq!(r.size, DEFAULT_RENDER_SIZE);
    assert_eq!(r.view, DEFAULT_RENDER_VIEW);
    assert_eq!(r.case, RenderCaseMode::SeeThrough);
    assert!(r.caps && !r.xray && !r.dark && !r.perspective);
    assert_eq!(RenderCaseMode::Hidden.name(), "hidden");
    assert_eq!(RenderCaseMode::SeeThrough.name(), "see-through");
    assert_eq!(RenderCaseMode::Solid.name(), "solid");
}
