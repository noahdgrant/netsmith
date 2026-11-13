//! The rasterizer's tests, with the golden pictures' hashes in `golden.txt`.
//!
//! `NETSMITH_RENDER3D_BLESS=1 cargo test -p netsmith-3d --lib raster3d::` rewrites
//! `golden.txt` from what the code draws now; read the pictures before committing it.
//! `NETSMITH_RENDER3D_OUT=<folder>` writes every golden picture there as a PNG to look at.

use super::*;
use crate::mesh::{Polygon, circle, rounded_rect};
use netsmith_testkit::TempDir;
use std::collections::BTreeSet;

fn plain(width: usize, height: usize) -> RenderOptions {
    RenderOptions {
        width,
        height,
        supersample: 1,
        outlines: false,
        ..RenderOptions::default()
    }
}

fn top_camera(half_height: f64) -> Camera {
    Camera {
        eye: [0.0, 0.0, 50.0],
        target: [0.0, 0.0, 0.0],
        up: [0.0, 1.0, 0.0],
        lens: Lens::Orthographic { half_height },
    }
}

fn is_bg(img: &Image, o: &RenderOptions, x: usize, y: usize) -> bool {
    img.pixel(x, y) == o.palette.background
}

/// The box of the pixels that are not the background: (x0, y0, x1, y1), inclusive.
fn content(img: &Image, o: &RenderOptions) -> Option<(usize, usize, usize, usize)> {
    let mut b: Option<(usize, usize, usize, usize)> = None;
    for y in 0..img.height {
        for x in 0..img.width {
            if !is_bg(img, o, x, y) {
                b = Some(match b {
                    None => (x, y, x, y),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                });
            }
        }
    }
    b
}

