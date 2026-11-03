//! `cargo run -p netsmith-check --example check -- examples/led.nsm`
//! Checks a file and prints what was found: problems, narrowed values, nets, and what
//! is left for the solver.

use std::{env, fs, process::ExitCode};

use netsmith_check::{Domain, Loader, check};
use netsmith_lang::ast::CmpOp;

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: check <file.nsm>");
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

    let models = checked
        .boards
        .iter()
        .map(|b| (format!("board {}", b.name), &b.model))
        .chain(
            checked
                .systems
                .iter()
                .map(|s| (format!("system {}", s.name), &s.model)),
        );
    for (heading, m) in models {
        println!("{heading}");

        println!("  values");
        let mut any = false;
        for (id, field) in m.fields() {
            if !matches!(field.domain, Domain::Unset) {
                println!("    {} = {}", m.path_of_field(id), field.domain);
                any = true;
            }
        }
        if !any {
            println!("    (none)");
        }

        println!("  nets");
        for net in m.nets() {
            let terminals: Vec<_> = net.terminals.iter().map(|&p| m.path_of(p)).collect();
            let name = if net.explicit {
                net.name.clone()
            } else {
                format!("{} (generated)", net.name)
            };
            print!("    {name}: {}", terminals.join(", "));
            // With fewer than two terminals the net is joined through ports; show them.
            if net.terminals.len() < 2 {
                let ports: Vec<_> = net
                    .pins
                    .iter()
                    .filter(|p| !net.terminals.contains(p))
                    .map(|&p| m.path_of(p))
                    .collect();
                print!("  (ports: {})", ports.join(", "));
            }
            println!();
        }

        println!("  chains");
        for chain in m.chains() {
            let parts: Vec<_> = chain.parts.iter().map(|&p| m.path_of(p)).collect();
            println!(
                "    {} ~> {} ~> {}",
                m.path_of(chain.start),
                parts.join(" ~> "),
                m.path_of(chain.end)
            );
        }

        println!("  left for the solver");
        for r in m.relations() {
            println!(
                "    {} {} {}",
                m.show_expr(&r.lhs),
                op(r.op),
                m.show_expr(&r.rhs)
            );
        }
    }

    if checked.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn op(op: CmpOp) -> &'static str {
    op.symbol()
}
