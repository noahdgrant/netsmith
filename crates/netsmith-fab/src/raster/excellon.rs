//! The Excellon (NC drill) reader.

use super::RasterError;
use std::collections::HashMap;

/// A drilled hole or a slot, in millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct Hole {
    /// X of the hole's centre, or of the slot's first end, as written in the file.
    pub x: f64,
    /// Y of the hole's centre, or of the slot's first end, as written in the file.
    pub y: f64,
    /// The tool's diameter.
    pub diameter: f64,
    /// The other end of a routed slot, in the same coordinates; `None` for a round hole.
    pub slot_to: Option<(f64, f64)>,
    /// Whether the hole is plated through.
    pub plated: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Drill,
    Route { linear: bool },
}

struct Tool {
    diameter: f64,
    plated: Option<bool>,
}

fn plated_in(text: &str) -> Option<bool> {
    if text.contains("NonPlated") {
        Some(false)
    } else if text.contains("Plated") {
        Some(true)
    } else {
        None
    }
}

/// Reads an Excellon drill file into holes, in file order.
///
/// Coordinates and diameters are converted to millimetres (`METRIC`/`INCH`, `M71`/`M72`;
/// metric if the file says nothing), but the sign convention is that of the file: Y is as
/// written. KiCad writes Gerber orientation (Y up) with its drill origin and negative Y
/// below the origin, so the caller handles any flip.
///
/// Supported: decimal coordinates (`X24.29Y-26.77`, as KiCad writes with `FMAT,2`);
/// fixed-format coordinates, using the digit counts of a `METRIC,TZ,000.000`-style header
/// line or of a `; FORMAT={3:3/ ...}` comment (metric 3.3 and inch 2.4 otherwise) and the
/// zero suppression named by `TZ`/`LZ` or the comment (leading zeros omitted if the file
/// does not say); tools `T1C0.150` in the header or the body; `G90`, `G05`; routed slots
/// (`G00X..Y..`, `M15`, `G01X..Y..`, `M16`; every `G01` move with the tool down is one
/// slot); and `G85` slots (`X..Y..G85X..Y..`). Arc routing (`G02`, `G03`), incremental
/// coordinates and repeats (`R`) are errors that name the command and its line.
///
/// A hole is plated unless the tool's `TA.AperFunction` comment (`; #@! TA.AperFunction,
/// NonPlated,...`) or else the file's `TF.FileFunction` comment says `NonPlated`.
pub fn parse_excellon(text: &str) -> Result<Vec<Hole>, RasterError> {
    let mut unit = 1.0;
    let mut digits: Option<(usize, usize)> = None;
    let mut trailing_omitted = false;
    let mut file_plated: Option<bool> = None;
    let mut pending_plated: Option<bool> = None;
    let mut tools: HashMap<u32, Tool> = HashMap::new();
    let mut tool: Option<u32> = None;
    let mut mode = Mode::Drill;
    let mut down = false;
    let mut pos = (0.0, 0.0);
    let mut holes = Vec::new();
    let mut header = false;

    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(comment) = line.strip_prefix(';') {
            let comment = comment.trim();
            if let Some(body) = comment.strip_prefix("#@!") {
                if body.contains("TF.FileFunction") {
                    file_plated = plated_in(body);
                } else if body.contains("TA.AperFunction") {
                    pending_plated = plated_in(body);
                }
            } else if let Some(f) = comment.strip_prefix("FORMAT=") {
                // `{3:3/ absolute / metric / suppress leading zeros}`
                let f = f.trim_start_matches('{');
                let head = f.split('/').next().unwrap_or("");
                if let Some((a, b)) = head.split_once(':')
                    && let (Ok(a), Ok(b)) = (a.trim().parse(), b.trim().parse())
                {
                    digits = Some((a, b));
                }
                if f.contains("suppress trailing") {
                    trailing_omitted = true;
                } else if f.contains("suppress leading") {
                    trailing_omitted = false;
                }
                if f.contains("inch") {
                    unit = 25.4;
                } else if f.contains("metric") {
                    unit = 1.0;
                }
            }
            continue;
        }
        if line == "M48" {
            header = true;
            continue;
        }
        if line == "%" || line == "M95" {
            header = false;
            continue;
        }
        if let Some(rest) = line
            .strip_prefix("METRIC")
            .map(|r| (1.0, r))
            .or_else(|| line.strip_prefix("INCH").map(|r| (25.4, r)))
        {
            unit = rest.0;
            for part in rest.1.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                match part {
                    "TZ" => trailing_omitted = false,
                    "LZ" => trailing_omitted = true,
                    p if p.contains('.') && p.bytes().all(|b| b == b'0' || b == b'.') => {
                        let (a, b) = p.split_once('.').unwrap_or((p, ""));
                        digits = Some((a.len(), b.len()));
                    }
                    _ => {}
                }
            }
            continue;
        }
        if line.starts_with("FMAT") || line.starts_with("VER") || line.starts_with("DETECT") {
            continue;
        }
        if line.starts_with("ICI") {
            return Err(RasterError::new(
                line_no,
                format!("unsupported command `{line}` (incremental coordinates)"),
            ));
        }

        // A tool: `T1` selects, `T1C0.150F200S65` defines.
        if let Some(rest) = line.strip_prefix('T') {
            let n_len = rest.chars().take_while(char::is_ascii_digit).count();
            let n: u32 = rest[..n_len]
                .parse()
                .map_err(|_| RasterError::new(line_no, format!("unsupported command `{line}`")))?;
            let params = &rest[n_len..];
            if let Some(c) = params.strip_prefix('C') {
                let len = c
                    .chars()
                    .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
                    .count();
                let d: f64 = c[..len].parse().map_err(|_| {
                    RasterError::new(line_no, format!("bad tool diameter in `{line}`"))
                })?;
                tools.insert(
                    n,
                    Tool {
                        diameter: d * unit,
                        plated: pending_plated.take(),
                    },
                );
                if !header {
                    tool = Some(n);
                }
            } else if params.is_empty() {
                tool = if n == 0 { None } else { Some(n) };
                if n != 0 && !tools.contains_key(&n) {
                    return Err(RasterError::new(
                        line_no,
                        format!("tool T{n} is used but not defined"),
                    ));
                }
            } else {
                return Err(RasterError::new(
                    line_no,
                    format!("unsupported command `{line}`"),
                ));
            }
            continue;
        }

        // Anything else is a block of G, M, X and Y words.
        let mut x: Option<f64> = None;
        let mut y: Option<f64> = None;
        let mut slot_start: Option<(f64, f64)> = None;
        let mut has_move = false;
        let mut ignore_coords = false;
        let mut done = false;
        let mut chars = line.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let start = i + 1;
            let mut end = start;
            while let Some(&(j, d)) = chars.peek() {
                if d.is_ascii_digit() || matches!(d, '+' | '-' | '.') {
                    end = j + 1;
                    chars.next();
                } else {
                    break;
                }
            }
            let v = &line[start..end];
            let num = || -> Result<u32, RasterError> {
                v.parse()
                    .map_err(|_| RasterError::new(line_no, format!("unsupported command `{line}`")))
            };
            match c {
                'X' => {
                    x = Some(
                        coordinate(
                            v,
                            digits.unwrap_or(default_digits(unit)),
                            trailing_omitted,
                            line_no,
                        )? * unit,
                    );
                    has_move = true;
                }
                'Y' => {
                    y = Some(
                        coordinate(
                            v,
                            digits.unwrap_or(default_digits(unit)),
                            trailing_omitted,
                            line_no,
                        )? * unit,
                    );
                    has_move = true;
                }
                'G' => match num()? {
                    0 => mode = Mode::Route { linear: false },
                    1 => mode = Mode::Route { linear: true },
                    5 => {
                        mode = Mode::Drill;
                        down = false;
                    }
                    85 => {
                        slot_start = Some((x.unwrap_or(pos.0), y.unwrap_or(pos.1)));
                        x = None;
                        y = None;
                        has_move = false;
                    }
                    90 => {}
                    93 => ignore_coords = true,
                    n @ (2 | 3) => {
                        return Err(RasterError::new(
                            line_no,
                            format!("unsupported command `G{n:02}` (arc routing) in `{line}`"),
                        ));
                    }
                    n => {
                        return Err(RasterError::new(
                            line_no,
                            format!("unsupported command `G{n:02}` in `{line}`"),
                        ));
                    }
                },
                'M' => match num()? {
                    30 | 0 => done = true,
                    15 => down = true,
                    16 => down = false,
                    17 | 47 => {}
                    71 => unit = 1.0,
                    72 => unit = 25.4,
                    n => {
                        return Err(RasterError::new(
                            line_no,
                            format!("unsupported command `M{n:02}` in `{line}`"),
                        ));
                    }
                },
                _ => {
                    return Err(RasterError::new(
                        line_no,
                        format!("unsupported command `{line}`"),
                    ));
                }
            }
        }
        if ignore_coords {
            continue;
        }
        if has_move || slot_start.is_some() {
            let target = (x.unwrap_or(pos.0), y.unwrap_or(pos.1));
            let hole_for =
                |from: (f64, f64), to: Option<(f64, f64)>| -> Result<Hole, RasterError> {
                    let t = tool.and_then(|n| tools.get(&n)).ok_or_else(|| {
                        RasterError::new(line_no, format!("no tool selected for `{line}`"))
                    })?;
                    Ok(Hole {
                        x: from.0,
                        y: from.1,
                        diameter: t.diameter,
                        slot_to: to,
                        plated: t.plated.or(file_plated).unwrap_or(true),
                    })
                };
            if let Some(start) = slot_start {
                holes.push(hole_for(start, Some(target))?);
            } else {
                match mode {
                    Mode::Drill => holes.push(hole_for(target, None)?),
                    Mode::Route { linear } => {
                        if linear && down {
                            holes.push(hole_for(pos, Some(target))?);
                        }
                    }
                }
            }
            pos = target;
        }
        if done {
            break;
        }
    }
    Ok(holes)
}

