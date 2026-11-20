//! `netsmith project migrate-offers` (ADR 0143): a project's catalog files written before a
//! supplier's number became an offer, moved to the format of today.
//!
//! Two things move, and the command does both or writes nothing:
//!
//! - **The `lcsc` and `library` columns of the project's own catalog files** (its `catalog_dirs`
//!   folders and its `catalogs` list) go, every other cell as it was written
//!   ([`netsmith_solve::migrate::catalog`]), and each row with an `lcsc` makes an offer of
//!   `lcsc` and, with a `library`, one of `jlcpcb`, appended to the project's offers files of
//!   those suppliers (in its `offer_dirs` folders; the first folder, `offers/` by default, when
//!   the file is new). An offer the file already has, of the same part and sku, is not written
//!   again, so a second run changes nothing.
//! - **Each `.lcsc &= "C..."` in the project's sources** (the top-level files, the modules
//!   under `src/` and the physical files beside them) becomes `.mpn &= "..."`, the part the sku
//!   is an offer of (the standard offers, the project's and the ones just moved), with `;
//!   .manufacturer &= "..."` when two parts share the number. A sku no offer has is an error at
//!   its line; in a comment it is left as it is, with a note.
//!
//! What it does not do: write a pick lock, whose `lcsc` keys `netsmith project lock --update`
//! turns into the skus of its channels once the catalogs read (the command says so), or touch a
//! dependency's files, which its own project migrates.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use netsmith_solve::migrate::{self, MovedOffer, OFFERS_HEADER};
use netsmith_solve::offers::{Offers, SUPPLIERS};

use crate::project::{MANIFEST, Project};

/// One file the migration writes, with its text after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    pub path: PathBuf,
    /// What messages call it: relative to the project root, with `/`.
    pub file: String,
    pub text: String,
    /// What changes in it, one line each, for a person.
    pub says: Vec<String>,
}

/// What the migration of a project would write, and what it says about what it did not.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// The files to write, catalogs, then offers, then sources.
    pub rewrites: Vec<Rewrite>,
    /// What was left as it was, or dropped, one line each.
    pub notes: Vec<String>,
}

impl Plan {
    /// Whether the project is migrated already: nothing to write.
    pub fn is_empty(&self) -> bool {
        self.rewrites.is_empty()
    }

