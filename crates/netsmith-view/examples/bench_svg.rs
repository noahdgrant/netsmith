//! Writes the schematic of the invented bench board (see `tests/common/bench.rs`) as SVG, for
//! looking at: `cargo run -p netsmith-view --example bench_svg -- <folder>`. With a second
//! argument `plain` the rails are not told to the drawing, as they were not before the power
//! symbols.

#[path = "../tests/common/bench.rs"]
mod bench;

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("a folder to write to");
    let plain = args.next().as_deref() == Some("plain");
    std::fs::create_dir_all(&out).unwrap();
    let b = bench::bench();
    let rails = if plain {
        netsmith_view::Rails::default()
    } else {
        b.rails.clone()
    };
    let svg =
        netsmith_view::render_svg_by_module_with(&b.design, &b.groups, &rails, &b.mapping, &b.lib)
            .unwrap();
    std::fs::write(format!("{out}/Bench.svg"), svg).unwrap();
}
