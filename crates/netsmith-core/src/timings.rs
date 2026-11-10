//! Seconds per stage, for whoever asks (`NETSMITH_TIMINGS=1`).
//!
//! Each stage of a run is wrapped in [`stage`]. With the variable unset (or empty, or `0`) it
//! runs the work and does nothing else, so the results and the output of a run are the same
//! bytes as without it. With the variable set it prints one line per stage to stderr when the
//! stage ends:
//!
//! ```text
//! timing: solve LedExample 12.3 ms
//! ```
//!
//! A stage inside another (`solve.dc` is part of `solve`) prints when it ends, so before the
//! line of the stage around it, and its time is counted in that one too.

use std::{ffi::OsStr, time::Duration, time::Instant};

/// The variable that turns the lines on.
pub const VARIABLE: &str = "NETSMITH_TIMINGS";

/// Whether the value of [`VARIABLE`] asks for timings: set, and neither empty nor `0`.
pub fn wanted(value: Option<&OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty() && v != OsStr::new("0"))
}

/// Whether this run prints timings.
pub fn enabled() -> bool {
    wanted(std::env::var_os(VARIABLE).as_deref())
}

/// The line for a stage that took `elapsed`: `timing: <stage> <subject> <time>`, with no subject
/// when there is none.
pub fn line(stage: &str, subject: &str, elapsed: Duration) -> String {
    let ms = elapsed.as_secs_f64() * 1000.0;
    let time = if ms >= 1000.0 {
        format!("{:.3} s", ms / 1000.0)
    } else {
        format!("{ms:.1} ms")
    };
    if subject.is_empty() {
        format!("timing: {stage} {time}")
    } else {
        format!("timing: {stage} {subject} {time}")
    }
}

/// Runs `work` and, when timings are wanted, says on stderr how long it took.
pub fn stage<R>(stage: &str, subject: &str, work: impl FnOnce() -> R) -> R {
    timed(enabled(), stage, subject, &mut std::io::stderr(), work)
}

/// [`stage`] with the choice and the sink given, for a test.
pub fn timed<R>(
    on: bool,
    stage: &str,
    subject: &str,
    sink: &mut dyn std::io::Write,
    work: impl FnOnce() -> R,
) -> R {
    if !on {
        return work();
    }
    let started = Instant::now();
    let result = work();
    // A closed stderr is no reason to stop a run.
    let _ = writeln!(sink, "{}", line(stage, subject, started.elapsed()));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_printed_when_it_is_not_wanted() {
        let mut sink = Vec::new();
        let result = timed(false, "solve", "Board", &mut sink, || 7);
        assert_eq!(result, 7);
        assert!(sink.is_empty());
    }

    #[test]
    fn the_variable_unset_empty_or_zero_is_not_a_request() {
        assert!(!wanted(None));
        assert!(!wanted(Some(OsStr::new(""))));
        assert!(!wanted(Some(OsStr::new("0"))));
        assert!(wanted(Some(OsStr::new("1"))));
        assert!(wanted(Some(OsStr::new("yes"))));
    }

    #[test]
    fn one_line_is_printed_per_stage_with_its_name_and_subject() {
        let mut sink = Vec::new();
        let result = timed(true, "solve", "Board", &mut sink, || "done");
        assert_eq!(result, "done");
        let text = String::from_utf8(sink).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.starts_with("timing: solve Board "), "{text}");
        assert!(text.trim_end().ends_with(" ms"), "{text}");
    }

    #[test]
    fn a_long_stage_is_told_in_seconds() {
        assert_eq!(
            line("route", "Board", Duration::from_millis(2500)),
            "timing: route Board 2.500 s"
        );
        assert_eq!(
            line("check", "", Duration::from_micros(1500)),
            "timing: check 1.5 ms"
        );
    }
}
