//! `cargo run -p netsmith-solve --example solve -- examples/led.nsm`
//! Checks a file, picks real parts for it, and prints what was decided and what was not.

use std::{env, fs, process::ExitCode};

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, solve, solve_system};

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: solve <file.nsm>");
        return ExitCode::from(2);
    };
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };

    let checked = check(&path, &text, &Loader::std());
    eprint!("{}", checked.render());
    let mut ok = checked.is_ok();

    let solved = checked
        .boards
        .iter()
        .map(|b| {
            (
                format!("board {}", b.name),
                &b.model,
                solve(b, &Catalogs::jlcpcb()),
            )
        })
        .chain(checked.systems.iter().map(|s| {
            (
                format!("system {}", s.name),
                &s.model,
                solve_system(s, &Catalogs::jlcpcb()),
            )
        }));
    for (heading, model, solution) in solved {
        println!("{heading}");

        println!("  picked");
        for p in &solution.picks {
            let sourcing = match (&p.lcsc, p.library) {
                (Some(lcsc), Some(library)) => format!(" [{lcsc}, {library}]"),
                _ => String::new(),
            };
            println!(
                "    {}: {}{sourcing} from `{}` (after rejecting {})",
                solution.model.path_of(p.part),
                p.candidate,
                p.catalog,
                p.rejected
            );
        }
        if solution.picks.is_empty() {
            println!("    (nothing)");
        }

        println!("  not picked");
        for u in &solution.unresolved {
            println!("    {}: {}", solution.model.path_of(u.part), u.reason);
        }

        println!("  worked out");
        for (id, field) in solution.model.fields() {
            let before = model.field(id).domain.clone();
            if before != field.domain && !matches!(field.domain, Domain::Unset) {
                println!(
                    "    {} = {}",
                    solution.model.path_of_field(id),
                    field.domain
                );
            }
        }

        println!("  still waiting on something");
        for &i in &solution.deferred {
            let r = &solution.model.relations()[i];
            println!(
                "    {} {} {}",
                solution.model.show_expr(&r.lhs),
                r.op.symbol(),
                solution.model.show_expr(&r.rhs)
            );
        }

        for d in &solution.diagnostics {
            eprint!("{}", d.render(&checked.sources));
            ok = false;
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
