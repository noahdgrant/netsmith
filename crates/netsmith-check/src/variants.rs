//! The order codes of a part and the features a design needs of it (issue 1311).
//!
//! **Decision** (owner, issue 1311). A part that is sold as several order codes with different
//! features (a module with and without a GNSS receiver) states them in a `.variants` table, an
//! order code and its features to a row. A board, or a module, states what it needs of the part
//! with `.u.needs &= GNSS`. The bill of materials names the order code, and a code without a
//! needed feature is refused.
//!
//! **Readings** (the implementer's). A feature is a plain name that the table writes after a
//! code; it is the part's own, so `.needs` takes it bare with no import, as a pin's `.function`
//! takes a name of its pin table. Each `.needs &= F` narrows the codes to those that have `F`,
//! so two statements need both features. The `.needs` member is a part's only when it states
//! `.variants`, so no part that declared a `.needs` of its own before is changed. Once a board's
//! statements are read, the part's `.mpn` is the one code left: a code the design names with
//! `.mpn &=` must be in the table and have every need; with none named, the design must leave
//! exactly one code, and the message lists them when it leaves several, as a keyed catalog does
//! (`12-catalogs-and-how-the-solver-picks-parts.md`). The `.mpn` it narrows is a hand pick (ADR
//! 0122) for a part with a catalog keyed by `.mpn`, and the bill of materials' number for a part
//! with none.

use netsmith_lang::ast::{self, CmpOp, Expr, Variants};

use crate::checker::Checker;
use crate::lower::{closest, list_of};
use crate::model::{
    Domain, FieldId, FieldKind, InstId, InstKind, Member, Need, OrderCode, PartVariants,
};
use netsmith_lang::ast::DefKind;

/// An example of the statement, for the messages.
const NEEDS_EXAMPLE: &str = "`.u.needs &= GNSS`";

