//! `netsmith catalog refresh` and `netsmith catalog age` (ADR 0039).
//!
//! A refresh is a check, not a data update. It looks each row's `lcsc` number up at LCSC
//! (ADR 0039 says how) and compares the manufacturer's number and the package
//! with the row's. A row that agrees is VERIFIED, and if the file has a `checked` column the
//! day is written there: the one cell a refresh ever writes. A row whose record disagrees is a
//! CONFLICT and a row LCSC no longer lists is MISSING; neither is changed and none is deleted.
//!
//! The board is assembled from JLCPCB's parts library, and LCSC's price and stock are not
//! JLCPCB's, so a refresh never writes `price`, `stock`, `price_breaks`, `library` or `date`.
//! It may read LCSC's stock to say, as information, that a row deserves a look at JLCPCB's page
//! (ADR 0039, the 2026-11-02 decisions). JLCPCB's own parts library is not asked.
//!
//! The solver never calls this: its input stays the CSV files, and it works offline.
//!
//! # What it does (ADR 0039, a current-state record since the owner's review of 2026-11-04)
//!
//! `netsmith catalog refresh` is the only command that touches the network. `build`, `fab`,
//! `compare`, the solver, the editor's live checks and the tests do not. The editor's "Check parts
//! against LCSC" button runs the same code over the project's own catalog files
//! (`POST /api/catalog/refresh`, which needs the `X-Netsmith` header, reports each looked-up part
//! over the WebSocket as `progress` of task `catalog`, and answers with a `CatalogRefresh`).
//!
//! For each row with an `lcsc` number it asks LCSC's product record
//! (`GET .../ftps/wm/product/detail?productCode=C25104`) and compares the manufacturer's number
//! (`productModel`) with the row's `mpn`, and the package (`encapStandard`) with the row's
//! `package`. The suppliers spell packages their own way (`SOT-23-3L` for `SOT-23`), so the row's
//! package may be the supplier's or one of its words, with a trailing `-<digit>L` removed. The
//! manufacturer's number is the real check; the package only catches a number reissued for another
//! package.
//!
//! - **VERIFIED** when both agree, **CONFLICT** when either differs (with both values), **MISSING**
//!   when LCSC no longer lists the part, **SKIPPED** with `--offline` when no answer was kept. The
//!   exit code is 1 for a CONFLICT or MISSING and 2 when the run had to stop.
//! - **It writes one cell, `checked`**: the day (UTC) the row last verified, or with `--offline`
//!   the day the kept answer was got, only if the file has the column (the standard files
//!   do, empty). Nothing reads it; it exists so a diff and `git log` show which rows were last
//!   verified. It never writes `price`, `stock`, `price_breaks`, `library`, `date` or any other
//!   cell. A test asserts that every cell but `checked` is byte-identical after a refresh of
//!   every row of every standard file, against a fake LCSC that lists a different stock and price
//!   for each. The file is edited in place, by byte span, the rows are never reordered (the order
//!   is the selection policy, ADR 0022), and files are written after every row has been looked
//!   up, each by a rename of a finished temporary file, so a run that stops writes nothing.
//! - **CONFLICT and MISSING rows are reported and never changed or deleted**: removing a row is a
//!   person's decision, and `considered.csv` is where it goes.
//! - **JLCPCB is not asked.** No request goes to a JLCPCB host or an `/api` path; the fetcher
//!   refuses such an endpoint, even if `NETSMITH_LCSC_URL` names one, before it sends anything.
//!   A test shows the fake server was asked only `GET /lcsc?productCode=C...`.
//! - **A note, not a finding:** when LCSC lists stock 0 where the row has some or none, or the row
//!   has 0 and LCSC lists some, the report says to check JLCPCB's parts page by hand. The exit
//!   code is unaffected. LCSC's prices are not read.
//! - **Selection:** `--dry-run` prints the diff and writes nothing; `--only <catalog>` and
//!   `--lcsc <C123>` choose catalogs and rows. With no path the files are the project's catalogs
//!   and, in the netsmith repository, `data/jlcpcb/catalog/`. The standard catalogs are also
//!   inside the binary, which cannot be written, so they are checked in the repository and reach
//!   users with a release.
//! - **Polite, and honest when it cannot be.** One request at a time, 500 ms apart (`--delay`);
//!   a User-Agent that says what it is (`netsmith-catalog-refresh/<version> ... one request at a
//!   time`); a cache of the answers (`$NETSMITH_CACHE_DIR`, else `$XDG_CACHE_HOME/netsmith`, else
//!   `~/.cache/netsmith`, in `catalog-refresh/`) used for 24 hours (`--cache-ttl`, `--no-cache`);
//!   `--offline` asks nothing and uses kept answers of any age; three retries with a doubling wait
//!   (2 s, 4 s, ...; a `Retry-After` is honoured up to a minute) after a dropped connection, a 429
//!   or a 5xx; a stop, naming the endpoint, on a 403 or another refusal, a web page where JSON
//!   should be, or JSON of another shape, and then nothing is written. An LCSC number is sent only
//!   if it is `C` and digits. `NETSMITH_LCSC_URL` points the command at another address (a mirror,
//!   a proxy of your own, the tests' fake server).
//! - **`netsmith catalog age [--days N]`** (default 90, `STALE_DAYS`) lists the rows whose price
//!   and stock (`date`) are older or missing. A refresh never changes `date`, so it never makes a
//!   row less stale. ADR 0047 ages a provider's quotes, and the standard catalogs ship none.
//! - **The editor** shows the date of each bill line's price and stock under the price, with the
//!   age in a tooltip, and marks a line older than 90 days in amber with a dot in front of its
//!   designators; a chip above the table counts them. `Bom` carries `staleAfterDays` and the
//!   project's own catalog files, and each `BomLine` its `date`, `ageDays` and `stale`, computed
//!   on the server (typed in `api.rs`). The file watcher builds again after a refresh; the bill
//!   does not change, because only `checked` was written. "Preview only" is `--dry-run`;
//!   "offline" is `--offline`. A project that lists no catalogs is told so.
//!
//! # Why a check and not a data update
//!
//! The first version (2026-11-02, chosen by the agent from the user's task) read two records for
//! each row, JLCPCB's parts library and LCSC's product record, and wrote `price`, `stock`,
//! `library`, `date` and `price_breaks`. The same day the maintainer read the `robots.txt` of both
//! sites: JLCPCB's disallows automated clients on the path its parts library is served from, and
//! LCSC's has no rule for the path of its product endpoint. The user decided, for v0.1.0, to drop
//! JLCPCB's endpoint, keep LCSC's only, tell users that JLCPCB data is not fetched, and take the
//! official APIs after v0.1.0 (JLCAPI at api.jlcpcb.com; the LCSC API, free with an account and a
//! review; both need the user's own account). For a few hours the refresh wrote LCSC's price, stock
//! and `price_breaks` and reported `library` as not fetched (a choice among writing nothing, writing
//! LCSC's numbers and saying so, and adding columns). Later the same day the user decided that the
//! board is assembled from JLCPCB's own parts library and that LCSC's numbers differ: for C25104
//! LCSC lists 1,085,700 in stock where JLCPCB lists 2,059,008, and LCSC's first price break is at 5
//! to 100 pieces where the rows hold the one-piece price. The solver, the bill of materials and the
//! stock-0 rule read those cells as the board house's numbers, so writing LCSC's put the wrong
//! supplier's numbers where the right one's are expected. So a refresh became the check above. It
//! is not `date` that records the verification, because `date` means the day the `price` and
//! `stock` were read (the bill marks a line stale from it, and `catalog age` lists rows by it), and
//! moving it would make a stale JLCPCB price look fresh: one meaning for each column (ADR 0001), so
//! `checked` is a column of its own. Recording nothing was rejected: a person wants to know which
//! rows have been through the check. The plan for C25102 and C47339, which JLCPCB flags Preferred,
//! to change from `Extended` to `Preferred` on the first refresh was withdrawn: the `library` is
//! not fetched, so they stay `Extended` until a person or an official API says otherwise. The
//! editor's button went from "Refresh prices" through "Refresh prices from LCSC (not JLCPCB)" to
//! "Check parts against LCSC"; the route and the command kept their names.
//!
//! The rule of the first version that a row whose `stock` cell is `0` is not offered by the solver
//! (it amended ADR 0022, item 6) and the `price_breaks` column written by the refresh are gone:
//! ADR 0047 made price, stock and date a provider's quotes, and the solver does not read stock.
//! `price_breaks` is a hand-entered column that nothing reads.
//!
//! # Cost, and what is not settled
//!
//! A new dependency, `ureq` (with rustls), in the `netsmith` crate only; the solver, the language
//! and the checker have none. The tests never reach the network: a server on `127.0.0.1` answers
//! from `tests/fixtures/catalog-refresh/`, the responses of twelve parts captured once from the
//! live endpoints on 2026-11-02 (public data), reduced to the fields netsmith reads because the
//! full answers carry signed, expiring download links and images; the JLCPCB captures were deleted
//! afterwards. Another test builds answers from the rows of every standard file and checks that a
//! refresh changes nothing but `checked`.
//!
//! **LCSC's terms of use are unread.** The endpoint is unofficial (the one the lcsc.com pages
//! call), the repository has never agreed anything with LCSC, and the sites' general terms could
//! not be read (the pages are rendered by script). The measures above (opt-in, sequential, a
//! pause, a cache, an honest User-Agent) are courtesy and not permission. A person must read LCSC's
//! terms before the command is advertised beyond this repository (a person must do this;
//! checklist in issue #10), and the command and the editor's button should be withdrawn or
//! changed if they forbid it. Until the official APIs are used, the JLCPCB price, stock and
//! library are read by hand.
//!
//! **Not built.** Refreshing the standard catalogs on a schedule or on a release, and who commits
//! them, is not planned until a board needs it. A live price in the editor without writing a file,
//! for a part of the standard catalogs: issue #8. A second supplier (Mouser, Digi-Key, which have
//! official APIs and keys) or the official JLCPCB and LCSC APIs: issue #30; the code has one place
//! that reads a record, which is where another would go. That this is the only command that
//! touches the network is a fact of the code and not a rule in `docs/DESIGN-RULES.md`; it is not
//! planned as one until a board needs it.

