//! The copper, solder mask and silkscreen of a scene as meshes (`netsmith_3d::mesh::pieces`)
//! and in the `.glb` file: the board of `scene3d`'s tests, with a track, a via and a pour
//! routed by hand (`board3d` places but does not route).

use netsmith_3d::mesh::{Material, Mesh, NO_NET, Pieces, SILK_LIFT, of_scene, pieces};
use netsmith_3d::scene3d::{Plan, PlanArea, Scene, SceneCopper};
use netsmith_pcb::Side;

use crate::common;
use crate::scene3d::{board_with, in_area, routed, scene};

fn copper(s: &Scene) -> &SceneCopper {
    s.copper.as_ref().unwrap_or_else(|e| panic!("{e}"))
}

/// The triangles of `m` whose normal points along +z (`up`) or -z, with their area and centroid.
fn caps(m: &Mesh, up: bool) -> Vec<(f64, Plan, f64)> {
    m.indices
        .chunks(3)
        .filter_map(|t| {
            let [a, b, c] = [0, 1, 2].map(|k| m.positions[t[k] as usize]);
            let nz = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if (nz > 0.0) != up || nz == 0.0 || a[2] != b[2] || a[2] != c[2] {
                return None;
            }
            let centre = Plan {
                x: (a[0] + b[0] + c[0]) / 3.0,
                y: (a[1] + b[1] + c[1]) / 3.0,
            };
            Some((0.5 * nz.abs(), centre, a[2]))
        })
        .collect()
}

fn cap_area(m: &Mesh, up: bool) -> f64 {
    caps(m, up).iter().map(|c| c.0).sum()
}

fn in_any(areas: &[PlanArea], p: Plan) -> bool {
    areas.iter().any(|a| in_area(a, p))
}

/// The share of the top of `m` whose triangles' centres `keep` accepts.
fn share(m: &Mesh, up: bool, keep: impl Fn(Plan) -> bool) -> f64 {
    let all = caps(m, up);
    let total: f64 = all.iter().map(|c| c.0).sum();
    all.iter().filter(|c| keep(c.1)).map(|c| c.0).sum::<f64>() / total
}

fn net_meshes<'a>(p: &'a Pieces, layer: &str, net: Option<&str>) -> Vec<&'a Mesh> {
    let l = p.copper.iter().find(|l| l.layer == layer).unwrap();
    l.nets
        .iter()
        .filter(|n| n.net.as_deref() == net)
        .flat_map(|n| n.copper.iter().chain(&n.finish))
        .collect()
}

#[test]
fn each_net_on_each_layer_is_one_piece_of_copper_named_by_its_net() {
    let s = scene(&routed());
    let p = pieces(&s);
    let c = copper(&s);
    let names: Vec<&str> = p.copper.iter().map(|l| l.layer.as_str()).collect();
    assert_eq!(names, ["F.Cu", "B.Cu"]);
    let top: Vec<Option<&str>> = p.copper[0].nets.iter().map(|n| n.net.as_deref()).collect();
    // The row's pads have no net; the via brings SIG to both layers.
    assert_eq!(top, c.layers[0].nets());
    let bottom: Vec<Option<&str>> = p.copper[1].nets.iter().map(|n| n.net.as_deref()).collect();
    assert!(bottom.contains(&Some("SIG")), "{bottom:?}");
    for l in &p.copper {
        for n in &l.nets {
            let id = n.net.as_deref().unwrap_or(NO_NET);
            for m in n.copper.iter().chain(&n.finish) {
                assert_eq!(m.id, id);
                assert!(m.closed);
            }
            if let Some(m) = &n.copper {
                assert_eq!(m.material, Material::Copper);
            }
            if let Some(m) = &n.finish {
                assert_eq!(m.material, Material::Finish);
            }
        }
    }
}

