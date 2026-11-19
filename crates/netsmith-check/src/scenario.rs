//! `scenario backup { remove .usb; source .cell; sense .usb_sense }`: a state of a board's
//! supplies in which the power graph is analysed for the current that flows backwards
//! (issue #1258).
//!
//! The checker resolves the paths and keeps the scenario in the model
//! ([`Model::scenarios`](crate::Model::scenarios)); the solver works out, once the values are
//! known, which regulators the sources that stay drive (forwards, or backwards through a body
//! diode), which loads they power and where each sense net settles (`netsmith-solve`,
//! `scenario.rs`). `remove` and `source` name rails, which the solver holds to being supplies;
//! `sense` names a pin, or a rail for its `pos` net.

use netsmith_lang::ast::{self, ScenarioWord};

use crate::checker::{Checker, Res};
use crate::model::{InstKind, Scenario, ScenarioPath};

impl Checker<'_> {
    pub(crate) fn scenario(&mut self, q: &ast::Scenario) {
        if !self.in_board() {
            self.error(
                q.span,
                "`scenario` is a statement of a board, which knows its supplies; a part states what it does with current that flows backwards in its `.reverse`",
            );
            return;
        }
        let name = q.name.name.clone();
        if self.model.scenarios.iter().any(|s| s.name == name) {
            self.error(
                q.name.span,
                format!("the board has two scenarios called `{name}`: give each its own name"),
            );
            return;
        }
        let mut scenario = Scenario {
            name,
            removed: Vec::new(),
            sources: Vec::new(),
            senses: Vec::new(),
            source: self.cur_source(),
            span: q.span,
        };
        for step in &q.steps {
            let Some(path) = self.scenario_path(step.word, &step.path) else {
                continue;
            };
            let said = scenario
                .removed
                .iter()
                .map(|p| (p, ScenarioWord::Remove))
                .chain(scenario.sources.iter().map(|p| (p, ScenarioWord::Source)))
                .chain(scenario.senses.iter().map(|p| (p, ScenarioWord::Sense)))
                .find(|(p, _)| p.inst == path.inst)
                .map(|(_, word)| word);
            if let Some(word) = said {
                let message = if word == step.word {
                    format!(
                        "`{}` is named twice in the scenario: once is enough",
                        path.written
                    )
                } else {
                    format!(
                        "`{}` is both removed and a source that stays: a scenario says one of them",
                        path.written
                    )
                };
                self.error(step.span, message);
                continue;
            }
            match step.word {
                ScenarioWord::Remove => scenario.removed.push(path),
                ScenarioWord::Source => scenario.sources.push(path),
                ScenarioWord::Sense => scenario.senses.push(path),
            }
        }
        self.model.scenarios.push(scenario);
    }

    /// The instance a path of a scenario names: a rail for `remove` and `source`, a pin (or a
    /// rail's `pos`) for `sense`.
    fn scenario_path(&mut self, word: ScenarioWord, path: &ast::Path) -> Option<ScenarioPath> {
        let written = path.to_source();
        let inst = match self.resolve_path(path) {
            Res::Inst(i) => i,
            Res::Field(_) => {
                self.error(
                    path.span,
                    format!(
                        "`{written}` is a value, and `{}` names {}",
                        word.word(),
                        what(word)
                    ),
                );
                return None;
            }
            Res::Poison | Res::Bad => return None,
        };
        let instance = self.model.instance(inst);
        let rail = self.model.rail_of(inst);
        let inst = match (word, instance.kind, rail) {
            (ScenarioWord::Remove | ScenarioWord::Source, InstKind::Interface, Some(_)) => inst,
            (ScenarioWord::Sense, InstKind::Interface, Some((_, pos, _))) => pos,
            (ScenarioWord::Sense, InstKind::Pin, _) => inst,
            (_, kind, _) => {
                let is = match kind {
                    InstKind::Interface => {
                        format!("a `{}`, which is not a rail", instance.type_name)
                    }
                    InstKind::Pin => "a pin".to_string(),
                    _ => format!("a {} (`{}`)", kind.word(), instance.type_name),
                };
                self.error(
                    path.span,
                    format!(
                        "`{written}` is {is}, and `{}` names {}",
                        word.word(),
                        what(word)
                    ),
                );
                return None;
            }
        };
        Some(ScenarioPath {
            inst,
            written,
            span: path.span,
        })
    }
}

fn what(word: ScenarioWord) -> &'static str {
    match word {
        ScenarioWord::Remove => "a supply rail that is taken away (`remove .usb`)",
        ScenarioWord::Source => "a supply rail that stays (`source .cell`)",
        ScenarioWord::Sense => "a pin whose net it reads (`sense .mcu.usb_det`), or a rail",
    }
}
