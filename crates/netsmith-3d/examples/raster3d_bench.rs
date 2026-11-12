//! How long netsmith's rasterizer takes on a board of a thousand parts: a 160 x 100 mm board
//! with rounded corners, a cutout and 400 drilled holes with plated barrels, 1000 bodies and a
//! height limit, drawn at 1600 x 1000 pixels with 2 x 2 samples from the iso view.
//!
//! `cargo run --release -p netsmith-3d --example raster3d_bench [-- <out.png>]`

use std::time::Instant;

use netsmith_3d::mesh::{Material, Mesh, Polygon, circle, rounded_rect};
use netsmith_3d::raster3d::{RenderOptions, View, render_view};

fn main() {
    let start = Instant::now();
    let mut outline = Polygon::new(rounded_rect([0.0, 0.0], [160.0, 100.0], 3.0, 8))
        .with_hole(rounded_rect([120.0, 40.0], [150.0, 60.0], 2.0, 6));
    let mut meshes = Vec::new();
    for i in 0..20 {
        for j in 0..20 {
            let c = [5.0 + 5.5 * f64::from(i), 5.0 + 4.5 * f64::from(j)];
            outline.holes.push(circle(c, 0.4, 16));
            meshes
                .push(Mesh::cylinder_wall("net", Material::Copper, c, 0.4, -1.6, 0.0, 16).unwrap());
        }
    }
    meshes.insert(
        0,
        Mesh::prism("board", Material::Substrate, &outline, -1.6, 0.0).unwrap(),
    );
    for k in 0..1000 {
        let (x, y) = (2.0 + 4.6 * f64::from(k % 25), 2.0 + 2.4 * f64::from(k / 25));
        let h = 0.3 + 0.1 * f64::from(k % 7);
        meshes.push(Mesh::axis_box(
            format!("R{k}"),
            Material::Body,
            [x + 1.5, y + 0.8, 0.0],
            [x + 3.1, y + 1.6, h],
        ));
    }
    meshes.push(Mesh::axis_box(
        "limit:lid",
        Material::Limit,
        [0.0, 0.0, 0.0],
        [60.0, 100.0, 1.0],
    ));
    let built = start.elapsed();
    let options = RenderOptions {
        width: 1600,
        height: 1000,
        ..RenderOptions::default()
    };
    let t = Instant::now();
    let image = render_view(&meshes, View::Iso, &options);
    let drawn = t.elapsed();
    let t = Instant::now();
    let png = image.to_png();
    let encoded = t.elapsed();
    let triangles: usize = meshes.iter().map(Mesh::triangle_count).sum();
    println!(
        "{} meshes, {triangles} triangles: meshes {:.0} ms, draw {:.0} ms, png {:.0} ms ({} bytes)",
        meshes.len(),
        built.as_secs_f64() * 1e3,
        drawn.as_secs_f64() * 1e3,
        encoded.as_secs_f64() * 1e3,
        png.len()
    );
    if let Some(path) = std::env::args().nth(1) {
        std::fs::write(&path, png).unwrap();
    }
}