mod add;
mod csvfile;
mod date;
mod http;
mod records;
mod targets;

use std::{fs, path::Path};

pub use add::{AddError, Added, add, render_added, source_of};
pub use date::{age_days, date_of, days_of, now_secs};
pub use http::{
    Endpoints, Fetcher, LCSC_URL_VAR, Network, USER_AGENT, cache_base, check_endpoint,
    default_cache_dir, is_lcsc_number,
};
pub use records::{Product, package_matches, parse_product};
pub use targets::{Target, resolve_targets};

use csvfile::CsvFile;
use http::Got;

use crate::supply::{Lookup, PartKey, Supply};

/// What every refresh says about itself, first in its summary and in the editor: what it does,
/// what it never writes and why, and what has not been checked (ADR 0039).
pub const NOTICE: &str = "\
JLCPCB data is not fetched: netsmith asks LCSC only.
This is a check against LCSC's product record: each row's manufacturer's number and package (CONFLICT if they differ, MISSING if LCSC no longer lists the part). A row that agrees gets today's date in its `checked` column, if the file has one.
Never written: price, stock, price_breaks, library, date. They are JLCPCB's numbers, entered by hand, and LCSC's price and stock differ from them. Where LCSC's stock suggests a row needs a look (stock 0), the report says so; read JLCPCB's parts page by hand.
The solver does not read stock (ADR 0047): price and stock come from a supply provider, and `netsmith catalog age` lists the rows whose price and stock are old or unknown.
LCSC's terms of use have not been read: a person must read them before this command is advertised. Official APIs (JLCAPI, the LCSC API) need an account and are future work.
";

