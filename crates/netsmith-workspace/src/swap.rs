//! The editor's swap (issue #800): for a generic part that no catalog row stands for, its
//! nearest rows with the board's checks run again with each in its place (the solves of
//! `Built::rechecks`, ADR 0047), and the change to the source that chooses one.
//!
//! The change writes the value of the row (`240ohm +/- 1%`) for the member that holds the
//! part's value (`resistance`): over the right-hand side of the one statement that states it,
//! or, when nothing states it, as a statement in the block of the part's declaration. It is
//! refused, with the reason, where one edit would not change this part alone: the value stated
//! in more than one place, in a definition other parts share, in a board's `defaults`, or in the
//! standard library. When the source has a pick lock, the part takes a new pick in it and the
//! other parts stay on their rows (`project lock --update --part`, ADR 0100).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use netsmith_check::{DefaultFrom, InstId, Member, Model, SourceId, Sources};
use netsmith_editor::ApiError;
use netsmith_editor::api::{Swap, SwapAlternative, SwapApply, SwapCheck, SwapEdit, Swapped};
use netsmith_lang::Span;
use netsmith_pipeline::locate;
use netsmith_pipeline::power::api_location;
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, alternatives};

use super::Workspace;
use super::analysis::BoardData;
use super::files;
use netsmith_catalog::resolve::resolve_generics;

/// Where the swap writes: a cut of one source file.
#[derive(Debug, Clone)]
struct Site {
    source: SourceId,
    cut: Cut,
}

/// The text from `start` to `end` (bytes) replaced by `before`, the value, and `after`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Cut {
    start: usize,
    end: usize,
    before: String,
    after: String,
}

impl Cut {
    fn text(&self, value: &str) -> String {
        format!("{}{value}{}", self.before, self.after)
    }
}

/// What the swap of one part found: the answer for the page, and where each alternative's value
/// would be written.
struct Found {
    swap: Swap,
    site: Option<Site>,
}

impl Workspace {
    pub(super) fn swap_of(&self, board: &str, refdes: &str) -> Result<Swap, ApiError> {
        let data = self.fresh().snapshot.board_handle(board)?;
        let catalogs = self.target().catalogs().map_err(ApiError::internal)?;
        Ok(find(&data, &catalogs, refdes)?.swap)
    }

    pub(super) fn swap_apply(&self, edit: &SwapApply) -> Result<Swapped, ApiError> {
        let _one_at_a_time = self.editing.lock().unwrap_or_else(|e| e.into_inner());
        let (data, stale) = {
            let state = self.fresh();
            let stale = state
                .snapshot
                .boards
                .iter()
                .find(|(b, _)| b.built.label() == edit.board)
                .is_some_and(|(_, stale)| *stale);
            (state.snapshot.board_handle(&edit.board)?, stale)
        };
        if stale {
            return Err(refused(
                "the source does not build, so the board shown is the last good one: fix the problems first",
            ));
        }
        let catalogs = self.target().catalogs().map_err(ApiError::internal)?;
        let found = find(&data, &catalogs, &edit.refdes)?;
        let alternative = found
            .swap
            .alternatives
            .iter()
            .find(|a| a.name == edit.alternative)
            .ok_or_else(|| {
                ApiError::not_found(format!(
                    "`{}` is not an alternative of {}",
                    edit.alternative, edit.refdes
                ))
            })?;
        let (Some(site), Some(value)) = (found.site, alternative.value.as_ref()) else {
            return Err(refused(found.swap.refused.unwrap_or_else(|| {
                format!("the value of `{}` cannot be written", alternative.name)
            })));
        };
        let trace = &data.built.trace;
        let source = trace.sources.get(site.source);
        let entry = self.entry(&source.name)?;
        let Some(path) = entry.fs.filter(|_| !entry.readonly) else {
            return Err(refused(format!("{} is read-only", source.name)));
        };
        let disk = fs::read_to_string(&path)
            .map_err(|e| ApiError::internal(format!("{}: {e}", source.name)))?;
        // The places were found in the text the analysis read; another program may have changed
        // the file since.
        if disk != source.text {
            return Err(ApiError::conflict(format!(
                "{} changed on disk since the board was analysed: wait for the analysis and swap again",
                source.name
            )));
        }
        let text = format!(
            "{}{}{}",
            &disk[..site.cut.start],
            site.cut.text(value),
            &disk[site.cut.end..]
        );
        let lock = self.pick_again(&data, &catalogs, &path, &text, &found.swap.path)?;
        files::write_atomic(&path, &text)
            .map_err(|e| ApiError::internal(format!("{}: {e}", source.name)))?;
        let lock = match lock {
            Some((file, out)) => {
                files::write_atomic(&file, &out)
                    .map_err(|e| ApiError::internal(format!("{}: {e}", file.display())))?;
                Some(
                    file.file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                )
            }
            None => None,
        };
        Ok(Swapped {
            file: source.name.clone(),
            lock,
        })
    }

