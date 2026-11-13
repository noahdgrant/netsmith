//! Synthetic boards for the tests: placed by hand, not by the placer.
#![allow(dead_code)] // each test file uses some of these

use netsmith_core::{Design, Net, Part, PinRef};
use netsmith_pcb::{Board, FootprintLibrary, PlaceOptions, Pt, Rect};
use netsmith_testkit::TempDir;

/// The least distance from any pad, track or via to the board edge, from the board model.
pub fn edge_gap(board: &Board) -> f64 {
    let pads = board.parts.iter().flat_map(|part| {
        part.footprint
            .pads
            .iter()
            .filter(|p| part.pad_layers(p) != (false, false))
            .map(|p| part.pad_bounds(p))
    });
    let tracks = board.tracks.iter().map(|t| {
        Rect::new(
            Pt::new(t.from.x.min(t.to.x), t.from.y.min(t.to.y)),
            Pt::new(t.from.x.max(t.to.x), t.from.y.max(t.to.y)),
        )
        .expanded(t.width / 2.0)
    });
    let vias = board
        .vias
        .iter()
        .map(|v| Rect::new(v.at, v.at).expanded(v.diameter / 2.0));
    pads.chain(tracks)
        .chain(vias)
        .map(|r| board.edge_distance(&r))
        .fold(f64::MAX, f64::min)
}

/// `TwoLayer` as the whole pipeline resolves it, with the board choosing 1.6 mm and
/// narrowing each rule as `narrowings` say (`.min_track &= 0.25mm..1m`).
pub fn process_with(narrowings: &[&str]) -> netsmith_core::Process {
    let body: String = narrowings
        .iter()
        .chain(&[".stackup.thickness &= 1.6mm"])
        .map(|n| format!("        {n}\n"))
        .collect();
    let source = format!(
        "import std::mfg::jlcpcb::TwoLayer\nimport std::parts::passive::Resistor\nboard T {{\n    .process: TwoLayer {{\n{body}    }}\n    .r: Resistor\n}}\n"
    );
    process_of(&source)
}

