//! The `layout` block of a part (ADR 0030, issues #829 and #994): what the part's datasheet says
//! about where the parts around it go, with the document that says it.
//!
//! ```text
//! part Lmr16006y provides BuckIc {
//!     ...
//!     layout {
//!         source "SNVSA24, 11.1 Layout Guidelines, p.17"
//!         /// "The input bypass capacitor Cin must be placed close to the VIN pin."
//!         near input_cap bridging .vin.pos .vin.neg to .vin.pos
//!     }
//! }
//! ```
//!
//! A statement names the part's own pins; the board's parts it is about are found by the nets
//! when the board is placed (`netsmith_pcb::part_layout`), so the part names no other part and
//! no net. The `source` is mandatory: a guideline is knowledge of a part, written once with the
//! document it comes from (ADR 0030, the second of its three places). Only a part's own
//! definition holds a block, as only it holds a `support` section: a generic picks parts from
//! a catalog, whose datasheets differ. `layout "SOT-223" { }` holds the guidelines of one of the
//! part's packages; the design keeps them only for an instance in that package.
//!
//! A catalog row of a standard generic has no definition to hold a block, so the standard
//! library's `std/layouts.nsm` has one `layout "MPN" for Generic { }` block for each row that
//! has guidelines (issue #1032), keyed by the row's manufacturer's part number. Every instance
//! of the generic is given them, as guidelines with that number (`LayoutGuideline::mpn`), and
//! the design keeps the ones of the row the solver picked.
//!
//! The statements are `near`, `away`, `loop`, `keepout`, `edge`, `vias`, `route` and `kelvin` (see
//! [`netsmith_lang::ast::Rule`]). Every number is one value of its dimension: a length above
//! zero (a corner of a keepout may be any length), and a count of vias a whole number of one
//! or more.

use netsmith_core::PartKind;
use netsmith_lang::ast::{Definition, Expr, Guideline, Layout, LayoutStatement, Rule, Select};
use netsmith_quantity::Dimension;

use crate::checker::{Checker, LAYOUTS_MODULE};
use crate::lower::VTy;
use crate::model::{GuidelineRule, InstId, LayoutGuideline, NearSelect, RExpr};

/// What a keepout of a `layout` block may forbid, in the order of the `Keepout` interface.
const FORBIDS: [&str; 4] = ["tracks", "vias", "copper", "parts"];

