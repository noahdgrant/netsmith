//! `--highlight module:<path>` (issue #1129): a module id lights exactly the parts of that module
//! instance, each module named in its own colour, and a module the board lacks is refused with
//! the list of the ones it has. The board is the scene's invented one (`scene3d.rs`), its parts
//! `a` and `b` put in the module `reg` (`reg.a`, `reg.b`) and `under` in `io` (`io.under`).

use netsmith_3d::{RenderRequest, Scene, render_png, resolve_render_highlight};

use crate::scene3d::{board, scene};

fn with_modules() -> Scene {
    let mut b = board();
    for p in &mut b.parts {
        p.path = match p.path.as_str() {
            "a" | "b" => format!("reg.{}", p.path),
            "under" => "io.under".to_string(),
            other => other.to_string(),
        };
    }
    scene(&b)
}

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_module_lights_exactly_its_parts_and_each_module_is_its_own_group() {
    let s = with_modules();
    let lit = resolve_render_highlight(&s, &ids(&["module:reg"])).unwrap();
    assert!(lit.ids.is_empty(), "{lit:?}");
    assert_eq!(lit.modules, [ids(&["reg.a", "reg.b"])]);

    // Two modules: two groups, in the order named (the order of their colours); a module named
    // twice is one group; a part beside them is lit in the highlight colour.
    let two = resolve_render_highlight(
        &s,
        &ids(&["module:io", "module:reg", "module:io", "row"]),
    )
    .unwrap();
    assert_eq!(two.modules, [ids(&["io.under"]), ids(&["reg.a", "reg.b"])]);
    assert_eq!(two.ids, ids(&["row"]));
}

#[test]
fn a_module_the_board_lacks_is_refused_with_the_ones_it_has() {
    let s = with_modules();
    let e = resolve_render_highlight(&s, &ids(&["module:re", "module:reg.a"])).unwrap_err();
    assert!(e.contains("`module:re`") && e.contains("`module:reg.a`"), "{e}");
    assert!(e.contains("the modules: module:io, module:reg"), "{e}");
}

#[test]
fn a_module_is_drawn_in_its_own_colour_not_the_highlight_colour() {
    let s = with_modules();
    let draw = |highlight: &[&str]| {
        render_png(
            &s,
            &RenderRequest {
                size: (160, 100),
                highlight: ids(highlight),
                ..RenderRequest::default()
            },
        )
        .unwrap()
    };
    let plain = draw(&[]);
    let module = draw(&["module:reg"]);
    let parts = draw(&["reg.a", "reg.b"]);
    assert_ne!(module, plain);
    // The same parts, lit as a module, are in the module's colour, not the highlight's amber.
    assert_ne!(module, parts);
    // The second module named takes the second colour.
    assert_ne!(draw(&["module:io", "module:reg"]), draw(&["module:reg", "module:io"]));
    assert!(render_png(
        &s,
        &RenderRequest {
            highlight: ids(&["module:nope"]),
            ..RenderRequest::default()
        }
    )
    .is_err());
}
