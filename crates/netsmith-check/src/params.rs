//! Modules that take a part as a type parameter (ADR 0115).
//!
//! `module Buck<Ic: BuckIc> provides BuckConverter { .ic: Ic ... }` is a module with a
//! parameter `Ic`, bounded by the generic `BuckIc`, which only parts provide. Inside the module
//! the parameter is the type of members (`.ic: Ic`) and the argument of another module that
//! takes one (`.stage: Other<Ic>`); nothing else.
//!
//! # Where it is checked
//!
//! At the definition, once, with the bound standing in for the parameter: `.ic` is made from
//! `BuckIc` itself, and a path through it reaches only the members `BuckIc` declares (the rule
//! of a use, ADR 0068). So a module that is accepted works with every part that provides the
//! bound, and a mistake is said at the module's line, not at a board that names a part. A part
//! that has more (a `BST` pin) wires it in its own `support` section.
//!
//! # An argument
//!
//! `.reg: Buck<Lt8641>` names the part: it must provide the bound (or be the bound, when the
//! bound is its own provider, as the standard passives are). Inside another parametric module,
//! a parameter with the same bound is an argument too (`Other<Ic>`). A generic that is not its
//! own provider is not an argument: a pick over the parts is written with a generic the module
//! provides (`.reg: BuckConverter`). A module with parameters is never placed without its
//! arguments.
//!
//! # A parametric provider
//!
//! A parametric module that provides a generic stands for one candidate for each provider of
//! its bound (`Buck<Lt8641>`, `Buck<Zbk2>`), in the bound's order, at the place of the module
//! in the generic's order; with several parameters, the combinations, the last varying
//! fastest. Each is one trial of the search, under the same bound (`MAX_PROVIDER_TRIALS`).
//! The candidate's id is `module::Buck<argmodule::Lt8641>` and its name `Buck<Lt8641>`, which
//! is also the instance's type name.

use netsmith_lang::Span;
use netsmith_lang::ast::{DefKind, TypePath};

use crate::checker::{Checker, Ty};
use crate::model::{InstId, InstKind};
use crate::providers::{ProviderRef, generic_is_part_shaped};

/// The parameters of the definition whose text is being read, each bound to the part that
/// stands for it.
#[derive(Clone, Debug)]
pub(crate) struct ParamEnv {
    pub def: (usize, usize),
    pub binds: Vec<ParamBind>,
}

#[derive(Clone, Debug)]
pub(crate) struct ParamBind {
    pub name: String,
    /// The generic that bounds it, and the part that stands for it (the bound itself while
    /// the module is checked on its own). `None` when the bound is not one, already reported.
    pub bound: Option<((usize, usize), (usize, usize))>,
}

