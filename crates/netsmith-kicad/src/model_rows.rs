//! A footprint library's 3D models as `[[model]]` rows of `netsmith.toml` (issue #1005, the
//! model table of issue #763): the `(model ...)` line of each `.kicad_mod` in the libraries of a
//! KiCad project's `fp-lib-table`, with its offset and rotation, for a person to paste into the
//! manifest. A generator for porting a KiCad project, like [`crate::library_stanzas`]: the
//! libraries are read once, here, and the rows are not another way to configure.
//!
//! Only a STEP model becomes a row, as netsmith draws nothing else. A footprint that has no
//! model line, or whose model is a `.wrl`, or is scaled, is counted in a note and left out.

use std::path::{Path, PathBuf};

use crate::Sexp;

/// The rows and what was left out.
#[derive(Debug, Default, PartialEq)]
pub struct ModelRows {
    /// The `[[model]]` rows as TOML, in the order of the libraries of the table and then of the
    /// footprints' names; empty when no footprint has a STEP model.
    pub rows: String,
    /// What was not turned into a row: a library that cannot be read, footprints with no model
    /// or a model that is not a STEP file or is scaled.
    pub notes: Vec<String>,
}

/// One footprint's model line, as a row needs it.
#[derive(Debug, PartialEq)]
pub struct ModelLine {
    /// The model's file as the footprint states it, with `${KIPRJMOD}/` taken off.
    pub file: String,
    /// The offset in mm.
    pub offset: [f64; 3],
    /// The rotation in degrees.
    pub rotation: [f64; 3],
}

/// What a footprint's model line gave.
#[derive(Debug, PartialEq)]
pub enum Line {
    /// No `(model ...)` line.
    None,
    /// A model line that is no row, and why.
    Skipped(Skip),
    Row(ModelLine),
}

/// Why a footprint's model line is no row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// The file is not a `.step` or `.stp`.
    NotStep,
    /// The line scales the model, which `.model` cannot say.
    Scaled,
}

/// The model line of the footprint in `text` (the text of a `.kicad_mod`): its first
/// `(model ...)` line. An `(offset (xyz ...))` is mm; the older `(at (xyz ...))` is inches.
pub fn model_line(text: &str) -> Result<Line, String> {
    let root = Sexp::parse(text).map_err(|e| e.to_string())?;
    let Some(model) = root.find("model") else {
        return Ok(Line::None);
    };
    let Some(file) = model.args().first().and_then(Sexp::text) else {
        return Err("a `model` line names no file".to_string());
    };
    let xyz = |name: &str| -> Result<Option<[f64; 3]>, String> {
        let Some(node) = model.find(name) else {
            return Ok(None);
        };
        let numbers: Vec<f64> = node
            .find("xyz")
            .map(|v| {
                v.args()
                    .iter()
                    .filter_map(Sexp::text)
                    .map(str::parse)
                    .collect()
            })
            .unwrap_or(Ok(Vec::new()))
            .map_err(|_| format!("the `{name}` of the model line is not three numbers"))?;
        match numbers[..] {
            [x, y, z] if numbers.iter().all(|v| v.is_finite()) => Ok(Some([x, y, z])),
            _ => Err(format!(
                "the `{name}` of the model line is not three numbers"
            )),
        }
    };
    let file = file.replace('\\', "/");
    let lower = file.to_ascii_lowercase();
    if !(lower.ends_with(".step") || lower.ends_with(".stp")) {
        return Ok(Line::Skipped(Skip::NotStep));
    }
    if xyz("scale")?.is_some_and(|s| s.iter().any(|v| (v - 1.0).abs() > 1e-9)) {
        return Ok(Line::Skipped(Skip::Scaled));
    }
    let offset = match (xyz("offset")?, xyz("at")?) {
        (Some(mm), _) => mm,
        (None, Some(inches)) => inches.map(|v| v * 25.4),
        (None, None) => [0.0; 3],
    };
    let rotation = xyz("rotate")?.unwrap_or([0.0; 3]);
    let file = file
        .strip_prefix("${KIPRJMOD}/")
        .unwrap_or(&file)
        .to_string();
    Ok(Line::Row(ModelLine {
        file,
        offset,
        rotation,
    }))
}