impl Checker<'_> {
    /// Read a part's `.variants` table in the first pass: keep its rows on the model and give the
    /// part its `.needs`. Every refusal names the row or the statement.
    pub(crate) fn declare_variants(&mut self, v: &Variants) {
        if self.phys.is_some() || !self.in_kind(DefKind::Part) {
            self.error(
                v.span,
                "`.variants` lists the order codes of a part, in the part's own definition; a board or a module states what it needs of the part with `.u.needs &= FEATURE`",
            );
            return;
        }
        let part = self.current();
        if self.model.instance(part).kind != InstKind::Part {
            return;
        }
        if self.model.variants.contains_key(&part) {
            self.error(
                v.span,
                "a part has one `.variants` table; put every order code in it",
            );
            return;
        }
        let mut codes: Vec<OrderCode> = Vec::new();
        for row in &v.rows {
            if codes.iter().any(|c| c.code == row.code) {
                self.error(
                    row.code_span,
                    format!(
                        "the order code `{}` is in this `.variants` table twice; write each code once, with all its features",
                        row.code
                    ),
                );
                continue;
            }
            let mut features: Vec<String> = Vec::new();
            for f in &row.features {
                if features.contains(&f.name) {
                    self.error(
                        f.span,
                        format!(
                            "`{}` is written twice for the order code `{}`",
                            f.name, row.code
                        ),
                    );
                    continue;
                }
                features.push(f.name.clone());
            }
            codes.push(OrderCode {
                code: row.code.clone(),
                features,
            });
        }
        if self.model.member(part, "needs").is_some() {
            self.error(
                v.span,
                format!(
                    "a part that states `.variants` has `.needs`, what a design needs of its order code ({NEEDS_EXAMPLE}); do not declare a `.needs` of its own"
                ),
            );
        } else {
            let field = self.model.add_field(part, "needs", FieldKind::Text);
            self.model.add_member(part, "needs", Member::Field(field));
        }
        let at = (self.cur_source(), v.span);
        self.model.variants.insert(
            part,
            PartVariants {
                codes,
                needs: Vec::new(),
                at,
            },
        );
    }

    /// Whether `field` is the `.needs` of a part that states `.variants`.
    pub(crate) fn is_needs(&self, field: FieldId) -> bool {
        let f = self.model.field(field);
        f.name == "needs" && self.model.variants.contains_key(&f.owner)
    }

    /// `.u.needs &= GNSS`: the design needs the feature of the part's order code. Only `&=`
    /// states a need, and the name is one of the features of the part's table.
    pub(crate) fn need_statement(&mut self, field: FieldId, c: &ast::Constraint) {
        let part = self.model.field(field).owner;
        let path = self.model.path_of(part);
        let ty = self.model.instance(part).type_name.clone();
        let Some(features) = self.model.variants.get(&part).map(PartVariants::features) else {
            return;
        };
        let known = if features.is_empty() {
            format!("`{ty}` names no feature in its `.variants`")
        } else {
            let names: Vec<String> = features.iter().map(|f| format!("`{f}`")).collect();
            format!("the features of `{ty}` are {}", list_of(&names))
        };
        if c.op != CmpOp::Narrow {
            self.error(
                c.span,
                format!(
                    "`{}` does not state a need: a design states what it needs of `{path}` with `&=` ({NEEDS_EXAMPLE}), and each statement adds one",
                    c.op.symbol()
                ),
            );
            return;
        }
        let name = match &c.rhs {
            Expr::Path(p) if !p.leading_dot && p.segments.len() == 1 => &p.segments[0],
            other => {
                self.error(
                    other.span(),
                    format!(
                        "`.needs` takes the name of a feature, written bare ({NEEDS_EXAMPLE}); {known}"
                    ),
                );
                return;
            }
        };
        if !features.contains(&name.name) {
            let hint = closest(&name.name, &features)
                .map(|f| format!("; did you mean `{f}`?"))
                .unwrap_or_default();
            self.error(
                name.span,
                format!(
                    "`{}` is not a feature of `{ty}` (`{path}`); {known}{hint}",
                    name.name
                ),
            );
            return;
        }
        let at = (self.cur_source(), c.span);
        if let Some(v) = self.model.variants.get_mut(&part) {
            v.needs.push(Need {
                feature: name.name.clone(),
                at,
            });
        }
    }

    /// Once every statement of a board is read: narrow each `.mpn` of a part that states
    /// `.variants` to the one order code that has every feature the design needs, and refuse a
    /// code that the design names and that lacks one.
    pub(crate) fn resolve_variants(&mut self) {
        let parts: Vec<InstId> = self.model.variants.keys().copied().collect();
        for part in parts {
            self.resolve_part_variants(part);
        }
    }

    fn resolve_part_variants(&mut self, part: InstId) {
        let Some(v) = self.model.variants.get(&part).cloned() else {
            return;
        };
        let Some(Member::Field(mpn)) = self.model.member(part, "mpn") else {
            return;
        };
        let path = self.model.path_of(part);
        let ty = self.model.instance(part).type_name.clone();
        let needed = v.needed();
        let fitting: Vec<&OrderCode> = v.codes.iter().filter(|c| c.has_all(&needed)).collect();
        let named = match &self.model.field(mpn).domain {
            Domain::Text(code) => Some(code.clone()),
            _ => None,
        };
        let mpn_at = self.model.field(mpn).narrowed_at.last().copied();
        if let Some(code) = named {
            let at = mpn_at.unwrap_or_else(|| self.model.instance(part).at());
            let Some(row) = v.code(&code) else {
                let codes: Vec<String> = v.codes.iter().map(|c| format!("`{}`", c.code)).collect();
                self.report(
                    at.0,
                    at.1,
                    format!(
                        "`{path}.mpn` is `{code}`, which is not an order code of `{ty}`; its order codes are {}",
                        list_of(&codes)
                    ),
                );
                return;
            };
            let missing: Vec<String> = needed
                .iter()
                .filter(|n| !row.features.contains(n))
                .cloned()
                .collect();
            if missing.is_empty() {
                return;
            }
            let why: Vec<String> = missing
                .iter()
                .map(|m| {
                    let need = v
                        .needs
                        .iter()
                        .find(|n| &n.feature == m)
                        .expect("a missing feature was needed");
                    let (file, line) = self.line_in(need.at.0, need.at.1);
                    format!("`{m}` ({file}:{line})")
                })
                .collect();
            let instead = if fitting.is_empty() {
                format!("no order code of `{ty}` has them all")
            } else {
                let codes: Vec<String> = fitting.iter().map(|c| format!("`{}`", c.code)).collect();
                format!("the order codes that have them are {}", list_of(&codes))
            };
            self.report(
                at.0,
                at.1,
                format!(
                    "`{path}.mpn` is `{code}`, which has no {}, and the design needs {}; {instead}",
                    list_of(&missing.iter().map(|m| format!("`{m}`")).collect::<Vec<_>>()),
                    list_of(&why)
                ),
            );
            return;
        }
        match fitting.as_slice() {
            [one] => {
                // The code is the design's statement: the last need, or the table when the part
                // has one code.
                let at = v.needs.last().map_or(v.at, |n| n.at);
                self.model.set_domain(mpn, Domain::Text(one.code.clone()));
                self.model.fields[mpn.0].narrowed_at.push(at);
            }
            [] => {
                let at = v
                    .needs
                    .last()
                    .map_or_else(|| self.model.instance(part).at(), |n| n.at);
                let needs: Vec<String> = needed.iter().map(|n| format!("`{n}`")).collect();
                let has: Vec<String> = v.codes.iter().map(describe).collect();
                self.report(
                    at.0,
                    at.1,
                    format!(
                        "no order code of `{ty}` has {} together, which the design needs of `{path}`: {}",
                        list_of(&needs),
                        list_of(&has)
                    ),
                );
            }
            several => {
                let at = self.model.instance(part).at();
                let has: Vec<String> = several.iter().map(|c| describe(c)).collect();
                let example = several
                    .iter()
                    .flat_map(|c| c.features.iter())
                    .find(|f| !needed.contains(f))
                    .map(|f| format!("`.{path}.needs &= {f}`, "))
                    .unwrap_or_default();
                self.report(
                    at.0,
                    at.1,
                    format!(
                        "`{path}` is a `{ty}`, which is sold as {}; state what the design needs of it ({example}one statement for each feature) or name the order code (`.{path}.mpn &= \"{}\"`)",
                        list_of(&has),
                        several[0].code
                    ),
                );
            }
        }
    }
}

/// `` `MOD-100-G` (GNSS, LTE) `` or `` `MOD-100` (no feature) ``.
fn describe(code: &OrderCode) -> String {
    if code.features.is_empty() {
        format!("`{}` (no feature)", code.code)
    } else {
        format!("`{}` ({})", code.code, code.features.join(", "))
    }
}