/// A small board: an outline with rounded corners, a slot, drilled holes with plated
/// barrels, three bodies (one of unknown height, drawn flat), a pin, a height limit, a mark.
fn board() -> Vec<Mesh> {
    let outline = Polygon::new(rounded_rect([0.0, 0.0], [40.0, 25.0], 2.0, 6))
        .with_hole(vec![[28.0, 8.0], [36.0, 8.0], [36.0, 11.0], [28.0, 11.0]])
        .with_hole(circle([3.5, 3.5], 1.6, 24))
        .with_hole(circle([36.5, 21.5], 1.6, 24))
        .with_hole(circle([20.0, 4.0], 0.5, 12));
    let mut out = vec![
        Mesh::prism("board", Material::Substrate, &outline, -1.6, -0.02).unwrap(),
        Mesh::flat("mask.top", Material::MaskTop, &outline, 0.0, true).unwrap(),
        Mesh::flat("mask.bottom", Material::MaskBottom, &outline, -1.62, false).unwrap(),
        Mesh::cylinder_wall("GND", Material::Copper, [20.0, 4.0], 0.5, -1.62, 0.0, 12).unwrap(),
        Mesh::axis_box("U1", Material::Body, [8.0, 8.0, 0.0], [18.0, 18.0, 1.2]),
        Mesh::axis_box("C1", Material::Body, [22.0, 15.0, 0.0], [25.0, 17.0, 2.5]),
        Mesh::flat(
            "J1",
            Material::BodyUnknown,
            &Polygon::new(rounded_rect([28.0, 14.0], [34.0, 20.0], 0.0, 0)),
            0.01,
            true,
        )
        .unwrap(),
        Mesh::cylinder("C1.1", Material::Metal, [23.5, 14.5], 0.3, 0.0, 0.6, 16).unwrap(),
        Mesh::flat(
            "mark.U1",
            Material::Mark,
            &Polygon::new(rounded_rect([7.5, 18.5], [9.0, 19.0], 0.0, 0)),
            0.01,
            true,
        )
        .unwrap(),
    ];
    out.push(Mesh::axis_box(
        "limit.under-lid",
        Material::Limit,
        [6.0, 6.0, 0.0],
        [26.0, 20.0, 2.0],
    ));
    out
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[test]
fn a_box_seen_from_the_top_is_a_filled_rectangle_at_the_right_pixels() {
    let o = plain(200, 200);
    // 10 pixels a mm: the box from x -5 to 5 and y -2.5 to 2.5 covers columns 50..150 and rows
    // 75..125 (y up is up the picture).
    let b = Mesh::axis_box("R1", Material::Body, [-5.0, -2.5, 0.0], [5.0, 2.5, 1.0]);
    let img = render(&[b], &top_camera(10.0), &o);
    let lit = (0..200)
        .flat_map(|y| (0..200).map(move |x| (x, y)))
        .filter(|&(x, y)| !is_bg(&img, &o, x, y))
        .count();
    assert_eq!(lit, 100 * 50);
    assert_eq!(content(&img, &o), Some((50, 75, 149, 124)));
    // The top face is lit evenly, darker than its colour at full light would be but not black.
    let c = img.pixel(100, 100);
    assert_eq!(c, img.pixel(51, 76));
    assert!(c[0] > 40 && c[0] < 80, "{c:?}");

    // Moved up the y axis, it moves up the picture.
    let up = Mesh::axis_box("R1", Material::Body, [-5.0, 2.5, 0.0], [5.0, 7.5, 1.0]);
    let img = render(&[up], &top_camera(10.0), &o);
    assert_eq!(content(&img, &o), Some((50, 25, 149, 74)));
}

#[test]
fn the_z_buffer_hides_a_box_behind_another_in_either_order() {
    let o = plain(100, 100);
    let near = Mesh::axis_box("U1", Material::Body, [-4.0, -4.0, 0.0], [4.0, 4.0, 1.0]);
    let far = Mesh::axis_box(
        "pad",
        Material::Copper,
        [-3.0, -3.0, -2.0],
        [3.0, 3.0, -1.0],
    );
    let a = render(&[near.clone(), far.clone()], &top_camera(5.0), &o);
    let b = render(&[far.clone(), near.clone()], &top_camera(5.0), &o);
    assert_eq!(a, b);
    let alone = render(&[near], &top_camera(5.0), &o);
    assert_eq!(a, alone);
    // Seen from below, it is the copper that shows.
    let below = Camera {
        eye: [0.0, 0.0, -50.0],
        ..top_camera(5.0)
    };
    let c = render(std::slice::from_ref(&far), &below, &o);
    let d = render(
        &[
            Mesh::axis_box("U1", Material::Body, [-4.0, -4.0, 0.0], [4.0, 4.0, 1.0]),
            far,
        ],
        &below,
        &o,
    );
    assert_eq!(c.pixel(50, 50), d.pixel(50, 50));
}

#[test]
fn every_named_view_frames_the_board_with_a_margin() {
    let meshes = board();
    for lens in [
        Lens::Orthographic { half_height: 1.0 },
        Lens::Perspective { fov_y_deg: 30.0 },
    ] {
        for view in View::ALL {
            let o = RenderOptions {
                outlines: false,
                ..plain(160, 100)
            };
            let cam = fit_view(&meshes, view, &o, lens);
            let img = render(&meshes, &cam, &o);
            let (x0, y0, x1, y1) =
                content(&img, &o).unwrap_or_else(|| panic!("{} drew nothing", view.name()));
            let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
            assert!(
                x0 >= 2 && y0 >= 2 && x1 <= 157 && y1 <= 97,
                "{} {lens:?}: {x0} {y0} {x1} {y1}",
                view.name()
            );
            // The tighter side fills the picture up to the margin (5 % on each side).
            let fill = (w as f64 / 160.0).max(h as f64 / 100.0);
            assert!(fill > 0.8, "{} {lens:?} fills {fill}", view.name());
        }
    }
}

#[test]
fn the_bottom_view_is_the_board_turned_over_left_to_right() {
    let o = plain(100, 100);
    let left = Mesh::axis_box("L", Material::Body, [-4.0, -1.0, -1.0], [-2.0, 1.0, 1.0]);
    let bounds = Aabb {
        min: [-5.0, -5.0, -1.0],
        max: [5.0, 5.0, 1.0],
    };
    let top = Camera::fit(
        View::Top,
        bounds,
        1.0,
        0.0,
        Lens::Orthographic { half_height: 1.0 },
    );
    let bottom = Camera::fit(
        View::Bottom,
        bounds,
        1.0,
        0.0,
        Lens::Orthographic { half_height: 1.0 },
    );
    let (t, _, _, _) = content(&render(std::slice::from_ref(&left), &top, &o), &o).unwrap();
    let (b, _, _, _) = content(&render(&[left], &bottom, &o), &o).unwrap();
    assert!(t < 20 && b > 60, "top at {t}, bottom at {b}");
}

#[test]
fn a_limit_is_see_through_over_a_body_and_hidden_behind_one() {
    let o = plain(100, 100);
    let body = Mesh::axis_box("U1", Material::Body, [-4.0, -4.0, 0.0], [4.0, 4.0, 1.0]);
    let over = Mesh::axis_box("lid", Material::Limit, [-2.0, -2.0, 0.0], [2.0, 2.0, 3.0]);
    let under = Mesh::axis_box(
        "floor",
        Material::Limit,
        [-2.0, -2.0, -3.0],
        [2.0, 2.0, -0.5],
    );
    let cam = top_camera(5.0);
    let plain_body = render(std::slice::from_ref(&body), &cam, &o);
    let with_lid = render(&[over.clone(), body.clone()], &cam, &o);
    let (b, l) = (plain_body.pixel(50, 50), with_lid.pixel(50, 50));
    let limit = o.palette.colour(Material::Limit);
    // Blended: between the body and the limit's colour, channel by channel.
    for k in 0..3 {
        let (lo, hi) = (b[k].min(limit[k]), b[k].max(limit[k]));
        assert!(l[k] >= lo && l[k] <= hi, "{b:?} {l:?}");
    }
    assert_ne!(b, l);
    // Outside the lid the body is as before.
    assert_eq!(plain_body.pixel(15, 50), with_lid.pixel(15, 50));
    // Below the body, the limit does not show.
    assert_eq!(render(&[body, under], &cam, &o), plain_body);
}

#[test]
fn a_solid_case_hides_the_body_inside_it_and_a_lit_or_x_rayed_one_does_not() {
    let o = plain(100, 100);
    let solid = RenderOptions {
        case_solid: true,
        ..o.clone()
    };
    let body = Mesh::axis_box("U1", Material::Body, [-2.0, -2.0, 0.0], [2.0, 2.0, 1.0]);
    let case = Mesh::axis_box(
        "case:lid",
        Material::Case,
        [-4.0, -4.0, -0.5],
        [4.0, 4.0, 3.0],
    );
    let both = [case.clone(), body.clone()];
    let cam = top_camera(5.0);
    let at = |o: &RenderOptions, meshes: &[Mesh]| render(meshes, &cam, o).pixel(50, 50);
    // See-through (the default): the body shows through, so the picture is not the case alone.
    let see_through = at(&o, &both);
    assert_ne!(see_through, at(&o, std::slice::from_ref(&case)));
    assert_ne!(see_through, at(&o, std::slice::from_ref(&body)));
    // Solid: the case is opaque, so the body is not in the picture at all.
    let hides = at(&solid, &both);
    assert_eq!(hides, at(&solid, std::slice::from_ref(&case)));
    assert_ne!(hides, see_through);
    assert_eq!(hides[3], 255);
    // Highlighted, a case stays see-through whatever the mode, in the highlight colour.
    let lit = |o: &RenderOptions| RenderOptions {
        highlight: vec!["case:lid".into()],
        ..o.clone()
    };
    assert_eq!(at(&lit(&solid), &both), at(&lit(&o), &both));
    assert_ne!(at(&lit(&solid), &both), hides);
    // X-ray sees through it too, so the body is back.
    let xray = RenderOptions {
        xray: true,
        ..solid.clone()
    };
    assert_ne!(at(&xray, &both), hides);
    assert_ne!(at(&xray, &both), at(&xray, std::slice::from_ref(&case)));
}

#[test]
fn x_ray_shows_what_is_hidden_and_highlight_lights_one_mesh() {
    let o = plain(100, 100);
    let near = Mesh::axis_box("U1", Material::Body, [-4.0, -4.0, 0.0], [4.0, 4.0, 1.0]);
    let far = Mesh::axis_box(
        "pad",
        Material::Copper,
        [-3.0, -3.0, -2.0],
        [3.0, 3.0, -1.0],
    );
    let cam = top_camera(5.0);
    let normal = render(&[near.clone(), far.clone()], &cam, &o);
    let xray = RenderOptions {
        xray: true,
        ..o.clone()
    };
    let without_far = render(std::slice::from_ref(&near), &cam, &xray);
    let with_far = render(&[near.clone(), far.clone()], &cam, &xray);
    assert_ne!(normal.pixel(50, 50), with_far.pixel(50, 50));
    assert_ne!(without_far.pixel(50, 50), with_far.pixel(50, 50));
    // Highlighted in x-ray, the hidden pad is drawn solid in the highlight colour, seen through
    // the see-through body.
    let lit = RenderOptions {
        highlight: vec!["pad".into()],
        ..xray
    };
    let img = render(&[near.clone(), far.clone()], &cam, &lit);
    let c = img.pixel(50, 50);
    assert!(c[0] > c[2] + 40, "{c:?}");
    // Highlighted without x-ray, the body is the highlight colour and the dimmed rest is nearer
    // the background.
    let lit = RenderOptions {
        highlight: vec!["U1".into()],
        ..o.clone()
    };
    let wide = Mesh::axis_box(
        "board",
        Material::MaskTop,
        [-5.0, -5.0, -0.5],
        [5.0, 5.0, -0.1],
    );
    let plain_img = render(&[near.clone(), wide.clone()], &cam, &o);
    let img = render(&[near, wide], &cam, &lit);
    let h = img.pixel(50, 50);
    assert!(h[0] > 150 && h[1] > 100 && h[2] < 60, "{h:?}");
    let (before, after) = (plain_img.pixel(2, 2), img.pixel(2, 2));
    let bg = o.palette.background;
    let dist = |a: Rgba| {
        (0..3)
            .map(|k| (i32::from(a[k]) - i32::from(bg[k])).abs())
            .sum::<i32>()
    };
    assert!(dist(after) < dist(before), "{before:?} {after:?}");
}

#[test]
fn a_section_plane_cuts_away_one_side() {
    let o = plain(100, 100);
    let body = Mesh::axis_box("U1", Material::Body, [-4.0, -4.0, 0.0], [4.0, 4.0, 1.0]);
    let cut = RenderOptions {
        section: Some(Section {
            axis: Axis::X,
            at: 0.0,
            keep_below: true,
        }),
        ..o.clone()
    };
    let img = render(&[body], &top_camera(5.0), &cut);
    // 10 pixels a mm: x from -4 to 0 is columns 10..50.
    assert_eq!(content(&img, &o).map(|b| (b.0, b.2)), Some((10, 49)));
}

#[test]
fn a_section_fills_the_cut_of_a_closed_mesh_with_a_cap_and_caps_can_be_left_off() {
    let o = plain(100, 100);
    let body = Mesh::axis_box("U1", Material::Body, [-4.0, -4.0, 0.0], [4.0, 4.0, 1.0]);
    let section = Some(Section {
        axis: Axis::X,
        at: 0.0,
        keep_below: false,
    });
    // Seen from -x (the removed side is x < 0), so the cut at x = 0 faces the camera.
    let cam = Camera {
        eye: [-50.0, 0.0, 0.5],
        target: [0.0, 0.0, 0.5],
        up: [0.0, 0.0, 1.0],
        lens: Lens::Orthographic { half_height: 5.0 },
    };
    let capped = RenderOptions {
        section,
        ..o.clone()
    };
    let open = RenderOptions {
        caps: false,
        ..capped.clone()
    };
    let with = render(std::slice::from_ref(&body), &cam, &capped);
    let without = render(std::slice::from_ref(&body), &cam, &open);
    // 10 pixels a mm: the cut is 8 mm wide and 1 mm high, centred.
    let body_colour = o.palette.colour(Material::Body);
    let lit = with.pixel(50, 50);
    // The cap is the body's colour, lit: the same hue, not the background.
    assert!(!is_bg(&with, &o, 50, 50));
    assert!(
        lit[0] > 0 && (lit[0] as i32 - lit[2] as i32).abs() < 12,
        "{lit:?} {body_colour:?}"
    );
    assert!(with.pixel(50, 40) == o.palette.background || with.pixel(50, 40) != lit);
    // The cap lies in the plane, in front of the box's far wall: a mesh a little behind the cut
    // but in front of that wall is hidden by the cap and is not by an open cut.
    let inside = Mesh::axis_box("in", Material::Metal, [1.0, -1.0, 0.2], [2.0, 1.0, 0.8]);
    let hidden = render(&[body.clone(), inside.clone()], &cam, &capped);
    assert_eq!(hidden.pixel(50, 50), with.pixel(50, 50));
    let seen = render(&[body.clone(), inside], &cam, &open);
    assert_ne!(seen.pixel(50, 50), without.pixel(50, 50));
    let again = render(std::slice::from_ref(&body), &cam, &capped);
    assert_eq!(with, again);
}

#[test]
fn two_caps_in_one_plane_meet_with_the_later_over_the_earlier_and_no_line_between() {
    let o = RenderOptions {
        width: 100,
        height: 100,
        supersample: 1,
        outlines: true,
        section: Some(Section {
            axis: Axis::X,
            at: 0.0,
            keep_below: false,
        }),
        ..RenderOptions::default()
    };
    let cam = Camera {
        eye: [-50.0, 0.0, 0.0],
        target: [0.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
        lens: Lens::Orthographic { half_height: 5.0 },
    };
    // A slab, and a thin one on top of it that shares its cut plane's face: two layers of a
    // stack, side by side along z.
    let a = Mesh::axis_box(
        "board",
        Material::Substrate,
        [-3.0, -3.0, -2.0],
        [3.0, 3.0, 0.0],
    );
    let b = Mesh::axis_box(
        "board",
        Material::Prepreg,
        [-3.0, -3.0, 0.0],
        [3.0, 3.0, 2.0],
    );
    let img = render(&[a, b], &cam, &o);
    let (low, high) = (img.pixel(50, 60), img.pixel(50, 40));
    assert_ne!(low, high);
    // Every pixel down the column between the two bands is one of the two (no line).
    for y in 42..58 {
        let p = img.pixel(50, y);
        assert!(p == low || p == high, "row {y}: {p:?}");
    }
}

#[test]
fn an_open_mesh_is_drawn_from_both_sides_and_flat_shading_works() {
    let o = plain(60, 60);
    let sq = Polygon::new(rounded_rect([-2.0, -2.0], [2.0, 2.0], 0.0, 0));
    let up = Mesh::flat("x", Material::Mark, &sq, 0.0, true).unwrap();
    let from_top = render(std::slice::from_ref(&up), &top_camera(3.0), &o);
    let below = Camera {
        eye: [0.0, 0.0, -50.0],
        ..top_camera(3.0)
    };
    let from_below = render(std::slice::from_ref(&up), &below, &o);
    assert_eq!(from_top.pixel(30, 30), from_below.pixel(30, 30));
    assert!(!is_bg(&from_top, &o, 30, 30));
    let flat = RenderOptions {
        shading: Shading::Flat,
        ..o.clone()
    };
    let c = Mesh::cylinder("p", Material::Metal, [0.0, 0.0], 2.0, -1.0, 1.0, 24).unwrap();
    let side = Camera {
        eye: [0.0, -50.0, 0.0],
        target: [0.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
        lens: Lens::Orthographic { half_height: 3.0 },
    };
    let smooth_img = render(std::slice::from_ref(&c), &side, &o);
    let flat_img = render(&[c], &side, &flat);
    assert_ne!(smooth_img, flat_img);
    assert_eq!(content(&smooth_img, &o), content(&flat_img, &o));
}

#[test]
fn outlines_mark_the_edges_and_supersampling_softens_them() {
    let body = Mesh::axis_box("U1", Material::Body, [-4.0, -4.0, 0.0], [4.0, 4.0, 1.0]);
    let board = Mesh::axis_box(
        "board",
        Material::MaskTop,
        [-5.0, -5.0, -1.0],
        [5.0, 5.0, 0.0],
    );
    let o = RenderOptions {
        outlines: true,
        ..plain(100, 100)
    };
    let img = render(&[body.clone(), board.clone()], &top_camera(5.0), &o);
    let plain_img = render(
        &[body.clone(), board.clone()],
        &top_camera(5.0),
        &plain(100, 100),
    );
    // The edge between the body (columns 10..90) and the board is dark; the middle is not.
    let edge = img.pixel(9, 50);
    assert!(edge[0] < 40 && edge[1] < 40, "{edge:?}");
    assert_eq!(img.pixel(11, 50), plain_img.pixel(11, 50));
    assert_eq!(img.pixel(50, 50), plain_img.pixel(50, 50));
    let ss = RenderOptions {
        supersample: 3,
        ..o
    };
    let soft = render(&[body, board], &top_camera(5.0), &ss);
    let distinct: BTreeSet<Rgba> = (0..100).map(|x| soft.pixel(x, 50)).collect();
    assert!(distinct.len() > 3);
}

#[test]
fn the_same_scene_gives_the_same_bytes() {
    let meshes = board();
    let o = RenderOptions {
        width: 120,
        height: 90,
        ..RenderOptions::default()
    };
    let a = render_view(&meshes, View::Iso, &o).to_png();
    let b = render_view(&board(), View::Iso, &o).to_png();
    assert_eq!(a, b);
}

#[test]
fn a_render_written_as_a_png_reads_back() {
    let dir = TempDir::new("raster3d-png");
    let img = render_view(&board(), View::Top, &RenderOptions::default());
    let path = dir.path().join("top.png");
    std::fs::write(&path, img.to_png()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(
        &bytes[..8],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
    );
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 800);
    assert!(bytes.len() < 800 * 600 * 4 / 10, "{} bytes", bytes.len());
}

/// The golden scenes: a name and the picture.
fn goldens() -> Vec<(&'static str, Vec<u8>)> {
    let meshes = board();
    let small = RenderOptions {
        width: 160,
        height: 100,
        ..RenderOptions::default()
    };
    let dark = RenderOptions {
        palette: Theme::Dark.palette(),
        ..small.clone()
    };
    let persp = fit_view(
        &meshes,
        View::Iso,
        &small,
        Lens::Perspective { fov_y_deg: 35.0 },
    );
    let xray = RenderOptions {
        xray: true,
        highlight: vec!["GND".into()],
        ..small.clone()
    };
    let cut = RenderOptions {
        section: Some(Section {
            axis: Axis::Y,
            at: 12.5,
            keep_below: false,
        }),
        ..small.clone()
    };
    vec![
        ("top", render_view(&meshes, View::Top, &small).to_png()),
        ("iso", render_view(&meshes, View::Iso, &small).to_png()),
        ("iso-dark", render_view(&meshes, View::Iso, &dark).to_png()),
        ("iso-perspective", render(&meshes, &persp, &small).to_png()),
        (
            "iso-bottom-xray",
            render_view(&meshes, View::IsoBottom, &xray).to_png(),
        ),
        (
            "front-section",
            render_view(&meshes, View::Front, &cut).to_png(),
        ),
    ]
}

#[test]
fn the_golden_pictures_have_not_changed() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/raster3d/golden.txt");
    let pictures = goldens();
    if let Some(dir) = std::env::var_os("NETSMITH_RENDER3D_OUT") {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, png) in &pictures {
            std::fs::write(std::path::Path::new(&dir).join(format!("{name}.png")), png).unwrap();
        }
    }
    let now: Vec<String> = pictures
        .iter()
        .map(|(name, png)| format!("{name} {:016x}", fnv(png)))
        .collect();
    if std::env::var_os("NETSMITH_RENDER3D_BLESS").is_some() {
        let mut text = String::from(
            "# FNV-1a 64 of the PNG bytes of each golden picture of raster3d's tests.\n\
             # Rewrite with NETSMITH_RENDER3D_BLESS=1 cargo test -p netsmith-3d --lib raster3d::\n\
             # and look at the pictures first (NETSMITH_RENDER3D_OUT=<folder>).\n",
        );
        for line in &now {
            text.push_str(line);
            text.push('\n');
        }
        std::fs::write(&path, text).unwrap();
        return;
    }
    let text = std::fs::read_to_string(&path).unwrap();
    let want: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(
        now,
        want,
        "a golden picture changed: look at them (NETSMITH_RENDER3D_OUT=<folder>), then rewrite \
         {} with NETSMITH_RENDER3D_BLESS=1",
        path.display()
    );
}

#[test]
fn a_thousand_parts_draw_quickly() {
    // Not a benchmark (the example `raster3d_bench` is, in release): a guard against a
    // slow path, generous enough for a loaded machine.
    let mut meshes = board();
    for i in 0..1000 {
        let (x, y) = (f64::from(i % 40), f64::from(i / 40));
        meshes.push(Mesh::axis_box(
            format!("R{i}"),
            Material::Body,
            [x, y * 0.9, 0.0],
            [x + 0.6, y * 0.9 + 0.3, 0.35],
        ));
    }
    let o = RenderOptions {
        width: 400,
        height: 250,
        ..RenderOptions::default()
    };
    let t = std::time::Instant::now();
    let img = render_view(&meshes, View::Iso, &o);
    assert!(t.elapsed().as_secs_f64() < 20.0);
    assert_eq!(img.rgba.len(), 400 * 250 * 4);
}
