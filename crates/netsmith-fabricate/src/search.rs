//! `netsmith catalog search` and `catalog add --from` (issues #1460, #1477): the library is in
//! `netsmith-catalog`; which provider a project names, and the credentials the environment holds
//! for it, need the project's manifest, so they are here.

pub use netsmith_catalog::search::*;

use crate::project::{MANIFEST, Project};
use netsmith_catalog::search::digikey::AccountId;
use netsmith_catalog::search::http::Credentials;

/// What a project writes to search with Nexar, and what a person must do first.
pub const SETUP: &str = "\
To search with Nexar (Octopart's API), a person must first read the terms of use of Nexar and of the sellers it lists; netsmith has not read them for you (issue #10). Then:
  1. make an application at https://portal.nexar.com and note its client id and secret;
  2. put them in two environment variables of your shell (never in a file of the project);
  3. name the variables, not their values, in netsmith.toml:
       [supply.nexar]
       client_id_env = \"NEXAR_CLIENT_ID\"
       client_secret_env = \"NEXAR_CLIENT_SECRET\"
Nothing is asked of Nexar until the table is there and both variables are set.";

/// What a project writes to search with Digi-Key's own API, and what a person must do first.
pub const DIGIKEY_SETUP: &str = "\
To search with Digi-Key's API, a person must first read Digi-Key's API terms of use (https://developer.digikey.com/api-user-agreement); netsmith has not read them for you (issue #10). Then:
  1. make an application for the Product Information API at https://developer.digikey.com and note its client id and secret;
  2. put them in two environment variables of your shell (never in a file of the project);
  3. name the variables, not their values, in netsmith.toml (account_id_env is optional: with it, `catalog add --from digikey` asks Digi-Key's product details of the number, which Digi-Key says needs the account id; without it, a keyword search):
       [supply.digikey]
       client_id_env = \"DIGIKEY_CLIENT_ID\"
       client_secret_env = \"DIGIKEY_CLIENT_SECRET\"
       account_id_env = \"DIGIKEY_ACCOUNT_ID\"
Nothing is asked of Digi-Key until the table is there and its variables are set.";

/// The search providers a project can name, as `--provider` takes them.
pub const PROVIDERS: [&str; 2] = [nexar::NAME, digikey::NAME];

/// The provider a run uses, with what the environment holds for it.
#[derive(Debug, Clone)]
pub enum Chosen {
    Nexar(Credentials),
    DigiKey(Credentials, Option<AccountId>),
}

/// The provider of a run and the `[supply] cache_hours`. `provider` is `--provider`; without it,
/// the one table the project has, and with both, Nexar, except for `catalog add --from digikey`
/// (`supplier`), which Digi-Key's own record verifies. An error says what is missing and how to
/// set it up; it never holds a value.
pub fn choose(
    project: Option<&Project>,
    provider: Option<&str>,
    supplier: Option<&str>,
) -> Result<(Chosen, Option<u64>), String> {
    let setup_of = |name: &str| {
        if name == digikey::NAME {
            DIGIKEY_SETUP
        } else {
            SETUP
        }
    };
    let Some(project) = project else {
        return Err(format!(
            "no project here: a search provider is named in the {MANIFEST} of a project\n{SETUP}\n{DIGIKEY_SETUP}"
        ));
    };
    let supply = project.manifest.supply.as_ref();
    let nexar = supply.and_then(|s| s.nexar.as_ref());
    let digikey = supply.and_then(|s| s.digikey.as_ref());
    let name = match provider {
        Some(name) if !PROVIDERS.contains(&name) => {
            return Err(format!(
                "`{name}` is not a search provider netsmith has: {}",
                PROVIDERS.join(", ")
            ));
        }
        Some(name) => {
            let named = if name == digikey::NAME {
                digikey.is_some()
            } else {
                nexar.is_some()
            };
            if !named {
                return Err(format!(
                    "--provider {name}, and {MANIFEST} has no [supply.{name}] table\n{}",
                    setup_of(name)
                ));
            }
            name
        }
        None => match (nexar.is_some(), digikey.is_some()) {
            (false, false) => {
                return Err(format!(
                    "{MANIFEST} names no search provider ([supply.nexar] or [supply.digikey])\n{SETUP}\n{DIGIKEY_SETUP}"
                ));
            }
            (_, true) if supplier == Some(digikey::NAME) => digikey::NAME,
            (true, _) => nexar::NAME,
            (false, true) => digikey::NAME,
        },
    };
    if name == digikey::NAME
        && let Some(other) = supplier.filter(|s| *s != digikey::NAME)
    {
        return Err(format!(
            "Digi-Key's API has only Digi-Key's own offers, not {other}'s: `catalog add --from {other}` needs [supply.nexar] (or --provider nexar)\n{SETUP}"
        ));
    }
    let read = |key: &str, var: &str, where_: &str| -> Result<String, String> {
        match std::env::var(var) {
            Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_string()),
            _ => Err(format!(
                "{MANIFEST}: [supply.{name}] {key} names the environment variable {var}, which is not set here: set it to your application's value ({where_}) in the shell that runs netsmith\n{}",
                setup_of(name)
            )),
        }
    };
    let hours = supply.and_then(|s| s.cache_hours);
    if name == digikey::NAME {
        let config = digikey.expect("chosen only with its table");
        let where_ = "developer.digikey.com";
        let credentials = Credentials {
            client_id: read("client_id_env", &config.client_id_env, where_)?,
            client_secret: read("client_secret_env", &config.client_secret_env, where_)?,
        };
        let account = config
            .account_id_env
            .as_deref()
            .map(|var| read("account_id_env", var, where_).map(AccountId))
            .transpose()?;
        return Ok((Chosen::DigiKey(credentials, account), hours));
    }
    let config = nexar.expect("chosen only with its table");
    let where_ = "portal.nexar.com";
    let credentials = Credentials {
        client_id: read("client_id_env", &config.client_id_env, where_)?,
        client_secret: read("client_secret_env", &config.client_secret_env, where_)?,
    };
    Ok((Chosen::Nexar(credentials), hours))
}
