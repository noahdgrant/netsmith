//! Running Windows KiCad one process at a time, and again when WSL fails to start it.
//!
//! Under WSL, `kicad-cli.exe`, KiCad's `python.exe` and `cmd.exe` are started through WSL's
//! interop channel. When many of them start at once (the tests of one `cargo test` run, and
//! the runs of several agents on one machine), the channel sometimes gives up before the
//! program runs: the program "fails" with only `WSL (...) ERROR: UtilAcceptVsock:273:
//! accept4 failed 110` (a timeout) on stderr. KiCad also keeps per-user configuration and
//! lock files on the Windows side that two processes should not write at once.
//!
//! So every launch goes through [`run`], which
//!
//! 1. takes a lock that no other KiCad launch of this process or of any other process on the
//!    machine holds (a mutex, then `flock` on a file in the temp directory), waiting for it
//!    for a long time and saying who it waited for if it gives up, and
//! 2. starts the program again, a bounded number of times, when the failure is the interop
//!    channel's or a signal's rather than the program's, and keeps what each failed attempt
//!    said so that the final report shows it.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::kicad::CliError;

/// The longest one launch waits for the others (a launch takes seconds; a whole test run
/// queued ahead of it takes minutes).
pub(crate) const LOCK_WAIT: Duration = Duration::from_secs(30 * 60);

/// How many times a program is started when the interop channel fails it.
pub(crate) const ATTEMPTS: u32 = 3;

/// The pause after the first failed attempt; the second waits twice as long.
pub(crate) const BACKOFF: Duration = Duration::from_secs(2);

/// The lock file every process on the machine shares: it is in the system's temp directory.
pub(crate) fn lock_path() -> PathBuf {
    std::env::temp_dir().join("netsmith-kicad.lock")
}

static IN_PROCESS: Mutex<()> = Mutex::new(());

/// Held while one KiCad program runs; releases both locks when dropped.
#[derive(Debug)]
pub(crate) struct Guard {
    _file: File,
    _threads: MutexGuard<'static, ()>,
}

/// Takes the lock on `path`, waiting up to `wait` for the other launches (of this process,
/// then of other processes) to finish.
pub(crate) fn lock(path: &Path, wait: Duration) -> Result<Guard, String> {
    let started = std::time::Instant::now();
    let threads = IN_PROCESS.lock().unwrap_or_else(|e| e.into_inner());
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .map_err(|e| format!("cannot open the KiCad lock file {}: {e}", path.display()))?;
    let step = Duration::from_millis(50);
    loop {
        match file.try_lock() {
            Ok(()) => {
                return Ok(Guard {
                    _file: file,
                    _threads: threads,
                });
            }
            Err(fs::TryLockError::WouldBlock) if started.elapsed() < wait => thread::sleep(step),
            Err(fs::TryLockError::WouldBlock) => {
                return Err(format!(
                    "another process held the KiCad lock {} for more than {} seconds (it is \
                     taken for each kicad-cli run, so a test run or fab of another checkout is \
                     probably still going; stop it, or remove the file if nothing is running)",
                    path.display(),
                    wait.as_secs_f64()
                ));
            }
            Err(fs::TryLockError::Error(e)) => {
                return Err(format!("cannot lock {}: {e}", path.display()));
            }
        }
    }
}

/// What a launch produced, and what the attempts before it said.
#[derive(Debug)]
pub struct Ran {
    pub output: Output,
    /// For each attempt that was thrown away: why, and what it printed.
    pub earlier: Vec<String>,
}

impl Ran {
    pub fn success(&self) -> bool {
        self.output.status.success()
    }

    /// The exit status, both output streams, and the earlier attempts, for an error message.
    pub fn report(&self) -> String {
        let mut text = format!(
            "{}{}",
            String::from_utf8_lossy(&self.output.stdout),
            String::from_utf8_lossy(&self.output.stderr)
        );
        if !text.ends_with('\n') && !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&format!("({})", describe(&self.output)));
        for (i, e) in self.earlier.iter().enumerate() {
            text.push_str(&format!(
                "\nearlier attempt {} of {ATTEMPTS}, started again: {e}",
                i + 1
            ));
        }
        text
    }
}

fn describe(output: &Output) -> String {
    match output.status.code() {
        Some(code) => format!("exit status {code}"),
        None => format!("ended by a signal: {}", output.status),
    }
}