    /// Write every file of the plan, each by a rename of a finished file.
    pub fn write(&self) -> Result<(), String> {
        for r in &self.rewrites {
            if let Some(dir) = r.path.parent() {
                fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            let mut temp = r.path.as_os_str().to_owned();
            temp.push(".migrate-tmp");
            let temp = PathBuf::from(temp);
            fs::write(&temp, &r.text).map_err(|e| format!("{}: {e}", r.file))?;
            fs::rename(&temp, &r.path).map_err(|e| {
                let _ = fs::remove_file(&temp);
                format!("{}: {e}", r.file)
            })?;
        }
        Ok(())
    }
}

/// The migration of `project`: what it would write, or the errors (each `file:line: ...`, one a
/// line) that stop it before anything is written.
pub fn plan(project: &Project) -> Result<Plan, String> {
    let mut plan = Plan::default();
    let mut errors = Vec::new();
    // The catalogs.
    let mut moved: Vec<MovedOffer> = Vec::new();
    for file in project.catalog_files()? {
        let text = fs::read_to_string(&file.path).map_err(|e| format!("{}: {e}", file.file))?;
        match migrate::catalog(&file.file, &text) {
            Ok(None) => {}
            Ok(Some(m)) => {
                let rows = m.offers.iter().filter(|o| o.supplier == "lcsc").count();
                plan.rewrites.push(Rewrite {
                    path: file.path.clone(),
                    file: file.file.clone(),
                    text: m.catalog,
                    says: vec![match rows {
                        1 => "removed the lcsc and library columns; 1 row has its number in the offers files now".to_string(),
                        n => format!("removed the lcsc and library columns; {n} rows have their numbers in the offers files now"),
                    }],
                });
                plan.notes.extend(m.notes);
                moved.extend(m.offers);
            }
            Err(e) => errors.push(e),
        }
    }
    // The offers: the standard ones and the project's, then the moved ones not there yet.
    let mut known = netsmith_solve::Catalogs::jlcpcb().offers().clone();
    let mut existing: Vec<(String, PathBuf, String, String)> = Vec::new();
    for supplier in SUPPLIERS {
        let Some((path, file)) = project.offer_file_of(supplier)? else {
            if moved.iter().any(|o| o.supplier == supplier) {
                errors.push(format!(
                    "{MANIFEST}: `offer_dirs` lists no folder, so the offers of {supplier} have nowhere to go: list one (`offer_dirs = [\"offers\"]`)"
                ));
            }
            continue;
        };
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("{file}: {e}")),
        };
        if !text.is_empty()
            && let Err(e) = known.add_file(supplier, &file, &text)
        {
            errors.push(e);
        }
        existing.push((supplier.to_string(), path, file, text));
    }
    for (supplier, path, file, text) in existing {
        let new: Vec<MovedOffer> = moved
            .iter()
            .filter(|o| o.supplier == supplier)
            .filter(|o| {
                // An offer of the same part and sku is there already: a second run.
                !matches!(
                    known.of(&supplier, o.manufacturer.as_deref(), &o.mpn),
                    Ok(Some(had)) if had.sku == o.sku
                )
            })
            .cloned()
            .collect();
        if new.is_empty() {
            continue;
        }
        let mut after = if text.is_empty() {
            OFFERS_HEADER.to_string()
        } else {
            text.clone()
        };
        if !after.ends_with('\n') {
            after.push('\n');
        }
        let lines = migrate::offer_lines(&new);
        after.push_str(&lines);
        // The moved offers are checked as a build reads them: a sku of another part, or a part
        // with another sku, is an error here and not at the next build.
        let added = format!("{OFFERS_HEADER}{lines}");
        if let Err(e) = known.add_file(&supplier, &file, &added) {
            errors.push(e.replacen(
                &format!("{file}:"),
                &format!("{file} (the offer moved from {}):", new[0].from),
                1,
            ));
            continue;
        }
        plan.rewrites.push(Rewrite {
            path,
            file,
            text: after,
            says: vec![match new.len() {
                1 => "added 1 offer".to_string(),
                n => format!("added {n} offers"),
            }],
        });
    }
    // The sources.
    for (file, path) in sources_of(project)? {
        let text = fs::read_to_string(&path).map_err(|e| format!("{file}: {e}"))?;
        let (after, says, notes, wrong) = hand_picks(&file, &text, &known);
        errors.extend(wrong);
        plan.notes.extend(notes);
        if after != text {
            plan.rewrites.push(Rewrite {
                path,
                file,
                text: after,
                says,
            });
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(plan)
}

/// The sources of a project, as `project fmt` finds them: the top-level files, the modules under
/// `src/`, and the physical file beside each, by the name a message calls them.
fn sources_of(project: &Project) -> Result<Vec<(String, PathBuf)>, String> {
    let mut paths: BTreeSet<PathBuf> = BTreeSet::new();
    for name in &project.manifest.top_level {
        let path = project.root.join(name);
        if path.is_file() {
            paths.insert(path);
        }
    }
    for module in project.modules()? {
        paths.insert(module.path);
    }
    let physicals: Vec<PathBuf> = paths
        .iter()
        .filter_map(|p| {
            let stem = p.file_name()?.to_str()?.strip_suffix(".nsm")?;
            let physical = p.with_file_name(format!("{stem}.physical.nsm"));
            (!stem.ends_with(".physical") && physical.is_file()).then_some(physical)
        })
        .collect();
    paths.extend(physicals);
    Ok(paths
        .into_iter()
        .map(|path| {
            let name = path
                .strip_prefix(&project.root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            (name, path)
        })
        .collect())
}

/// The text of a source with each `.lcsc &= "C..."` rewritten as the `.mpn` (and, for a number
/// two parts share, the `.manufacturer`) of the part its sku is an offer of in `offers`; what
/// changed, line by line; notes; and the errors, a sku no offer has outside a comment.
fn hand_picks(
    file: &str,
    text: &str,
    offers: &Offers,
) -> (String, Vec<String>, Vec<String>, Vec<String>) {
    let (mut out, mut says, mut notes, mut wrong) =
        (String::new(), Vec::new(), Vec::new(), Vec::new());
    let mut rest = 0;
    let mut from = 0;
    while let Some(found) = text[from..].find(".lcsc") {
        let at = from + found;
        from = at + ".lcsc".len();
        let Some((end, sku)) = narrowing(text, at) else {
            continue;
        };
        let line = text[..at].matches('\n').count() + 1;
        let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
        let in_comment = text[line_start..at].contains("//");
        let Some(offer) = offers.by_sku(&sku) else {
            if in_comment {
                notes.push(format!(
                    "{file}:{line}: `.lcsc &= \"{sku}\"` in a comment is left as it is: no offer has the sku {sku}"
                ));
            } else {
                wrong.push(format!(
                    "{file}:{line}: `.lcsc &= \"{sku}\"`: no offer has the sku {sku}, so the part it named is not known; write `.mpn &= \"...\"` with the manufacturer's part number of its row by hand, and run again"
                ));
            }
            continue;
        };
        // The path before `.lcsc` (`.stage.c1` of `.stage.c1.lcsc`), for a `.manufacturer` beside it.
        let prefix_start = text[..at]
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .map_or(0, |i| i + 1);
        let prefix = &text[prefix_start..at];
        let shared = offers.of(&offer.supplier, None, &offer.mpn).is_err();
        let mut written = format!(".mpn &= \"{}\"", offer.mpn);
        if let (true, Some(maker)) = (shared, &offer.manufacturer) {
            written.push_str(&format!("; {prefix}.manufacturer &= \"{maker}\""));
        }
        out.push_str(&text[rest..at]);
        out.push_str(&written);
        rest = end;
        from = end;
        says.push(format!(
            "line {line}: `{prefix}.lcsc &= \"{sku}\"` -> `{prefix}{written}`"
        ));
    }
    out.push_str(&text[rest..]);
    (out, says, notes, wrong)
}

/// The narrowing that starts with `.lcsc` at `at`: where it ends (after the closing quote) and
/// the text it narrows to, or `None` when `.lcsc` there is not a narrowing of the member (a
/// longer name, or no `&= "..."` after it).
fn narrowing(text: &str, at: usize) -> Option<(usize, String)> {
    let after = &text[at + ".lcsc".len()..];
    if after
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let trimmed = after.trim_start_matches([' ', '\t']);
    let trimmed = trimmed.strip_prefix("&=")?;
    let trimmed = trimmed.trim_start_matches([' ', '\t']);
    let inner = trimmed.strip_prefix('"')?;
    let close = inner.find('"')?;
    let sku = inner[..close].to_string();
    let end = text.len() - inner.len() + close + 1;
    Some((end, sku))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offers() -> Offers {
        let mut o = Offers::empty();
        o.add_file(
            "lcsc",
            "offers/lcsc.csv",
            &format!("{OFFERS_HEADER},ZZ-1,C1,,x,,\nAcme,ZZ-2,C2,,x,,\nZenith,ZZ-2,C3,,x,,\n"),
        )
        .unwrap();
        o
    }

    #[test]
    fn a_narrowing_by_lcsc_becomes_one_by_mpn_with_its_path() {
        let text = "board B {\n    .c1: Capacitor { .lcsc &= \"C1\" }\n    .stage.c2.lcsc &=\"C1\"\n    .x.lcscish &= \"C1\"\n}\n";
        let (after, says, notes, wrong) = hand_picks("main.nsm", text, &offers());
        assert_eq!(
            after,
            "board B {\n    .c1: Capacitor { .mpn &= \"ZZ-1\" }\n    .stage.c2.mpn &= \"ZZ-1\"\n    .x.lcscish &= \"C1\"\n}\n"
        );
        assert_eq!(
            says,
            [
                "line 2: `.lcsc &= \"C1\"` -> `.mpn &= \"ZZ-1\"`",
                "line 3: `.stage.c2.lcsc &= \"C1\"` -> `.stage.c2.mpn &= \"ZZ-1\"`"
            ]
        );
        assert!(notes.is_empty() && wrong.is_empty());
    }

    #[test]
    fn a_number_two_parts_share_gets_the_manufacturer() {
        let (after, ..) = hand_picks("m.nsm", "    .u.lcsc &= \"C3\"\n", &offers());
        assert_eq!(
            after,
            "    .u.mpn &= \"ZZ-2\"; .u.manufacturer &= \"Zenith\"\n"
        );
    }

    #[test]
    fn a_sku_no_offer_has_is_an_error_and_in_a_comment_a_note() {
        let text = "// `.lcsc &= \"C9\"` was a hand pick\n.r1.lcsc &= \"C9\"\n";
        let (after, _, notes, wrong) = hand_picks("m.nsm", text, &offers());
        assert_eq!(after, text);
        assert_eq!(
            notes,
            ["m.nsm:1: `.lcsc &= \"C9\"` in a comment is left as it is: no offer has the sku C9"]
        );
        assert_eq!(wrong.len(), 1);
        assert!(wrong[0].starts_with("m.nsm:2: `.lcsc &= \"C9\"`: no offer has the sku C9"));
    }
}
