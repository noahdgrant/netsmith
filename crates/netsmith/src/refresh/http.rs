//! Asking LCSC's product endpoint, politely (ADR 0039). It is unofficial: the one the lcsc.com
//! product page uses. JLCPCB is not asked at all (ADR 0039, the 2026-11-02 decision), and
//! [`check_endpoint`] refuses to send anything to a JLCPCB host or an `/api` path even if an
//! environment variable names one. So: one request at a time with a pause between them, a User-Agent that says what this is, a cache so a part is not asked for twice
//! in a day, a retry with a growing wait for the answers that mean "later" (429, 5xx, a dropped
//! connection), and a stop with a message for the answers that mean "no" (403, a web page where
//! JSON should be, a changed shape).

use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use super::date::now_secs;

/// What netsmith calls itself to the endpoints.
pub const USER_AGENT: &str = concat!(
    "netsmith-catalog-refresh/",
    env!("CARGO_PKG_VERSION"),
    " (opt-in check of catalog rows against LCSC's record; one request at a time)"
);

/// Where the endpoint is. Tests point it at a server of their own.
#[derive(Debug, Clone)]
pub struct Endpoints {
    /// `GET` with `?productCode=C...`.
    pub lcsc: String,
}

/// The environment variable that names another place for the endpoint, for a mirror or a proxy
/// of your own and for the tests, which never reach the real one.
pub const LCSC_URL_VAR: &str = "NETSMITH_LCSC_URL";

impl Endpoints {
    /// The real endpoint, unless `NETSMITH_LCSC_URL` says otherwise.
    pub fn live() -> Self {
        Endpoints {
            lcsc: std::env::var(LCSC_URL_VAR)
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "https://wmsc.lcsc.com/ftps/wm/product/detail".to_string()),
        }
    }
}

/// Refuse an endpoint that is JLCPCB's or that is under an `/api` path: this command does not
/// fetch JLCPCB data, and sites commonly disallow automated clients on `/api`.
pub fn check_endpoint(url: &str) -> Result<(), String> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    let path = path.split(['?', '#']).next().unwrap_or("");
    if host.contains("jlcpcb") {
        return Err(format!(
            "`{url}` is a JLCPCB address. netsmith does not fetch JLCPCB data (ADR 0039)"
        ));
    }
    if path == "api" || path.starts_with("api/") {
        return Err(format!(
            "`{url}` is under /api, which robots.txt files disallow for automated clients, and \
             netsmith does not request it (ADR 0039)"
        ));
    }
    Ok(())
}

/// What a fetcher does. `Default` is the polite run against the live endpoints.
#[derive(Debug, Clone)]
pub struct Network {
    pub endpoints: Endpoints,
    /// The pause between two requests that go out.
    pub delay: Duration,
    /// How many times a request that failed for a reason that may pass is tried again.
    pub retries: u32,
    /// The wait before the first retry; it doubles each time (a `Retry-After` is honoured if
    /// longer, up to a minute).
    pub backoff: Duration,
    /// Where answers are kept; `None` keeps none.
    pub cache_dir: Option<PathBuf>,
    /// How long a kept answer is used instead of asking.
    pub ttl: Duration,
    /// Ask nothing: use kept answers of any age, and report the parts with none.
    pub offline: bool,
    /// The time to stamp answers with and to age them by; `None` is now.
    pub now: Option<u64>,
}

impl Default for Network {
    fn default() -> Self {
        Network {
            endpoints: Endpoints::live(),
            delay: Duration::from_millis(500),
            retries: 3,
            backoff: Duration::from_secs(2),
            cache_dir: default_cache_dir(),
            ttl: Duration::from_secs(24 * 3600),
            offline: false,
            now: None,
        }
    }
}

/// `$NETSMITH_CACHE_DIR`, else `$XDG_CACHE_HOME/netsmith`, else `~/.cache/netsmith`, and
/// `catalog-refresh` in it.
pub fn default_cache_dir() -> Option<PathBuf> {
    let base = if let Some(d) = std::env::var_os("NETSMITH_CACHE_DIR") {
        PathBuf::from(d)
    } else if let Some(d) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(d).join("netsmith")
    } else {
        PathBuf::from(std::env::var_os("HOME")?)
            .join(".cache")
            .join("netsmith")
    };
    Some(base.join("catalog-refresh"))
}

/// An answer, and when it was got (Unix seconds).
#[derive(Debug)]
pub enum Got<T> {
    Answer {
        value: T,
        secs: u64,
        /// Whether it came from the cache and not from the endpoint.
        cached: bool,
    },
    /// Offline, and the cache has nothing for it.
    NotCached,
}

pub struct Fetcher {
    net: Network,
    agent: ureq::Agent,
    last_request: Option<Instant>,
    /// Requests that went out, for a report.
    pub requests: u32,
}

/// Why one attempt failed.
enum Failure {
    /// May pass: try again after at least this long.
    Later(String, Duration),
    /// Will not pass: stop the run.
    No(String),
}

impl Fetcher {
    pub fn new(net: Network) -> Fetcher {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();
        Fetcher {
            net,
            agent,
            last_request: None,
            requests: 0,
        }
    }

    pub fn now(&self) -> u64 {
        self.net.now.unwrap_or_else(now_secs)
    }

