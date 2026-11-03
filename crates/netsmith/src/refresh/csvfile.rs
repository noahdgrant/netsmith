//! A catalog file as text, with the byte span of every cell, so a refresh can change a few
//! cells and leave every other byte as it was: the order, the quoting, the line endings, the
//! final newline. Re-writing the whole file from parsed cells would pick its own quoting and
//! make a diff of every row.
//!
//! The rules for what a field is are those of `netsmith_solve::parse_csv` (a quote starts a
//! quoted field only at its start, `""` is a quote inside one, a blank line is no record).

/// Where one cell is in the text: from its opening quote, if it has one, to just after its
/// closing quote or its last character.
#[derive(Debug, Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
}

/// One line of the file that holds cells (a quoted cell may hold newlines, so a record may
/// cover several lines).
#[derive(Debug, Clone)]
struct Record {
    /// The 1-based line the record starts on, as a message names it.
    line: usize,
    cells: Vec<Span>,
}

#[derive(Debug, Clone)]
pub struct CsvFile {
    text: String,
    records: Vec<Record>,
    /// Cells to replace: the span and the text that goes there. At most one for a cell.
    edits: Vec<(Span, String)>,
}

impl CsvFile {
    /// Read `text`. `file` is what messages call it.
    pub fn parse(file: &str, text: String) -> Result<CsvFile, String> {
        let mut records: Vec<Record> = Vec::new();
        let mut cells: Vec<Span> = Vec::new();
        let mut cell_start: Option<usize> = None;
        let mut quoted = false;
        let mut started = false;
        let (mut line, mut record_line, mut quote_line) = (1, 1, 1);
        let mut chars = text.char_indices().peekable();
        let close = |cells: &mut Vec<Span>, start: Option<usize>, at: usize| {
            let start = start.unwrap_or(at);
            // A carriage return before the newline belongs to the line ending.
            let end = if at > start && text.as_bytes()[at - 1] == b'\r' {
                at - 1
            } else {
                at
            };
            cells.push(Span { start, end });
        };
        while let Some((i, c)) = chars.next() {
            if c == '\n' {
                line += 1;
            }
            if quoted {
                match c {
                    '"' if chars.peek().is_some_and(|&(_, n)| n == '"') => {
                        chars.next();
                    }
                    '"' => quoted = false,
                    _ => {}
                }
                continue;
            }
            if !started && !matches!(c, '\n' | '\r') {
                record_line = line;
            }
            match c {
                '"' if cell_start.is_none() => {
                    quoted = true;
                    quote_line = line;
                    started = true;
                    cell_start = Some(i);
                }
                ',' => {
                    close(&mut cells, cell_start.take(), i);
                    started = true;
                }
                '\r' => {}
                '\n' => {
                    if started || cell_start.is_some() {
                        close(&mut cells, cell_start.take(), i);
                        records.push(Record {
                            line: record_line,
                            cells: std::mem::take(&mut cells),
                        });
                    }
                    started = false;
                }
                _ => {
                    cell_start.get_or_insert(i);
                    started = true;
                }
            }
        }
        if quoted {
            return Err(format!(
                "{file}:{quote_line}: a quoted field is never closed"
            ));
        }
        if started || cell_start.is_some() {
            close(&mut cells, cell_start.take(), text.len());
            records.push(Record {
                line: record_line,
                cells,
            });
        }
        Ok(CsvFile {
            text,
            records,
            edits: Vec::new(),
        })
    }

    /// The number of records, the header included.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The 1-based line a record starts on.
    pub fn line(&self, record: usize) -> usize {
        self.records[record].line
    }

    /// The number of cells in a record.
    pub fn width(&self, record: usize) -> usize {
        self.records[record].cells.len()
    }

    /// The header's names, trimmed.
    pub fn header(&self) -> Vec<String> {
        (0..self.width(0))
            .map(|c| self.value(0, c).trim().to_string())
            .collect()
    }

    /// A cell as the CSV means it: without its quotes, with `""` read as `"`. The text of a
    /// cell that an edit has changed is not what this gives; call it on the file as read.
    pub fn value(&self, record: usize, cell: usize) -> String {
        let span = self.records[record].cells[cell];
        let raw = &self.text[span.start..span.end];
        match raw.strip_prefix('"') {
            Some(inner) => inner
                .strip_suffix('"')
                .unwrap_or(inner)
                .replace("\"\"", "\""),
            None => raw.to_string(),
        }
    }

