//! `netsmith project migrate-offers` (ADR 0143): move a project's catalog columns `lcsc` and
//! `library` into its offers files, and its `.lcsc &=` hand picks to `.mpn &=`. The work is
//! `netsmith_fabricate::migrate_offers`; this prints what it does and gives the exit code.

use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_fabricate::migrate_offers;
use netsmith_fabricate::project::Project;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "A catalog row is the part, its `manufacturer` and `mpn`, and what a supplier calls it is an offer, a row of `offers/<supplier>.csv` (ADR 0143). A catalog file that still has the `lcsc` or `library` column, and a source that narrows `.lcsc`, are refused; `migrate-offers` moves them. For each of the project's own catalog files (its `catalog_dirs` folders and its `catalogs` list) it removes the two columns, every other cell as it was written, and appends to the project's offers files (its `offer_dirs` folders, `offers/` by default; a file is made with its header when there is none) an offer of `lcsc` for each row with an `lcsc`, and one of `jlcpcb` with the row's `library` as its `class` when the row has one, each with the row's `manufacturer`, `mpn`, `source` and `checked`, and `notes` that name the row. In each source of the project (the top-level files, the modules under src/ and the physical files beside them) it rewrites `.lcsc &= \"C307331\"` as `.mpn &= \"...\"`, the part the number is an offer of (the standard offers, the project's and the ones just moved), adding `.manufacturer &= \"...\"` when two parts share the number.\n\nIt writes everything or nothing: a row with an `lcsc` and no `mpn`, a sku no offer has, or an offer that another part already has the sku of stops it with the place of each, and nothing is written. An offer already in the file is not written again, so a second run changes nothing. It does not touch a pick lock: run `netsmith project lock --update` after it, which records the sku of each channel in place of the lock's `lcsc`. A dependency's files are migrated in its own project.\n\nIt prints each file it changes and what changes in it (with `--check`, what it would change). The exit code is 0 when it is done (with `--check`, when there is nothing to move), 1 with `--check` when there is, and 2 when it cannot move them (the errors above, or no project)."
)]
pub struct Args {
    /// Write nothing: print what would change, and exit 1 if anything would.
    #[arg(long)]
    pub check: bool,
}

pub fn run(args: Args) -> ExitCode {
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    };
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => return fail(format!("current directory: {e}")),
    };
    let project = match Project::find(&cwd) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return fail(format!(
                "no netsmith.toml in {} or any directory above it: migrate-offers moves a project's catalogs and sources",
                cwd.display()
            ));
        }
        Err(e) => return fail(e),
    };
    let plan = match migrate_offers::plan(&project) {
        Ok(plan) => plan,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("migrate-offers: nothing was written");
            return ExitCode::from(2);
        }
    };
    for note in &plan.notes {
        eprintln!("note: {note}");
    }
    if plan.is_empty() {
        println!(
            "nothing to move: the catalogs have no lcsc or library column and no source narrows .lcsc"
        );
        return ExitCode::SUCCESS;
    }
    for r in &plan.rewrites {
        println!("{}", r.file);
        for line in &r.says {
            println!("  {line}");
        }
    }
    if args.check {
        eprintln!(
            "migrate-offers: the files above have the old columns or narrowings; `netsmith project migrate-offers` moves them"
        );
        return ExitCode::FAILURE;
    }
    if let Err(e) = plan.write() {
        return fail(e);
    }
    println!(
        "moved; now run `netsmith project lock --update`, which records the sku of each channel in the pick locks in place of their lcsc"
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn migrate_offers_takes_check() {
        let Ok(Leaf::MigrateOffers(a)) = parse(&["project", "migrate-offers", "--check"]) else {
            panic!("not parsed");
        };
        assert!(a.check);
        let Ok(Leaf::MigrateOffers(a)) = parse(&["project", "migrate-offers"]) else {
            panic!("not parsed");
        };
        assert!(!a.check);
    }
}
