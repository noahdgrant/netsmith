//! [`Rule::Redundant`] for an attribute (ADR 0125, issue #1132): `#[kind(...)]`, `#[value(...)]`,
//! `#[series(...)]` or `#[assembly(...)]` on a `part` or a `generic` that says what the part gets
//! without it.
//!
//! A part has the attributes of its own definition and then of the generics it provides, its own
//! first, so an attribute is judged where it is read first: on the definition a part of a board
//! is made from. It is a candidate when a part of some board is made from it, and the linter
//! checks the project again without it. It is redundant when every board, system and panel that
//! reads its file then checks with the same values, and every part has the same attributes as
//! before: a generic the part provides says the same (`#[kind(connector)]` on a part that
//! provides `Connector`).
//!
//! `#[assembly(...)]` is read last of all that says who assembles a part (ADR 0024): the design's
//! `.assembly`, then the row's `assembly` cell, then the manifest's `assembly`, then the
//! attribute, then `fab`. So the parts whose attribute changes when it is taken out are judged
//! one by one, and each must get the same word from what is read in its place: the manifest's
//! `assembly`, a generic it provides, `fab` when nothing says, or the row its key names (a
//! row the solver picks is not known to the linter, and rows differ by pick). A part whose design
//! states `.assembly`, or whose row says another word, never reads the attribute and is left
//! out; at least one part must get the word from elsewhere.
//!
//! Not judged: `#[series(...)]` against the project's `series` (the attribute offers series and
//! the project picks one of them; a definition with no series gets no made values), `#[bank]`
//! (it names the definition that carries it) and `#[role(...)]` on a member (nothing else gives
//! a role). An attribute of a generic that only its providers read is not judged either: there it
//! is what the providers inherit.

use std::collections::BTreeSet;

use netsmith_check::{Domain, Instance, Member, Model};
use netsmith_lang::ast::{AttrArg, Attribute, DefKind, Definition};

use super::{keyed_row, models, recheck, same_values};
use crate::{Context, Lint, Rule, SourceFile};

/// The attributes this half of the rule judges.
const JUDGED: [&str; 4] = ["kind", "value", "series", "assembly"];

pub(super) fn lint(cx: &Context) -> Vec<Lint> {
    let mut out = Vec::new();
    for file in cx.files.values() {
        for def in &file.ast.definitions {
            if !matches!(def.kind, DefKind::Generic | DefKind::Part) {
                continue;
            }
            for attr in &def.attrs {
                if !JUDGED.contains(&attr.name.name.as_str()) {
                    continue;
                }
                if let Some(from) = gives_the_same(cx, file, def, attr) {
                    let stated = super::statement(file, attr.span);
                    out.push(file.lint(
                        Rule::Redundant,
                        attr.span,
                        format!("`{stated}` says what {from} already gives; delete it"),
                    ));
                }
            }
        }
    }
    out
}

/// Whether a part of some board is made from `def` of `file`: the definition it reads its
/// attributes from first.
fn made_from(cx: &Context, file: &SourceFile, def: &Definition) -> bool {
    cx.models().any(|(checked, model)| {
        model.instances().any(|(_, inst)| {
            inst.type_name == def.name.name && checked.sources.get(inst.source).name == file.name
        })
    })
}