    /// The pick lock beside the board's source with the part at `part` picked again, the source
    /// file at `edited` reading `text`; `None` when the source has no lock. Nothing is written:
    /// an error is the solver's reason, and then the swap writes nothing either.
    fn pick_again(
        &self,
        data: &BoardData,
        catalogs: &Catalogs,
        edited: &PathBuf,
        text: &str,
        part: &str,
    ) -> Result<Option<(PathBuf, String)>, ApiError> {
        let target = self.target();
        let file = target
            .files
            .iter()
            .find(|f| f.name == data.file)
            .ok_or_else(|| {
                ApiError::internal(format!("{} is not a file of the project", data.file))
            })?;
        let lock = netsmith_pipeline::pick_lock::lock_path(&file.path);
        if !lock.exists() {
            return Ok(None);
        }
        let same = |a: &PathBuf, b: &PathBuf| match (fs::canonicalize(a), fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => a == b,
        };
        let mut unsaved = HashMap::new();
        let top = if same(edited, &file.path) {
            text.to_string()
        } else {
            unsaved.insert(edited.clone(), text.to_string());
            fs::read_to_string(&file.path)
                .map_err(|e| ApiError::internal(format!("{}: {e}", file.name)))?
        };
        let loader = target.loader_with(&unsaved).map_err(ApiError::internal)?;
        let refresh = netsmith_pipeline::pick_lock::refresh_parts(
            &file.path,
            &file.name,
            &top,
            &loader,
            catalogs,
            &netsmith_pipeline::pick_lock::Parts::Only(vec![part.to_string()]),
        )
        .map_err(|e| {
            refused(format!(
                "with this value the board does not build, so nothing was written: {}",
                e.trim()
            ))
        })?;
        Ok(Some((
            refresh.file.clone(),
            netsmith_pipeline::pick_lock::render(&refresh.now),
        )))
    }
}

fn refused(message: impl Into<String>) -> ApiError {
    ApiError::unprocessable(message, "swap_refused", &[])
}