/// The age in days after which a price is called stale: the bill of materials marks it and
/// `catalog age` lists it unless told another number.
pub const STALE_DAYS: u32 = 90;

/// What a refresh is asked to do.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Say what would change and write nothing.
    pub dry_run: bool,
    /// Only the catalogs with these names (a file's name without `.csv`); empty is all.
    pub only: Vec<String>,
    /// Only the rows with these LCSC numbers; empty is all.
    pub lcsc: Vec<String>,
    pub network: Network,
}

/// One cell of a row that changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub column: String,
    pub from: String,
    pub to: String,
}

/// What became of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// LCSC's record agrees with the row about the manufacturer's number and the package. The
    /// `checked` cell (if the file has the column) is today's date; nothing else is written.
    Verified,
    /// The records disagree with the row about the part; the row is as it was.
    Conflict(String),
    /// A record of the part is gone; the row is as it was.
    Missing(String),
    /// Not looked at: offline and not in the cache.
    Skipped(String),
}

impl Status {
    pub fn word(&self) -> &'static str {
        match self {
            Status::Verified => "VERIFIED",
            Status::Conflict(_) => "CONFLICT",
            Status::Missing(_) => "MISSING",
            Status::Skipped(_) => "SKIPPED",
        }
    }

    /// The reason, for the statuses that have one.
    pub fn why(&self) -> Option<&str> {
        match self {
            Status::Conflict(w) | Status::Missing(w) | Status::Skipped(w) => Some(w),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowReport {
    /// The 1-based line of the row.
    pub line: usize,
    pub lcsc: String,
    pub status: Status,
    pub changes: Vec<Change>,
    /// Things to know about the row that are not a problem with it: LCSC lists no stock, so
    /// JLCPCB's page is worth a look. Information only; the row's cells are not touched.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReport {
    /// The path as given, as a message names it.
    pub file: String,
    pub catalog: String,
    pub rows: Vec<RowReport>,
    /// Rows with no `lcsc` number, which a refresh has nothing to ask about.
    pub without_lcsc: usize,
    /// Whether the file was written (never in a dry run, never if nothing changed).
    pub written: bool,
    /// About the file as a whole: that it has no `checked` column to record the check in.
    pub notes: Vec<String>,
}

impl FileReport {
    pub fn count(&self, pick: impl Fn(&Status) -> bool) -> usize {
        self.rows.iter().filter(|r| pick(&r.status)).count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub files: Vec<FileReport>,
    pub dry_run: bool,
    /// Requests that went to LCSC (the rest came from the cache).
    pub requests: u32,
    /// Rows asked for by `--lcsc` that no file has.
    pub unknown_lcsc: Vec<String>,
}

impl Report {
    /// Whether any row is a conflict or missing: something a person has to look at.
    pub fn has_findings(&self) -> bool {
        self.files.iter().any(|f| {
            f.rows
                .iter()
                .any(|r| matches!(r.status, Status::Conflict(_) | Status::Missing(_)))
        }) || !self.unknown_lcsc.is_empty()
    }
}

/// The columns of a file that a refresh reads, and the one it writes (`checked`).
struct Columns {
    lcsc: usize,
    mpn: Option<usize>,
    package: Option<usize>,
    stock: Option<usize>,
    date: Option<usize>,
    checked: Option<usize>,
}

impl Columns {
    fn of(file: &str, header: &[String]) -> Result<Columns, String> {
        let find = |name: &str| header.iter().position(|h| h == name);
        let lcsc = find("lcsc").ok_or_else(|| {
            format!("{file}: there is no `lcsc` column, so there is nothing to look up")
        })?;
        Ok(Columns {
            lcsc,
            mpn: find("mpn"),
            package: find("package"),
            stock: find("stock"),
            date: find("date"),
            checked: find("checked"),
        })
    }
}

/// Refresh the catalogs in `targets`. `say` is told what is being looked up. An `Err` is a
/// stop (the network refused, an endpoint changed shape, a file could not be read): then
/// nothing has been written.
pub fn refresh(
    targets: &[Target],
    options: &Options,
    say: &dyn Fn(&str),
) -> Result<Report, String> {
    let mut fetcher = Fetcher::new(options.network.clone());
    let mut report = Report {
        dry_run: options.dry_run,
        ..Report::default()
    };
    let mut writes: Vec<(&Path, String, usize)> = Vec::new();
    let mut seen_lcsc: Vec<String> = Vec::new();
    for target in targets {
        if !options.only.is_empty() && !options.only.contains(&target.catalog) {
            continue;
        }
        let text = fs::read_to_string(&target.path)
            .map_err(|e| format!("{}: {e}", target.path.display()))?;
        let mut file = CsvFile::parse(&target.shown, text)?;
        if file.is_empty() {
            return Err(format!(
                "{}: the file is empty; the first line names the columns",
                target.shown
            ));
        }
        let cols = Columns::of(&target.shown, &file.header())?;
        let mut rows = Vec::new();
        let mut without_lcsc = 0;
        let total = file.len() - 1;
        for record in 1..file.len() {
            if file.width(record) != file.width(0) {
                return Err(format!(
                    "{}:{}: the row has {} fields but the header has {}",
                    target.shown,
                    file.line(record),
                    file.width(record),
                    file.width(0)
                ));
            }
            let lcsc = file.value(record, cols.lcsc).trim().to_string();
            if lcsc.is_empty() {
                without_lcsc += 1;
                continue;
            }
            if !options.lcsc.is_empty() && !options.lcsc.contains(&lcsc) {
                continue;
            }
            seen_lcsc.push(lcsc.clone());
            say(&format!("{} {record}/{total}: {lcsc}", target.catalog));
            rows.push(refresh_row(&mut file, record, &cols, &lcsc, &mut fetcher)?);
        }
        let mut notes = Vec::new();
        if cols.checked.is_none() && rows.iter().any(|r| r.status == Status::Verified) {
            notes.push(
                "the file has no `checked` column, so the check is not recorded; add one (after `date`) to keep the day each row was verified"
                    .to_string(),
            );
        }
        let changed = file.is_changed();
        if changed && !options.dry_run {
            writes.push((&target.path, file.render(), report.files.len()));
        }
        report.files.push(FileReport {
            file: target.shown.clone(),
            catalog: target.catalog.clone(),
            rows,
            without_lcsc,
            written: false,
            notes,
        });
    }
    report.unknown_lcsc = options
        .lcsc
        .iter()
        .filter(|c| !seen_lcsc.contains(c))
        .cloned()
        .collect();
    // Everything was fetched and nothing stopped the run: now write.
    for (path, text, at) in writes {
        write_atomically(path, &text)?;
        report.files[at].written = true;
    }
    report.requests = fetcher.requests;
    Ok(report)
}

/// Look up one row, check it, and record the day in its `checked` cell.
fn refresh_row(
    file: &mut CsvFile,
    record: usize,
    cols: &Columns,
    lcsc: &str,
    fetcher: &mut Fetcher,
) -> Result<RowReport, String> {
    let line = file.line(record);
    let row = |status: Status, changes: Vec<Change>, notes: Vec<String>| RowReport {
        line,
        lcsc: lcsc.to_string(),
        status,
        changes,
        notes,
    };
    let (product, secs) = match fetcher.get(lcsc, |b| parse_product(b, lcsc))? {
        Got::Answer { value, secs, .. } => (value, secs),
        Got::NotCached => {
            return Ok(row(
                Status::Skipped("offline and not in the cache".to_string()),
                Vec::new(),
                Vec::new(),
            ));
        }
    };
    let Some(product) = product else {
        return Ok(row(
            Status::Missing(format!(
                "LCSC has no record of {lcsc}; the row is as it was"
            )),
            Vec::new(),
            Vec::new(),
        ));
    };

    // The part has to be the part the row is.
    let mut disagree = Vec::new();
    let cell = |c: Option<usize>| {
        c.map(|c| file.value(record, c).trim().to_string())
            .filter(|v| !v.is_empty())
    };
    if let Some(mpn) = cell(cols.mpn)
        && !product.mpn.eq_ignore_ascii_case(&mpn)
    {
        disagree.push(format!(
            "LCSC lists the manufacturer's number {}, the row has {mpn}",
            product.mpn
        ));
    }
    if let Some(package) = cell(cols.package)
        && let Some(theirs) = &product.package
        && !package_matches(&package, theirs)
    {
        disagree.push(format!(
            "LCSC lists the package {theirs}, the row has {package}"
        ));
    }
    if !disagree.is_empty() {
        return Ok(row(
            Status::Conflict(format!("{}; the row is as it was", disagree.join("; "))),
            Vec::new(),
            Vec::new(),
        ));
    }

    // LCSC's stock is not JLCPCB's and is never written. A big difference in kind (one has
    // none, the other has some) is worth saying, so a person looks at JLCPCB's page.
    let mut notes = Vec::new();
    let row_stock = cell(cols.stock).and_then(|s| s.parse::<u64>().ok());
    match (product.stock, row_stock) {
        (Some(0), stock) if stock != Some(0) => notes.push(format!(
            "LCSC lists stock 0 where the row has {}: the row is not changed; check JLCPCB's parts page by hand",
            stock.map_or("no stock entered".to_string(), |n| format!("stock {n}"))
        )),
        (Some(n), Some(0)) if n > 0 => notes.push(format!(
            "the row has stock 0 and LCSC lists stock {n}: the row is not changed; check JLCPCB's parts page by hand"
        )),
        _ => {}
    }

    // The one cell a refresh writes.
    let mut changes = Vec::new();
    if let Some(c) = cols.checked {
        let (old, new) = (file.value(record, c).trim().to_string(), date_of(secs));
        if old != new {
            file.set(record, c, &new);
            changes.push(Change {
                column: "checked".to_string(),
                from: old,
                to: new,
            });
        }
    }
    Ok(row(Status::Verified, changes, notes))
}

/// Write by renaming a finished file over the old one, so a crash leaves one or the other.
fn write_atomically(path: &Path, text: &str) -> Result<(), String> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".refresh-tmp");
    let temp = std::path::PathBuf::from(temp);
    fs::write(&temp, text).map_err(|e| format!("{}: {e}", temp.display()))?;
    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("{}: {e}", path.display())
    })
}