/// What gives every part that reads the attribute the same, in words, when taking it out of the
/// project changes nothing.
fn gives_the_same(
    cx: &Context,
    file: &SourceFile,
    def: &Definition,
    attr: &Attribute,
) -> Option<String> {
    if !made_from(cx, file, def) {
        return None;
    }
    let rechecked = recheck(cx, file, attr.span)?;
    if !rechecked
        .iter()
        .all(|(before, after)| crate::checks(after) && same_values(before, after))
    {
        return None;
    }
    let assembly = attr.name.name == "assembly";
    let word = match attr.args.as_slice() {
        [AttrArg::Name(i)] if assembly => Some(i.name.as_str()),
        _ => None,
    };
    let mut givers: BTreeSet<String> = BTreeSet::new();
    let mut provided: BTreeSet<String> = BTreeSet::new();
    let mut from_generic = false;
    for (before, after) in &rechecked {
        let (ms_before, ms_after) = (models(before), models(after));
        if ms_before.len() != ms_after.len() {
            return None;
        }
        for (m_before, m_after) in ms_before.into_iter().zip(ms_after) {
            let (a, b): (Vec<_>, Vec<_>) = (
                m_before.instances().collect(),
                m_after.instances().collect(),
            );
            if a.len() != b.len() {
                return None;
            }
            for ((part, x), (_, y)) in a.into_iter().zip(b) {
                if x.type_name != y.type_name {
                    return None;
                }
                let mut xm = x.marks.clone();
                let mut ym = y.marks.clone();
                if assembly {
                    xm.assembly = None;
                    ym.assembly = None;
                }
                if xm != ym {
                    return None;
                }
                if x.marks.assembly == y.marks.assembly {
                    // A part that reads the attribute first and has the same attributes without
                    // it: a generic it provides says the same.
                    if x.type_name == def.name.name
                        && before.sources.get(x.source).name == file.name
                    {
                        from_generic = true;
                        provided.extend(x.provides.iter().cloned());
                    }
                    continue;
                }
                // The part read `#[assembly(word)]`, and reads something else without it.
                let word = word?;
                match assembly_without(cx, m_before, part, y, word) {
                    Giver::Gives(from) => {
                        givers.insert(from);
                    }
                    Giver::Unread => {}
                    Giver::Differs => return None,
                }
            }
        }
    }
    if givers.is_empty() && !from_generic {
        // Every part made from the definition states `.assembly` or has a row that says
        // another word: the attribute is never read, which is not what this rule finds.
        return None;
    }
    if from_generic {
        givers.insert(match provided.len() {
            0 => "a generic it provides".to_string(),
            1 => format!("`{}`, the generic it provides,", provided.iter().next()?),
            _ => format!(
                "a generic it provides ({})",
                provided
                    .iter()
                    .map(|p| format!("`{p}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    Some(givers.into_iter().collect::<Vec<_>>().join(" and "))
}

/// What a part reads in place of its `#[assembly(word)]`.
enum Giver {
    /// The same word, from this.
    Gives(String),
    /// The part never read the attribute: its design states `.assembly`, or the row its key
    /// names has an `assembly` cell of another word.
    Unread,
    /// Another word, or one the linter cannot know (a row the solver picks).
    Differs,
}

/// What `part` of `model` gets as its assembly without the attribute, `after` being the part
/// when the project is checked again without it.
fn assembly_without(
    cx: &Context,
    model: &Model,
    part: netsmith_check::InstId,
    after: &Instance,
    word: &str,
) -> Giver {
    // The design's `.assembly`, read before all else.
    if let Some(Member::Field(f)) = model.member(part, "assembly")
        && matches!(model.field(f).domain, Domain::Text(_))
    {
        return Giver::Unread;
    }
    let catalogs = cx.project.catalogs;
    // The row the part's key names, read next.
    let keyed = catalogs.and_then(|c| keyed_row(c, model, part));
    if let Some(keyed) = &keyed
        && let Some(cell) = &keyed.row.assembly
    {
        return if cell == word {
            Giver::Gives(format!(
                "the `assembly` cell of the row that `.{}` names (`{}`)",
                keyed.key_column, keyed.wanted
            ))
        } else {
            Giver::Unread
        };
    }
    // A part picked by the solver may get a row with a cell of another word; it reads what
    // follows only when its row has none, so what follows must give the same.
    let fallback = match catalogs.and_then(|c| c.default_assembly()) {
        Some(manifest) => (
            manifest.to_string(),
            format!("the manifest's `assembly = \"{manifest}\"`"),
        ),
        None => match &after.marks.assembly {
            Some(next) => (
                next.clone(),
                match after.provides.as_slice() {
                    [one] => format!("`{one}`, the generic it provides,"),
                    _ => "a generic it provides".to_string(),
                },
            ),
            None => (
                "fab".to_string(),
                "`fab`, who assembles a part when nothing says,".to_string(),
            ),
        },
    };
    if fallback.0 == word {
        Giver::Gives(fallback.1)
    } else {
        Giver::Differs
    }
}
