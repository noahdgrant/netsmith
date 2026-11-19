//! The `within` and `route` statements of a physical file (issue #1274): a part within a
//! distance of a pin, and a net routed on one copper layer with no via.
//!
//! **Decided by the owner** (issue #1274): "Two requirements of the physical file: a distance
//! from a part to a pin (`within 8mm of u.ant`) and a net routed on one layer with no vias
//! (`route one_layer`); a net with a stated impedance (a `TrackImpedance`) is an RF net by
//! default: placed and routed first, short, on one layer, unless the net says otherwise. Exact
//! syntax the implementer's within this."
//!
//! **The syntax built** (the implementer's, within the owner's words):
//! - `.ant within 8mm of .radio.ant`: the subject is a part, or a pin of a part, and the
//!   statement ends with a pin of a part. The distance is one length above zero. Measured pad
//!   centre to pad centre, from the subject's nearest pad on the net of the pin (any of its
//!   pads when none is on it; a pin's own pads) to the pin's nearest pad, as a datasheet's
//!   `near ... within` is measured, so the two read alike.
//! - `.radio.ant route one_layer`: the net the pin is on is routed on one copper layer with no
//!   via. `.radio.ant route any_layer` is how a net with a stated impedance, an RF net, which is
//!   routed on one layer by default, says otherwise. The pin names the net, as a
//!   `TrackImpedance`'s `.line` does: a net's name is the board's, and a pin is not a string a
//!   typo would miss.
//!
//! `within`, `of` and `route` are read as words only after a path, so they stay usable as
//! names. Both statements belong in a physical file, a board's or a module's: they say where
//! things are and how copper runs, not what the circuit is.

use netsmith_lang::ast::{RouteLayers, Within};

use crate::checker::{Checker, Res};
use crate::model::{InstId, InstKind, NetRoute, PinDistance};

impl Checker<'_> {
    /// The part an instance is, or is a pin of (a pin of an interface of a part counts).
    fn part_of(&self, mut id: InstId) -> Option<InstId> {
        loop {
            let i = self.model.instance(id);
            if i.kind == InstKind::Part {
                return Some(id);
            }
            if !matches!(i.kind, InstKind::Pin | InstKind::Interface) {
                return None;
            }
            id = i.parent?;
        }
    }

    /// A path that names a pin of a part, or `None` after saying why it does not.
    fn pin_of_part(
        &mut self,
        path: &netsmith_lang::ast::Path,
        what: &str,
    ) -> Option<(InstId, InstId)> {
        let id = match self.resolve_path(path) {
            Res::Inst(id) => id,
            Res::Field(_) => {
                self.error(
                    path.span,
                    format!("`{}` is a value, and {what}", path.to_source()),
                );
                return None;
            }
            // Reported by `resolve_path`.
            _ => return None,
        };
        let kind = self.model.instance(id).kind;
        match (kind, self.part_of(id)) {
            (InstKind::Pin, Some(part)) => Some((id, part)),
            _ => {
                self.error(
                    path.span,
                    format!("`{}` is a {}, and {what}", path.to_source(), kind.word()),
                );
                None
            }
        }
    }

    /// `.ant within 8mm of .radio.ant`.
    pub(crate) fn within(&mut self, w: &Within) {
        let text = format!(
            "{} within {} of {}",
            w.subject.to_source(),
            w.distance.to_source(),
            w.pin.to_source()
        );
        if self.phys.is_none() {
            self.error(
                w.span,
                format!(
                    "`{text}` is a requirement on where a part goes, which is physical intent (ADR 0048); put it in the board's physical file (`physical for Board {{ ... }}`)"
                ),
            );
            return;
        }
        let subject = match self.resolve_path(&w.subject) {
            Res::Inst(id) => {
                let kind = self.model.instance(id).kind;
                match (kind, self.part_of(id)) {
                    (InstKind::Part, _) => Some((id, id)),
                    (InstKind::Pin, Some(part)) => Some((id, part)),
                    _ => {
                        self.error(
                            w.subject.span,
                            format!(
                                "`{text}` is about `{}`, which is a {}; the statement is about a part, or a pin of a part",
                                w.subject.to_source(),
                                kind.word()
                            ),
                        );
                        None
                    }
                }
            }
            Res::Field(_) => {
                self.error(
                    w.subject.span,
                    format!(
                        "`{text}` is about `{}`, which is a value; the statement is about a part, or a pin of a part",
                        w.subject.to_source()
                    ),
                );
                None
            }
            _ => None,
        };
        let pin = self.pin_of_part(
            &w.pin,
            &format!("`{text}` measures to a pin of a part, such as `.radio.ant`"),
        );
        let within = self.positive_length(&w.distance, "within", &format!("`{text}`"));
        let (Some((subject, subject_part)), Some((pin, pin_part)), Some(within)) =
            (subject, pin, within)
        else {
            return;
        };
        if subject_part == pin_part {
            self.error(
                w.span,
                format!(
                    "`{text}` measures a part to its own pin; name another part, or a pin of another part"
                ),
            );
            return;
        }
        let source = self.cur_source();
        self.model.pin_distances.push(PinDistance {
            subject,
            subject_part,
            pin,
            pin_part,
            within,
            source,
            span: w.span,
        });
    }

    /// `.radio.ant route one_layer` and `route any_layer`.
    pub(crate) fn route_layers(&mut self, r: &RouteLayers) {
        let text = format!("{} route {}", r.pin.to_source(), r.word());
        if self.phys.is_none() {
            self.error(
                r.span,
                format!(
                    "`{text}` says how a net's copper runs, which is physical intent (ADR 0048); put it in the board's physical file (`physical for Board {{ ... }}`)"
                ),
            );
            return;
        }
        let id = match self.resolve_path(&r.pin) {
            Res::Inst(id) => id,
            Res::Field(_) => {
                self.error(
                    r.pin.span,
                    format!(
                        "`{text}` names the net by a pin on it, and `{}` is a value",
                        r.pin.to_source()
                    ),
                );
                return;
            }
            // Reported by `resolve_path`.
            _ => return,
        };
        let kind = self.model.instance(id).kind;
        if kind != InstKind::Pin {
            self.error(
                r.pin.span,
                format!(
                    "`{text}` names the net by a pin on it, and `{}` is a {}",
                    r.pin.to_source(),
                    kind.word()
                ),
            );
            return;
        }
        let source = self.cur_source();
        self.model.net_routes.push(NetRoute {
            pin: id,
            one_layer: r.one_layer,
            source,
            span: r.span,
        });
    }
}