// ---- the report as text ----------------------------------------------------------------

/// The diff of a refresh, cell by cell: for each row that changed, the file and line and the
/// cells with their old and new text.
pub fn render_diff(report: &Report) -> String {
    let mut out = String::new();
    for file in &report.files {
        let changed: Vec<&RowReport> = file.rows.iter().filter(|r| !r.changes.is_empty()).collect();
        if changed.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "--- {}\n+++ {} (refreshed)\n",
            file.file, file.file
        ));
        for row in changed {
            out.push_str(&format!("@@ line {} {} @@\n", row.line, row.lcsc));
            for c in &row.changes {
                out.push_str(&format!(
                    "-{}: {}\n+{}: {}\n",
                    c.column, c.from, c.column, c.to
                ));
            }
        }
    }
    out
}

/// What the command prints at the end: the rows that need a look, then a line for each file.
pub fn render_summary(report: &Report) -> String {
    let mut out = String::from(NOTICE);
    for file in &report.files {
        for row in &file.rows {
            if let Some(why) = row.status.why() {
                out.push_str(&format!(
                    "{} {}:{} {}: {why}\n",
                    row.status.word(),
                    file.file,
                    row.line,
                    row.lcsc
                ));
            }
            for note in &row.notes {
                out.push_str(&format!(
                    "note {}:{} {}: {note}\n",
                    file.file, row.line, row.lcsc
                ));
            }
        }
        for note in &file.notes {
            out.push_str(&format!("note {}: {note}\n", file.file));
        }
    }
    for c in &report.unknown_lcsc {
        out.push_str(&format!(
            "MISSING {c}: no row of the catalogs has this number\n"
        ));
    }
    let mut totals = [0usize; 4];
    for file in &report.files {
        if file.rows.is_empty() {
            continue;
        }
        let n = |f: fn(&Status) -> bool| file.count(f);
        let counts = [
            n(|s| *s == Status::Verified),
            n(|s| matches!(s, Status::Conflict(_))),
            n(|s| matches!(s, Status::Missing(_))),
            n(|s| matches!(s, Status::Skipped(_))),
        ];
        for (t, c) in totals.iter_mut().zip(counts) {
            *t += c;
        }
        let changes = file.rows.iter().any(|r| !r.changes.is_empty());
        let verb = match (report.dry_run, changes) {
            (true, true) => "would change",
            (false, _) if file.written => "written",
            _ => "nothing to change",
        };
        out.push_str(&format!(
            "{}: {} rows looked up, {} verified, {} conflict, {} missing, {} skipped; {verb}\n",
            file.file,
            file.rows.len(),
            counts[0],
            counts[1],
            counts[2],
            counts[3],
        ));
    }
    out.push_str(&format!(
        "total: {} verified, {} conflict, {} missing, {} skipped; {} requests to LCSC\n",
        totals[0], totals[1], totals[2], totals[3], report.requests
    ));
    out
}

