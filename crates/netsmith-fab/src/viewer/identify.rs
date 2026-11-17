//! What a Gerber file is for: from its X2 attribute `%TF.FileFunction` when it has one, and
//! from its name otherwise, by the names KiCad (`<board>-F_Cu.gbr`, `-In1_Cu`, `-Edge_Cuts`,
//! KiCad 5's `-F_SilkS`), Protel and the board houses' guides (`.GTL`, `.GBL`, `.G1`, `.GTS`,
//! `.GTO`, `.GTP`, `.GKO`, `.GM1`) and Eagle's CAM jobs (`.cmp`, `.sol`, `.stc`, `.plc`, `.crc`,
//! `.dim`) give. A file neither names is [`Function::Other`]: it is read and summed up, and
//! drawn only with every layer.

/// The face of a board a mask, paste or silkscreen file is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Face {
    Top,
    Bottom,
}

/// The copper layer a copper file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Top,
    /// The inner layer counted from the top, from 1 (`In1.Cu`, X2's `L2`).
    Inner(u32),
    Bottom,
}

/// What a file is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Function {
    Copper(Layer),
    /// The mask's openings (what such a file draws, whatever its polarity attribute says).
    Mask(Face),
    Paste(Face),
    Silk(Face),
    Outline,
    Drill,
    Other,
}

impl Function {
    /// Its word in the summary.
    pub fn word(self) -> &'static str {
        match self {
            Function::Copper(_) => "copper",
            Function::Mask(_) => "mask",
            Function::Paste(_) => "paste",
            Function::Silk(_) => "silk",
            Function::Outline => "outline",
            Function::Drill => "drill",
            Function::Other => "other",
        }
    }

    /// The `--layer` it is drawn with as a side or a layer (`top`, `bottom`, `inner2`), for a
    /// file of one.
    pub fn layer(self) -> Option<String> {
        let face = |f: Face| match f {
            Face::Top => "top".to_string(),
            Face::Bottom => "bottom".to_string(),
        };
        match self {
            Function::Copper(Layer::Top) => Some("top".into()),
            Function::Copper(Layer::Bottom) => Some("bottom".into()),
            Function::Copper(Layer::Inner(n)) => Some(format!("inner{n}")),
            Function::Mask(f) | Function::Paste(f) | Function::Silk(f) => Some(face(f)),
            _ => None,
        }
    }
}

/// The function of a Gerber file, and whether its X2 attribute (`x2`, the text after
/// `%TF.FileFunction,`) or its name said so; `None` when neither did.
pub fn identify(x2: Option<&str>, name: &str) -> (Function, Option<&'static str>) {
    if let Some(f) = x2.and_then(from_x2) {
        return (f, Some("x2"));
    }
    match from_name(name) {
        Some(f) => (f, Some("name")),
        None => (Function::Other, None),
    }
}

fn face(word: Option<&str>) -> Option<Face> {
    match word? {
        "Top" => Some(Face::Top),
        "Bot" => Some(Face::Bottom),
        _ => None,
    }
}

/// `Copper,L1,Top`, `Copper,L2,Inr`, `Soldermask,Bot`, `Paste,Top`, `Legend,Top`, `Profile,NP`,
/// `Plated,1,2,PTH`, `NonPlated,1,2,NPTH`.
fn from_x2(value: &str) -> Option<Function> {
    let mut parts = value.split(',');
    let kind = parts.next()?;
    Some(match kind {
        "Copper" => {
            let n: u32 = parts.next()?.strip_prefix('L')?.parse().ok()?;
            match parts.next()? {
                "Top" => Function::Copper(Layer::Top),
                "Bot" => Function::Copper(Layer::Bottom),
                "Inr" if n > 1 => Function::Copper(Layer::Inner(n - 1)),
                _ => return None,
            }
        }
        "Soldermask" => Function::Mask(face(parts.next())?),
        "Paste" => Function::Paste(face(parts.next())?),
        "Legend" => Function::Silk(face(parts.next())?),
        "Profile" => Function::Outline,
        "Plated" | "NonPlated" => Function::Drill,
        _ => Function::Other,
    })
}

