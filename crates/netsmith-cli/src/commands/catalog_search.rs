use std::process::ExitCode;
use std::{env, time::Duration};

use clap::{ArgGroup, Args as ClapArgs};
use netsmith_fabricate::project::Project;
use netsmith_fabricate::search::digikey::{self, DigiKey};
use netsmith_fabricate::search::nexar::{self, Nexar};
use netsmith_fabricate::search::{self, Query, SearchProvider, SupplyError};

use super::common::{Format, print_json};

/// What `catalog search` and `catalog add --from` say about the terms of use and the network.
macro_rules! terms {
    () => {
        "A search asks Nexar (Octopart's API, https://api.nexar.com/graphql) or Digi-Key's own Product Information API (https://api.digikey.com/products/v4) with the project's own application: it is opt-in and off unless the project's netsmith.toml has a `[supply.nexar]` or `[supply.digikey]` table and the environment variables it names (`client_id_env`, `client_secret_env`, and for Digi-Key optionally `account_id_env`) are set; the manifest holds the variables' names, never their values, and the values are never written to a file, the cache or a message. `--provider nexar` or `--provider digikey` chooses when a project names both; without it, Nexar, which lists the offers of LCSC, Digi-Key and Mouser, except that `catalog add --from digikey` asks Digi-Key's own record. The terms of use of Nexar, of the sellers it lists and of Digi-Key's API have not been read for netsmith: a person must read them before this command is advertised (issue #10). It is polite: one request at a time, `--delay` apart, a User-Agent that says what it is, a retry with a growing wait on a 429 or a 5xx (a `Retry-After` is honoured), and the answers kept for the `[supply] cache_hours` (24; `--cache-ttl`) in `search/nexar/` or `search/digikey/` of netsmith's cache directory, so a search asked twice in a day asks once; `--offline` uses a kept answer of any age, and one older than 90 days is called stale. Nexar's plans limit how many parts an application is answered with (the limits are in your account at portal.nexar.com), and Digi-Key's Product Information API allows 120 requests a minute and 1000 a day: `--limit` is 5 unless told, and the cache keeps a repeated search from counting twice."
    };
}
pub(crate) use terms;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(group(ArgGroup::new("what").required(true).multiple(true).args(["keywords", "mpn", "filters"])))]
#[command(
    after_long_help = concat!("`catalog search` lists parts a catalog does not have yet: by words (`netsmith catalog search 100nF 0402 X7R`), by a manufacturer's number (`--mpn LM358`, which also finds numbers that contain it) and by parametric filters (`--filter case_package=0402`, the short names of Octopart's attributes as https://octopart.com/api/v4/values lists them, each value as that list writes it; netsmith passes them on as given, and only to Nexar: Digi-Key's filters are by its own ids, so `--filter` with Digi-Key is refused). Each part is the manufacturer and the `mpn` as the provider spells them, and for each supplier netsmith has an offers file of (`lcsc`, `digikey`, `mouser`) its sku, its stock (a count, or why there is none: `not stocked`, `in stock, no quantity given`, `on request`, `unknown`; never a guessed number) and its price breaks in the supplier's currency (none is converted). With `--details` it also asks for the values, the package (Octopart's `case_package`) and the datasheet's address, which Nexar gives only to a plan that includes them (the PRO tier with TECH_SPECS and DATASHEETS); without them the answer is an error that says so. Digi-Key answers with its values (its parameters, by name, the short name its parameter id) and the datasheet always, names no package, and has only its own offers, one for each packaging. A part found is not a catalog row: `netsmith catalog add --from <supplier> <sku> --to catalogs/<file>.csv` asks for its record again and writes the row and the offers with a `source` that names it.\n\nWith no `[supply.nexar]` or `[supply.digikey]` table, or with a variable the chosen one names not set, the command asks nothing and says how to set it up (exit 2).\n\n", terms!())
)]
pub struct Args {
    /// Words to look for in the parts' descriptions, categories and values.
    #[arg(value_name = "KEYWORDS")]
    pub keywords: Vec<String>,
    /// Look for a manufacturer's number, or part of one, instead of words.
    #[arg(long, value_name = "MPN", conflicts_with = "keywords")]
    pub mpn: Option<String>,
    /// A parametric filter, an attribute's short name and a value (repeatable; a name twice takes both values).
    #[arg(long = "filter", value_name = "NAME=VALUE", value_parser = parse_filter)]
    pub filters: Vec<(String, String)>,
    /// Only parts some seller has in stock.
    #[arg(long)]
    pub in_stock: bool,
    /// How many parts to list (1 to 100; Digi-Key gives at most 50).
    #[arg(long, value_name = "N", default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..=100))]
    pub limit: u32,
    /// Also ask Nexar for the values, the package and the datasheet (a Nexar plan that includes them; Digi-Key gives its values and datasheet always).
    #[arg(long)]
    pub details: bool,
    /// The search provider to ask: `nexar` or `digikey` (default the one the project names; with both, nexar).
    #[arg(long, value_name = "PROVIDER", value_parser = ["nexar", "digikey"])]
    pub provider: Option<String>,
    /// Ask nothing, and use a kept answer of any age.
    #[arg(long, conflicts_with_all = ["delay_ms", "cache_ttl_hours"])]
    pub offline: bool,
    /// The pause between requests, in milliseconds (default 500).
    #[arg(long = "delay", value_name = "MS")]
    pub delay_ms: Option<u64>,
    /// How long a kept answer is used, in hours (default the `[supply] cache_hours`, else 24).
    #[arg(long = "cache-ttl", value_name = "HOURS")]
    pub cache_ttl_hours: Option<u64>,
    /// Keep no answer.
    #[arg(long)]
    pub no_cache: bool,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` prints one JSON document on stdout and nothing else there: `command` (`catalog search`), `ok`, the `provider` (`nexar` or `digikey`), the `query` (`keywords`, `mpn`, the `filters` as `name` and `value`, `in_stock`, `limit`, `details`), the day the answer was `read`, whether it was `cached` and is `stale` (kept more than 90 days), the `requests` made, `hits` (how many parts match, which may be more than are listed) and the `parts`. Each part has its `manufacturer`, `mpn`, `description`, `package`, `values` (`name`, `shortname`, `value`), `datasheet` and `url` (null or empty when not asked or not given), its `offers` and the count of `other_sellers` not shown. Each offer has its `supplier` (`lcsc`, `digikey`, `mouser`), the `seller` as the provider names it, `authorized`, the `sku`, `stock` (a count, or null with the reason in `stock_note`), `packaging`, `moq`, `price_breaks` (`from`, `price`), `currency` and `updated`. A run that could not be made (no provider set up, the network, a refusal) is an error on stderr with exit code 2 and nothing on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

fn parse_filter(text: &str) -> Result<(String, String), String> {
    match text.split_once('=') {
        Some((name, value)) if !name.trim().is_empty() && !value.trim().is_empty() => {
            Ok((name.trim().to_string(), value.trim().to_string()))
        }
        _ => Err(format!(
            "`{text}` is not NAME=VALUE, such as case_package=0402"
        )),
    }
}

/// What the flags of a run change of a provider's polite defaults.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Run {
    pub offline: bool,
    pub delay_ms: Option<u64>,
    pub ttl_hours: Option<u64>,
    pub no_cache: bool,
}

impl Run {
    fn delay(&self, default: Duration) -> Duration {
        self.delay_ms.map_or(default, Duration::from_millis)
    }

    fn ttl(&self, hours: Option<u64>, default: Duration) -> Duration {
        self.ttl_hours
            .or(hours)
            .map_or(default, |h| Duration::from_secs(h.saturating_mul(3600)))
    }
}

/// The search provider of the project that contains the current directory: `--provider`, else
/// the one the project names (see [`search::choose`]); the error says how to set one up.
/// `supplier` is the supplier of `catalog add --from`.
pub(super) fn provider(
    run: Run,
    named: Option<&str>,
    supplier: Option<&str>,
) -> Result<Box<dyn SearchProvider>, String> {
    let cwd = env::current_dir().map_err(|e| e.to_string())?;
    let project = Project::find(&cwd)?;
    let (chosen, hours) = search::choose(project.as_ref(), named, supplier)?;
    Ok(match chosen {
        search::Chosen::Nexar(credentials) => {
            let mut s = nexar::Settings::live();
            s.offline = run.offline;
            s.delay = run.delay(s.delay);
            s.ttl = run.ttl(hours, s.ttl);
            if run.no_cache {
                s.cache_dir = None;
            }
            Box::new(Nexar::new(s, credentials))
        }
        search::Chosen::DigiKey(credentials, account) => {
            let mut s = digikey::Settings::live();
            s.offline = run.offline;
            s.delay = run.delay(s.delay);
            s.ttl = run.ttl(hours, s.ttl);
            if run.no_cache {
                s.cache_dir = None;
            }
            Box::new(DigiKey::new(s, credentials, account))
        }
    })
}

/// `netsmith catalog search`: exit 0 with the parts found (none is not a failure), 2 when the run
/// could not be made.
pub fn run(args: Args) -> ExitCode {
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    };
    let run = Run {
        offline: args.offline,
        delay_ms: args.delay_ms,
        ttl_hours: args.cache_ttl_hours,
        no_cache: args.no_cache,
    };
    let mut provider = match provider(run, args.provider.as_deref(), None) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let query = Query {
        keywords: (!args.keywords.is_empty()).then(|| args.keywords.join(" ")),
        mpn: args.mpn.clone(),
        filters: args.filters.clone(),
        in_stock: args.in_stock,
        limit: args.limit,
        details: args.details,
    };
    let answer = match provider.search(&query) {
        Ok(answer) => answer,
        Err(SupplyError::Offline) => {
            return fail(format!(
                "--offline, and no answer to this search is kept: run without --offline to ask {}",
                provider.describe()
            ));
        }
        Err(SupplyError::Unavailable(why)) => return fail(why),
    };
    let now = netsmith_fabricate::refresh::now_secs();
    if args.format == Format::Json {
        print_json(&search::json_answer(&query, &answer, now));
    } else {
        print!("{}", search::render_answer(&query, &answer, now));
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use crate::commands::catalog;
    use crate::commands::common::Format;
    use crate::commands::common::testing::{Leaf, kind, parse};
    use clap::error::ErrorKind;

    #[test]
    fn catalog_search_takes_words_an_mpn_or_filters_and_the_flags_of_a_polite_run() {
        let Ok(Leaf::Catalog(catalog::Args {
            command: catalog::Command::Search(s),
        })) = parse(&[
            "catalog",
            "search",
            "100nF",
            "0402",
            "--filter",
            "case_package=0402",
            "--limit",
            "3",
            "--details",
        ])
        else {
            panic!()
        };
        assert_eq!(s.keywords, ["100nF", "0402"]);
        assert_eq!(
            s.filters,
            [("case_package".to_string(), "0402".to_string())]
        );
        assert_eq!((s.limit, s.details, s.format), (3, true, Format::Text));
        let Ok(Leaf::Catalog(catalog::Args {
            command: catalog::Command::Search(s),
        })) = parse(&["catalog", "search", "--mpn", "LM358", "--format", "json"])
        else {
            panic!()
        };
        assert_eq!((s.mpn.as_deref(), s.limit), (Some("LM358"), 5));
        // Something to look for, a filter of the right shape, a limit Nexar allows.
        assert_eq!(
            kind(&["catalog", "search"]),
            ErrorKind::MissingRequiredArgument
        );
        assert!(
            parse(&["catalog", "search", "--filter", "0402"])
                .unwrap_err()
                .contains("is not NAME=VALUE")
        );
        assert_eq!(
            kind(&["catalog", "search", "x", "--limit", "500"]),
            ErrorKind::ValueValidation
        );
        assert_eq!(
            kind(&["catalog", "search", "x", "--mpn", "y"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["catalog", "search", "x", "--offline", "--delay", "9"]),
            ErrorKind::ArgumentConflict
        );
    }
}
