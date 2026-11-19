//! The `keep` statement of a board's physical file (issue 1277):
//! `keep 10mm between BuckIc and Crystal`.
//!
//! **Decision** (owner, issue 1277). Where a datasheet gives no number, the distance between
//! kinds of parts (a radio module and a switching regulator, a crystal and a buck converter) is a
//! rule of thumb of the project. It is a requirement of the physical file, checked as a
//! requirement on where parts go and kept by the placer, and the kinds are the definitions of the
//! standard library and the project: a part or a generic, as a line of `defaults` names one.
//!
//! **Readings** (the implementer's). The statement begins with the word `keep` followed by the
//! distance, so `keep` stays usable as a name. The distance is one fixed length of zero or more,
//! as a `soft margin`'s is; it is measured from courtyard to courtyard in the plane of the board,
//! whatever faces the parts are on (a regulator under a radio is no further from it). A kind
//! reaches every part made from it or providing it, at any depth of the board; a module is not a
//! kind here (name the kind of its part). The two kinds may be the same: two radios kept apart.
//! A pair of kinds is stated once. A statement that reaches no pair of parts is a note, as a line
//! of `defaults` that reaches nothing is. Only a board's physical file states one, outside its
//! `case { }`.

use netsmith_lang::ast::{DefKind, Expr, KeepApart};

use crate::checker::Checker;
use crate::lower::VTy;
use crate::model::{DefaultNote, InstId, InstKind, KeepApart as Stated};
use crate::polygon::{NotFixed, fixed};
use netsmith_quantity::Dimension;

const EXAMPLE: &str = "`keep 10mm between BuckIc and Crystal`";

impl Checker<'_> {
    /// Keep a `keep` statement on the model, its parts found when the board has been read
    /// ([`Checker::resolve_keep_apart`]). Every refusal names the statement and what to write.
    pub(crate) fn keep_apart(&mut self, k: &KeepApart) {
        let text = k.span_text();
        let refused = match &self.phys {
            None => Some(format!(
                "`{text}` belongs in a board's physical file (`physical for Board {{ {} }}`, ADR 0048); the circuit source holds no placement",
                &EXAMPLE[1..EXAMPLE.len() - 1]
            )),
            Some(p) if p.module.is_some() => Some(format!(
                "a module's own physical file states how its parts stand to one another, and `{text}` is a rule of the whole board: state it in the board's physical file"
            )),
            Some(p) if p.in_case => Some(format!(
                "`{text}` is not part of the case: put it at the top of the board's physical file"
            )),
            Some(_) => None,
        };
        if let Some(message) = refused {
            self.error(k.span, message);
            return;
        }
        let Some(distance) = self.fixed_length(&k.distance, &text, "distance", "10mm") else {
            return;
        };
        let mut kinds = Vec::new();
        for kind in &k.kinds {
            let Some(def) = self.default_kind(kind, "`keep` names a part or a generic") else {
                return;
            };
            if self.modules[def.0].defs[def.1].kind == DefKind::Module {
                self.error(
                    kind.span,
                    format!(
                        "`{}` is a module; `{text}` keeps parts apart, so name the kind of the part inside it that must keep its distance (a part or a generic)",
                        kind.name
                    ),
                );
                return;
            }
            kinds.push(def);
        }
        let defs = [kinds[0], kinds[1]];
        let board = self.current();
        let same = |a: &[(usize, usize); 2]| *a == defs || (a[0] == defs[1] && a[1] == defs[0]);
        if let Some(first) = self
            .model
            .keep_aparts
            .iter()
            .find(|s| s.board == board && same(&s.defs))
        {
            let place = self.place_of(first.source, first.span);
            self.error(
                k.span,
                format!(
                    "`{}` and `{}` already keep a distance, at {place}: a pair of kinds keeps one, so state the larger",
                    k.kinds[0].name, k.kinds[1].name
                ),
            );
            return;
        }
        let source = self.cur_source();
        self.model.keep_aparts.push(Stated {
            text,
            kinds: [k.kinds[0].name.clone(), k.kinds[1].name.clone()],
            defs,
            distance,
            parts: [Vec::new(), Vec::new()],
            board,
            source,
            span: k.span,
        });
    }

    /// Find the parts of each `keep` statement's two kinds on its board, when every statement of
    /// the board has been read; a statement that reaches no pair is a note.
    pub(crate) fn resolve_keep_apart(&mut self) {
        if self.model.keep_aparts.is_empty() {
            return;
        }
        let parts: Vec<(InstId, (usize, usize))> = self
            .model
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Part)
            .filter_map(|(id, i)| i.def.map(|d| (id, d)))
            .collect();
        let mut kinds_of: std::collections::HashMap<(usize, usize), Vec<(usize, usize)>> =
            std::collections::HashMap::new();
        let mut stated = std::mem::take(&mut self.model.keep_aparts);
        for s in &mut stated {
            for &(inst, def) in &parts {
                if !self.model.is_inside(inst, s.board) {
                    continue;
                }
                let kinds = kinds_of.entry(def).or_insert_with(|| {
                    let d = self.modules[def.0].defs[def.1].clone();
                    let saved = self.enter_def(def);
                    let provided = self.provided(&d);
                    self.leave_def(saved);
                    std::iter::once(def)
                        .chain(provided.into_iter().map(|(_, r)| r))
                        .collect()
                });
                for side in 0..2 {
                    if kinds.contains(&s.defs[side]) {
                        s.parts[side].push(inst);
                    }
                }
            }
            let pairs = s.parts[0].iter().any(|a| s.parts[1].iter().any(|b| a != b));
            if !pairs {
                let missing = if s.parts[0].is_empty() {
                    format!("the board has no `{}`", s.kinds[0])
                } else if s.parts[1].is_empty() {
                    format!("the board has no `{}`", s.kinds[1])
                } else {
                    format!("the board has one `{}` only", s.kinds[0])
                };
                self.model.default_notes.push(DefaultNote {
                    board: s.board,
                    source: s.source,
                    span: s.span,
                    message: format!("`{}` keeps no two parts apart: {missing}", s.text),
                });
            }
        }
        self.model.keep_aparts = stated;
    }

    /// The value of `e` in mm, one fixed length of zero or more, or `None` after reporting why
    /// not: the distance of a `keep` or a `soft margin` (`noun`, with an `example` length).
    pub(crate) fn fixed_length(
        &mut self,
        e: &Expr,
        text: &str,
        noun: &str,
        example: &str,
    ) -> Option<f64> {
        let shown = e.to_source();
        let why = match self.lower(e) {
            None => NotFixed::Reported,
            Some(t) if t.ty != VTy::Q(Dimension::LENGTH) => NotFixed::Kind,
            Some(t) => match fixed(&t.e) {
                Ok(q) if q.hi() - q.lo() < 1e-12 => {
                    let mm = (q.mid() * 1e12).round() / 1e9;
                    if mm < 0.0 {
                        self.error(
                            e.span(),
                            format!("`{text}`: a {noun} is zero or more, and `{shown}` is {mm} mm"),
                        );
                        return None;
                    }
                    return Some(mm);
                }
                Ok(_) => NotFixed::Relation,
                Err(why) => why,
            },
        };
        let message = match why {
            NotFixed::Reported => return None,
            NotFixed::Kind => {
                format!(
                    "`{text}`: the {noun} is a length such as `{example}`, and `{shown}` is not"
                )
            }
            _ => format!(
                "`{text}`: the {noun} is one fixed length such as `{example}`, and `{shown}` is a range or reads another value"
            ),
        };
        self.error(e.span(), message);
        None
    }
}