/// The swap of the part `refdes` of a built board.
fn find(data: &BoardData, catalogs: &Catalogs, refdes: &str) -> Result<Found, ApiError> {
    let built = &data.built;
    let index = built
        .design
        .parts
        .iter()
        .position(|p| p.refdes == refdes)
        .ok_or_else(|| ApiError::not_found(format!("`{}` has no part {refdes}", built.label())))?;
    let (_, unresolved) = resolve_generics(&built.sourcing, catalogs);
    let Some(unresolved) = unresolved.iter().find(|u| u.refdes == refdes) else {
        return Err(ApiError::unprocessable(
            format!(
                "{refdes} is not a generic part that no catalog row stands for, so it has nothing to swap"
            ),
            "not_unresolved",
            &[],
        ));
    };
    let generic = built.sourcing[index]
        .generic
        .as_ref()
        .ok_or_else(|| ApiError::internal(format!("{refdes} has no generic part")))?;
    let trace = &built.trace;
    let model = &trace.model;
    let inst = *trace
        .origins
        .get(index)
        .ok_or_else(|| ApiError::internal(format!("{refdes} has no instance")))?;
    let path = model.path_of(inst);
    let member = generic.value_column().map(str::to_string);
    let place = match &member {
        Some(m) => site(model, &trace.sources, inst, m),
        None => Err(format!(
            "a {} has no value for the swap to write: change the design or add a row to the `{}` catalog by hand",
            built.design.parts[index].kind, generic.catalog
        )),
    };
    let stated = member.as_ref().and_then(|m| match model.member(inst, m) {
        Some(Member::Field(f)) => match model.field(f).narrowed_at.as_slice() {
            [(src, span)] => Some(api_location(locate(&trace.sources, *src, *span))),
            _ => None,
        },
        _ => None,
    });
    let checks = built.rechecks.get(catalogs);
    let (site, refused) = match place {
        Ok(site) => (Some(site), None),
        Err(why) => (None, Some(why)),
    };
    let alternatives = alternatives(generic, catalogs)
        .into_iter()
        .map(|alt| {
            let check = checks
                .iter()
                .find(|c| c.refdes == refdes && c.alternative == alt.name);
            let value = alt.quantity.as_ref().map(value_text);
            let edit = site.as_ref().zip(value.as_ref()).map(|(site, value)| {
                let source = trace.sources.get(site.source);
                SwapEdit {
                    at: api_location(locate(
                        &trace.sources,
                        site.source,
                        Span::new(site.cut.start, site.cut.end),
                    )),
                    old: source.text[site.cut.start..site.cut.end].to_string(),
                    new: site.cut.text(value),
                }
            });
            SwapAlternative {
                check: match check.map(|c| &c.outcome) {
                    None => SwapCheck::NotChecked,
                    Some(Ok(())) => SwapCheck::Passes,
                    Some(Err(_)) => SwapCheck::Fails,
                },
                why: check.and_then(|c| c.outcome.clone().err()),
                name: alt.name,
                line: alt.line,
                lcsc: alt.lcsc,
                mpn: alt.mpn,
                value,
                edit,
            }
        })
        .collect();
    Ok(Found {
        swap: Swap {
            board: built.label(),
            refdes: refdes.to_string(),
            path,
            message: unresolved.message.clone(),
            member,
            stated,
            refused,
            alternatives,
        },
        site,
    })
}