/// Why a finished program's failure is WSL's or the system's and not the program's, or
/// `None` if it succeeded or failed for its own reasons (those are not repeated).
pub(crate) fn transient(output: &Output) -> Option<String> {
    if output.status.success() {
        return None;
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.status.code().is_none() {
        return Some(format!("ended by a signal ({})", output.status));
    }
    // `<3>WSL (40093 - ) ERROR: UtilAcceptVsock:273: accept4 failed 110`
    text.lines()
        .find(|l| l.contains("WSL (") && l.contains("ERROR:"))
        .map(|l| format!("WSL could not start the program: {}", l.trim()))
}

/// Starts the program `make` builds, alone on the machine, and again (up to [`ATTEMPTS`]
/// times) if WSL fails to start it. `make` builds a fresh `Command` for each attempt. The
/// `Err` is a launch that cannot be repeated (no such program, or the lock was never free).
pub fn run(make: impl Fn() -> Command) -> Result<Ran, CliError> {
    run_with(make, &lock_path(), LOCK_WAIT, BACKOFF)
}

/// [`run`] with its lock file, patience and pause chosen.
pub(crate) fn run_with(
    make: impl Fn() -> Command,
    lock_file: &Path,
    wait: Duration,
    backoff: Duration,
) -> Result<Ran, CliError> {
    let mut earlier = Vec::new();
    for attempt in 1..=ATTEMPTS {
        let launched = {
            let _held = lock(lock_file, wait).map_err(CliError::Io)?;
            let mut command = make();
            let shown = format!("{command:?}");
            command.output().map_err(|e| (shown, e))
        };
        let reason = match launched {
            Ok(output) => match transient(&output) {
                Some(reason) if attempt < ATTEMPTS => {
                    earlier.push(format!(
                        "{reason}; it printed: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                    reason
                }
                _ => return Ok(Ran { output, earlier }),
            },
            Err((shown, e)) => {
                let message = format!("cannot run {shown}: {e}");
                if e.kind() == std::io::ErrorKind::NotFound || attempt == ATTEMPTS {
                    let mut text = message;
                    for (i, p) in earlier.iter().enumerate() {
                        text.push_str(&format!("\nearlier attempt {} of {ATTEMPTS}: {p}", i + 1));
                    }
                    return Err(CliError::Io(text));
                }
                earlier.push(message.clone());
                message
            }
        };
        eprintln!("netsmith: attempt {attempt} of {ATTEMPTS} failed ({reason}); starting it again");
        thread::sleep(backoff * attempt);
    }
    unreachable!("the last attempt returns")
}

/// A name for a scratch directory that no other directory made by this process, by another
/// process, or by an earlier run that used the same process id has: the process id, a
/// counter of this process, and the time.
pub(crate) fn scratch_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "netsmith-fab-{}-{}-{nanos:x}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_testkit::TempFile;
    use std::collections::HashSet;
    use std::sync::atomic::AtomicUsize;

    fn temp(name: &str) -> TempFile {
        TempFile::new("fab-exclusive", name)
    }

    #[test]
    fn scratch_names_do_not_repeat_across_threads() {
        let names: Vec<String> = (0..8)
            .map(|_| thread::spawn(|| (0..500).map(|_| scratch_name()).collect::<Vec<_>>()))
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        let unique: HashSet<_> = names.iter().collect();
        assert_eq!(
            unique.len(),
            names.len(),
            "repeated names among {}",
            names.len()
        );
        assert!(names[0].starts_with(&format!("netsmith-fab-{}-", std::process::id())));
    }

    #[test]
    fn only_one_launch_holds_the_lock_at_a_time() {
        let path = temp("mutual.lock");
        static INSIDE: AtomicUsize = AtomicUsize::new(0);
        static MOST: AtomicUsize = AtomicUsize::new(0);
        let workers: Vec<_> = (0..6)
            .map(|_| {
                let path = path.to_path_buf();
                thread::spawn(move || {
                    for _ in 0..5 {
                        let _held = lock(&path, Duration::from_secs(30)).unwrap();
                        let now = INSIDE.fetch_add(1, Ordering::SeqCst) + 1;
                        MOST.fetch_max(now, Ordering::SeqCst);
                        thread::sleep(Duration::from_millis(2));
                        INSIDE.fetch_sub(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().unwrap();
        }
        assert_eq!(MOST.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn another_process_holding_the_lock_makes_a_launch_wait_then_say_so() {
        let path = temp("other-process.lock");
        // A different open file description conflicts even inside this process, as another
        // process's would; the in-process mutex is not taken for it.
        let other = File::options()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .unwrap();
        other.lock().unwrap();
        let e = lock(&path, Duration::from_millis(200)).unwrap_err();
        assert!(
            e.contains(&path.display().to_string()) && e.contains("for more than 0.2 seconds"),
            "{e}"
        );
        drop(other);
        lock(&path, Duration::from_millis(200)).unwrap();
    }

    fn shell(script: &str) -> impl Fn() -> Command {
        let script = script.to_string();
        move || {
            let mut c = Command::new("sh");
            c.args(["-c", &script]);
            c
        }
    }

    const WSL_FAILURE: &str =
        "echo '<3>WSL (40093 - ) ERROR: UtilAcceptVsock:273: accept4 failed 110' >&2; exit 1";

    #[test]
    fn a_wsl_failure_is_started_again_and_the_second_try_counts() {
        let (lock_file, marker) = (temp("retry.lock"), temp("retry.marker"));
        let script = format!(
            "if [ -e '{m}' ]; then echo fine; else touch '{m}'; {WSL_FAILURE}; fi",
            m = marker.display()
        );
        let ran = run_with(
            shell(&script),
            &lock_file,
            Duration::from_secs(5),
            Duration::from_millis(1),
        )
        .unwrap();
        assert!(ran.success());
        assert_eq!(String::from_utf8_lossy(&ran.output.stdout).trim(), "fine");
        assert_eq!(ran.earlier.len(), 1);
        assert!(
            ran.earlier[0].contains("accept4 failed 110"),
            "{:?}",
            ran.earlier
        );
    }

    #[test]
    fn a_wsl_failure_that_keeps_happening_is_reported_with_every_attempt() {
        let lock_file = temp("again.lock");
        let ran = run_with(
            shell(WSL_FAILURE),
            &lock_file,
            Duration::from_secs(5),
            Duration::from_millis(1),
        )
        .unwrap();
        assert!(!ran.success());
        assert_eq!(ran.earlier.len() as u32, ATTEMPTS - 1);
        let report = ran.report();
        assert!(report.contains("accept4 failed 110"), "{report}");
        assert!(report.contains("exit status 1"), "{report}");
        assert!(report.contains("earlier attempt 2 of 3"), "{report}");
    }

    #[test]
    fn a_failure_of_the_program_itself_is_not_repeated() {
        let (lock_file, counter) = (temp("own.lock"), temp("own.count"));
        let script = format!(
            "echo x >> '{c}'; echo 'bad board' >&2; exit 3",
            c = counter.display()
        );
        let ran = run_with(
            shell(&script),
            &lock_file,
            Duration::from_secs(5),
            Duration::ZERO,
        )
        .unwrap();
        assert!(!ran.success() && ran.earlier.is_empty());
        assert!(ran.report().contains("bad board") && ran.report().contains("exit status 3"));
        assert_eq!(fs::read_to_string(&counter).unwrap().lines().count(), 1);
    }

    #[test]
    fn a_program_killed_by_a_signal_is_started_again() {
        let (lock_file, marker) = (temp("signal.lock"), temp("signal.marker"));
        let script = format!(
            "if [ -e '{m}' ]; then echo fine; else touch '{m}'; kill -9 $$; fi",
            m = marker.display()
        );
        let ran = run_with(
            shell(&script),
            &lock_file,
            Duration::from_secs(5),
            Duration::ZERO,
        )
        .unwrap();
        assert!(ran.success());
        assert!(ran.earlier[0].contains("signal"), "{:?}", ran.earlier);
    }

    #[test]
    fn a_missing_program_is_reported_at_once_with_its_command() {
        let lock_file = temp("missing.lock");
        let e = run_with(
            || Command::new("/nonexistent/kicad-cli.exe"),
            &lock_file,
            Duration::from_secs(5),
            Duration::ZERO,
        )
        .unwrap_err();
        assert!(e.to_string().contains("/nonexistent/kicad-cli.exe"), "{e}");
    }
}
