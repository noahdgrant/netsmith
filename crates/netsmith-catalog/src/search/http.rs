//! What the search providers share to ask a supplier's API politely: the address check a
//! credential must pass, the credentials themselves, one request at a time with a pause and
//! retries, how an answer's status is read, and the cache of answers (ADR 0047; see the header of
//! [`super`]).

use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use crate::refresh::check_endpoint;

/// What netsmith calls itself to a search provider.
pub const USER_AGENT: &str = concat!(
    "netsmith-catalog-search/",
    env!("CARGO_PKG_VERSION"),
    " (opt-in part search for an electronics design tool; one request at a time)"
);

/// A client id and secret, read from the environment. Never written anywhere, and never shown:
/// its `Debug` says only that it is there.
#[derive(Clone)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials(..)")
    }
}

/// Refuse an address a credential or a token must not go to: one of JLCPCB's or under `/api`
/// (as `catalog refresh` refuses, ADR 0039), and one that is not `https`, unless it is this
/// machine's (a proxy of your own, or a test's server).
pub fn check(url: &str) -> Result<(), String> {
    check_endpoint(url)?;
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err(format!("`{url}` is not an address like https://host/path"));
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(v6) = host.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    }
    .to_ascii_lowercase();
    let local = matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1");
    match scheme.to_ascii_lowercase().as_str() {
        "https" => Ok(()),
        "http" if local => Ok(()),
        _ => Err(format!(
            "`{url}` is not an https address: a client secret and a token are sent only over https, or to this machine"
        )),
    }
}

/// Why one attempt failed.
pub enum Failure {
    /// May pass: try again after at least this long.
    Later(String, Duration),
    /// The token was refused: ask for a new one, once.
    Unauthorized,
    /// Will not pass.
    No(String),
}

/// The agent, and the pacing of the requests of one run.
pub struct Client {
    pub agent: ureq::Agent,
    /// The pause between two requests that go out.
    pub delay: Duration,
    /// How many times a request that failed for a reason that may pass is tried again.
    pub retries: u32,
    /// The wait before the first retry; it doubles (a `Retry-After` is honoured, up to a minute).
    pub backoff: Duration,
    last_request: Option<Instant>,
    /// Requests that went out, the tokens' included.
    pub requests: u32,
}

impl Client {
    pub fn new(delay: Duration, retries: u32, backoff: Duration) -> Client {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();
        Client {
            agent,
            delay,
            retries,
            backoff,
            last_request: None,
            requests: 0,
        }
    }

    fn pause(&self) {
        if let Some(last) = self.last_request {
            let since = last.elapsed();
            if since < self.delay {
                thread::sleep(self.delay - since);
            }
        }
    }

    /// One request with the pause before it and the retries after a failure that may pass.
    pub fn with_retries(
        &mut self,
        mut send: impl FnMut(&ureq::Agent) -> Result<String, Failure>,
    ) -> Result<String, Failure> {
        let mut wait = self.backoff;
        for attempt in 0..=self.retries {
            self.pause();
            let outcome = send(&self.agent);
            self.last_request = Some(Instant::now());
            self.requests += 1;
            match outcome {
                Err(Failure::Later(why, asked)) => {
                    if attempt == self.retries {
                        return Err(Failure::No(format!(
                            "{why}; gave up after {} tries",
                            attempt + 1
                        )));
                    }
                    thread::sleep(wait.max(asked.min(Duration::from_secs(60))));
                    wait *= 2;
                }
                other => return other,
            }
        }
        unreachable!("the last attempt returns")
    }
}

/// What an error answer says of itself, when it is JSON with a message in one of the fields the
/// providers use (`detail`, `title`, `ErrorMessage`, `error_description`, `error`).
fn said(body: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    [
        "detail",
        "title",
        "ErrorMessage",
        "error_description",
        "error",
    ]
    .iter()
    .find_map(|k| json.get(*k).and_then(|v| v.as_str()))
    .map(|s| s.trim().chars().take(200).collect::<String>())
    .filter(|s| !s.is_empty())
}