    /// The record of `lcsc`, from the cache if it is fresh (or if offline) and from the
    /// endpoint if not, read by `parse`. An answer `parse` refuses is not kept. An `Err` is
    /// final: the run should stop with it.
    pub fn get<T>(
        &mut self,
        lcsc: &str,
        parse: impl Fn(&str) -> Result<T, String>,
    ) -> Result<Got<T>, String> {
        if !is_lcsc_number(lcsc) {
            return Err(format!("`{lcsc}` is not an LCSC part number like C25104"));
        }
        let ttl = if self.net.offline {
            u64::MAX
        } else {
            self.net.ttl.as_secs()
        };
        if let Some((secs, body)) = self.kept(lcsc)
            && self.now().saturating_sub(secs) <= ttl
            && let Ok(value) = parse(&body)
        {
            return Ok(Got::Answer {
                value,
                secs,
                cached: true,
            });
        }
        if self.net.offline {
            return Ok(Got::NotCached);
        }
        let body = self.request(lcsc)?;
        let value = parse(&body).map_err(|why| {
            format!(
                "{why}. The endpoint may have changed or may be blocking this client; nothing was written. \
                 Check it by hand as data/jlcpcb/README.md shows, and see ADR 0039"
            )
        })?;
        let secs = self.now();
        self.keep(lcsc, secs, &body);
        Ok(Got::Answer {
            value,
            secs,
            cached: false,
        })
    }

    fn cache_file(&self, lcsc: &str) -> Option<PathBuf> {
        let dir = self.net.cache_dir.as_ref()?;
        Some(dir.join(format!("lcsc-{lcsc}.json")))
    }

    /// A kept answer: the first line of the file is the Unix time it was got.
    fn kept(&self, lcsc: &str) -> Option<(u64, String)> {
        let text = fs::read_to_string(self.cache_file(lcsc)?).ok()?;
        let (first, body) = text.split_once('\n')?;
        Some((first.trim().parse().ok()?, body.to_string()))
    }

    fn keep(&self, lcsc: &str, secs: u64, body: &str) {
        let Some(path) = self.cache_file(lcsc) else {
            return;
        };
        // A cache that cannot be written only costs a request next time.
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, format!("{secs}\n{body}"));
    }

    /// One request, with the pause before it and the retries after a failure that may pass.
    fn request(&mut self, lcsc: &str) -> Result<String, String> {
        let name = "LCSC";
        check_endpoint(&self.net.endpoints.lcsc)?;
        let mut wait = self.net.backoff;
        for attempt in 0..=self.net.retries {
            if let Some(last) = self.last_request {
                let since = last.elapsed();
                if since < self.net.delay {
                    thread::sleep(self.net.delay - since);
                }
            }
            let outcome = self.send(lcsc);
            self.last_request = Some(Instant::now());
            self.requests += 1;
            match outcome {
                Ok(body) => return Ok(body),
                Err(Failure::No(why)) => return Err(format!("{name}: {why}")),
                Err(Failure::Later(why, asked)) => {
                    if attempt == self.net.retries {
                        return Err(format!(
                            "{name}: {why}; gave up on {lcsc} after {} tries",
                            attempt + 1
                        ));
                    }
                    thread::sleep(wait.max(asked.min(Duration::from_secs(60))));
                    wait *= 2;
                }
            }
        }
        unreachable!("the last attempt returns")
    }

    fn send(&self, lcsc: &str) -> Result<String, Failure> {
        let response = self
            .agent
            .get(&self.net.endpoints.lcsc)
            .query("productCode", lcsc)
            .header("Accept", "application/json")
            .call();
        let mut response =
            response.map_err(|e| Failure::Later(format!("no answer ({e})"), Duration::ZERO))?;
        let status = response.status().as_u16();
        let asked = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map_or(Duration::ZERO, Duration::from_secs);
        match status {
            200 => {
                let body = response
                    .body_mut()
                    .read_to_string()
                    .map_err(|e| Failure::Later(format!("the answer broke off ({e})"), asked))?;
                if body.trim_start().starts_with('<') {
                    return Err(Failure::No(format!(
                        "answered a web page, not JSON (a block or a captcha?); it starts {:?}",
                        body.trim().chars().take(60).collect::<String>()
                    )));
                }
                Ok(body)
            }
            429 | 500..=599 => Err(Failure::Later(format!("answered {status}"), asked)),
            _ => Err(Failure::No(format!(
                "answered {status}: it refuses this client or the endpoint has moved"
            ))),
        }
    }
}

/// `C25104`: a capital C and digits. Anything else is not sent anywhere and not used in a
/// file name.
pub fn is_lcsc_number(text: &str) -> bool {
    text.len() > 1
        && text.len() < 12
        && text.starts_with('C')
        && text[1..].bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_jlcpcb_host_or_an_api_path_is_refused_and_lcscs_endpoint_is_not() {
        for url in [
            "https://jlcpcb.com/parts/list",
            "https://cart.jlcpcb.com/anything",
            "http://user@JLCPCB.com:443/x",
        ] {
            let e = check_endpoint(url).unwrap_err();
            assert!(
                e.contains("does not fetch JLCPCB data") && e.contains(url),
                "{e}"
            );
        }
        for url in [
            "https://wmsc.lcsc.com/api/x",
            "http://127.0.0.1:9/api",
            "http://h/api?x=1",
        ] {
            let e = check_endpoint(url).unwrap_err();
            assert!(e.contains("is under /api"), "{e}");
        }
        for url in [
            "https://wmsc.lcsc.com/ftps/wm/product/detail",
            "http://127.0.0.1:8080/lcsc",
            "http://127.0.0.1:8080/apiary",
        ] {
            assert!(check_endpoint(url).is_ok(), "{url}");
        }
    }
}
