//! [`Rule::UnusedImport`]: a name an `import` brings into a file that the file never writes.
//!
//! A name is used when the file writes it anywhere but in its imports, as a bare name: a type
//! (`.r1: Resistor`), an enum (`AssemblyMode::Hand`), a kind of a `defaults` line, a generic a
//! part provides. One exception, which the guide's chapter on generics teaches: an import that
//! brings the providers of its file into scope (`import src::parts::acme::AcmeCoax   // not used
//! by name`) is used when a provider of the file it names was a candidate of a use of a generic
//! on a board of the project (by itself, or as the part of a module's type parameter,
//! `Buck<Zbk9>`), and no other import of the file names that file by a name it uses (which would
//! bring the providers in scope already).

use std::collections::BTreeSet;

use netsmith_lang::ast;

use crate::{Context, Lint, Rule};

pub(crate) fn lint(cx: &Context) -> Vec<Lint> {
    // The providers that were candidates of a use of a generic on a board of the project, by
    // their ids (`src::zbk9::Zbk9`, `std::parts::regulator::Buck<src::zbk9::Zbk9>`).
    let candidates: Vec<&str> = cx
        .models()
        .flat_map(|(_, m)| m.uses.iter())
        .flat_map(|u| u.candidates.iter().map(|p| p.id.as_str()))
        .collect();
    // Whether the module `key` put a provider in scope that a use had as a candidate.
    let holds_used_provider = |key: &str| -> bool {
        let prefix = format!("{key}::");
        candidates.iter().any(|id| id.contains(&prefix))
    };

    let mut out = Vec::new();
    for file in cx.files.values() {
        if file.ast.imports.is_empty() {
            continue;
        }
        let Some(names) = netsmith_lang::bare_names(&file.text) else {
            continue;
        };
        let in_import = |at: usize| {
            file.ast
                .imports
                .iter()
                .any(|i| i.span.start <= at && at < i.span.end)
        };
        let written: BTreeSet<&str> = names
            .iter()
            .filter(|(_, span)| !in_import(span.start))
            .map(|(n, _)| n.as_str())
            .collect();
        // Each imported name with the module it is from.
        let imported: Vec<(String, &ast::Ident)> = file
            .ast
            .imports
            .iter()
            .flat_map(|i| match &i.names {
                Some(names) => {
                    let module = join(&i.path);
                    names
                        .iter()
                        .map(|n| (module.clone(), n))
                        .collect::<Vec<_>>()
                }
                None => match i.path.split_last() {
                    Some((name, module)) => vec![(join(module), name)],
                    None => Vec::new(),
                },
            })
            .collect();
        // The modules the file names by a name it uses: their providers are in scope anyway.
        let loaded: BTreeSet<&str> = imported
            .iter()
            .filter(|(_, n)| written.contains(n.name.as_str()))
            .map(|(m, _)| m.as_str())
            .collect();
        for (module, name) in &imported {
            if written.contains(name.name.as_str()) {
                continue;
            }
            if !loaded.contains(module.as_str()) && holds_used_provider(module) {
                continue;
            }
            out.push(file.lint(
                Rule::UnusedImport,
                name.span,
                format!(
                    "`{}` is imported from `{module}` and never used in this file; delete it from the import",
                    name.name
                ),
            ));
        }
    }
    out
}

fn join(path: &[ast::Ident]) -> String {
    path.iter()
        .map(|i| i.name.as_str())
        .collect::<Vec<_>>()
        .join("::")
}