// ---- age -------------------------------------------------------------------------------

/// A row whose numbers are old, or not known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aged {
    pub file: String,
    pub line: usize,
    pub lcsc: String,
    /// The day the price and stock were read: the supply provider's answer, else the row's
    /// `date` cell. `None` if they are unknown, or the cell is not a date.
    pub date: Option<String>,
    /// How many days before today, `None` if there is no date or it is not one.
    pub days: Option<i64>,
    /// Why nobody knows the price and stock (ADR 0047), for a row in
    /// [`AgeReport::unknown`]; `None` for a row in [`AgeReport::old`].
    pub unknown: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgeReport {
    pub today: String,
    pub days: u32,
    /// Rows with an `lcsc` number, which are the rows with a price to age.
    pub rows: usize,
    /// Rows whose price and stock were read more than `days` days ago, or whose `date` cell
    /// is not a date.
    pub old: Vec<Aged>,
    /// Rows whose price and stock are unknown: no provider has the part and the row has no
    /// `date`. Unknown is not old, and not zero.
    pub unknown: Vec<Aged>,
}

/// The rows of `targets` whose price and stock were read more than `days` days before
/// `today`, and the rows whose price and stock are unknown. What is known comes from
/// `supply` (ADR 0047) and, for a row it has no answer for, from the row's own `date` cell.
pub fn age(
    targets: &[Target],
    only: &[String],
    days: u32,
    today: &str,
    supply: &Supply,
) -> Result<AgeReport, String> {
    let mut report = AgeReport {
        today: today.to_string(),
        days,
        rows: 0,
        old: Vec::new(),
        unknown: Vec::new(),
    };
    for target in targets {
        if !only.is_empty() && !only.contains(&target.catalog) {
            continue;
        }
        let text = fs::read_to_string(&target.path)
            .map_err(|e| format!("{}: {e}", target.path.display()))?;
        let file = CsvFile::parse(&target.shown, text)?;
        if file.is_empty() {
            continue;
        }
        let cols = Columns::of(&target.shown, &file.header())?;
        for record in 1..file.len() {
            let lcsc = file.value(record, cols.lcsc).trim().to_string();
            if lcsc.is_empty() {
                continue;
            }
            report.rows += 1;
            let cell = |c: Option<usize>| {
                c.map(|c| file.value(record, c).trim().to_string())
                    .filter(|d| !d.is_empty())
            };
            let key = PartKey {
                lcsc: Some(lcsc.clone()),
                mpn: cell(cols.mpn),
            };
            let aged = |date: Option<String>, days: Option<i64>, unknown: Option<String>| Aged {
                file: target.shown.clone(),
                line: file.line(record),
                lcsc: lcsc.clone(),
                date,
                days,
                unknown,
            };
            let (date, why) = match supply.lookup(&key) {
                Lookup::Quoted(q) => (Some(date_of(q.fetched)), String::new()),
                Lookup::Unknown(why) => (cell(cols.date), why),
            };
            match date {
                None => report.unknown.push(aged(None, None, Some(why))),
                Some(date) => {
                    let age = age_days(&date, today);
                    if age.is_none_or(|a| a > i64::from(days)) {
                        report.old.push(aged(Some(date), age, None));
                    }
                }
            }
        }
    }
    Ok(report)
}

pub fn render_age(report: &AgeReport) -> String {
    let mut out = String::new();
    for a in &report.old {
        let how = match (&a.date, a.days) {
            (Some(d), Some(n)) => format!("read {d}, {n} days ago"),
            (Some(d), None) => format!("date `{d}` is not like 2026-10-31"),
            (None, _) => "unknown".to_string(),
        };
        out.push_str(&format!("{}:{} {}: {how}\n", a.file, a.line, a.lcsc));
    }
    for a in &report.unknown {
        out.push_str(&format!(
            "{}:{} {}: price and stock unknown ({})\n",
            a.file,
            a.line,
            a.lcsc,
            a.unknown.as_deref().unwrap_or("no answer")
        ));
    }
    out.push_str(&format!(
        "{} of {} rows have a price and stock read more than {} days before {}; {} of {} have them unknown{}\n",
        report.old.len(),
        report.rows,
        report.days,
        report.today,
        report.unknown.len(),
        report.rows,
        if report.old.is_empty() && report.unknown.is_empty() {
            ""
        } else {
            "; configure a supply provider ([supply] in netsmith.toml, ADR 0047) or read the supplier's page by hand (`netsmith catalog refresh` does not write price or stock, ADR 0039)"
        }
    ));
    out
}