impl Checker<'_> {
    /// The parameters of a module with their bounds, read in the module's file. A bound that is
    /// not a generic that parts provide is reported at its name and gives `None`.
    pub(crate) fn param_bounds(
        &mut self,
        def_ref: (usize, usize),
    ) -> Vec<(String, Option<(usize, usize)>)> {
        let def = self.modules()[def_ref.0].defs[def_ref.1].clone();
        if def.params.is_empty() {
            return Vec::new();
        }
        let saved = self.enter_def(def_ref);
        let mut out: Vec<(String, Option<(usize, usize)>)> = Vec::new();
        for p in &def.params {
            if out.iter().any(|(n, _)| *n == p.name.name) {
                self.error(
                    p.name.span,
                    format!(
                        "`{}` is a type parameter of `{}` already",
                        p.name.name, def.name.name
                    ),
                );
                continue;
            }
            if self.modules()[def_ref.0].scope.contains_key(&p.name.name) {
                self.error(
                    p.name.span,
                    format!(
                        "`{}` is a type parameter and also a type of this file; give the parameter a name of its own",
                        p.name.name
                    ),
                );
            }
            let tp = TypePath {
                segments: vec![p.bound.clone()],
                args: Vec::new(),
                of_member: false,
            };
            let bound = match self.resolve_type(&tp) {
                Ty::Def(m, i) => {
                    let g = &self.modules()[m].defs[i];
                    if g.kind == DefKind::Generic && generic_is_part_shaped(g) {
                        Some((m, i))
                    } else {
                        let what = if g.kind == DefKind::Generic {
                            "a generic that modules provide".to_string()
                        } else {
                            let word = crate::providers::kind_word(g);
                            let article = if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
                                "an"
                            } else {
                                "a"
                            };
                            format!("{article} {word}")
                        };
                        self.error(
                            p.bound.span,
                            format!(
                                "the bound of a type parameter is a generic that parts provide, and `{}` is {what}; a module takes a part as a parameter",
                                p.bound.name
                            ),
                        );
                        None
                    }
                }
                Ty::Poison | Ty::Bad => None,
                _ => {
                    self.error(
                        p.bound.span,
                        format!(
                            "the bound of a type parameter is a generic that parts provide, and `{}` is not one",
                            p.bound.name
                        ),
                    );
                    None
                }
            };
            out.push((p.name.name.clone(), bound));
        }
        self.leave_def(saved);
        out
    }

    /// The bindings for a module being made: each parameter to its argument, or to its bound
    /// when the module is checked on its own. `None` for a module with no parameters.
    pub(crate) fn param_env(
        &mut self,
        def_ref: (usize, usize),
        args: Option<&[(usize, usize)]>,
    ) -> Option<ParamEnv> {
        let bounds = self.param_bounds(def_ref);
        if bounds.is_empty() {
            return None;
        }
        let binds = bounds
            .into_iter()
            .enumerate()
            .map(|(k, (name, bound))| {
                let bound = bound.map(|b| (b, args.and_then(|a| a.get(k).copied()).unwrap_or(b)));
                ParamBind { name, bound }
            })
            .collect();
        Some(ParamEnv {
            def: def_ref,
            binds,
        })
    }

    /// A type parameter of the definition being read, by name.
    pub(crate) fn param_named(&self, name: &str) -> Option<&ParamBind> {
        let env = self.params.as_ref()?;
        if Some(env.def) != self.cur_def {
            return None;
        }
        env.binds.iter().find(|b| b.name == name)
    }

    /// The type name of a module made with arguments: `Buck<Lt8641>`.
    pub(crate) fn applied_name(&self, def_ref: (usize, usize), args: &[(usize, usize)]) -> String {
        let name = &self.modules()[def_ref.0].defs[def_ref.1].name.name;
        let args: Vec<&str> = args
            .iter()
            .map(|a| self.modules()[a.0].defs[a.1].name.name.as_str())
            .collect();
        format!("{name}<{}>", args.join(", "))
    }

    /// The arguments of a declaration whose type is `target` (`Buck<Lt8641>`), checked against
    /// its parameters' bounds. Reports each mistake and gives `None` when there is one.
    pub(crate) fn type_arguments(
        &mut self,
        tp: &TypePath,
        target: (usize, usize),
    ) -> Option<Vec<(usize, usize)>> {
        let def = self.modules()[target.0].defs[target.1].clone();
        let span = Span::new(
            tp.segments[0].span.start,
            tp.segments.last().expect("a type has a name").span.end,
        );
        if def.params.is_empty() {
            if !tp.args.is_empty() {
                self.error(
                    span,
                    format!(
                        "`{}` takes no type arguments; only a module declared with parameters does (`module Buck<Ic: BuckIc>`)",
                        def.name.name
                    ),
                );
                return None;
            }
            return Some(Vec::new());
        }
        let bounds = self.param_bounds(target);
        let names: Vec<String> = def.params.iter().map(|p| p.name.name.clone()).collect();
        if tp.args.len() != def.params.len() {
            let wanted: Vec<String> = def
                .params
                .iter()
                .map(|p| {
                    format!(
                        "`{}` (a part that provides `{}`)",
                        p.name.name, p.bound.name
                    )
                })
                .collect();
            let message = if tp.args.is_empty() {
                format!(
                    "`{}` takes a part for {}: name it, `{}<...>`, or let the solver pick one by using a generic that `{}` provides",
                    def.name.name,
                    wanted.join(" and "),
                    def.name.name,
                    def.name.name
                )
            } else {
                format!(
                    "`{}` takes {} type argument{}, for {}, and this gives {}",
                    def.name.name,
                    names.len(),
                    if names.len() == 1 { "" } else { "s" },
                    wanted.join(" and "),
                    tp.args.len()
                )
            };
            self.error(span, message);
            return None;
        }
        let mut out = Vec::new();
        let mut failed = false;
        for (k, arg) in tp.args.iter().enumerate() {
            let Some(bound) = bounds.get(k).and_then(|(_, b)| *b) else {
                failed = true;
                continue;
            };
            let bound_name = self.modules()[bound.0].defs[bound.1].name.name.clone();
            let arg_span = Span::new(
                arg.segments[0].span.start,
                arg.segments.last().expect("a type has a name").span.end,
            );
            let param = &names[k];
            match self.resolve_type(arg) {
                Ty::Param {
                    arg: a, bound: b, ..
                } => {
                    if b == bound {
                        out.push(a);
                    } else {
                        let other = self.modules()[b.0].defs[b.1].name.name.clone();
                        self.error(
                            arg_span,
                            format!(
                                "`{}` is bounded by `{other}`, and `{param}` of `{}` by `{bound_name}`; a parameter is an argument only where the bounds are the same generic",
                                arg.segments[0].name, def.name.name
                            ),
                        );
                        failed = true;
                    }
                }
                Ty::Def(m, i) => {
                    let providers = self.providers_of_unexpanded(bound);
                    let fits = providers
                        .iter()
                        .any(|p| p.def == (m, i) && p.info.kind == InstKind::Part);
                    if fits {
                        out.push((m, i));
                    } else {
                        let a = &self.modules()[m].defs[i];
                        let (a_name, why) = if (m, i) == bound {
                            (
                                a.name.name.clone(),
                                format!(
                                    "it is the bound itself, which stands for no part; name a part that provides `{bound_name}`, or use a generic that `{}` provides to let the solver pick",
                                    def.name.name
                                ),
                            )
                        } else if self.modules()[m].std {
                            (
                                a.name.name.clone(),
                                format!(
                                    "it does not provide `{bound_name}`; name a part that does"
                                ),
                            )
                        } else {
                            (
                                a.name.name.clone(),
                                format!(
                                    "it does not provide `{bound_name}`; write `part {} provides {bound_name}`, or name a part that does",
                                    a.name.name
                                ),
                            )
                        };
                        self.error(
                            arg_span,
                            format!(
                                "`{a_name}` is not an argument for `{param}` of `{}`, which is a part that provides `{bound_name}`: {why}",
                                def.name.name
                            ),
                        );
                        failed = true;
                    }
                }
                Ty::Poison | Ty::Bad => failed = true,
                Ty::Builtin(_) | Ty::Enum(..) => {
                    self.error(
                        arg_span,
                        format!(
                            "`{}` is not an argument for `{param}` of `{}`, which is a part that provides `{bound_name}`",
                            arg.segments[0].name, def.name.name
                        ),
                    );
                    failed = true;
                }
            }
        }
        (!failed).then_some(out)
    }

    /// The candidates a parametric module that provides a generic stands for: one for each
    /// combination of the providers of its parameters' bounds, the last varying fastest. Only
    /// parts are arguments, so the expansion never reaches another parametric module.
    pub(crate) fn expand_parametric(&mut self, base: &ProviderRef) -> Vec<ProviderRef> {
        let bounds = self.param_bounds(base.def);
        if bounds.iter().any(|(_, b)| b.is_none()) {
            return Vec::new();
        }
        let mut lists: Vec<Vec<ProviderRef>> = Vec::new();
        for (_, bound) in &bounds {
            let bound = bound.expect("checked above");
            let parts: Vec<ProviderRef> = self
                .providers_of_unexpanded(bound)
                .into_iter()
                .filter(|p| p.info.kind == InstKind::Part)
                .collect();
            lists.push(parts);
        }
        let mut combos: Vec<Vec<ProviderRef>> = vec![Vec::new()];
        for list in &lists {
            let mut next = Vec::new();
            for combo in &combos {
                for p in list {
                    let mut c = combo.clone();
                    c.push(p.clone());
                    next.push(c);
                }
            }
            combos = next;
        }
        combos
            .into_iter()
            .map(|combo| {
                let ids: Vec<&str> = combo.iter().map(|p| p.info.id.as_str()).collect();
                let names: Vec<&str> = combo.iter().map(|p| p.info.name.as_str()).collect();
                let mut r = base.clone();
                r.info.id = format!("{}<{}>", base.info.id, ids.join(", "));
                r.info.name = format!("{}<{}>", base.info.name, names.join(", "));
                r.args = combo.iter().map(|p| p.def).collect();
                r
            })
            .collect()
    }

    /// A member whose type is a type parameter (`.ic: Ic`): made from the part that stands for
    /// it, and reached as a use of the bound is, through the bound's members only.
    pub(crate) fn instantiate_param(
        &mut self,
        name: &str,
        parent: InstId,
        arg: (usize, usize),
        bound: (usize, usize),
        param: &str,
    ) -> Option<InstId> {
        if self.making.contains(&arg) {
            self.report_cycle(name, None, arg);
            return None;
        }
        let def = self.modules()[arg.0].defs[arg.1].clone();
        let bound_name = self.modules()[bound.0].defs[bound.1].name.name.clone();
        let id = self.instantiate_as(&def, arg, name, Some(parent), Some((bound, bound_name)));
        self.param_insts.insert(id, param.to_string());
        Some(id)
    }

    /// A path through a member made from a type parameter to a member its bound does not
    /// declare: reported, with the parameter, when `owner` is such a member.
    pub(crate) fn report_outside_bound(
        &mut self,
        owner: InstId,
        name: &netsmith_lang::ast::Ident,
    ) -> bool {
        let Some(param) = self.param_insts.get(&owner).cloned() else {
            return false;
        };
        let bound = self
            .model
            .instance(owner)
            .placed_as
            .clone()
            .unwrap_or_default();
        self.error(
            name.span,
            format!(
                "`{}` is not declared by `{bound}`, the bound of the type parameter `{param}`; a module reaches only the members of its parameter's bound, so that it works with every part that provides it (declare it in `{bound}`, or wire it in the `support` section of the parts that have it)",
                name.name
            ),
        );
        true
    }
}
