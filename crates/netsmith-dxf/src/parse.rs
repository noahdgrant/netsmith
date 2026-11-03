//! The group-code pairs of an ASCII DXF file, gathered into sections, blocks and entities.

use std::collections::HashMap;

use crate::Error;

/// One pair of lines: a group code and its value.
#[derive(Debug, Clone)]
pub(crate) struct Group {
    pub code: i32,
    pub value: String,
    /// The line of the file the code is on (from 1), for messages.
    pub line: usize,
}

/// An entity as the file has it: its type and its groups.
#[derive(Debug, Clone)]
pub(crate) struct Raw {
    pub kind: String,
    pub groups: Vec<Group>,
    pub line: usize,
}

impl Raw {
    pub fn first(&self, code: i32) -> Option<&Group> {
        self.groups.iter().find(|g| g.code == code)
    }

    pub fn text(&self, code: i32) -> Option<&str> {
        self.first(code).map(|g| g.value.as_str())
    }

    pub fn number(&self, code: i32) -> Result<Option<f64>, Error> {
        self.first(code).map(number_of).transpose()
    }

    pub fn number_or(&self, code: i32, default: f64) -> Result<f64, Error> {
        Ok(self.number(code)?.unwrap_or(default))
    }

    /// The numbers of every group with `code`, in order.
    pub fn numbers(&self, code: i32) -> Result<Vec<f64>, Error> {
        self.groups
            .iter()
            .filter(|g| g.code == code)
            .map(number_of)
            .collect()
    }
}

pub(crate) fn number_of(g: &Group) -> Result<f64, Error> {
    let n: f64 = g.value.trim().parse().map_err(|_| Error::Syntax {
        line: g.line,
        message: format!("group {} should be a number, not `{}`", g.code, g.value),
    })?;
    if n.is_finite() {
        Ok(n)
    } else {
        Err(Error::Syntax {
            line: g.line,
            message: format!("group {} is not a finite number: `{}`", g.code, g.value),
        })
    }
}

/// A `BLOCK`: a base point and the entities drawn relative to it.
#[derive(Debug, Default)]
pub(crate) struct Block {
    pub base: (f64, f64),
    pub entities: Vec<Raw>,
}

#[derive(Debug, Default)]
pub(crate) struct Dxf {
    /// `$INSUNITS`, if the header has it.
    pub insunits: Option<i32>,
    pub blocks: HashMap<String, Block>,
    /// The entities of the model: of the `ENTITIES` section.
    pub entities: Vec<Raw>,
}

const BINARY: &str = "AutoCAD Binary DXF";

pub(crate) fn parse(text: &str) -> Result<Dxf, Error> {
    if text.trim_start_matches('\u{feff}').starts_with(BINARY) {
        return Err(Error::Binary);
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut groups: Vec<Group> = Vec::with_capacity(lines.len() / 2);
    let mut i = 0;
    while i < lines.len() {
        let code_line = lines[i].trim();
        if code_line.is_empty() && i + 1 >= lines.len() {
            break; // a trailing blank line
        }
        let code: i32 = code_line.parse().map_err(|_| Error::Syntax {
            line: i + 1,
            message: format!(
                "expected a group code (a whole number), found `{}`; this is not an ASCII DXF file",
                code_line.chars().take(40).collect::<String>()
            ),
        })?;
        let Some(value) = lines.get(i + 1) else {
            return Err(Error::Syntax {
                line: i + 1,
                message: format!("group code {code} has no value: the file ends here"),
            });
        };
        if code != 999 {
            groups.push(Group {
                code,
                value: value.trim().to_string(),
                line: i + 1,
            });
        }
        i += 2;
    }

    let mut dxf = Dxf::default();
    let mut section = String::new();
    let mut header_var = String::new();
    let mut block: Option<(String, Block)> = None;
    let mut current: Option<Raw> = None;
    let mut saw_section = false;
    let finish = |raw: Raw,
                  section: &str,
                  block: &mut Option<(String, Block)>,
                  dxf: &mut Dxf|
     -> Result<(), Error> {
        match (section, raw.kind.as_str()) {
            ("BLOCKS", "BLOCK") => {
                let name = raw.text(2).unwrap_or("").to_string();
                let base = (raw.number_or(10, 0.0)?, raw.number_or(20, 0.0)?);
                *block = Some((
                    name,
                    Block {
                        base,
                        entities: Vec::new(),
                    },
                ));
            }
            ("BLOCKS", "ENDBLK") => {
                if let Some((name, b)) = block.take() {
                    dxf.blocks.insert(name.to_ascii_uppercase(), b);
                }
            }
            ("BLOCKS", _) => {
                if let Some((_, b)) = block.as_mut() {
                    b.entities.push(raw);
                }
            }
            ("ENTITIES", _) => dxf.entities.push(raw),
            _ => {}
        }
        Ok(())
    };
    for g in groups {
        if g.code == 0 {
            if let Some(raw) = current.take() {
                finish(raw, &section, &mut block, &mut dxf)?;
            }
            match g.value.as_str() {
                "SECTION" => {
                    saw_section = true;
                    section.clear();
                    current = Some(Raw {
                        kind: "SECTION".into(),
                        groups: Vec::new(),
                        line: g.line,
                    });
                }
                "ENDSEC" => section.clear(),
                "EOF" => break,
                other => {
                    current = Some(Raw {
                        kind: other.to_ascii_uppercase(),
                        groups: Vec::new(),
                        line: g.line,
                    });
                }
            }
            continue;
        }
        if section.is_empty() {
            // The name of the section just opened, or stray groups outside any section.
            if let Some(raw) = current.as_mut().filter(|r| r.kind == "SECTION") {
                if g.code == 2 {
                    section = g.value.to_ascii_uppercase();
                    current = None;
                } else {
                    raw.groups.push(g);
                }
            }
            continue;
        }
        if section == "HEADER" {
            if g.code == 9 {
                header_var = g.value.clone();
            } else if header_var == "$INSUNITS" && g.code == 70 {
                dxf.insunits = Some(g.value.parse().map_err(|_| Error::Syntax {
                    line: g.line,
                    message: format!("$INSUNITS should be a whole number, not `{}`", g.value),
                })?);
                header_var.clear();
            }
            continue;
        }
        if let Some(raw) = current.as_mut() {
            raw.groups.push(g);
        }
    }
    if let Some(raw) = current.take() {
        finish(raw, &section, &mut block, &mut dxf)?;
    }
    if !saw_section {
        return Err(Error::Syntax {
            line: 1,
            message: "no SECTION found; this is not a DXF file".into(),
        });
    }
    Ok(dxf)
}