#[test]
fn the_copper_meshes_cover_the_copper_and_leave_the_drills_open() {
    let s = scene(&routed());
    let p = pieces(&s);
    let c = copper(&s);
    let top = &c.layers[0];
    for net in top.nets() {
        let meshes = net_meshes(&p, "F.Cu", net);
        let area: f64 = meshes.iter().map(|m| cap_area(m, true)).sum();
        // The pieces overlap in the data (a pad in its pour), so the mesh, which joins them, has
        // at most their sum, the via rings added.
        let rings: f64 = c
            .vias
            .iter()
            .filter(|v| v.net.as_deref() == net)
            .map(|v| std::f64::consts::PI * 0.25 * v.diameter * v.diameter)
            .sum();
        let data = top.area_of(net) + rings;
        assert!(
            area > 0.0 && area <= data + 1e-6,
            "{net:?}: {area} > {data}"
        );
        // The top and the bottom of the solid are the same shape, at the layer's own Z.
        let below: f64 = meshes.iter().map(|m| cap_area(m, false)).sum();
        assert!((area - below).abs() < 1e-6 * area.max(1.0));
        for m in &meshes {
            let b = m.bounds().unwrap();
            assert_eq!((b.min[2], b.max[2]), (top.z_bottom, top.z_top));
        }
    }
    // SIG has no pour: its pads, its track and its via ring do not overlap, so the mesh has the
    // copper's area less the via's drill.
    let sig: f64 = net_meshes(&p, "F.Cu", Some("SIG"))
        .iter()
        .map(|m| cap_area(m, true))
        .sum();
    let via = &c.vias[0];
    let want = top.area_of(Some("SIG"))
        + std::f64::consts::PI * 0.25 * (via.diameter * via.diameter - via.drill * via.drill);
    assert!((sig - want).abs() < 0.01, "{sig} and {want}");
    // Every point of a pad's copper is in its net's mesh, and no point of a drill is in any.
    let gnd = net_meshes(&p, "F.Cu", Some("GND"));
    let pad = top
        .pads
        .iter()
        .find(|p| p.net.as_deref() == Some("GND"))
        .unwrap();
    let inside = |meshes: &[&Mesh], q: Plan| {
        meshes.iter().any(|m| {
            m.indices.chunks(3).any(|t| {
                let [a, b, c] = [0, 1, 2].map(|k| m.positions[t[k] as usize]);
                let o = |u: [f64; 3], v: [f64; 3]| {
                    (v[0] - u[0]) * (q.y - u[1]) - (v[1] - u[1]) * (q.x - u[0])
                };
                let (d0, d1, d2) = (o(a, b), o(b, c), o(c, a));
                (d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0) || (d0 <= 0.0 && d1 <= 0.0 && d2 <= 0.0)
            })
        })
    };
    assert!(inside(&gnd, pad.at));
    let all: Vec<&Mesh> = p
        .copper
        .iter()
        .flat_map(|l| &l.nets)
        .flat_map(|n| n.copper.iter().chain(&n.finish))
        .collect();
    assert!(!inside(&all, via.at), "the via's drill is open");
    for h in &s.board.holes {
        assert!(!inside(&all, h.from), "{:?}", h.owner);
    }
}

#[test]
fn the_finish_is_where_the_mask_is_open_and_the_mask_is_open_over_it() {
    let s = scene(&routed());
    let p = pieces(&s);
    let c = copper(&s);
    let open_top = &c.mask[0].areas;
    let mut finished = 0;
    for n in &p.copper[0].nets {
        if let Some(f) = &n.finish {
            finished += 1;
            assert!(
                share(f, true, |q| in_any(open_top, q)) > 0.99,
                "{:?}",
                n.net
            );
        }
        if let Some(m) = &n.copper {
            assert!(
                share(m, true, |q| !in_any(open_top, q)) > 0.99,
                "{:?}",
                n.net
            );
        }
    }
    assert!(finished >= 3, "the pads of the top are bare");
    let mask = &p.masks[0];
    assert_eq!(mask.side, Side::Front);
    let bare = mask.bare.as_ref().unwrap();
    let over = mask.over_copper.as_ref().unwrap();
    assert_eq!(
        (bare.material, over.material),
        (Material::MaskTop, Material::MaskOverCopper)
    );
    for m in [bare, over] {
        assert!(share(m, true, |q| !in_any(open_top, q)) > 0.99);
    }
    // The mask over copper is over the copper: the GND pour fills most of the top.
    let pour: Vec<PlanArea> = c.layers[0].pours.iter().map(|p| p.area.clone()).collect();
    let pour = &pour;
    assert!(share(over, true, |q| in_any(pour, q)) > 0.95);
    assert!(share(bare, true, |q| !in_any(pour, q)) > 0.95);
    // The bottom face has its own mask.
    assert_eq!(p.masks[1].side, Side::Back);
    assert_eq!(
        p.masks[1].bare.as_ref().unwrap().material,
        Material::MaskBottom
    );
}