/// Read an answer: its body on 200, and what a status means otherwise. `refused` is what a 400 of
/// a token endpoint means (`what` is then `the token endpoint`), said by the provider.
pub fn read(
    response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    what: &str,
    refused: &str,
) -> Result<String, Failure> {
    let mut response =
        response.map_err(|e| Failure::Later(format!("{what}: no answer ({e})"), Duration::ZERO))?;
    let status = response.status().as_u16();
    let asked = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(Duration::ZERO, Duration::from_secs);
    if status == 200 {
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| Failure::Later(format!("{what}: the answer broke off ({e})"), asked))?;
        if body.trim_start().starts_with('<') {
            return Err(Failure::No(format!("{what} answered a web page, not JSON")));
        }
        return Ok(body);
    }
    if status == 401 {
        return Err(Failure::Unauthorized);
    }
    let body = response.body_mut().read_to_string().unwrap_or_default();
    let detail = said(&body).map(|s| format!(" ({s})")).unwrap_or_default();
    match status {
        400 if what == TOKEN_ENDPOINT => Err(Failure::No(format!(
            "{what} refused the client id and secret (answered 400{detail}): {refused}"
        ))),
        429 | 500..=599 => Err(Failure::Later(
            format!("{what} answered {status}{detail}"),
            asked,
        )),
        _ => Err(Failure::No(format!(
            "{what} answered {status}{detail}: it refuses this client or this request, or the address has moved"
        ))),
    }
}

/// What [`read`] calls a token endpoint.
pub const TOKEN_ENDPOINT: &str = "the token endpoint";

/// The `access_token` of a token endpoint's answer.
pub fn access_token(body: &str) -> Result<String, String> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| format!("{TOKEN_ENDPOINT} answered something that is not JSON"))?;
    json.get("access_token")
        .and_then(|v| v.as_str())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "{TOKEN_ENDPOINT}'s answer has no `access_token` of the kind it had when netsmith was written"
            )
        })
}

/// 64-bit FNV-1a: the name of a kept answer, from the request's text (which holds no credential).
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The answers a provider keeps: one file for each request, named by the hash of its text, whose
/// first line is the Unix time it was got. It holds what the supplier said of the parts and
/// nothing of the credentials or the token.
#[derive(Debug, Clone)]
pub struct Cache {
    /// Where answers are kept; `None` keeps none.
    pub dir: Option<PathBuf>,
}

impl Cache {
    /// The name of the answer to `request`.
    pub fn key(request: &str) -> String {
        format!("{:016x}", fnv1a(request.as_bytes()))
    }

    fn file(&self, key: &str) -> Option<PathBuf> {
        Some(self.dir.as_ref()?.join(format!("{key}.json")))
    }

    /// A kept answer and the time it was got.
    pub fn kept(&self, key: &str) -> Option<(u64, String)> {
        let text = fs::read_to_string(self.file(key)?).ok()?;
        let (first, body) = text.split_once('\n')?;
        Some((first.trim().parse().ok()?, body.to_string()))
    }

    /// Keep an answer.
    pub fn keep(&self, key: &str, secs: u64, body: &str) {
        let Some(path) = self.file(key) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, format!("{secs}\n{body}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_goes_only_over_https_or_to_this_machine_and_never_to_jlcpcb() {
        for ok in [
            "https://api.nexar.com/graphql",
            "https://api.digikey.com/v1/oauth2/token",
            "http://127.0.0.1:8080/graphql",
            "http://localhost/connect/token",
            "http://[::1]:9/graphql",
        ] {
            assert!(check(ok).is_ok(), "{ok}");
        }
        for (bad, says) in [
            ("http://api.nexar.com/graphql", "not an https address"),
            (
                "http://127.0.0.2.example.com/graphql",
                "not an https address",
            ),
            ("ftp://127.0.0.1/x", "not an https address"),
            ("https://jlcpcb.com/graphql", "does not fetch JLCPCB data"),
            ("https://example.com/api/graphql", "is under /api"),
            ("api.nexar.com/graphql", "not an address"),
        ] {
            let e = check(bad).unwrap_err();
            assert!(e.contains(says), "{bad}: {e}");
        }
    }

    #[test]
    fn the_credentials_are_never_shown() {
        let c = Credentials {
            client_id: "the-id".into(),
            client_secret: "the-secret".into(),
        };
        let shown = format!("{c:?}");
        assert!(
            !shown.contains("the-id") && !shown.contains("the-secret"),
            "{shown}"
        );
    }

    #[test]
    fn an_error_answer_says_its_message() {
        assert_eq!(
            said(r#"{"title":"Bad","detail":"Keywords too long","status":400}"#).as_deref(),
            Some("Keywords too long")
        );
        assert_eq!(
            said(r#"{"StatusCode":429,"ErrorMessage":"BurstLimit exceeded"}"#).as_deref(),
            Some("BurstLimit exceeded")
        );
        assert_eq!(said("<html>"), None);
    }
}
