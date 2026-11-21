//! `netsmith catalog search` and `catalog add --from` (issue #1460): the library is in
//! `netsmith-catalog`; which provider a project names, and the credentials the environment holds
//! for it, need the project's manifest, so they are here.

pub use netsmith_catalog::search::*;

use crate::project::{MANIFEST, Project};
use netsmith_catalog::search::nexar::Credentials;

/// What a project writes to search with Nexar, and what a person must do first. Said when no
/// provider is configured.
pub const SETUP: &str = "\
To search with Nexar (Octopart's API), a person must first read the terms of use of Nexar and of the sellers it lists; netsmith has not read them for you (issue #10). Then:
  1. make an application at https://portal.nexar.com and note its client id and secret;
  2. put them in two environment variables of your shell (never in a file of the project);
  3. name the variables, not their values, in netsmith.toml:
       [supply.nexar]
       client_id_env = \"NEXAR_CLIENT_ID\"
       client_secret_env = \"NEXAR_CLIENT_SECRET\"
Nothing is asked of Nexar until the table is there and both variables are set.";

/// The client id and secret of the project's `[supply.nexar]` table, read from the variables it
/// names, and the `[supply] cache_hours`. An error says what is missing and how to set it up; it
/// never holds a value.
pub fn nexar_credentials(project: Option<&Project>) -> Result<(Credentials, Option<u64>), String> {
    let Some(project) = project else {
        return Err(format!(
            "no project here: a search provider is named in the {MANIFEST} of a project\n{SETUP}"
        ));
    };
    let supply = project.manifest.supply.as_ref();
    let Some(config) = supply.and_then(|s| s.nexar.as_ref()) else {
        return Err(format!(
            "{MANIFEST} names no search provider ([supply.nexar])\n{SETUP}"
        ));
    };
    let read = |key: &str, name: &str| -> Result<String, String> {
        match std::env::var(name) {
            Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_string()),
            _ => Err(format!(
                "{MANIFEST}: [supply.nexar] {key} names the environment variable {name}, which is not set here: set it to your Nexar application's value (portal.nexar.com) in the shell that runs netsmith\n{SETUP}"
            )),
        }
    };
    let client_id = read("client_id_env", &config.client_id_env)?;
    let client_secret = read("client_secret_env", &config.client_secret_env)?;
    Ok((
        Credentials {
            client_id,
            client_secret,
        },
        supply.and_then(|s| s.cache_hours),
    ))
}