/// A coordinate in file units; one with a decimal point is taken as written.
fn coordinate(
    s: &str,
    (ints, decs): (usize, usize),
    trailing_omitted: bool,
    line: usize,
) -> Result<f64, RasterError> {
    let bad = || RasterError::new(line, format!("`{s}` is not a coordinate"));
    if s.contains('.') {
        return s.parse().map_err(|_| bad());
    }
    let (neg, d) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    if d.is_empty() {
        return Err(bad());
    }
    let mut d = d.to_string();
    if trailing_omitted {
        while d.len() < ints + decs {
            d.push('0');
        }
    }
    let v = d.parse::<f64>().map_err(|_| bad())? / 10f64.powi(decs as i32);
    Ok(if neg { -v } else { v })
}

/// Integer and decimal digits when the file does not say: metric 3.3, inch 2.4.
fn default_digits(unit: f64) -> (usize, usize) {
    if unit == 1.0 { (3, 3) } else { (2, 4) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KICAD: &str = "M48\n; DRILL file {KiCad 9.0.0}\n; FORMAT={-:-/ absolute / metric / decimal}\n; #@! TF.GenerationSoftware,Kicad,Pcbnew,9.0.0\n; #@! TF.FileFunction,Plated,1,2,PTH\nFMAT,2\nMETRIC\n; #@! TA.AperFunction,Plated,PTH,ViaDrill\nT1C0.150\n; #@! TA.AperFunction,Plated,PTH,ComponentDrill\nT2C1.000\n%\nG90\nG05\nT1\nX32.3Y-25.5\nT2\nX24.29Y-24.23\nX24.29Y-26.77\nM30\n";

    #[test]
    fn kicad_decimal_file() {
        let holes = parse_excellon(KICAD).unwrap();
        assert_eq!(
            holes,
            vec![
                Hole {
                    x: 32.3,
                    y: -25.5,
                    diameter: 0.15,
                    slot_to: None,
                    plated: true
                },
                Hole {
                    x: 24.29,
                    y: -24.23,
                    diameter: 1.0,
                    slot_to: None,
                    plated: true
                },
                Hole {
                    x: 24.29,
                    y: -26.77,
                    diameter: 1.0,
                    slot_to: None,
                    plated: true
                },
            ]
        );
    }

    #[test]
    fn non_plated_from_the_file_function() {
        let src = "M48\n; #@! TF.FileFunction,NonPlated,1,2,NPTH\nMETRIC\nT1C3.2\n%\nG90\nG05\nT1\nX5.0Y6.0\nM30\n";
        let holes = parse_excellon(src).unwrap();
        assert_eq!(holes.len(), 1);
        assert!(!holes[0].plated);
        assert_eq!((holes[0].x, holes[0].y), (5.0, 6.0));
        // With no comment at all a hole is plated.
        let bare = parse_excellon("M48\nMETRIC\nT1C0.3\n%\nT1\nX1.5Y-2.5\nM30\n").unwrap();
        assert!(bare[0].plated);
        // A tool's own comment wins over the file's.
        let mixed = "M48\n; #@! TF.FileFunction,MixedPlating,1,2\nMETRIC\n; #@! TA.AperFunction,Plated,PTH,ViaDrill\nT1C0.3\n; #@! TA.AperFunction,NonPlated,NPTH,MechanicalDrill\nT2C2.0\n%\nT1\nX1Y1\nT2\nX2Y2\nM30\n";
        let h = parse_excellon(mixed).unwrap();
        assert!(h[0].plated && !h[1].plated);
    }

    #[test]
    fn inch_file_is_converted_to_millimetres() {
        let src = "M48\nINCH\nT1C0.0394\n%\nT1\nX1.0Y-0.5\nM30\n";
        let h = parse_excellon(src).unwrap();
        assert!((h[0].x - 25.4).abs() < 1e-9 && (h[0].y + 12.7).abs() < 1e-9);
        assert!((h[0].diameter - 0.0394 * 25.4).abs() < 1e-9);
    }

    #[test]
    fn fixed_format_with_leading_and_trailing_zero_conventions() {
        // Metric 3.3, trailing zeros present (leading zeros omitted).
        let tz = "M48\nMETRIC,TZ\nT1C0.5\n%\nT1\nX1500Y-25500\nM30\n";
        let h = parse_excellon(tz).unwrap();
        assert!((h[0].x - 1.5).abs() < 1e-9 && (h[0].y + 25.5).abs() < 1e-9);
        // Leading zeros present (trailing zeros omitted): X015 is 15.000.
        let lz = "M48\nMETRIC,LZ\nT1C0.5\n%\nT1\nX015Y-0255\nM30\n";
        let h = parse_excellon(lz).unwrap();
        assert!(
            (h[0].x - 15.0).abs() < 1e-9 && (h[0].y + 25.5).abs() < 1e-9,
            "{h:?}"
        );
        // The digit counts from the header: inch 2.4.
        let inch = "M48\nINCH,TZ,00.0000\nT1C0.02\n%\nT1\nX12500Y5000\nM30\n";
        let h = parse_excellon(inch).unwrap();
        assert!((h[0].x - 1.25 * 25.4).abs() < 1e-9 && (h[0].y - 0.5 * 25.4).abs() < 1e-9);
        // The digit counts from a FORMAT comment: metric 2.4, leading zeros suppressed.
        let fmt = "M48\n;FORMAT={2:4/ absolute / metric / suppress leading zeros}\nMETRIC\nT1C0.5\n%\nT1\nX123000Y-5000\nM30\n";
        let h = parse_excellon(fmt).unwrap();
        assert!(
            (h[0].x - 12.3).abs() < 1e-9 && (h[0].y + 0.5).abs() < 1e-9,
            "{h:?}"
        );
    }

    #[test]
    fn routed_slot() {
        let src = "M48\nMETRIC\nT1C0.8\nT2C1.0\n%\nG90\nG05\nT1\nX1.0Y1.0\nT2\nG00X10.0Y5.0\nM15\nG01X14.0Y5.0\nM16\nG05\nT1\nX3.0Y3.0\nM30\n";
        let h = parse_excellon(src).unwrap();
        assert_eq!(h.len(), 3);
        assert_eq!(
            h[1],
            Hole {
                x: 10.0,
                y: 5.0,
                diameter: 1.0,
                slot_to: Some((14.0, 5.0)),
                plated: true
            }
        );
        assert_eq!((h[2].x, h[2].y, h[2].slot_to), (3.0, 3.0, None));
    }

    #[test]
    fn routed_path_gives_one_slot_per_move_and_tool_up_moves_are_not_cut() {
        let src = "M48\nMETRIC\nT1C1.0\n%\nT1\nG00X0Y0\nM15\nG01X5.0Y0.0\nX5.0Y5.0\nM16\nG00X9.0Y9.0\nG01X9.0Y12.0\nM30\n";
        let h = parse_excellon(src).unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[1].slot_to, Some((5.0, 5.0)));
        assert_eq!((h[1].x, h[1].y), (5.0, 0.0));
    }

    #[test]
    fn g85_slot() {
        let src = "M48\nMETRIC\nT1C0.7\n%\nT1\nX2.0Y3.0G85X6.0Y3.0\nX9.0Y9.0\nM30\n";
        let h = parse_excellon(src).unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(
            h[0],
            Hole {
                x: 2.0,
                y: 3.0,
                diameter: 0.7,
                slot_to: Some((6.0, 3.0)),
                plated: true
            }
        );
        assert_eq!(h[1].slot_to, None);
    }

    #[test]
    fn modal_coordinates_and_tool_definitions_with_feeds() {
        let src = "M48\nMETRIC\nT01C0.600F200S65\n%\nT01\nX1.0Y2.0\nX3.0\nY4.0\nM30\n";
        let h = parse_excellon(src).unwrap();
        let xy: Vec<_> = h.iter().map(|h| (h.x, h.y)).collect();
        assert_eq!(xy, vec![(1.0, 2.0), (3.0, 2.0), (3.0, 4.0)]);
        assert_eq!(h[0].diameter, 0.6);
    }

    #[test]
    fn errors_name_the_line() {
        let cases = [
            ("M48\nMETRIC\nT1C1\n%\nT1\nX1Y1\nG02X1Y1\nM30\n", 7, "G02"),
            ("M48\nMETRIC\nT1C1\n%\nT2\nX1Y1\n", 5, "T2"),
            ("M48\nMETRIC\nT1C1\n%\nX1Y1\n", 5, "tool"),
            ("M48\nMETRIC\nT1C1\n%\nT1\nR3X1Y1\n", 6, "R3X1Y1"),
            ("M48\nICI,ON\n", 2, "incremental"),
            ("M48\nMETRIC\nT1C1\n%\nT1\nG91\n", 6, "G91"),
            ("M48\nMETRIC\nT1C1\n%\nT1\nX1.2.3Y1\n", 6, "coordinate"),
        ];
        for (src, line, what) in cases {
            let e = parse_excellon(src).unwrap_err();
            assert_eq!(e.line, line, "{e}");
            assert!(e.message.contains(what), "{e}");
        }
    }
}