/// Where the value of the part's `member` is written: over the right-hand side of the one
/// statement that states it, or into the block of the part's declaration when nothing does. The
/// error says why neither can change this part alone.
fn site(model: &Model, sources: &Sources, inst: InstId, member: &str) -> Result<Site, String> {
    let Some(Member::Field(field)) = model.member(inst, member) else {
        return Err(format!("the part has no member `{member}`"));
    };
    let f = model.field(field);
    let place = |src: SourceId, span: Span| {
        let at = locate(sources, src, span);
        format!("{}:{}", at.file, at.line)
    };
    match f.narrowed_at.as_slice() {
        [] => declaration_site(model, sources, inst, member),
        [(src, span)] => {
            let (src, span) = (*src, *span);
            if f.default_from == Some(DefaultFrom::Board) {
                return Err(format!(
                    "`.{member}` comes from the board's `defaults` ({}), which every part of the kind takes: state the value on the part by hand",
                    place(src, span)
                ));
            }
            let others = model
                .fields()
                .filter(|(id, other)| *id != field && other.narrowed_at.contains(&(src, span)))
                .count();
            if others > 0 {
                return Err(format!(
                    "`.{member}` is stated in a definition that {} other part{} share{} ({}): change it there, or state the value where the part is used, by hand",
                    others,
                    if others == 1 { "" } else { "s" },
                    if others == 1 { "s" } else { "" },
                    place(src, span)
                ));
            }
            statement_site(&sources.get(src).text, span, member)
                .map(|cut| Site { source: src, cut })
                .ok_or_else(|| {
                    format!(
                        "`.{member}` is stated as the swap cannot rewrite ({}): change it by hand",
                        place(src, span)
                    )
                })
        }
        many => Err(format!(
            "`.{member}` is stated in {} places ({}): change them by hand",
            many.len(),
            many.iter()
                .map(|(src, span)| place(*src, *span))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The right-hand side of `.member &= value` (or `=`, `?=`), when the statement at `span` reads
/// so.
fn statement_site(text: &str, span: Span, member: &str) -> Option<Cut> {
    let statement = text.get(span.start..span.end)?;
    let eq = statement.find('=')?;
    let lhs = match statement[..eq].chars().last() {
        Some('&' | '?') => &statement[..eq - 1],
        Some('<' | '>' | '!' | '=') => return None,
        _ if statement[eq + 1..].starts_with('=') => return None,
        _ => &statement[..eq],
    };
    let lhs = lhs.trim_end();
    if !(lhs.ends_with(&format!(".{member}")) && lhs.starts_with('.')) {
        return None;
    }
    let rhs = &statement[eq + 1..];
    let lead = rhs.len() - rhs.trim_start().len();
    let start = span.start + eq + 1 + lead;
    let end = span.start + statement.trim_end().len();
    (start < end).then(|| Cut {
        start,
        end,
        before: String::new(),
        after: String::new(),
    })
}

/// A new statement in the block of the part's declaration (`.r1: Resistor {}`), made when that
/// declaration is this part's alone.
fn declaration_site(
    model: &Model,
    sources: &Sources,
    inst: InstId,
    member: &str,
) -> Result<Site, String> {
    let Some((src, span)) = model.instance(inst).declared else {
        return Err(format!(
            "nothing states `.{member}`, and the part has no declaration of its own to state it in: state it by hand"
        ));
    };
    let shared = model
        .instances()
        .filter(|(id, i)| *id != inst && i.declared == Some((src, span)))
        .count();
    if shared > 0 {
        let at = locate(sources, src, span);
        return Err(format!(
            "nothing states `.{member}`, and the part is declared in a definition that {shared} other part{} share{} ({}:{}): state it there, or where the part is used, by hand",
            if shared == 1 { "" } else { "s" },
            if shared == 1 { "s" } else { "" },
            at.file,
            at.line
        ));
    }
    let text = &sources.get(src).text;
    block_site(text, span.end, member)
        .map(|cut| Site { source: src, cut })
        .ok_or_else(|| {
        let at = locate(sources, src, span);
        format!(
            "nothing states `.{member}`, and the swap cannot add it to the declaration ({}:{}): state it by hand",
            at.file, at.line
        )
    })
}

/// Where a statement of `member` goes after the declaration that ends at `end`: into its block,
/// or a new block when it has none.
fn block_site(text: &str, end: usize, member: &str) -> Option<Cut> {
    let rest = text.get(end..)?;
    let gap = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let statement = format!(".{member} &= ");
    if !rest[gap..].starts_with('{') {
        return Some(Cut {
            start: end,
            end,
            before: format!(" {{ {statement}"),
            after: " }".to_string(),
        });
    }
    let open = end + gap;
    let close = open + matching_brace(&text[open..])?;
    let inside = &text[open + 1..close];
    if inside.trim().is_empty() {
        return Some(Cut {
            start: open,
            end: close + 1,
            before: format!("{{ {statement}"),
            after: " }".to_string(),
        });
    }
    if !inside.contains('\n') {
        return None;
    }
    // A block of lines: the new line goes after the last one, indented as the first is.
    let indent: String = inside
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| *c == ' ' || *c == '\t').collect())
        .unwrap_or_default();
    let last = open + 1 + inside.trim_end().len();
    Some(Cut {
        start: last,
        end: last,
        before: format!("\n{indent}{statement}"),
        after: String::new(),
    })
}

/// The offset of the `}` that closes the `{` that `text` starts with, passing over strings and
/// `//` comments.
fn matching_brace(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => {
                for (_, d) in chars.by_ref() {
                    if d == '"' {
                        break;
                    }
                }
            }
            '/' if chars.peek().is_some_and(|(_, d)| *d == '/') => {
                for (_, d) in chars.by_ref() {
                    if d == '\n' {
                        break;
                    }
                }
            }
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// A row's value as the source writes it: `240ohm +/- 1%`, `4.7uF +/- 10%`, `10uH` for a row
/// with no tolerance.
pub fn value_text(q: &Quantity) -> String {
    let mid = q.mid();
    let point = Quantity::point(q.dim(), mid)
        .map(|p| p.to_string())
        .unwrap_or_else(|_| q.to_string())
        .replace('Ω', "ohm");
    if q.is_point() || mid <= 0.0 {
        return point;
    }
    let percent = (q.hi() - q.lo()) / (q.hi() + q.lo()) * 100.0;
    let percent = format!("{percent:.3}");
    let percent = percent.trim_end_matches('0').trim_end_matches('.');
    format!("{point} +/- {percent}%")
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_quantity::Dimension;

    fn tolerance(nominal: f64, percent: f64, dim: Dimension) -> Quantity {
        let d = nominal * percent / 100.0;
        Quantity::interval(dim, nominal - d, nominal + d).unwrap()
    }

    #[test]
    fn a_value_is_written_as_the_source_writes_it() {
        assert_eq!(
            value_text(&tolerance(240.0, 1.0, Dimension::RESISTANCE)),
            "240ohm +/- 1%"
        );
        assert_eq!(
            value_text(&tolerance(2200.0, 5.0, Dimension::RESISTANCE)),
            "2.2kohm +/- 5%"
        );
        assert_eq!(
            value_text(&tolerance(4.7e-6, 10.0, Dimension::CAPACITANCE)),
            "4.7uF +/- 10%"
        );
        assert_eq!(
            value_text(&Quantity::point(Dimension::INDUCTANCE, 1e-5).unwrap()),
            "10uH"
        );
    }

    fn one(text: &str, span: (usize, usize)) -> Option<(String, String)> {
        statement_site(text, Span::new(span.0, span.1), "resistance")
            .map(|s| (text[s.start..s.end].to_string(), s.text("X")))
    }

    #[test]
    fn the_right_hand_side_of_a_statement_is_what_is_replaced() {
        let text = ".resistance &= 249ohm +/- 1%";
        assert_eq!(
            one(text, (0, text.len())),
            Some(("249ohm +/- 1%".into(), "X".into()))
        );
        let text = ".r1.resistance = 249ohm";
        assert_eq!(
            one(text, (0, text.len())),
            Some(("249ohm".into(), "X".into()))
        );
        let text = ".resistance ?= 1kohm";
        assert_eq!(
            one(text, (0, text.len())),
            Some(("1kohm".into(), "X".into()))
        );
        for refused in [
            ".resistance >= 1kohm",
            ".resistance <= 1kohm",
            ".max_power &= 1W",
        ] {
            assert_eq!(one(refused, (0, refused.len())), None, "{refused}");
        }
    }

    fn insert(text: &str, end: usize) -> Option<String> {
        block_site(text, end, "resistance")
            .map(|s| format!("{}{}{}", &text[..s.start], s.text("X"), &text[s.end..]))
    }

    #[test]
    fn a_statement_goes_into_the_block_of_the_declaration() {
        let decl = ".r1: Resistor";
        assert_eq!(
            insert(".r1: Resistor {}\n", decl.len()).as_deref(),
            Some(".r1: Resistor { .resistance &= X }\n")
        );
        assert_eq!(
            insert(".r1: Resistor\n.r2: Resistor\n", decl.len()).as_deref(),
            Some(".r1: Resistor { .resistance &= X }\n.r2: Resistor\n")
        );
        assert_eq!(
            insert(
                ".r1: Resistor {\n    .package &= \"0603\" // {\n}\n",
                decl.len()
            )
            .as_deref(),
            Some(".r1: Resistor {\n    .package &= \"0603\" // {\n    .resistance &= X\n}\n")
        );
        assert_eq!(
            insert(".r1: Resistor { .package &= \"0603\" }", decl.len()),
            None
        );
    }
}