    /// Whether the cell is written in quotes.
    fn is_quoted(&self, record: usize, cell: usize) -> bool {
        let span = self.records[record].cells[cell];
        self.text[span.start..span.end].starts_with('"')
    }

    /// Replace the text of a cell. The new text is written in quotes if the cell was, or if it
    /// has to be (a comma, a quote or a newline in it); an empty cell stays unquoted.
    pub fn set(&mut self, record: usize, cell: usize, value: &str) {
        let span = self.records[record].cells[cell];
        let needs = value.contains([',', '"', '\n', '\r']);
        let written = if needs || (self.is_quoted(record, cell) && !value.is_empty()) {
            format!("\"{}\"", value.replace('"', "\"\""))
        } else {
            value.to_string()
        };
        self.edits.retain(|(s, _)| s.start != span.start);
        self.edits.push((span, written));
    }

    /// Whether the text would be written as it was read.
    pub fn is_changed(&self) -> bool {
        self.edits
            .iter()
            .any(|(s, w)| self.text[s.start..s.end] != *w)
    }

    /// The text with the edits made, and nothing else changed.
    pub fn render(&self) -> String {
        let mut edits: Vec<&(Span, String)> = self.edits.iter().collect();
        edits.sort_by_key(|(s, _)| s.start);
        let mut out = String::with_capacity(self.text.len() + 64);
        let mut at = 0;
        for (span, written) in edits {
            out.push_str(&self.text[at..span.start]);
            out.push_str(written);
            at = span.end;
        }
        out.push_str(&self.text[at..]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> CsvFile {
        CsvFile::parse("t.csv", text.to_string()).unwrap()
    }

    #[test]
    fn a_file_with_no_edits_renders_as_it_was_whatever_its_quoting_and_line_endings() {
        for text in [
            "a,b,c\n1,2,3\n",
            "a,b,c\r\n1,\"x,y\",3\r\n",
            "a,b\n\"he said \"\"hi\"\"\",\n\n\n3,4",
            "a,b\n\"two\nlines\",2\n",
            "a,b\n,\n",
        ] {
            assert_eq!(parse(text).render(), text);
        }
    }

    #[test]
    fn cells_read_as_the_loader_reads_them() {
        let f = parse("a,b,c\r\n1,\"x,y\"\"z\",3\r\n\r\n4,5,6");
        assert_eq!(f.len(), 3);
        assert_eq!(f.header(), ["a", "b", "c"]);
        assert_eq!(f.value(1, 1), "x,y\"z");
        assert_eq!(f.value(1, 2), "3");
        assert_eq!((f.line(1), f.line(2)), (2, 4));
        assert_eq!(f.value(2, 2), "6");
    }

    #[test]
    fn an_edit_changes_that_cell_only() {
        let mut f = parse("a,b,c\r\n1,\"x\",3\r\n4,5,6\r\n");
        f.set(1, 0, "10");
        f.set(1, 1, "y");
        f.set(2, 2, "");
        assert_eq!(f.render(), "a,b,c\r\n10,\"y\",3\r\n4,5,\r\n");
        f.set(1, 0, "11");
        assert_eq!(f.render(), "a,b,c\r\n11,\"y\",3\r\n4,5,\r\n");
    }

    #[test]
    fn a_new_value_with_a_comma_is_quoted_and_a_quoted_empty_one_is_not() {
        let mut f = parse("a,b\n1,\"q\"\n");
        f.set(1, 0, "x,y");
        f.set(1, 1, "");
        assert_eq!(f.render(), "a,b\n\"x,y\",\n");
    }

    #[test]
    fn writing_a_cell_as_it_is_is_not_a_change() {
        let mut f = parse("a,b\n1,\"2\"\n");
        f.set(1, 0, "1");
        f.set(1, 1, "2");
        assert!(!f.is_changed());
        f.set(1, 1, "3");
        assert!(f.is_changed());
    }

    #[test]
    fn an_unclosed_quote_is_refused_with_its_line() {
        let e = CsvFile::parse("t.csv", "a\n\"b\n".to_string()).unwrap_err();
        assert_eq!(e, "t.csv:2: a quoted field is never closed");
    }
}
