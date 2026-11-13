//! A server on 127.0.0.1 that stands in for LCSC's product endpoint, so no test reaches the
//! real one. It answers from the saved responses in `tests/fixtures/catalog-refresh/` or from a
//! function the test gives it, and it records what it was asked: the method, the path and the
//! query of every request, so a test can show that nothing but `GET /lcsc?productCode=C...` was
//! ever sent (in particular nothing under `/api`, and nothing JLCPCB's: ADR 0039).

#![allow(dead_code)]

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use netsmith_catalog::refresh::{Endpoints, Network};

/// One request the server got.
#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    /// The path and query: `/lcsc?productCode=C25104`.
    pub target: String,
    pub user_agent: String,
    pub body: String,
    pub at: Instant,
}

impl Seen {
    /// The LCSC number asked for, from the query.
    pub fn lcsc(&self) -> String {
        self.target
            .split_once("productCode=")
            .map(|(_, c)| c.to_string())
            .unwrap_or_default()
    }

    /// The path without the query.
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or_default()
    }
}

pub struct Reply {
    pub status: u16,
    pub body: String,
    pub headers: Vec<(String, String)>,
}

impl Reply {
    pub fn ok(body: impl Into<String>) -> Reply {
        Reply {
            status: 200,
            body: body.into(),
            headers: Vec::new(),
        }
    }

    pub fn status(status: u16, body: &str) -> Reply {
        Reply {
            status,
            body: body.to_string(),
            headers: Vec::new(),
        }
    }
}

type Handler = Box<dyn Fn(&Seen) -> Reply + Send + Sync>;

pub struct Fake {
    pub port: u16,
    seen: Arc<Mutex<Vec<Seen>>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// `crates/netsmith/tests/fixtures/catalog-refresh/`, from this crate's tests or the binary's.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../netsmith/tests/fixtures/catalog-refresh")
}

/// The saved responses, and for a number with none the answer for "no such part".
pub fn fixture_reply(request: &Seen) -> Reply {
    let code = request.lcsc();
    match std::fs::read_to_string(fixtures_dir().join(format!("lcsc-{code}.json"))) {
        Ok(body) => Reply::ok(body),
        Err(_) => Reply::ok(r#"{"code":200,"msg":null,"result":null,"ok":true}"#),
    }
}

impl Fake {
    pub fn start(handler: impl Fn(&Seen) -> Reply + Send + Sync + 'static) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let handler: Arc<Handler> = Arc::new(Box::new(handler));
        {
            let (seen, stop) = (Arc::clone(&seen), Arc::clone(&stop));
            thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let (seen, handler) = (Arc::clone(&seen), Arc::clone(&handler));
                            thread::spawn(move || serve(stream, &seen, &handler));
                        }
                        Err(_) => thread::sleep(Duration::from_millis(2)),
                    }
                }
            });
        }
        Fake { port, seen, stop }
    }

    /// A server that answers with the saved responses.
    pub fn fixtures() -> Fake {
        Fake::start(fixture_reply)
    }

    pub fn endpoints(&self) -> Endpoints {
        Endpoints {
            lcsc: format!("http://127.0.0.1:{}/lcsc", self.port),
        }
    }

    /// Network settings for a test: this server, no pause, quick retries, and the cache in
    /// `cache` (none if `None`). The clock is fixed so dates are known.
    pub fn network(&self, cache: Option<&Path>, now: u64) -> Network {
        Network {
            endpoints: self.endpoints(),
            delay: Duration::ZERO,
            retries: 2,
            backoff: Duration::from_millis(1),
            cache_dir: cache.map(Path::to_path_buf),
            ttl: Duration::from_secs(24 * 3600),
            offline: false,
            now: Some(now),
        }
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn serve(mut stream: TcpStream, seen: &Mutex<Vec<Seen>>, handler: &Handler) {
    stream.set_nonblocking(false).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let (mut length, mut user_agent) = (0usize, String::new());
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            match name.trim().to_ascii_lowercase().as_str() {
                "content-length" => length = value.trim().parse().unwrap_or(0),
                "user-agent" => user_agent = value.trim().to_string(),
                _ => {}
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok();
    let request = Seen {
        method,
        target,
        user_agent,
        body: String::from_utf8_lossy(&body).into_owned(),
        at: Instant::now(),
    };
    seen.lock().unwrap().push(request.clone());
    let reply = handler(&request);
    let mut head = format!(
        "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        reply.status,
        reply.body.len()
    );
    for (name, value) in &reply.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(reply.body.as_bytes());
}
