use netsmith_fabricate::project::Project;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, time::Duration};

use clap::{ArgGroup, Args as ClapArgs, Subcommand};
use netsmith_pipeline::supply::Supply;
use netsmith_tools::findings::{Finding, Severity};

use super::common::{Format, print_json};

#[derive(Debug, PartialEq, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Check LCSC's offers of the catalogs' parts against its record; write the day in `checked`.
    ///
    /// See ADR 0039 and ADR 0143.
    Refresh(RefreshArgs),
    /// Append a catalog row and offers from a supplier's record of a number, or print the record.
    ///
    /// The record is LCSC's, or a search provider's.
    Add(AddArgs),
    /// List the catalog rows whose price and stock are older than `--days` days, and the unknown ones.
    Age(AgeArgs),
    /// Find parts by words, MPN or filters, with supplier stock and price (Nexar, Digi-Key;
    /// opt-in).
    ///
    /// The words may be a manufacturer's number, and the filters are parametric; each part has each
    /// supplier's sku, stock and price.
    Search(super::catalog_search::Args),
    /// Compare each row's package with the footprint it names: its pads and its body against the package's known size.
    Check(super::catalog_check::Args),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`catalog refresh` is the one command that uses the network, and it is opt-in (ADR 0039): nothing else, and never the solver, asks anyone anything. JLCPCB data is not fetched: the command asks LCSC only, and checks LCSC's offers (ADR 0143): a catalog row holds no supplier's number, and what LCSC calls a part is a row of the offers file `offers/lcsc.csv`. For each of those offers it reads LCSC's record of its `sku` and checks the manufacturer's number against the offer's `mpn` and the package against the `package` of the part's catalog row. An offer that agrees is `VERIFIED` and gets the day in its `checked` column: that is the only cell the command ever writes, and a catalog file is never written. An offer's `class` (the sorting JLCPCB gives the part) is not checked. It never writes `price`, `stock`, `price_breaks`, `class` or `date`: the board is assembled from JLCPCB's parts library, those are JLCPCB's numbers entered by hand, and LCSC's price (often at a first break above one piece) and stock differ from them. It may say, as a note and not a finding, that LCSC lists stock 0 where the row has some (or the reverse), so you read JLCPCB's parts page by hand; the age of a row's own numbers is still its `date`, so a refresh never makes a stale row fresh. It keeps the order, quoting and line endings of the file, so the result is a small diff.\n\nWith no path it acts on the project's catalog and offers files (the `*.csv` of its `catalog_dirs` folders and the files under `catalogs`, and its `offer_dirs` folders) and, in the netsmith repository, on `data/jlcpcb/catalog/` and `data/offers/`; a path is a catalog file, an offers file (`lcsc.csv`) or a directory of them, and with no offers file among them the project's are read (the standard catalogs and offers inside the binary cannot be written, so they are checked in the repository and arrive with a release). An offer is reported, not changed, when the records disagree with it: `CONFLICT` if the manufacturer's number or the package is not the part's, `MISSING` if LCSC no longer lists the number (an offer is never deleted); `SKIPPED` is an offer `--offline` has no kept answer for.\n\nLCSC's endpoint is unofficial and LCSC's terms of use have not been read: a person must read them before the command is advertised, and official APIs (JLCAPI, the LCSC API; they need an account) are future work. Meanwhile the command is polite: one request at a time, `--delay` apart, a User-Agent that says what it is, answers kept for `--cache-ttl` and retried with a growing wait.\n\nThe exit code is 0, 1 if any offer is `CONFLICT` or `MISSING`, and 2 when the run had to stop (the endpoint refused, or answered in another shape: nothing is written then)."
)]
pub struct RefreshArgs {
    /// Catalog or offers files, or folders of them; none is the project's catalogs and offers.
    #[arg(value_name = "CATALOG.CSV|DIR")]
    pub paths: Vec<PathBuf>,
    /// Print the diff, cell by cell, and write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Ask nothing, and use the kept answers of any age.
    #[arg(long, conflicts_with_all = ["delay_ms", "cache_ttl_hours"])]
    pub offline: bool,
    /// Only the offers of the parts of this catalog (repeatable).
    #[arg(long, value_name = "CATALOG")]
    pub only: Vec<String>,
    /// Only the offer of this LCSC number (repeatable).
    #[arg(long, value_name = "C123")]
    pub lcsc: Vec<String>,
    /// The pause between requests, in milliseconds (default 500).
    #[arg(long = "delay", value_name = "MS")]
    pub delay_ms: Option<u64>,
    /// How long a kept answer is used, in hours (default 24).
    #[arg(long = "cache-ttl", value_name = "HOURS")]
    pub cache_ttl_hours: Option<u64>,
    /// Keep no answer.
    #[arg(long)]
    pub no_cache: bool,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` prints one JSON document on stdout and nothing else there: `command` (`catalog refresh`), `ok` (false when a row is a `conflict` or `missing`, as the exit code 1 says), `dry_run`, the `requests` made to LCSC, the `files` and `unknown_lcsc`, the numbers given to `--lcsc` that no offer has. Each file is an offers file of LCSC and has its `file`, its supplier as `catalog`, the counts `looked_up`, `verified`, `conflict`, `missing`, `skipped` and `without_lcsc` (rows of the catalogs read with a part number and no offer in the file), whether it was `written`, its `notes`, and the `rows` the text names: the offers that changed, are not verified or have a note, each with its `line`, its sku as `lcsc`, `status` (`verified`, `conflict`, `missing` or `skipped`), `why` for the last three, the `changes` (`column`, `from`, `to`) and its `notes`. The run's own notice is left out. The progress, one line for each offer looked up, is on stderr; a run that had to stop is an error on stderr with exit code 2 and nothing on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
#[command(group(ArgGroup::new("record").required(true).args(["lcsc", "from"])))]
#[command(
    after_long_help = concat!("`catalog add --lcsc <C123> --to <catalog.csv>` asks LCSC's record of the number (never JLCPCB's) and appends a row with `mpn`, `package` and a `source` that names the record and the day, and `manufacturer` as LCSC spells it when the file has that column (check it against the datasheet), leaving every other cell empty and listing the columns that need the datasheet; the number itself is an offer (ADR 0143), appended to the project's `offers/lcsc.csv` (the first folder of `offer_dirs`, or `offers/` beside the folder the command runs in outside a project; the file is made when there is none) with the same `source` and the day in `checked`. A part JLCPCB assembles also needs the offer's `class`, the library from JLCPCB's page, which the command does not write (an offer with no `class` is not in the `jlcpcb-assembly` channel). Without `--to` it prints the record and writes nothing.\n\nBefore it writes a row, `catalog add` checks the record's package as `netsmith catalog check` does (issue #1658): the footprint the package names for the kinds of the catalog the file is named for, measured against the package's known size, and each description the record gives, which must not name another package (LCSC writes `SOT-23` in the package field of a part its description calls a SOT-323). When either disagrees it prints what it found, with both values and the source, and asks whether to write the row anyway, on a terminal; with no terminal to ask on (a script, an agent, `--format json`) it writes nothing and exits 1, unless `--accept-package-mismatch` is given. What was accepted is printed after the row and is the `package_check` of the JSON.\n\n`catalog add --from <supplier> <sku or mpn> --to <catalog.csv>` asks the project's search provider (`--provider`, else for `--from digikey` Digi-Key's own API when the project names `[supply.digikey]`, else the one the project names, Nexar when both) for the record of the part that `supplier` (`lcsc`, `digikey` or `mouser`) offers under that sku, or whose manufacturer's number is exactly that, and writes nothing unless the record names one such part with one offer of that supplier (two packagings of one supplier, two manufacturers of one number: give the sku). It appends a row with `mpn`, `manufacturer` when the file has the column, `package` when the record states it (on Nexar with `--details`, which needs a plan that includes it; Digi-Key's record names no package, so it is listed to fill), a `source` that names the record and the day, and `notes`; and an offer for the named supplier and for each other supplier with one offer in the record, in the project's `offers/<supplier>.csv` (made when there is none), each with a `source` and the day in `checked`. A supplier with several offers is listed and not written. Every other cell stays empty and is listed as needing the datasheet, with the `class` of an LCSC offer (JLCPCB's library, which no record read here gives).\n\n", super::catalog_search::terms!())
)]
#[command(args_override_self = true)]
pub struct AddArgs {
    /// The LCSC number to add, such as C25104 (asks LCSC's record).
    #[arg(long, value_name = "C123")]
    pub lcsc: Option<String>,
    /// A supplier and its sku, or a manufacturer's number, to add from the search provider's record.
    #[arg(long, num_args = 2, value_names = ["SUPPLIER", "SKU|MPN"])]
    pub from: Option<Vec<String>>,
    /// With `--from`, also ask Nexar for the package, the values and the datasheet (a Nexar plan that includes them).
    #[arg(long, conflicts_with = "lcsc")]
    pub details: bool,
    /// With `--from`, the search provider, `nexar` or `digikey` (default: the project's, else
    /// nexar).
    ///
    /// The default is digikey for `--from digikey` when the project names it, else the one the
    /// project names, and nexar when it names both.
    #[arg(long, value_name = "PROVIDER", value_parser = ["nexar", "digikey"], conflicts_with = "lcsc")]
    pub provider: Option<String>,
    /// With `--from`, the pause between requests, in milliseconds (default 500).
    #[arg(long = "delay", value_name = "MS", conflicts_with = "lcsc")]
    pub delay_ms: Option<u64>,
    /// The catalog file to append the row to; without it the record is printed.
    #[arg(long, value_name = "CATALOG.CSV")]
    pub to: Option<PathBuf>,
    /// Ask nothing, and use a kept answer.
    #[arg(long)]
    pub offline: bool,
    /// Keep no answer.
    #[arg(long)]
    pub no_cache: bool,
    /// Write the row even if its package disagrees with its footprint or the record's description.
    ///
    /// Otherwise a run with no terminal to ask on writes nothing.
    #[arg(long)]
    pub accept_package_mismatch: bool,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` prints one JSON document on stdout and nothing else there: `command` (`catalog add`), `ok`, the `lcsc` number, the `manufacturer` as LCSC spells it (null when the record names none), the `mpn`, the `package` (null when LCSC lists none), the day it was `read`, `written` (the `file` and `line` of the row added, null with no `--to`), `offer` (the `file` and `line` of the LCSC offer added, null with no `--to`), the columns `filled`, `to_fill`, the columns that need the datasheet and the offer's class to write, and `package_check`, what the check of the package found and was accepted (empty when the package agrees). A number LCSC has no record of, one the offers file already has, or a part the catalog file already has, is `ok` false with a `findings` list of one error (the code `catalog`) and exit code 1; a run that could not be made is an error on stderr with exit code 2 and nothing on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`catalog age` prints the rows whose price and stock were read more than `--days` days ago (the supply provider's answer, else the row's `date`) and, apart, the rows whose price and stock are unknown because no provider has them and the row has no `date` (unknown is not old and not 0); each row is named and asked for by the sku of its offer (LCSC's, else Digi-Key's, else Mouser's, ADR 0143), and a row with no offer has no price to age. It reads the `[supply]` table of the project that contains the directory."
)]
#[command(args_override_self = true)]
pub struct AgeArgs {
    /// Catalog files or folders of them, as for `refresh`.
    #[arg(value_name = "CATALOG.CSV|DIR")]
    pub paths: Vec<PathBuf>,
    /// The age in days from which a row is old.
    #[arg(long, value_name = "N", default_value_t = netsmith_fabricate::refresh::STALE_DAYS)]
    pub days: u32,
    /// Only this catalog (repeatable).
    #[arg(long, value_name = "CATALOG")]
    pub only: Vec<String>,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` prints one JSON document on stdout and nothing else there: `command` (`catalog age`), `ok`, `today`, `days` (the age from which a row is old), `rows` (the rows with an offer), the `old` rows (`file`, `line`, the sku of the offer as `lcsc`, the `date` the price and stock were read and `days` ago, each null when the cell is not a date) and the `unknown` rows (`file`, `line`, `lcsc`, `why` nobody knows them).
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::Refresh(a) => run_catalog_refresh(&a),
        Command::Add(a) => run_catalog_add(&a),
        Command::Age(a) => run_catalog_age(&a),
        Command::Search(a) => super::catalog_search::run(a),
        Command::Check(a) => super::catalog_check::run(a),
    }
}

/// `netsmith catalog refresh`: the exit code is 1 if a row is a CONFLICT or MISSING, and 2 if
/// the run had to stop (an endpoint refused, a file could not be read) and wrote nothing.
/// `netsmith catalog add` (#454): exit 0 when the row is written (or the record printed), 1
/// when LCSC has no record or the file already has the number, 2 when the run could not be made.
fn run_catalog_add(request: &AddArgs) -> ExitCode {
    use netsmith_fabricate::refresh::{self, AddError, Network};
    if let Some(from) = &request.from {
        return run_catalog_add_from(request, (&from[0], &from[1]));
    }
    let lcsc = request
        .lcsc
        .as_deref()
        .expect("clap requires --lcsc or --from");
    let mut network = Network {
        offline: request.offline,
        ..Network::default()
    };
    if request.no_cache {
        network.cache_dir = None;
    }
    let to: Option<&Path> = request.to.as_deref();
    let shown = to.map(|p| p.display().to_string());
    // The LCSC offers file the number goes in: the project's, else `offers/lcsc.csv` here.
    let offers = match to.map(|_| lcsc_offers_file()).transpose() {
        Ok(offers) => offers,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let files = to
        .zip(shown.as_deref())
        .zip(offers.as_ref().map(|(p, s)| (p.as_path(), s.as_str())));
    let mut vet = match package_vet(request) {
        Ok(vet) => vet,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    match refresh::add(lcsc, files, network, &mut vet) {
        Ok(added) => {
            if request.format == Format::Json {
                print_json(&refresh::json_added(&added));
            } else {
                print!("{}", refresh::render_added(&added));
            }
            ExitCode::SUCCESS
        }
        Err(AddError::Refused(why)) if request.format == Format::Json => {
            print_json(&serde_json::json!({
                "command": "catalog add",
                "ok": false,
                "findings": [Finding::plain(Severity::Error, "catalog", why)],
            }));
            ExitCode::FAILURE
        }
        Err(AddError::Refused(why)) => {
            eprintln!("error: {why}");
            ExitCode::FAILURE
        }
        Err(AddError::Stopped(why)) => {
            eprintln!("error: {why}");
            ExitCode::from(2)
        }
    }
}

/// `netsmith catalog add --from <supplier> <number>`: exit 0 when the row is written (or the record
/// printed), 1 when the record does not name the part or the files already have it, 2 when the
/// run could not be made (no provider set up, the network, a file).
fn run_catalog_add_from(request: &AddArgs, (supplier, number): (&str, &str)) -> ExitCode {
    use netsmith_fabricate::refresh::AddError;
    use netsmith_fabricate::search::add;
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    };
    let run = super::catalog_search::Run {
        offline: request.offline,
        delay_ms: request.delay_ms,
        ttl_hours: None,
        no_cache: request.no_cache,
    };
    // A supplier netsmith has no offers of is refused before a provider is chosen.
    if !netsmith_fabricate::search::SUPPLIERS.contains(&supplier) {
        return fail(format!(
            "`{supplier}` is not a supplier netsmith has offers of: {}",
            netsmith_fabricate::search::SUPPLIERS.join(", ")
        ));
    }
    let mut provider =
        match super::catalog_search::provider(run, request.provider.as_deref(), Some(supplier)) {
            Ok(p) => p,
            Err(e) => return fail(e),
        };
    let to: Option<&Path> = request.to.as_deref();
    let shown = to.map(|p| p.display().to_string());
    let offers = |supplier: &str| offers_file(supplier);
    let mut vet = match package_vet(request) {
        Ok(vet) => vet,
        Err(e) => return fail(e),
    };
    match add::add_from(
        provider.as_mut(),
        (supplier, number),
        request.details,
        to.zip(shown.as_deref()),
        &offers,
        &mut vet,
    ) {
        Ok(added) => {
            if request.format == Format::Json {
                print_json(&add::json_added_from(&added));
            } else {
                print!("{}", add::render_added_from(&added));
            }
            ExitCode::SUCCESS
        }
        Err(AddError::Refused(why)) if request.format == Format::Json => {
            print_json(&serde_json::json!({
                "command": "catalog add",
                "ok": false,
                "findings": [Finding::plain(Severity::Error, "catalog", why)],
            }));
            ExitCode::FAILURE
        }
        Err(AddError::Refused(why)) => {
            eprintln!("error: {why}");
            ExitCode::FAILURE
        }
        Err(AddError::Stopped(why)) => fail(why),
    }
}

/// The check `catalog add` makes before it writes a row to `--to` (issue #1658): the footprints
/// the record's package names for the kinds of the catalog the file is named for, and the
/// record's descriptions ([`netsmith_fabricate::package_check::Context::check_record`]). What it
/// finds is accepted with `--accept-package-mismatch`, or by the person on a terminal (a text run
/// whose stdin and stderr are terminals); otherwise the row is refused. With no `--to` nothing is
/// written and nothing is checked.
type PackageVet = Box<dyn FnMut(Option<&str>, &[&str]) -> Result<Vec<String>, String>>;

fn package_vet(request: &AddArgs) -> Result<PackageVet, String> {
    use std::io::IsTerminal;
    let checked = match &request.to {
        Some(to) => {
            let cwd = env::current_dir().map_err(|e| e.to_string())?;
            let project = Project::find(&cwd)?;
            let context = netsmith_fabricate::package_check::Context::of(project.as_ref())?;
            let catalog = to
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            Some((context, catalog))
        }
        None => None,
    };
    let accept = request.accept_package_mismatch;
    let ask = request.format == Format::Text
        && std::io::stdin().is_terminal()
        && std::io::stderr().is_terminal();
    Ok(Box::new(
        move |package: Option<&str>, descriptions: &[&str]| {
            let Some((context, catalog)) = &checked else {
                return Ok(Vec::new());
            };
            let found = context.check_record(catalog, package, descriptions);
            if found.is_empty() || accept {
                return Ok(found);
            }
            if ask {
                for f in &found {
                    eprintln!("package check: {f}");
                }
                eprint!("write the row anyway? [y/N] ");
                let mut answer = String::new();
                std::io::stdin()
                    .read_line(&mut answer)
                    .map_err(|e| e.to_string())?;
                if matches!(answer.trim(), "y" | "Y" | "yes") {
                    return Ok(found);
                }
                return Err("the package was not confirmed: nothing was written".to_string());
            }
            Err(format!(
                "{}: nothing was written; check the package against the datasheet, and give `--accept-package-mismatch` to write the row as the record has it",
                found.join("; ")
            ))
        },
    ))
}

/// Where `catalog add` writes an offer of `supplier`: the offers file of that supplier of the
/// project that contains the current directory ([`Project::offer_file_of`]), or
/// `offers/<supplier>.csv` in the current directory outside a project.
fn offers_file(supplier: &str) -> Result<(PathBuf, String), String> {
    let cwd = env::current_dir().map_err(|e| e.to_string())?;
    if let Some(project) = Project::find(&cwd)?
        && let Some((path, file)) = project.offer_file_of(supplier)?
    {
        return Ok((path, file));
    }
    let file = format!("offers/{supplier}.csv");
    Ok((cwd.join(&file), file))
}

/// Where `catalog add` writes the LCSC offer: the offers file of LCSC of the project that
/// contains the current directory ([`Project::offer_file_of`]), or `offers/lcsc.csv` in the
/// current directory outside a project.
fn lcsc_offers_file() -> Result<(PathBuf, String), String> {
    offers_file("lcsc")
}

fn run_catalog_refresh(request: &RefreshArgs) -> ExitCode {
    use netsmith_fabricate::refresh::{self, Network};
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    };
    let cwd = match env::current_dir() {
        Ok(d) => d,
        Err(e) => return fail(e.to_string()),
    };
    let targets = match refresh::resolve_targets(&cwd, &request.paths) {
        Ok(t) => t,
        Err(e) => return fail(e),
    };
    let mut network = Network {
        offline: request.offline,
        ..Network::default()
    };
    if let Some(ms) = request.delay_ms {
        network.delay = Duration::from_millis(ms);
    }
    if let Some(hours) = request.cache_ttl_hours {
        network.ttl = Duration::from_secs(hours.saturating_mul(3600));
    }
    if request.no_cache {
        network.cache_dir = None;
    }
    let options = refresh::Options {
        dry_run: request.dry_run,
        only: request.only.clone(),
        lcsc: request.lcsc.clone(),
        network,
    };
    // One line on stderr for each row that is looked up.
    let say = |m: &str| eprintln!("{m}");
    let report = match refresh::refresh(&targets, &options, &say) {
        Ok(r) => r,
        Err(e) => {
            eprint!("{}", refresh::NOTICE);
            return fail(e);
        }
    };
    if request.format == Format::Json {
        print_json(&refresh::json_report(&report));
    } else {
        print!("{}", refresh::render_diff(&report));
        print!("{}", refresh::render_summary(&report));
    }
    if report.has_findings() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run_catalog_age(request: &AgeArgs) -> ExitCode {
    let (paths, only, days) = (&request.paths, &request.only, request.days);
    use netsmith_fabricate::refresh;
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    };
    let cwd = match env::current_dir() {
        Ok(d) => d,
        Err(e) => return fail(e.to_string()),
    };
    let today = refresh::date_of(refresh::now_secs());
    // The supply provider of the project that contains the directory, if there is one.
    let supply =
        match Project::find(&cwd).and_then(|p| p.map_or(Ok(Supply::none()), |p| p.supply())) {
            Ok(supply) => supply,
            Err(e) => return fail(e),
        };
    match refresh::resolve_targets(&cwd, paths)
        .and_then(|t| refresh::age(&t, only, days, &today, &supply))
    {
        Ok(report) => {
            if request.format == Format::Json {
                print_json(&refresh::json_age(&report));
            } else {
                print!("{}", refresh::render_age(&report));
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::catalog;
    use crate::commands::common::Format;
    use crate::commands::common::testing::Leaf;
    use crate::commands::common::testing::{kind, parse};
    use clap::error::ErrorKind;
    use std::path::PathBuf;

    #[test]
    fn catalog_add_takes_a_number_and_a_file() {
        assert_eq!(
            parse(&[
                "catalog",
                "add",
                "--lcsc",
                "C2286",
                "--to",
                "catalogs/led.csv"
            ]),
            Ok(Leaf::Catalog(catalog::Args {
                command: catalog::Command::Add(catalog::AddArgs {
                    lcsc: Some("C2286".into()),
                    from: None,
                    details: false,
                    provider: None,
                    delay_ms: None,
                    to: Some(PathBuf::from("catalogs/led.csv")),
                    offline: false,
                    no_cache: false,
                    accept_package_mismatch: false,
                    format: Format::Text,
                })
            }))
        );
        assert!(matches!(
            parse(&[
                "catalog",
                "add",
                "--lcsc",
                "C2286",
                "--offline",
                "--no-cache"
            ]),
            Ok(Leaf::Catalog(catalog::Args {
                command: catalog::Command::Add(catalog::AddArgs {
                    to: None,
                    offline: true,
                    no_cache: true,
                    ..
                })
            }))
        ));
        let e = parse(&["catalog", "add", "--to", "x.csv"]).unwrap_err();
        assert!(e.contains("--lcsc <C123>"), "{e}");
        // A record of a search provider: a supplier and its sku or a manufacturer's number.
        let Ok(Leaf::Catalog(catalog::Args {
            command: catalog::Command::Add(a),
        })) = parse(&[
            "catalog",
            "add",
            "--from",
            "digikey",
            "TEST-DK-1",
            "--to",
            "x.csv",
            "--details",
        ])
        else {
            panic!()
        };
        assert_eq!(
            a.from,
            Some(vec!["digikey".to_string(), "TEST-DK-1".to_string()])
        );
        assert!(a.details && a.lcsc.is_none());
        assert_eq!(
            kind(&["catalog", "add", "--from", "digikey"]),
            ErrorKind::WrongNumberOfValues
        );
        assert_eq!(
            kind(&["catalog", "add", "--lcsc", "C1", "--from", "lcsc", "C1"]),
            ErrorKind::ArgumentConflict
        );
        // --details and --delay are of a search provider's record.
        for flag in [
            &["--details"][..],
            &["--delay", "9"],
            &["--provider", "digikey"],
        ] {
            let mut args = vec!["catalog", "add", "--lcsc", "C1"];
            args.extend_from_slice(flag);
            assert_eq!(kind(&args), ErrorKind::ArgumentConflict, "{flag:?}");
        }
        assert_eq!(
            kind(&["catalog", "add", "--lcsc", "C1", "--dry-run"]),
            ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn catalog_refresh_takes_paths_and_the_flags_of_a_polite_opt_in_run() {
        assert_eq!(
            parse(&["catalog", "refresh"]),
            Ok(Leaf::Catalog(catalog::Args {
                command: catalog::Command::Refresh(catalog::RefreshArgs::default())
            }))
        );
        assert_eq!(
            parse(&[
                "catalog",
                "refresh",
                "catalogs/a.csv",
                "data",
                "--dry-run",
                "--only",
                "ldo",
                "--only",
                "led",
                "--lcsc",
                "C5446",
                "--delay",
                "1500",
                "--cache-ttl",
                "6",
                "--no-cache"
            ]),
            Ok(Leaf::Catalog(catalog::Args {
                command: catalog::Command::Refresh(catalog::RefreshArgs {
                    paths: vec!["catalogs/a.csv".into(), "data".into()],
                    dry_run: true,
                    offline: false,
                    only: vec!["ldo".into(), "led".into()],
                    lcsc: vec!["C5446".into()],
                    delay_ms: Some(1500),
                    cache_ttl_hours: Some(6),
                    no_cache: true,
                    format: Format::Text,
                })
            }))
        );
        let Ok(Leaf::Catalog(catalog::Args {
            command: catalog::Command::Refresh(r),
        })) = parse(&["catalog", "refresh", "--offline"])
        else {
            panic!()
        };
        assert!(r.offline);
        let e = parse(&["catalog", "refresh", "--delay", "soon"]).unwrap_err();
        assert!(e.contains("--delay") && e.contains("soon"), "{e}");
        // --offline asks nothing, so the pace of the asking means nothing with it.
        for pace in ["--delay", "--cache-ttl"] {
            assert_eq!(
                kind(&["catalog", "refresh", "--offline", pace, "9"]),
                ErrorKind::ArgumentConflict
            );
        }
        assert_eq!(
            kind(&["catalog", "refresh", "--days", "3"]),
            ErrorKind::UnknownArgument
        );
        assert!(parse(&["catalog", "refresh", "--lcsc"]).is_err());
    }

    #[test]
    fn catalog_age_takes_paths_a_catalog_and_the_days_with_90_as_the_default() {
        assert_eq!(
            parse(&["catalog", "age"]),
            Ok(Leaf::Catalog(catalog::Args {
                command: catalog::Command::Age(catalog::AgeArgs {
                    paths: vec![],
                    only: vec![],
                    days: 90,
                    format: Format::Text,
                })
            }))
        );
        assert_eq!(
            parse(&["catalog", "age", "x.csv", "--days", "30", "--only", "ldo"]),
            Ok(Leaf::Catalog(catalog::Args {
                command: catalog::Command::Age(catalog::AgeArgs {
                    paths: vec!["x.csv".into()],
                    only: vec!["ldo".into()],
                    days: 30,
                    format: Format::Text,
                })
            }))
        );
        assert_eq!(
            kind(&["catalog", "age", "--dry-run"]),
            ErrorKind::UnknownArgument
        );
        assert!(parse(&["catalog"]).is_err());
        assert_eq!(kind(&["catalog", "prices"]), ErrorKind::InvalidSubcommand);
    }
}
