//! The pin file edited in place (ADR 0101): the editor's padlock and move change the `pin`,
//! `lock` and `source` lines of one entry, and every other byte of the file stays.

use netsmith_check::*;

/// A hand-kept file: comments, a blank-line style, a `lock` that is written, a `source` that is
/// not, and entries in an order `to_text` would not write.
const FILE: &str = "# Pins of the sensor board. Kept by hand.\nversion = 1\n\n# the status LED\n[B.pins.status]\npin = \"p01\"   # next to the LED\nfunction = \"GPIO\"\nlock = true\nsource = \"hand\"\n\n[B.pins.alert]\npin = \"p02\"\nfunction = \"GPIO\"\nsource = \"auto\"\n\n# another board's entry, which must not move\n[Other.pins.x]\npin = \"p00\"\nfunction = \"GPIO\"\nsource = \"auto\"\n";

fn entry(pin: &str, lock: bool, source: EntrySource) -> PinEntry {
    PinEntry {
        pin: pin.to_string(),
        function: "GPIO".to_string(),
        lock,
        source,
        at: None,
    }
}

#[test]
fn unlock_removes_only_the_lock_line() {
    let out = edit_entry(FILE, "B", "status", &entry("p01", false, EntrySource::Hand)).unwrap();
    assert_eq!(out, FILE.replace("lock = true\n", ""));
}

#[test]
fn lock_adds_only_the_lock_line_after_the_function() {
    let out = edit_entry(FILE, "B", "alert", &entry("p02", true, EntrySource::Auto)).unwrap();
    assert_eq!(
        out,
        FILE.replace(
            "pin = \"p02\"\nfunction = \"GPIO\"\n",
            "pin = \"p02\"\nfunction = \"GPIO\"\nlock = true\n"
        )
    );
    assert_eq!(out.lines().count(), FILE.lines().count() + 1);
    assert!(PinFile::parse(&out).unwrap().boards["B"]["alert"].lock);
}

#[test]
fn a_move_changes_the_pin_and_the_source_and_keeps_the_comment() {
    let out = edit_entry(FILE, "B", "alert", &entry("p03", false, EntrySource::Hand)).unwrap();
    assert_eq!(
        out,
        FILE.replace(
            "pin = \"p02\"\nfunction = \"GPIO\"\nsource = \"auto\"\n\n# another",
            "pin = \"p03\"\nfunction = \"GPIO\"\nsource = \"hand\"\n\n# another"
        )
    );
    // A trailing comment on the line that changes stays too.
    let out = edit_entry(FILE, "B", "status", &entry("p03", true, EntrySource::Hand)).unwrap();
    assert!(out.contains("pin = \"p03\"   # next to the LED\n"), "{out}");
}

#[test]
fn unrelated_lines_and_comments_are_byte_identical() {
    for (key, to) in [
        ("status", entry("p00", false, EntrySource::Auto)),
        ("alert", entry("p03", true, EntrySource::Hand)),
        ("new", entry("p03", true, EntrySource::Auto)),
    ] {
        let out = edit_entry(FILE, "B", key, &to).unwrap();
        for line in [
            "# Pins of the sensor board. Kept by hand.",
            "# the status LED",
            "# another board's entry, which must not move",
            "[Other.pins.x]",
            "pin = \"p00\"",
        ] {
            assert!(
                out.contains(&format!("{line}\n")),
                "{key}: lost `{line}`\n{out}"
            );
        }
        assert!(out.starts_with("# Pins of the sensor board. Kept by hand.\nversion = 1\n\n# the status LED\n[B.pins.status]\n"));
        assert!(
            out.ends_with(
                "[Other.pins.x]\npin = \"p00\"\nfunction = \"GPIO\"\nsource = \"auto\"\n"
            )
        );
        let got = PinFile::parse(&out).unwrap();
        assert_eq!(got.boards["B"][key], to, "{key}");
    }
}

