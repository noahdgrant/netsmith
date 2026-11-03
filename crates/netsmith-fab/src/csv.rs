//! Comma-separated values, written the way spreadsheets and JLCPCB read them: a field is
//! quoted when it holds a comma, a quote or a line break, and a quote inside is doubled.

/// One row, quoted where needed, without a line ending.
pub fn row<S: AsRef<str>>(fields: &[S]) -> String {
    let mut out = String::new();
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let f = f.as_ref();
        if f.contains([',', '"', '\n', '\r']) {
            out.push('"');
            out.push_str(&f.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(f);
        }
    }
    out
}

/// A whole file: the header, then the rows, each ended by `\n`.
pub fn file(header: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = row(header);
    out.push('\n');
    for r in rows {
        out.push_str(&row(r));
        out.push('\n');
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
pub enum CsvError {
    UnterminatedQuote { line: usize },
    MissingColumn(String),
}

impl std::fmt::Display for CsvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnterminatedQuote { line } => {
                write!(f, "line {line}: a quoted field is never closed")
            }
            Self::MissingColumn(c) => write!(f, "there is no `{c}` column in the header"),
        }
    }
}

impl std::error::Error for CsvError {}

/// A parsed file: rows of fields, header first. A leading byte-order mark and blank lines
/// are ignored.
pub fn parse(src: &str) -> Result<Vec<Vec<String>>, CsvError> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut rows = Vec::new();
    let mut fields: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut line = 1;
    let mut chars = src.chars().peekable();
    let mut finish_row = |fields: &mut Vec<String>, field: &mut String| {
        fields.push(std::mem::take(field));
        let row = std::mem::take(fields);
        if !(row.len() == 1 && row[0].is_empty()) {
            rows.push(row);
        }
    };
    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => in_quotes = false,
                '\n' => {
                    line += 1;
                    field.push(c);
                }
                _ => field.push(c),
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => fields.push(std::mem::take(&mut field)),
                '\r' => {}
                '\n' => {
                    line += 1;
                    finish_row(&mut fields, &mut field);
                }
                _ => field.push(c),
            }
        }
    }
    if in_quotes {
        return Err(CsvError::UnterminatedQuote { line });
    }
    if !field.is_empty() || !fields.is_empty() {
        finish_row(&mut fields, &mut field);
    }
    Ok(rows)
}

/// The index of each wanted column in the header row.
pub fn columns(header: &[String], wanted: &[&str]) -> Result<Vec<usize>, CsvError> {
    wanted
        .iter()
        .map(|w| {
            header
                .iter()
                .position(|h| h.trim() == *w)
                .ok_or_else(|| CsvError::MissingColumn((*w).to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(row(&["R1", "10k"]), "R1,10k");
        assert_eq!(row(&["R1,R2", "10k"]), "\"R1,R2\",10k");
        assert_eq!(
            row(&["say \"hi\"", "a\nb", "plain"]),
            "\"say \"\"hi\"\"\",\"a\nb\",plain"
        );
        assert_eq!(row(&["", "x"]), ",x");
    }

    #[test]
    fn file_ends_every_row_with_a_newline() {
        let f = file(&["A", "B"], &[vec!["1".into(), "2,3".into()]]);
        assert_eq!(f, "A,B\n1,\"2,3\"\n");
    }

    #[test]
    fn parse_round_trips_what_row_writes() {
        let rows = vec![
            vec!["C1,C2".to_string(), "say \"hi\"".to_string(), String::new()],
            vec!["multi\nline".to_string(), "x".to_string(), "y".to_string()],
        ];
        let text = file(&["a", "b", "c"], &rows);
        let parsed = parse(&text).unwrap();
        assert_eq!(parsed[0], ["a", "b", "c"]);
        assert_eq!(&parsed[1..], &rows[..]);
    }

    #[test]
    fn parse_accepts_crlf_bom_and_a_missing_final_newline() {
        let parsed = parse("\u{feff}a,b\r\n1,2\r\n\r\n3,4").unwrap();
        assert_eq!(parsed, [["a", "b"], ["1", "2"], ["3", "4"]]);
    }

    #[test]
    fn parse_reports_an_open_quote() {
        assert_eq!(
            parse("a\n\"b"),
            Err(CsvError::UnterminatedQuote { line: 2 })
        );
    }

    #[test]
    fn columns_finds_by_name() {
        let header = vec!["x".to_string(), "Rot".to_string()];
        assert_eq!(columns(&header, &["Rot", "x"]).unwrap(), [1, 0]);
        assert_eq!(
            columns(&header, &["Side"]),
            Err(CsvError::MissingColumn("Side".into()))
        );
    }
}
