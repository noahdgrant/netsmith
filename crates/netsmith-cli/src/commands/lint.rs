//! `netsmith project lint` (issue #1104): the linter's findings for the project's sources, or one
//! file's. The rules are `netsmith_lint`'s; this reads the manifest's `[lint]` table, prints the
//! findings and gives the exit code.

use std::collections::BTreeSet;
use std::fs;
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_fabricate::project::Target;
use netsmith_lint::{Lint, Project, Root, Rule};
use netsmith_pipeline::pick_lock;
use netsmith_tools::findings::{Finding, Report, Severity};

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`lint` reads a source that checks and reports what is worth changing in it: an unused import (`lint.unused_import`), an unused definition or member (`lint.unused_definition`), a redundant statement (`lint.redundant`) and a name against the guide's convention (`lint.naming`). It never changes a file. A rule is turned off for the project with `[lint] off = [\"naming\"]` in netsmith.toml, and for one line with `// lint: off naming` at its end or alone on the line above it. With no file it lints every file of the project: the top-level files, the modules under src/ and the physical files beside them.\n\nThe exit code is 0 when no finding is an error (a finding is a warning unless `--strict` or `[lint] findings = \"error\"` makes it one), 1 when one is, and 2 when the command cannot run (a file that does not check, a rule that does not exist)."
)]
pub struct Args {
    /// The source file; with none, every file of the project.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// Make every finding an error for this run, as `[lint] findings = "error"` does.
    #[arg(long)]
    pub strict: bool,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` prints `command`, `ok` and the `findings`, each with its `severity` (`warning`, or
    /// `error` with `--strict`), its `code` (`lint.` and the rule), and the `file`, `line`,
    /// `column` and `message`.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

pub fn run(args: Args) -> ExitCode {
    let fail = |e: String| {
        eprintln!("{e}");
        ExitCode::from(2)
    };
    let target = match Target::resolve(args.file.as_deref()) {
        Ok(t) => t,
        Err(e) => return fail(e),
    };
    let loader = match target.loader() {
        Ok(l) => l,
        Err(e) => return fail(e),
    };
    let catalogs = match target.catalogs() {
        Ok(c) => c,
        Err(e) => return fail(e),
    };
    let config = target
        .project
        .as_ref()
        .and_then(|p| p.manifest.lint.clone())
        .unwrap_or_default();
    let mut off = BTreeSet::new();
    for name in &config.off {
        match Rule::from_name(name) {
            Some(rule) => {
                off.insert(rule);
            }
            None => {
                return fail(format!(
                    "netsmith.toml: [lint] off names `{name}`, which is not a rule of the linter; the rules are {}",
                    Rule::names()
                ));
            }
        }
    }
    let mut roots = Vec::new();
    for file in &target.files {
        match fs::read_to_string(&file.path) {
            Ok(text) => {
                // The providers the file's pick lock holds, as the build checks it with.
                let lock = match pick_lock::read(&pick_lock::lock_path(&file.path)) {
                    Ok(lock) => lock,
                    Err(e) => return fail(e),
                };
                roots.push(Root {
                    name: file.name.clone(),
                    text,
                    assignment: pick_lock::held_providers(lock.as_ref()),
                })
            }
            Err(e) => return fail(format!("{}: {e}", file.path.display())),
        }
    }
    // The definitions the manifest names: a board reaches them with no source naming them.
    let named = target
        .project
        .as_ref()
        .and_then(|p| p.manifest.environment.as_deref())
        .and_then(|e| e.rsplit("::").next())
        .map(str::to_string)
        .into_iter()
        .collect();
    let project = Project {
        loader: &loader,
        roots,
        // One file of a project is not the whole of it; a file outside one is.
        whole: args.file.is_none() || target.project.is_none(),
        catalogs: Some(&catalogs),
        off,
        named,
    };
    let found = match netsmith_lint::lint(&project) {
        Ok(found) => found,
        Err(e) => return fail(e.to_string()),
    };
    let error = args.strict || config.findings.as_deref() == Some("error");
    let severity = if error {
        Severity::Error
    } else {
        Severity::Warning
    };
    let failed = error && !found.is_empty();
    match args.format {
        common::Format::Json => {
            let mut report = Report::new("project lint");
            report.ok = !failed;
            report.findings = found.iter().map(|l| finding(l, severity)).collect();
            print!("{}", report.to_json());
        }
        common::Format::Text => {
            let word = if error { "error" } else { "warning" };
            for l in &found {
                eprintln!("{word}: {}", l.render());
            }
            let files: BTreeSet<&str> = found.iter().map(|l| l.file.as_str()).collect();
            match found.len() {
                0 => eprintln!("lint: no findings"),
                n => eprintln!(
                    "lint: {n} finding{} in {} file{}",
                    if n == 1 { "" } else { "s" },
                    files.len(),
                    if files.len() == 1 { "" } else { "s" }
                ),
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// A finding of the linter as the findings of `check` are written.
fn finding(l: &Lint, severity: Severity) -> Finding {
    Finding {
        file: Some(l.file.clone()),
        line: Some(l.line),
        column: Some(l.column),
        ..Finding::plain(severity, &l.rule.code(), l.message.clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common;
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn lint_takes_a_file_strict_and_a_format() {
        let Ok(Leaf::Lint(l)) =
            parse(&["project", "lint", "a.nsm", "--strict", "--format", "json"])
        else {
            panic!()
        };
        assert_eq!(l.file.as_deref(), Some("a.nsm"));
        assert!(l.strict);
        assert_eq!(l.format, common::Format::Json);
    }
}