#[test]
fn a_missing_source_line_is_added_after_the_last_key() {
    let text = "version = 1\n\n[B.pins.a]\npin = \"p00\"\nfunction = \"GPIO\"\n\n[B.pins.b]\npin = \"p01\"\nfunction = \"GPIO\"\n";
    let out = edit_entry(text, "B", "a", &entry("p02", false, EntrySource::Auto)).unwrap();
    assert_eq!(
        out,
        "version = 1\n\n[B.pins.a]\npin = \"p02\"\nfunction = \"GPIO\"\nsource = \"auto\"\n\n[B.pins.b]\npin = \"p01\"\nfunction = \"GPIO\"\n"
    );
    // `source` that is already what the file means (`hand` by default) is not added.
    let out = edit_entry(text, "B", "b", &entry("p03", false, EntrySource::Hand)).unwrap();
    assert!(
        !out[out.find("[B.pins.b]").unwrap()..].contains("source"),
        "{out}"
    );
}

#[test]
fn a_new_entry_goes_after_the_last_of_its_board() {
    let out = edit_entry(FILE, "B", "spare", &entry("p00", true, EntrySource::Auto)).unwrap();
    let added =
        "\n[B.pins.spare]\npin = \"p00\"\nfunction = \"GPIO\"\nlock = true\nsource = \"auto\"\n";
    assert_eq!(
        out,
        FILE.replace(
            "source = \"auto\"\n\n# another",
            &format!("source = \"auto\"\n{added}\n# another")
        )
    );
}

#[test]
fn a_new_entry_of_a_board_the_file_lacks_goes_at_the_end_and_a_missing_file_gets_a_version() {
    let out = edit_entry(FILE, "Third", "k", &entry("p00", false, EntrySource::Hand)).unwrap();
    assert!(out.starts_with(FILE));
    assert!(
        out.ends_with("\n[Third.pins.k]\npin = \"p00\"\nfunction = \"GPIO\"\nsource = \"hand\"\n")
    );
    let made = edit_entry("", "B", "hub.scl", &entry("p00", true, EntrySource::Auto)).unwrap();
    assert_eq!(
        made,
        "version = 1\n\n[B.pins.\"hub.scl\"]\npin = \"p00\"\nfunction = \"GPIO\"\nlock = true\nsource = \"auto\"\n"
    );
    // What a run would write for the same entries, but for the blank line the editor leaves.
    let commented = edit_entry(
        "# mine\n",
        "B",
        "a",
        &entry("p00", false, EntrySource::Hand),
    )
    .unwrap();
    assert!(
        commented.starts_with("# mine\nversion = 1\n"),
        "{commented}"
    );
}

#[test]
fn no_trailing_newline_and_crlf_files_are_kept() {
    let bare = "version = 1\n\n[B.pins.a]\npin = \"p00\"\nfunction = \"GPIO\"\nsource = \"auto\"";
    let out = edit_entry(bare, "B", "a", &entry("p00", true, EntrySource::Auto)).unwrap();
    assert_eq!(
        out,
        "version = 1\n\n[B.pins.a]\npin = \"p00\"\nfunction = \"GPIO\"\nlock = true\nsource = \"auto\""
    );
    let crlf = FILE.replace('\n', "\r\n");
    let out = edit_entry(&crlf, "B", "alert", &entry("p02", true, EntrySource::Auto)).unwrap();
    assert_eq!(
        out,
        crlf.replace(
            "pin = \"p02\"\r\nfunction = \"GPIO\"\r\n",
            "pin = \"p02\"\r\nfunction = \"GPIO\"\r\nlock = true\r\n"
        )
    );
    let out = edit_entry(&crlf, "B", "new", &entry("p03", false, EntrySource::Hand)).unwrap();
    assert!(
        !out.replace("\r\n", "").contains('\n'),
        "a bare newline in a CRLF file"
    );
}

#[test]
fn a_file_that_does_not_read_or_an_entry_in_another_form_is_refused_and_not_touched() {
    let broken = "version = 1\n[B.pins.a]\npin = 3\n";
    let e = edit_entry(broken, "B", "a", &entry("p00", true, EntrySource::Auto)).unwrap_err();
    assert!(e.contains("does not read"), "{e}");
    let inline =
        "version = 1\n\n[B.pins]\na = { pin = \"p00\", function = \"GPIO\", source = \"auto\" }\n";
    let e = edit_entry(inline, "B", "a", &entry("p01", false, EntrySource::Hand)).unwrap_err();
    assert!(e.contains("cannot change"), "{e}");
}