impl Checker<'_> {
    /// The blocks of `std/layouts.nsm` for the standard generic `def`, read as statements of
    /// the instance `inst` of it: each becomes guidelines that the design keeps only when the
    /// solver picks the row with the block's manufacturer's part number.
    pub(crate) fn keyed_layouts(&mut self, inst: InstId, def: &Definition) {
        let path: Vec<String> = LAYOUTS_MODULE.split("::").map(str::to_string).collect();
        let Some(module) = self.load_module(&path) else {
            return;
        };
        let blocks = self.modules[module].layouts.clone();
        let saved_module = std::mem::replace(&mut self.cur_module, module);
        let saved_scope = std::mem::replace(&mut self.scope, vec![inst]);
        for block in &blocks {
            let Some(keyed) = &block.keyed else { continue };
            if keyed.generic.name == def.name.name {
                self.layout(block);
            }
        }
        self.scope = saved_scope;
        self.cur_module = saved_module;
    }

    /// A `layout` block: its guidelines become [`LayoutGuideline`]s of the part instance.
    pub(crate) fn layout(&mut self, block: &Layout) {
        if block.keyed.is_none() && !self.support_allowed() {
            let what = if self.in_kind(netsmith_lang::ast::DefKind::Generic) {
                "a generic, whose catalog parts each have their own datasheet"
            } else {
                "not a part's own definition"
            };
            self.error(
                block.span,
                format!(
                    "`layout` can only appear directly inside a part, for the guidelines of its datasheet, and this is {what}"
                ),
            );
            return;
        }
        let sources: Vec<(&str, netsmith_lang::Span)> = block
            .statements
            .iter()
            .filter_map(|s| match s {
                LayoutStatement::Source { text, span } => Some((text.as_str(), *span)),
                LayoutStatement::Guideline(_) => None,
            })
            .collect();
        let source = match sources.as_slice() {
            [] => {
                self.error(
                    block.span,
                    "this `layout` block states no source: add `source \"...\"` with the document and the section that give its guidelines, such as `source \"SNVSA24, 11.1 Layout Guidelines, p.17\"`",
                );
                return;
            }
            [(text, span)] if text.trim().is_empty() => {
                self.error(
                    *span,
                    "the `source` of a `layout` block is empty: name the document and the section that give its guidelines",
                );
                return;
            }
            [(text, _)] => text.to_string(),
            [_, (_, second), ..] => {
                self.error(
                    *second,
                    "a `layout` block has one `source`; put the guidelines of another document in a `layout` block of their own",
                );
                return;
            }
        };
        let package = match &block.package {
            None => None,
            Some((package, span)) => {
                let part = self.current();
                let packages: Vec<String> = self
                    .model
                    .instance(part)
                    .tables
                    .iter()
                    .filter_map(|t| t.package.clone())
                    .collect();
                if package.trim().is_empty() {
                    self.error(
                        *span,
                        "the package of `layout \"...\"` is empty: name the package whose guidelines the block holds, or leave it out for every package",
                    );
                    return;
                }
                if !packages.is_empty() && !packages.iter().any(|p| p == package) {
                    let message = format!(
                        "the part has no pin table for the package \"{package}\"; its packages are {}",
                        packages
                            .iter()
                            .map(|p| format!("\"{p}\""))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    self.error(*span, message);
                    return;
                }
                Some(package.clone())
            }
        };
        let mpn = block.keyed.as_ref().map(|k| k.mpn.0.as_str());
        if let Some(keyed) = &block.keyed
            && keyed.mpn.0.trim().is_empty()
        {
            self.error(
                keyed.mpn.1,
                "the manufacturer's part number of `layout \"...\" for` is empty: name the catalog row whose guidelines the block holds",
            );
            return;
        }
        for statement in &block.statements {
            if let LayoutStatement::Guideline(g) = statement {
                self.guideline(g, &source, package.as_deref(), mpn);
            }
        }
    }

    fn guideline(&mut self, g: &Guideline, source: &str, package: Option<&str>, mpn: Option<&str>) {
        let part = self.current();
        let site = self.place_of(self.cur_source(), g.span);
        let name = g.name.name.clone();
        if let Some(other) = self.model.layout.iter().find(|l| {
            // Two rows' blocks may use one name: only one of them applies to a part.
            l.part == part
                && l.name == name
                && !matches!((&l.mpn, mpn), (Some(a), Some(b)) if a != b)
        }) {
            if other.site != site {
                let message = format!(
                    "the part already states a guideline `{name}` (at {}); give this one another name",
                    other.site
                );
                self.error(g.name.span, message);
            }
            return;
        }
        let word = g.rule.word();
        let what = format!("`{word} {name}`");
        let rule = match &g.rule {
            Rule::Near { select, to, within } => {
                let Some(select) = self.select(select, g) else {
                    return;
                };
                let Some(to) = self.part_pin(to) else { return };
                let Ok(within) = self.optional_length(within.as_ref(), "within", &what) else {
                    return;
                };
                GuidelineRule::Near { select, to, within }
            }
            Rule::Away {
                select,
                from,
                beyond,
            } => {
                let Some(select) = self.select(select, g) else {
                    return;
                };
                let Some(from) = self.part_pin(from) else {
                    return;
                };
                let Some(beyond) = self.positive_length(beyond, "beyond", &what) else {
                    return;
                };
                GuidelineRule::Away {
                    select,
                    from,
                    beyond,
                }
            }
            Rule::Loop { a, b, within } => {
                let (a, b) = (self.part_pin(a), self.part_pin(b));
                let (Some(a), Some(b)) = (a, b) else { return };
                if a == b {
                    self.error(
                        g.span,
                        "`bridging` names two different pins: the parts of the loop have a pad on the net of each",
                    );
                    return;
                }
                let Ok(within) = self.optional_length(within.as_ref(), "within", &what) else {
                    return;
                };
                GuidelineRule::Loop { a, b, within }
            }
            Rule::Keepout(area) => {
                let netsmith_lang::ast::KeepoutArea {
                    x,
                    y,
                    width,
                    height,
                    forbid,
                } = &**area;
                let x = self.any_length(x, "at", &what);
                let y = self.any_length(y, "at", &what);
                let w = self.positive_length(width, "size", &what);
                let h = self.positive_length(height, "size", &what);
                let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) else {
                    return;
                };
                let mut forbids = [forbid.is_empty(); 4];
                for f in forbid {
                    match FORBIDS.iter().position(|w| *w == f.name) {
                        Some(k) => forbids[k] = true,
                        None => {
                            self.error(
                                f.span,
                                format!(
                                    "a keepout forbids `tracks`, `vias`, `copper` or `parts`, not `{}`",
                                    f.name
                                ),
                            );
                            return;
                        }
                    }
                }
                let [tracks, vias, copper, parts] = forbids;
                GuidelineRule::Keepout {
                    at: (x, y),
                    size: (w, h),
                    tracks,
                    vias,
                    copper,
                    parts,
                }
            }
            Rule::Edge { pin, within } => {
                let Some(pin) = self.part_pin(pin) else {
                    return;
                };
                let Ok(within) = self.optional_length(within.as_ref(), "within", &what) else {
                    return;
                };
                GuidelineRule::Edge { pin, within }
            }
            Rule::Vias { pin, count } => {
                let Some(pin) = self.part_pin(pin) else {
                    return;
                };
                let n = match self.constant(count) {
                    Some((n, d)) if d == Dimension::NONE && n >= 1.0 && n.fract() == 0.0 => {
                        n as usize
                    }
                    _ => {
                        self.error(
                            count.span(),
                            format!(
                                "the count of {what} is a whole number of one or more, such as `4`, and `{}` is not",
                                count.to_source()
                            ),
                        );
                        return;
                    }
                };
                GuidelineRule::Vias { pin, count: n }
            }
            Rule::Route { pin, width, length } => {
                let Some(pin) = self.part_pin(pin) else {
                    return;
                };
                let Ok(width) = self.optional_length(width.as_ref(), "width", &what) else {
                    return;
                };
                let Ok(length) = self.optional_length(length.as_ref(), "length", &what) else {
                    return;
                };
                GuidelineRule::Route { pin, width, length }
            }
            Rule::Kelvin { select, to } => {
                let Some(select) = self.select(select, g) else {
                    return;
                };
                let Some(to) = self.part_pin(to) else { return };
                let among = match select {
                    NearSelect::Bridging(a, b) => a == to || b == to,
                    NearSelect::On { pin, .. } => pin == to,
                };
                if !among {
                    self.error(
                        g.span,
                        format!(
                            "the sense pin of {what} is a pin of its selector (`bridging .a .b` or `on .pin`): the pad it senses is the pad of the part found on the sense pin's net"
                        ),
                    );
                    return;
                }
                GuidelineRule::Kelvin { select, to }
            }
        };
        self.model.layout.push(LayoutGuideline {
            part,
            name,
            source: source.to_string(),
            site,
            package: package.map(str::to_string),
            mpn: mpn.map(str::to_string),
            rule,
        });
    }

    /// The parts a `near`, an `away` or a `kelvin` is about.
    fn select(&mut self, select: &Select, g: &Guideline) -> Option<NearSelect> {
        match select {
            Select::Bridging(a, b) => {
                let (a, b) = (self.part_pin(a), self.part_pin(b));
                let (a, b) = (a?, b?);
                if a == b {
                    self.error(
                        g.span,
                        "`bridging` names two different pins: the part with a pad on the net of each",
                    );
                    return None;
                }
                Some(NearSelect::Bridging(a, b))
            }
            Select::On { pin, kind } => {
                let pin = self.part_pin(pin)?;
                let kind = match kind {
                    None => None,
                    Some(k) => match PartKind::from_word(&k.name) {
                        Some(found) => Some(found),
                        None => {
                            let known: Vec<&str> = PartKind::ALL.iter().map(|k| k.word()).collect();
                            self.error(
                                k.span,
                                format!(
                                    "there is no kind `{}`; the kinds are {}",
                                    k.name,
                                    known.join(", ")
                                ),
                            );
                            return None;
                        }
                    },
                };
                Some(NearSelect::On { pin, kind })
            }
        }
    }

    /// The value of an expression that is one constant, with its dimension.
    fn constant(&mut self, e: &Expr) -> Option<(f64, Dimension)> {
        let t = self.lower(e)?;
        match (&t.e, t.ty) {
            (RExpr::Const(q), VTy::Q(d)) if q.lo() == q.hi() => Some((q.lo(), d)),
            _ => None,
        }
    }

    /// One length, any sign, in mm: a corner of a keepout.
    fn any_length(&mut self, e: &Expr, word: &str, what: &str) -> Option<f64> {
        match self.constant(e) {
            Some((metres, d)) if d == Dimension::LENGTH => Some(metres * 1000.0),
            _ => {
                self.error(
                    e.span(),
                    format!(
                        "`{word}` of {what} is one length, such as `-1.5mm`, and `{}` is not",
                        e.to_source()
                    ),
                );
                None
            }
        }
    }

    /// One length above zero, in mm.
    pub(crate) fn positive_length(&mut self, e: &Expr, word: &str, what: &str) -> Option<f64> {
        match self.constant(e) {
            Some((metres, d)) if d == Dimension::LENGTH && metres > 0.0 => Some(metres * 1000.0),
            _ => {
                self.error(
                    e.span(),
                    format!(
                        "`{word}` of {what} is one length above zero, such as `2mm`, and `{}` is not",
                        e.to_source()
                    ),
                );
                None
            }
        }
    }

    /// [`Self::positive_length`] of an optional number: `Err` when one is given and is wrong.
    fn optional_length(
        &mut self,
        e: Option<&Expr>,
        word: &str,
        what: &str,
    ) -> Result<Option<f64>, ()> {
        match e {
            None => Ok(None),
            Some(e) => self.positive_length(e, word, what).map(Some).ok_or(()),
        }
    }
}