#[test]
fn the_silkscreen_is_on_the_mask_and_off_its_openings() {
    let s = scene(&routed());
    let p = pieces(&s);
    let c = copper(&s);
    let top = p.silk.iter().find(|s| s.side == Side::Front).unwrap();
    let ink = top
        .mesh
        .as_ref()
        .expect("the parts' designators are printed");
    assert_eq!(
        (ink.id.as_str(), ink.material),
        ("silk:top", Material::Mark)
    );
    assert!(!ink.closed);
    let b = ink.bounds().unwrap();
    assert_eq!((b.min[2], b.max[2]), (SILK_LIFT, SILK_LIFT));
    assert!(share(ink, true, |q| !in_any(&c.mask[0].areas, q)) > 0.999);
    // The ink is no more than the legend drew.
    let drawn: f64 = c.silk[0].areas.iter().map(PlanArea::size).sum();
    assert!(cap_area(ink, true) <= drawn + 1e-6);
    // The part on the back has its designator on the back, facing down.
    let bottom = p.silk.iter().find(|s| s.side == Side::Back).unwrap();
    let ink = bottom.mesh.as_ref().unwrap();
    assert!(cap_area(ink, false) > 0.0 && cap_area(ink, true) == 0.0);
    assert_eq!(
        ink.bounds().unwrap().max[2],
        -s.board.stack_thickness - SILK_LIFT
    );
}

#[test]
fn nothing_shares_a_plane_from_the_silk_down_to_the_substrate() {
    let s = scene(&routed());
    let p = pieces(&s);
    let c = copper(&s);
    let z_top = |m: &Mesh| m.bounds().unwrap().max[2];
    let z_bottom = |m: &Mesh| m.bounds().unwrap().min[2];
    let silk = p.silk[0].mesh.as_ref().unwrap();
    let mask = &p.masks[0];
    let (bare, over) = (
        mask.bare.as_ref().unwrap(),
        mask.over_copper.as_ref().unwrap(),
    );
    let copper_top = c.layers[0].z_top;
    let substrate = &p.board[0];
    assert!(z_bottom(silk) > z_top(bare) && z_top(bare) == z_top(over));
    assert!(z_top(over) > copper_top);
    // The mask over copper sits on the copper; over laminate it fills down to the substrate.
    assert_eq!(z_bottom(over), copper_top);
    assert_eq!(z_bottom(bare), c.layers[0].z_bottom);
    assert_eq!(z_top(substrate), c.layers[0].z_bottom);
    assert_eq!(z_bottom(substrate), c.layers[1].z_top);
    // The bottom face mirrors it.
    let back = &p.masks[1];
    assert_eq!(
        z_top(back.over_copper.as_ref().unwrap()),
        c.layers[1].z_bottom
    );
    assert_eq!(z_top(back.bare.as_ref().unwrap()), c.layers[1].z_top);
}

#[test]
fn a_four_layer_board_has_its_planes_inside_and_the_same_meshes_every_time() {
    let board = board_with(common::four_layer_process());
    let s = scene(&board);
    let p = pieces(&s);
    let names: Vec<&str> = p.copper.iter().map(|l| l.layer.as_str()).collect();
    assert_eq!(names, ["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"]);
    let substrate = p.board[0].bounds().unwrap();
    for (l, net) in p.copper[1..3].iter().zip(["GND", "3V3"]) {
        let plane = l
            .nets
            .iter()
            .find(|n| n.net.as_deref() == Some(net))
            .unwrap();
        // An inner layer has no finish: the mask is not on it.
        assert!(plane.finish.is_none());
        let b = plane.copper.as_ref().unwrap().bounds().unwrap();
        assert!(b.min[2] > substrate.min[2] && b.max[2] < substrate.max[2]);
    }
    assert_eq!(p, pieces(&scene(&board_with(common::four_layer_process()))));
    let meshes = of_scene(&s);
    let triangles: usize = meshes.iter().map(Mesh::triangle_count).sum();
    assert!(triangles < 200_000, "{triangles} triangles");
}