fn from_name(name: &str) -> Option<Function> {
    let base = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    let (stem, ext) = base.rsplit_once('.').unwrap_or((base.as_str(), ""));
    // KiCad's layer names after the board's: `-F_Cu`, `-In2_Cu` (also with a dot), the name
    // whole or after a `-`, `_` or `.`.
    let tail = |a: &str, b: &str| {
        [format!("{a}_{b}"), format!("{a}.{b}")].iter().any(|t| {
            stem.strip_suffix(t.as_str())
                .is_some_and(|before| before.is_empty() || before.ends_with(['-', '_', '.']))
        })
    };
    let by_stem = [
        ("f", "cu", Function::Copper(Layer::Top)),
        ("b", "cu", Function::Copper(Layer::Bottom)),
        ("f", "mask", Function::Mask(Face::Top)),
        ("b", "mask", Function::Mask(Face::Bottom)),
        ("f", "paste", Function::Paste(Face::Top)),
        ("b", "paste", Function::Paste(Face::Bottom)),
        ("f", "silkscreen", Function::Silk(Face::Top)),
        ("b", "silkscreen", Function::Silk(Face::Bottom)),
        ("f", "silks", Function::Silk(Face::Top)),
        ("b", "silks", Function::Silk(Face::Bottom)),
        ("edge", "cuts", Function::Outline),
    ];
    for (a, b, f) in by_stem {
        if tail(a, b) {
            return Some(f);
        }
    }
    if let Some(n) = inner_of(stem) {
        return Some(Function::Copper(Layer::Inner(n)));
    }
    Some(match ext {
        "gtl" | "cmp" => Function::Copper(Layer::Top),
        "gbl" | "sol" => Function::Copper(Layer::Bottom),
        "gts" | "stc" => Function::Mask(Face::Top),
        "gbs" | "sts" => Function::Mask(Face::Bottom),
        "gtp" | "crc" => Function::Paste(Face::Top),
        "gbp" | "crs" => Function::Paste(Face::Bottom),
        "gto" | "plc" => Function::Silk(Face::Top),
        "gbo" | "pls" => Function::Silk(Face::Bottom),
        "gko" | "gm1" | "gml" | "gm" | "dim" => Function::Outline,
        _ => {
            // Protel's inner layers: `.G1`, `.G2`, `.GL2`... (`.GP1` planes too).
            let digits = ext
                .strip_prefix("gl")
                .or_else(|| ext.strip_prefix("gp"))
                .or_else(|| ext.strip_prefix('g'))?;
            let n: u32 = digits.parse().ok().filter(|n| *n > 0)?;
            Function::Copper(Layer::Inner(n))
        }
    })
}

/// `...-In2_Cu` or `...-In2.Cu`: 2.
fn inner_of(stem: &str) -> Option<u32> {
    let cut = stem
        .strip_suffix("_cu")
        .or_else(|| stem.strip_suffix(".cu"))?;
    let at = cut.rfind("in")?;
    let n: u32 = cut[at + 2..].parse().ok()?;
    let before = cut[..at].chars().last();
    (n > 0 && before.is_none_or(|c| matches!(c, '-' | '_' | '.'))).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x2_says_first_then_the_name() {
        assert_eq!(
            identify(Some("Copper,L2,Inr"), "x.gtl"),
            (Function::Copper(Layer::Inner(1)), Some("x2"))
        );
        assert_eq!(
            identify(Some("Soldermask,Bot"), "x"),
            (Function::Mask(Face::Bottom), Some("x2"))
        );
        assert_eq!(identify(Some("Profile,NP"), "x").0, Function::Outline);
        assert_eq!(
            identify(None, "gerber/led-F_Cu.gbr"),
            (Function::Copper(Layer::Top), Some("name"))
        );
        assert_eq!(
            identify(None, "led-In3_Cu.gbr").0,
            Function::Copper(Layer::Inner(3))
        );
        assert_eq!(
            identify(None, "mchck-F_SilkS.pho").0,
            Function::Silk(Face::Top)
        );
        assert_eq!(identify(None, "led-Edge_Cuts.gbr").0, Function::Outline);
        assert_eq!(identify(None, "board.GBS").0, Function::Mask(Face::Bottom));
        assert_eq!(
            identify(None, "board.G2").0,
            Function::Copper(Layer::Inner(2))
        );
        assert_eq!(identify(None, "board.gko").0, Function::Outline);
        assert_eq!(identify(None, "notes.gbr"), (Function::Other, None));
        // A name that only looks like a layer's is not one.
        assert_eq!(identify(None, "pin_cu.gbr").0, Function::Other);
        assert_eq!(identify(None, "pcb_cu.gbr").0, Function::Other);
    }
}