/// The process of the one board of `source`, as the check, the solver and the design stage
/// resolve it: what `netsmith::build_with` gives. These tests do not depend on the netsmith
/// crate, so that they are not among the dependents of every crate it links (issue #518).
fn process_of(source: &str) -> netsmith_core::Process {
    let checked = netsmith_check::check("t.nsm", source, &netsmith_check::Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = &checked.boards[0];
    let solution = netsmith_solve::solve(board, &netsmith_solve::Catalogs::jlcpcb());
    assert!(
        solution.diagnostics.is_empty(),
        "{:?}",
        solution.diagnostics
    );
    netsmith_design::design_board(board, &solution)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .design
        .process
        .expect("the board has a process")
}

/// The process the hard routing boards are made to: wide tracks and generous spaces, so a
/// small crowded board is hard to route. The library's own numbers are far finer.
pub fn process() -> netsmith_core::Process {
    process_with(&[
        ".min_track &= 0.25mm..1m",
        ".min_clearance &= 0.2mm..1m",
        ".min_drill &= 0.24mm..1m",
        ".min_via_diameter &= 0.6mm..1m",
        ".via_drill &= 0.24mm",
        ".via_diameter &= 0.6mm",
        ".min_hole_to_hole &= 0.5mm..1m",
        ".min_edge_clearance &= 0.3mm..1m",
    ])
}

pub fn fixtures() -> FootprintLibrary {
    FootprintLibrary::at(netsmith_testkit::sample_footprints())
}

/// The boards of a seeded series: eight 0402 resistors in two rows of four on a 13 by 7 mm
/// outline, with their sixteen pads shuffled into four nets of four. Net by net they cross
/// one another, so routing them in the wrong order, or without ripping anything up, strands
/// some. Board `n` is the same on every machine.
pub fn hard_boards(count: usize) -> Vec<Board> {
    let mut seed: u64 = 12345;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    (0..count)
        .map(|_| {
            let mut pads: Vec<(usize, &str)> = (1..=8).flat_map(|i| [(i, "1"), (i, "2")]).collect();
            for i in (1..pads.len()).rev() {
                let j = next() % (i + 1);
                pads.swap(i, j);
            }
            let design = Design {
                modules: Vec::new(),
                name: "Hard".into(),
                process: Some(process()),
                features: Default::default(),
                interfaces: Vec::new(),
                notes: Vec::new(),
                parts: (1..=8)
                    .map(|i| Part {
                        provides: Vec::new(),
                        refdes: format!("R{i}"),
                        kind: "Resistor".into(),
                        value: "1k".into(),
                        package: Some("0402".into()),
                        path: String::new(),
                        pins: Vec::new(),
                    })
                    .collect(),
                nets: (0..4)
                    .map(|n| Net {
                        name: format!("N{n}"),
                        pins: pads[n * 4..n * 4 + 4]
                            .iter()
                            .map(|(r, p)| PinRef {
                                refdes: format!("R{r}"),
                                pin: (*p).to_string(),
                            })
                            .collect(),
                    })
                    .collect(),
            };
            let mut board = netsmith_place_route::board_from_design(
                &design,
                &netsmith_kicad::Mapping::standard(),
                &fixtures(),
            )
            .unwrap();
            for (k, p) in board.parts.iter_mut().enumerate() {
                p.at = Pt::new(5.0 + (k % 4) as f64 * 2.6, 5.0 + (k / 4) as f64 * 2.0);
                p.rotation = 0.0;
            }
            board.outline = Rect::new(Pt::new(3.0, 3.0), Pt::new(16.0, 10.0));
            board
        })
        .collect()
}

/// `FourLayer` as the whole pipeline resolves it, with the board choosing 1.6 mm and the
/// inner layers poured on GND and 3V3.
pub fn four_layer_process() -> netsmith_core::Process {
    four_layer_process_with(&[])
}

/// [`four_layer_process`], with the board narrowing each of `narrowings` (`.min_track &= 0.2mm..1m`).
pub fn four_layer_process_with(narrowings: &[&str]) -> netsmith_core::Process {
    let body: String = narrowings
        .iter()
        .map(|n| format!("        {n}\n"))
        .collect();
    let source = format!(
        "import std::mfg::jlcpcb::FourLayer\nimport std::parts::passive::Resistor\nboard T {{\n    .process: FourLayer {{\n        .inner1.net &= \"GND\"\n        .inner2.net &= \"3V3\"\n        .stackup.thickness &= 1.6mm\n{body}    }}\n    .r: Resistor {{\n        .a.net &= \"GND\"\n        .b.net &= \"3V3\"\n    }}\n}}\n"
    );
    process_of(&source)
}

/// ACME's `TenLayer` (a fictional fabricator, ADR 0112): GND on the first inner layer, 3V3 on the
/// last, and signal layers between them.
pub fn ten_layer_process() -> netsmith_core::Process {
    let body: String = (1..=8)
        .map(|n| match n {
            1 => "        .inner1.net &= \"GND\"\n".to_string(),
            8 => "        .inner8.net &= \"3V3\"\n".to_string(),
            n => format!("        .inner{n}.use &= \"signal\"\n"),
        })
        .collect();
    let source = format!(
        "import std::mfg::acme::TenLayer\nimport std::parts::passive::Resistor\nboard T {{\n    .process: TenLayer {{\n{body}    }}\n    .r: Resistor {{\n        .a.net &= \"GND\"\n        .b.net &= \"3V3\"\n    }}\n}}\n"
    );
    process_of(&source)
}

/// A four-sided fine-pitch package: `per_side` pads on each side at 0.5 mm, numbered round
/// from the top left, the footprint a `.kicad_mod` file.
fn quad_footprint(per_side: usize) -> String {
    let pitch = 0.5;
    let reach = per_side as f64 * pitch / 2.0 + 1.0;
    let mut pads = String::new();
    for side in 0..4 {
        for k in 0..per_side {
            let along = (k as f64 - (per_side as f64 - 1.0) / 2.0) * pitch;
            let (x, y, w, h) = match side {
                0 => (-reach, along, 1.5, 0.3),
                1 => (along, reach, 0.3, 1.5),
                2 => (reach, -along, 1.5, 0.3),
                _ => (-along, -reach, 0.3, 1.5),
            };
            pads += &format!(
                "(pad \"{}\" smd roundrect (at {x:.3} {y:.3}) (size {w} {h}) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) ",
                side * per_side + k + 1
            );
        }
    }
    let court = reach + 1.2;
    let lines = [
        ((-court, -court), (court, -court)),
        ((court, -court), (court, court)),
        ((court, court), (-court, court)),
        ((-court, court), (-court, -court)),
    ]
    .map(|(a, b)| {
        format!(
            "(fp_line (start {} {}) (end {} {}) (stroke (width 0.05) (type solid)) (layer \"F.CrtYd\")) ",
            a.0, a.1, b.0, b.1
        )
    })
    .concat();
    format!(
        "(footprint \"QUAD\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 {} 0) (layer \"F.SilkS\") \
         (effects (font (size 1 1) (thickness 0.15)))) \
         (property \"Value\" \"QUAD\" (at 0 {} 0) (layer \"F.Fab\") \
         (effects (font (size 1 1) (thickness 0.15)))) \
         (attr smd) {lines}{pads})\n",
        -court - 0.7,
        court + 0.7
    )
}

/// A module with pads of 0.6 mm on a 1.1 mm pitch in `rings` rings round a hollow centre, on
/// a grid of `side` by `side` places: `4 * (side - 1)` pads in the outer ring, 8 fewer in the
/// next, and so on. `side` 12 and 2 rings make 44 + 36 = 80 pads. The pads are numbered ring
/// by ring, clockwise from the top left. The footprint's name is `QUAD`, for the caller to
/// replace.
pub fn module_footprint(side: usize, rings: usize) -> String {
    let (pitch, pad) = (1.1, 0.6);
    let mut pads = String::new();
    let mut number = 0;
    for ring in 0..rings {
        let n = side - 2 * ring;
        let mut cells: Vec<(usize, usize)> = Vec::new();
        for c in 0..n - 1 {
            cells.push((ring, ring + c));
        }
        for r in 0..n - 1 {
            cells.push((ring + r, ring + n - 1));
        }
        for c in 0..n - 1 {
            cells.push((ring + n - 1, ring + n - 1 - c));
        }
        for r in 0..n - 1 {
            cells.push((ring + n - 1 - r, ring));
        }
        for (row, col) in cells {
            let x = (col as f64 - (side as f64 - 1.0) / 2.0) * pitch;
            let y = (row as f64 - (side as f64 - 1.0) / 2.0) * pitch;
            number += 1;
            pads += &format!(
                "(pad \"{number}\" smd rect (at {x:.3} {y:.3}) (size {pad} {pad}) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) "
            );
        }
    }
    let court = side as f64 * pitch / 2.0 + 0.3;
    let lines = [
        ((-court, -court), (court, -court)),
        ((court, -court), (court, court)),
        ((court, court), (-court, court)),
        ((-court, court), (-court, -court)),
    ]
    .map(|(a, b)| {
        format!(
            "(fp_line (start {} {}) (end {} {}) (stroke (width 0.05) (type solid)) (layer \"F.CrtYd\")) ",
            a.0, a.1, b.0, b.1
        )
    })
    .concat();
    format!(
        "(footprint \"QUAD\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 {} 0) (layer \"F.SilkS\") \
         (effects (font (size 1 1) (thickness 0.15)))) \
         (property \"Value\" \"QUAD\" (at 0 {} 0) (layer \"F.Fab\") \
         (effects (font (size 1 1) (thickness 0.15)))) \
         (attr smd) {lines}{pads})\n",
        -court - 0.7,
        court + 0.7
    )
}

/// A tight board in the style of a real one that was placed and then not routed to the end:
/// about a hundred parts on both faces in a small area, round a module of 80 pads of 0.6 mm
/// on a 1.1 mm pitch (two rings round a hollow centre). 80 0402 resistors and capacitors and
/// 18 regulators in SOT-23, odd ones on the back; 60 of the module's pads join a signal, the
/// others ground and the supply. `area` is the outline's size in mm, width and height.
/// Seed `seed` decides the circuit and the placement.
pub fn tight_board(seed: u64, area: (f64, f64)) -> Board {
    let mut sides = std::collections::BTreeMap::new();
    for k in (2..=80).step_by(2) {
        let refdes = format!("{}{k}", if (k - 1) % 3 == 0 { "C" } else { "R" });
        sides.insert(refdes, netsmith_pcb::Side::Back);
    }
    for k in (2..=18).step_by(2) {
        sides.insert(format!("Q{k}"), netsmith_pcb::Side::Back);
    }
    let placement = PlaceOptions {
        sides,
        fit: Some(area),
        seed,
        ..PlaceOptions::default()
    };
    synthetic_board_with(
        false,
        seed,
        80,
        18,
        20,
        60,
        40,
        &placement,
        Some(&module_footprint(12, 2)),
    )
}

/// A random-looking board of the size a real one is, made from a seed: `chips` two-pad
/// parts and `regulators` three-pad parts round one package with `4 * per_side` pads. Of
/// the signal nets, `ic_nets` join a pad of the package to a pin of another part and
/// `part_nets` join two or three pins of other parts; GND and 3V3 are the inner planes and
/// take a few pins each. Nets run between parts that are near each other in the list, as a
/// circuit's do. The parts are placed by the placer.
pub fn synthetic_board(
    seed: u64,
    chips: usize,
    regulators: usize,
    per_side: usize,
    ic_nets: usize,
    part_nets: usize,
) -> Board {
    synthetic_board_placed(
        seed,
        chips,
        regulators,
        per_side,
        ic_nets,
        part_nets,
        &PlaceOptions::default(),
    )
}

/// [`synthetic_board`] placed as `placement` says.
pub fn synthetic_board_placed(
    seed: u64,
    chips: usize,
    regulators: usize,
    per_side: usize,
    ic_nets: usize,
    part_nets: usize,
    placement: &PlaceOptions,
) -> Board {
    synthetic_board_with(
        false, seed, chips, regulators, per_side, ic_nets, part_nets, placement, None,
    )
}

/// [`synthetic_board`] with the structure a design written in modules has: every part has an
/// instance path (`m3.r12`, eight parts to a module, in the order their nets run), the
/// package's ground and supply pads are `power` pins, and eight decoupling capacitors in a
/// module of their own sit between 3V3 and GND.
pub fn synthetic_board_structured(
    seed: u64,
    chips: usize,
    regulators: usize,
    per_side: usize,
    ic_nets: usize,
    part_nets: usize,
    placement: &PlaceOptions,
) -> Board {
    synthetic_board_with(
        true, seed, chips, regulators, per_side, ic_nets, part_nets, placement, None,
    )
}

#[allow(clippy::too_many_arguments)]
fn synthetic_board_with(
    structured: bool,
    seed: u64,
    chips: usize,
    regulators: usize,
    per_side: usize,
    ic_nets: usize,
    part_nets: usize,
    placement: &PlaceOptions,
    module: Option<&str>,
) -> Board {
    let mut state = seed;
    let mut next = move |bound: usize| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as usize % bound
    };
    let mut parts = vec![Part {
        provides: Vec::new(),
        refdes: "U1".into(),
        kind: "Quad".into(),
        value: "QUAD".into(),
        package: Some("QUAD".into()),
        path: String::new(),
        pins: Vec::new(),
    }];
    let mut free: Vec<(String, String)> = Vec::new();
    for i in 0..chips {
        let (kind, value) = if i % 3 == 0 {
            ("Capacitor", "100n")
        } else {
            ("Resistor", "10k")
        };
        let refdes = format!("{}{}", &kind[..1], i + 1);
        parts.push(Part {
            provides: Vec::new(),
            refdes: refdes.clone(),
            kind: kind.into(),
            value: value.into(),
            package: Some("0402".into()),
            path: String::new(),
            pins: Vec::new(),
        });
        free.extend(["1", "2"].map(|p| (refdes.clone(), p.to_string())));
    }
    for i in 0..regulators {
        let refdes = format!("Q{}", i + 1);
        parts.push(Part {
            provides: Vec::new(),
            refdes: refdes.clone(),
            kind: "LinearRegulator".into(),
            value: "reg".into(),
            package: Some("SOT-23".into()),
            path: String::new(),
            pins: Vec::new(),
        });
        free.extend(["1", "2", "3"].map(|p| (refdes.clone(), p.to_string())));
    }
    if structured {
        for (k, part) in parts.iter_mut().enumerate().skip(1) {
            part.path = format!("m{}.{}", (k - 1) / 8, part.refdes.to_lowercase());
        }
    }
    let pin = |(refdes, pin): &(String, String)| PinRef {
        refdes: refdes.clone(),
        pin: pin.clone(),
    };
    // Four pins for each plane, spread through the list.
    let mut gnd: Vec<PinRef> = Vec::new();
    let mut rails: Vec<PinRef> = Vec::new();
    for k in 0..8 {
        let at = (k * free.len()) / 8;
        let taken = pin(&free.remove(at.min(free.len() - 1)));
        if k % 2 == 0 { &mut gnd } else { &mut rails }.push(taken);
    }
    // A pin near `at` in the list of pins left.
    let near = |free: &mut Vec<(String, String)>, at: usize, back: usize| {
        (!free.is_empty()).then(|| pin(&free.remove(at.saturating_sub(back).min(free.len() - 1))))
    };
    let mut nets: Vec<Net> = Vec::new();
    let mut ic_left = ic_nets;
    for n in 0..ic_nets + part_nets {
        let at = n * free.len() / (ic_nets + part_nets);
        let mut pins = Vec::new();
        if next(ic_nets + part_nets - n) < ic_left {
            ic_left -= 1;
            pins.push(PinRef {
                refdes: "U1".into(),
                pin: (4 * per_side - ic_left).to_string(),
            });
            pins.extend(near(&mut free, at, next(6)));
        } else {
            for _ in 0..2 + usize::from(next(10) == 0) {
                pins.extend(near(&mut free, at, next(6)));
            }
        }
        if pins.len() < 2 {
            continue;
        }
        nets.push(Net {
            name: format!("S{n}"),
            pins,
        });
    }
    // What is left goes to the planes, and some more pads of the package to ground.
    for (k, p) in free.iter().enumerate() {
        if k % 2 == 0 { &mut gnd } else { &mut rails }.push(pin(p));
    }
    gnd.extend((1..=4 * per_side - ic_nets).step_by(9).map(|p| PinRef {
        refdes: "U1".into(),
        pin: p.to_string(),
    }));
    nets.push(Net {
        name: "GND".into(),
        pins: gnd,
    });
    nets.push(Net {
        name: "3V3".into(),
        pins: rails,
    });
    if structured {
        // Eight decoupling capacitors between the supply and ground, and the package's pins
        // for both.
        for k in 0..8 {
            let refdes = format!("Cd{}", k + 1);
            parts.push(Part {
                provides: Vec::new(),
                refdes: refdes.clone(),
                kind: "Capacitor".into(),
                value: "100n".into(),
                package: Some("0402".into()),
                path: format!("pwr.cd{}", k + 1),
                pins: Vec::new(),
            });
            nets.last_mut().unwrap().pins.push(PinRef {
                refdes: refdes.clone(),
                pin: "1".into(),
            });
            let gnd_net = nets.len() - 2;
            nets[gnd_net].pins.push(PinRef {
                refdes,
                pin: "2".into(),
            });
        }
        // The pads no signal uses: the first half to ground, the rest to the supply.
        let spare = 4 * per_side - ic_nets;
        let ground: Vec<String> = (1..=spare / 2).map(|p| p.to_string()).collect();
        let supply: Vec<String> = (spare / 2 + 1..=spare).map(|p| p.to_string()).collect();
        let gnd_net = nets.len() - 2;
        for pad in ground.iter().filter(|p| p.as_str() != "1") {
            nets[gnd_net].pins.push(PinRef {
                refdes: "U1".into(),
                pin: pad.clone(),
            });
        }
        for pad in &supply {
            nets.last_mut().unwrap().pins.push(PinRef {
                refdes: "U1".into(),
                pin: pad.clone(),
            });
        }
        let power = |name: &str, pads: Vec<String>| netsmith_core::PartPin {
            name: name.into(),
            datasheet: name.into(),
            kind: netsmith_core::PinKind::Power,
            pads,
            function: None,
            power_flow: None,
        };
        parts[0].pins = vec![power("vdd", supply), power("gnd", ground)];
    }
    let design = Design {
        modules: Vec::new(),
        name: "Synthetic".into(),
        process: Some(four_layer_process()),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts,
        nets,
    };

    // A folder of its own for each board, so that boards with different packages do not
    // meet; it is removed when the board is made.
    let folder = TempDir::new("synthetic");
    let name = if module.is_some() {
        format!("MODULE{}", 4 * per_side)
    } else {
        format!("QUAD{per_side}")
    };
    let text = module
        .map_or_else(|| quad_footprint(per_side), str::to_string)
        .replace("\"QUAD\"", &format!("\"{name}\""));
    std::fs::write(folder.join(format!("{name}.kicad_mod")), text).unwrap();
    let mapping = netsmith_kicad::Mapping::parse(
        &format!(
            "[[part]]\nkind = \"Quad\"\npackage = \"QUAD\"\nsymbol = \"synth:Q\"\nfootprint = \"synth:{name}\"\n"
        ),
        "synthetic",
        std::path::Path::new("."),
    )
    .unwrap();
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    netsmith_place_route::board_from_design_placed(
        &design,
        &mapping,
        &fixtures().with_library("synth", folder.path()),
        placement,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// A land grid package, `side` by `side` square pads of 0.9 mm on a 1.1 mm pitch numbered
/// along the rows, the footprint a `.kicad_mod` file. Only 0.2 mm is left between two pads,
/// not enough for a track to pass with its clearances (a track and its two spaces take
/// 0.3 mm), so the pads inside the outer ring can only be reached by a via in the pad.
fn land_grid_footprint(side: usize, pad: f64) -> String {
    let pitch = 1.1;
    let mut pads = String::new();
    for row in 0..side {
        for col in 0..side {
            let x = (col as f64 - (side as f64 - 1.0) / 2.0) * pitch;
            let y = (row as f64 - (side as f64 - 1.0) / 2.0) * pitch;
            pads += &format!(
                "(pad \"{}\" smd rect (at {x:.3} {y:.3}) (size {pad} {pad}) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) ",
                row * side + col + 1
            );
        }
    }
    let name_tag = land_grid_tag(pad);
    let court = side as f64 * pitch / 2.0 + 0.5;
    let lines = [
        ((-court, -court), (court, -court)),
        ((court, -court), (court, court)),
        ((court, court), (-court, court)),
        ((-court, court), (-court, -court)),
    ]
    .map(|(a, b)| {
        format!(
            "(fp_line (start {} {}) (end {} {}) (stroke (width 0.05) (type solid)) (layer \"F.CrtYd\")) ",
            a.0, a.1, b.0, b.1
        )
    })
    .concat();
    format!(
        "(footprint \"LGA{name_tag}\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 {} 0) (layer \"F.SilkS\") \
         (effects (font (size 1 1) (thickness 0.15)))) \
         (property \"Value\" \"LGA{name_tag}\" (at 0 {} 0) (layer \"F.Fab\") \
         (effects (font (size 1 1) (thickness 0.15)))) \
         (attr smd) {lines}{pads})\n",
        -court - 0.7,
        court + 0.7
    )
}

/// A land grid package of `side` by `side` pads with a resistor on each of `nets` of its
/// pads, taken in turn from the outer ring and from the inside, the resistors' other pads
/// on GND. Returns the board and the numbers of the pads that are inside the ring.
pub fn land_grid_board(side: usize, nets: usize) -> (Board, Vec<String>) {
    land_grid_board_with(side, nets, &[])
}

/// [`land_grid_board`] on a process the board has narrowed (`.via_in_pad &= 0mm`).
pub fn land_grid_board_with(side: usize, nets: usize, narrowings: &[&str]) -> (Board, Vec<String>) {
    land_grid_board_sized(side, nets, 0.9, narrowings)
}

/// The tag that tells land grids with different pads apart.
fn land_grid_tag(pad: f64) -> String {
    format!("{}", (pad * 100.0).round() as u32)
}

/// [`land_grid_board_with`] with pads of `pad_size` mm. Pads of 0.6 mm leave 0.5 mm between
/// them, room for a track.
pub fn land_grid_board_sized(
    side: usize,
    nets: usize,
    pad_size: f64,
    narrowings: &[&str],
) -> (Board, Vec<String>) {
    let inside = |p: usize| {
        let (row, col) = ((p - 1) / side, (p - 1) % side);
        (1..side - 1).contains(&row) && (1..side - 1).contains(&col)
    };
    let ring: Vec<usize> = (1..=side * side).filter(|&p| !inside(p)).collect();
    let inner: Vec<usize> = (1..=side * side).filter(|&p| inside(p)).collect();
    let mut parts = vec![Part {
        provides: Vec::new(),
        refdes: "U1".into(),
        kind: "Lga".into(),
        value: "LGA".into(),
        package: Some(format!("LGA{}", land_grid_tag(pad_size))),
        path: String::new(),
        pins: Vec::new(),
    }];
    let mut signal = Vec::new();
    let mut ground = Vec::new();
    let mut walled = Vec::new();
    for k in 0..nets {
        let refdes = format!("R{}", k + 1);
        parts.push(Part {
            provides: Vec::new(),
            refdes: refdes.clone(),
            kind: "Resistor".into(),
            value: "10k".into(),
            package: Some("0402".into()),
            path: String::new(),
            pins: Vec::new(),
        });
        let pad = if k % 2 == 0 {
            ring[k / 2 * 3 % ring.len()]
        } else {
            walled.push((inner[k / 2 * 4 % inner.len()]).to_string());
            inner[k / 2 * 4 % inner.len()]
        };
        signal.push(Net {
            name: format!("S{k}"),
            pins: vec![
                PinRef {
                    refdes: "U1".into(),
                    pin: pad.to_string(),
                },
                PinRef {
                    refdes: refdes.clone(),
                    pin: "1".into(),
                },
            ],
        });
        ground.push(PinRef {
            refdes,
            pin: "2".into(),
        });
    }
    // A capacitor between the planes.
    parts.push(Part {
        provides: Vec::new(),
        refdes: "C1".into(),
        kind: "Capacitor".into(),
        value: "100n".into(),
        package: Some("0402".into()),
        path: String::new(),
        pins: Vec::new(),
    });
    ground.push(PinRef {
        refdes: "C1".into(),
        pin: "2".into(),
    });
    let mut all = signal;
    all.push(Net {
        name: "GND".into(),
        pins: ground,
    });
    all.push(Net {
        name: "3V3".into(),
        pins: vec![PinRef {
            refdes: "C1".into(),
            pin: "1".into(),
        }],
    });
    let design = Design {
        modules: Vec::new(),
        name: "LandGrid".into(),
        process: Some(four_layer_process_with(narrowings)),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts,
        nets: all,
    };
    let folder = TempDir::new("synthetic");
    let tag = land_grid_tag(pad_size);
    std::fs::write(
        folder.join(format!("LGA{tag}.kicad_mod")),
        land_grid_footprint(side, pad_size),
    )
    .unwrap();
    let mapping = netsmith_kicad::Mapping::parse(
        &format!("[[part]]\nkind = \"Lga\"\npackage = \"LGA{tag}\"\nsymbol = \"synth:L\"\nfootprint = \"synth:LGA{tag}\"\n"),
        "synthetic",
        std::path::Path::new("."),
    )
    .unwrap();
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let board = netsmith_place_route::board_from_design(
        &design,
        &mapping,
        &fixtures().with_library("synth", folder.path()),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    (board, walled)
}

/// Writes a measured board's `.kicad_pcb` and `.kicad_pro` as `<name>.kicad_pcb` into the
/// folder `NETSMITH_BOARD_OUT` names, if it names one, for the route scorecard
/// (`scripts/route-scorecard`).
pub fn write_measured(board: &Board, name: &str) {
    let Some(dir) = std::env::var_os("NETSMITH_BOARD_OUT") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the folder can be made");
    std::fs::write(
        dir.join(format!("{name}.kicad_pcb")),
        netsmith_pcb::kicad_pcb(board),
    )
    .expect("the board file can be written");
    std::fs::write(
        dir.join(format!("{name}.kicad_pro")),
        netsmith_pcb::kicad_pro(board),
    )
    .expect("the project file can be written");
}
