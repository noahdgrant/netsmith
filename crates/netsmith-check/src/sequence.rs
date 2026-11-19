//! `sequence { .a.vout; then 6ms; .b.vout; after .e }`: the order in which a board's rails come
//! up and its enable pins go high at power-up, with the least delay between two of them
//! (issue #1267).
//!
//! The checker resolves the events and the delays and keeps the orders in the model
//! ([`Model::sequence`](crate::Model::sequence)); the solver works out when each event happens
//! once the values are known and holds each order to it (`netsmith-solve`, `sequence.rs`). An
//! event is a rail (an interface with a `pos`, a `neg` and a `voltage`, which comes up) or a pin
//! (which goes high); a part, a module or a value is refused, with what to write instead.

use netsmith_lang::Span;
use netsmith_lang::ast::{self, SequenceStep};
use netsmith_quantity::{Dimension, Quantity};

use crate::checker::{Checker, Res};
use crate::model::{EventKind, InstKind, SequenceEvent, SequenceOrder};

impl Checker<'_> {
    pub(crate) fn sequence(&mut self, q: &ast::Sequence) {
        if !self.in_board() {
            self.error(
                q.span,
                "`sequence` is a statement of a board, which knows how its rails and enable pins are powered; a part or a module states the delays of its own (`.enable_delay`, `.soft_start_time`), and the board orders the events",
            );
            return;
        }
        let source = self.cur_source();
        // The last event written, and the delay a `then` after it gives the next one.
        let mut last: Option<SequenceEvent> = None;
        let mut delay = 0.0;
        for step in &q.steps {
            match step {
                SequenceStep::Event(path) => {
                    let event = self.sequence_event(path);
                    if let (Some(earlier), Some(later)) = (&last, &event) {
                        self.sequence_order(earlier.clone(), later.clone(), delay, source);
                    }
                    delay = 0.0;
                    last = event;
                }
                SequenceStep::Then { delay: written, .. } => {
                    // A delay that is refused leaves the order with none, so its events are still
                    // read.
                    delay = written
                        .as_ref()
                        .and_then(|(n, span)| self.sequence_delay(n, *span))
                        .unwrap_or(0.0);
                }
                SequenceStep::After { event, span } => {
                    let Some(before) = self.sequence_event(event) else {
                        continue;
                    };
                    if let Some(later) = &last {
                        self.model.sequence.push(SequenceOrder {
                            earlier: before,
                            later: later.clone(),
                            delay: 0.0,
                            source,
                            span: *span,
                        });
                    }
                }
            }
        }
    }

    fn sequence_order(
        &mut self,
        earlier: SequenceEvent,
        later: SequenceEvent,
        delay: f64,
        source: crate::diag::SourceId,
    ) {
        if earlier.inst == later.inst {
            self.error(
                later.span,
                format!(
                    "`{}` cannot come after itself: a `sequence` orders two different events",
                    later.written
                ),
            );
            return;
        }
        let span = Span::new(earlier.span.start, later.span.end);
        self.model.sequence.push(SequenceOrder {
            earlier,
            later,
            delay,
            source,
            span,
        });
    }

    /// The delay after `then`, in seconds: a time that is more than zero.
    fn sequence_delay(&mut self, n: &ast::Number, span: Span) -> Option<f64> {
        let q: Quantity = match Quantity::literal(n.value, n.unit.as_deref()) {
            Ok(lit) => lit.quantity,
            Err(err) => {
                self.error(span, err.to_string());
                return None;
            }
        };
        if q.dim() != Dimension::TIME {
            self.error(
                span,
                format!(
                    "the delay after `then` is a time, such as `then 6ms`, and `{}` is not one",
                    n.text
                ),
            );
            return None;
        }
        if q.lo() <= 0.0 {
            self.error(
                span,
                format!(
                    "a delay of `{}` says nothing: write `then` alone for an event that comes at or after the one before",
                    n.text
                ),
            );
            return None;
        }
        Some(q.lo())
    }

    /// The event a path names: a rail that comes up, or a pin that goes high.
    fn sequence_event(&mut self, path: &ast::Path) -> Option<SequenceEvent> {
        let written = path.to_source();
        let inst = match self.resolve_path(path) {
            Res::Inst(i) => i,
            Res::Field(_) => {
                self.error(
                    path.span,
                    format!(
                        "`{written}` is a value, and an event of a `sequence` is a rail that comes up (`.reg.vout`) or a pin that goes high (`.radio.en`)"
                    ),
                );
                return None;
            }
            Res::Poison | Res::Bad => return None,
        };
        let instance = self.model.instance(inst);
        let kind = match instance.kind {
            InstKind::Pin => EventKind::Pin,
            InstKind::Interface if self.model.rail_of(inst).is_some() => EventKind::Rail,
            kind => {
                let what = match kind {
                    InstKind::Interface => {
                        format!("a `{}`, which is not a rail", instance.type_name)
                    }
                    _ => format!("a {} (`{}`)", kind.word(), instance.type_name),
                };
                self.error(
                    path.span,
                    format!(
                        "`{written}` is {what}, and an event of a `sequence` is a rail that comes up (`{written}.vout`, a rail of it) or a pin that goes high (`.radio.en`)"
                    ),
                );
                return None;
            }
        };
        Some(SequenceEvent {
            inst,
            kind,
            written,
            span: path.span,
        })
    }
}