/// A number as TOML, to the micrometre, with no negative zero.
fn num(v: f64) -> String {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 {
        "0".to_string()
    } else {
        format!("{r}")
    }
}

fn triple(v: [f64; 3]) -> String {
    format!("[{}, {}, {}]", num(v[0]), num(v[1]), num(v[2]))
}

/// The `[[model]]` row of footprint `id` with `line`.
pub fn model_row(id: &str, line: &ModelLine) -> String {
    let mut out = format!("[[model]]\nfootprint = {id:?}\nfile = {:?}\n", line.file);
    if line.offset != [0.0; 3] {
        out.push_str(&format!("offset = {}\n", triple(line.offset)));
    }
    if line.rotation != [0.0; 3] {
        out.push_str(&format!("rotation = {}\n", triple(line.rotation)));
    }
    out.push('\n');
    out
}

/// Where a library's folder is: `${KIPRJMOD}` is `base` (the folder of the table), another
/// `${NAME}` is the environment variable of that name, and a relative path is from `base`.
fn folder(uri: &str, base: &Path) -> Result<PathBuf, String> {
    let uri = uri.replace('\\', "/");
    let expanded = match uri.strip_prefix("${").and_then(|r| r.split_once('}')) {
        Some(("KIPRJMOD", rest)) => base.join(rest.trim_start_matches('/')),
        Some((name, rest)) => {
            let value = std::env::var_os(name)
                .ok_or_else(|| format!("the environment variable `{name}` is not set"))?;
            PathBuf::from(value).join(rest.trim_start_matches('/'))
        }
        None => PathBuf::from(&uri),
    };
    Ok(if expanded.is_absolute() {
        expanded
    } else {
        base.join(expanded)
    })
}

/// The rows of the models of the footprints in the libraries of `fp_table` (the text of KiCad's
/// `fp-lib-table`), whose folders are found from `base`, the folder of the table. With `only`
/// not empty, only the libraries of those names.
pub fn model_rows(fp_table: &str, base: &Path, only: &[String]) -> Result<ModelRows, String> {
    let table = Sexp::parse(fp_table).map_err(|e| e.to_string())?;
    let mut found = ModelRows::default();
    for lib in table.find_all("lib") {
        let field = |name: &str| {
            lib.find(name)
                .and_then(|f| f.args().first())
                .and_then(Sexp::text)
        };
        let (Some(name), Some(uri)) = (field("name"), field("uri")) else {
            return Err("a `lib` entry has no name or uri".to_string());
        };
        if !only.is_empty() && !only.iter().any(|o| o == name) {
            continue;
        }
        let dir = match folder(uri, base) {
            Ok(dir) if dir.is_dir() => dir,
            Ok(dir) => {
                found.notes.push(format!(
                    "the library `{name}` is `{}`, which is not a folder: left out",
                    dir.display()
                ));
                continue;
            }
            Err(why) => {
                found
                    .notes
                    .push(format!("the library `{name}` (`{uri}`): {why}: left out"));
                continue;
            }
        };
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "kicad_mod"))
            .collect();
        files.sort();
        let (mut bare, mut other, mut scaled) = (0, 0, 0);
        for file in files {
            let Some(stem) = file.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let id = format!("{name}:{stem}");
            let text =
                std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            match model_line(&text) {
                Ok(Line::Row(line)) => found.rows.push_str(&model_row(&id, &line)),
                Ok(Line::None) => bare += 1,
                Ok(Line::Skipped(Skip::NotStep)) => other += 1,
                Ok(Line::Skipped(Skip::Scaled)) => scaled += 1,
                Err(e) => found
                    .notes
                    .push(format!("{id} ({}): {e}: left out", file.display())),
            }
        }
        for (count, one, many) in [
            (
                bare,
                "footprint has no model line",
                "footprints have no model line",
            ),
            (
                other,
                "footprint names a model that is not a STEP file",
                "footprints name a model that is not a STEP file",
            ),
            (
                scaled,
                "footprint scales its model",
                "footprints scale their model",
            ),
        ] {
            if count > 0 {
                found.notes.push(format!(
                    "the library `{name}`: {count} {}: left out",
                    if count == 1 { one } else { many }
                ));
            }
        }
    }
    Ok(found)
}