#[test]
fn the_glb_has_a_node_per_net_per_layer_and_the_silk_and_mask_of_each_face() {
    use serde_json::Value;
    let s = scene(&routed());
    let glb = netsmith_3d::gltf::glb(&s, "Scene");
    let doc = netsmith_3d::gltf::json_chunk(&glb).unwrap();
    let nodes = doc["nodes"].as_array().unwrap();
    let node = |name: &str| -> &Value {
        nodes
            .iter()
            .find(|n| n["name"] == name)
            .unwrap_or_else(|| panic!("no node `{name}`"))
    };
    let children = |name: &str| -> Vec<String> {
        node(name)["children"]
            .as_array()
            .map(|c| {
                c.iter()
                    .map(|i| {
                        nodes[i.as_u64().unwrap() as usize]["name"]
                            .as_str()
                            .unwrap()
                            .into()
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let root = children("board");
    assert_eq!(
        &root[root.len() - 2..],
        ["copper:F.Cu".to_string(), "copper:B.Cu".to_string()]
    );
    let group = &node("copper:F.Cu")["extras"]["netsmith"];
    assert_eq!(group["kind"], "copper_layer");
    let top = children("copper:F.Cu");
    assert_eq!(
        top,
        ["(no net)", "3V3", "GND", "SIG"].map(|n| format!("copper:F.Cu:{n}"))
    );
    let sig = &node("copper:F.Cu:SIG")["extras"]["netsmith"];
    assert_eq!(
        (&sig["kind"], &sig["layer"], &sig["net"]),
        (
            &Value::from("copper"),
            &Value::from("F.Cu"),
            &Value::from("SIG")
        )
    );
    assert!(node("copper:F.Cu:(no net)")["extras"]["netsmith"]["net"].is_null());
    // A pad's net has its copper and its finish, two primitives of one mesh.
    let mesh = &doc["meshes"][node("copper:F.Cu:GND")["mesh"].as_u64().unwrap() as usize];
    let materials: Vec<&str> = mesh["primitives"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            doc["materials"][p["material"].as_u64().unwrap() as usize]["name"]
                .as_str()
                .unwrap()
        })
        .collect();
    assert_eq!(materials, ["copper", "finish"]);
    let substrate = children("substrate");
    for (name, kind, face) in [
        ("mask:top", "mask", "top"),
        ("mask:bottom", "mask", "bottom"),
        ("silk:top", "silk", "top"),
        ("silk:bottom", "silk", "bottom"),
    ] {
        assert!(substrate.contains(&name.to_string()), "{substrate:?}");
        let x = &node(name)["extras"]["netsmith"];
        assert_eq!(
            (x["kind"].as_str(), x["face"].as_str()),
            (Some(kind), Some(face))
        );
        assert!(node(name)["mesh"].is_u64(), "{name} has a mesh");
    }
    // The silk comes after the holes: the children the contract had keep their places.
    assert_eq!(
        &substrate[substrate.len() - 2..],
        ["silk:top", "silk:bottom"]
    );
    assert_eq!(glb, netsmith_3d::gltf::glb(&scene(&routed()), "Scene"));
}

/// The routed board drawn from the top and the iso view, into `NETSMITH_RENDER3D_OUT` when it is
/// set, to look at: the pour shows as the lighter green, the pads as gold.
#[test]
fn the_routed_board_draws_the_same_bytes_each_time() {
    use netsmith_3d::raster3d::{RenderOptions, View, render_view};
    let meshes = of_scene(&scene(&routed()));
    let options = RenderOptions {
        width: 800,
        height: 560,
        ..RenderOptions::default()
    };
    for view in [View::Top, View::Iso, View::IsoBottom] {
        let png = render_view(&meshes, view, &options).to_png();
        if let Some(out) = std::env::var_os("NETSMITH_RENDER3D_OUT") {
            std::fs::create_dir_all(&out).unwrap();
            let file = format!("routed-{}.png", view.name());
            std::fs::write(std::path::Path::new(&out).join(file), &png).unwrap();
        }
        assert_eq!(png, render_view(&meshes, view, &options).to_png());
    }
}
