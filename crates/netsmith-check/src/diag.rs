//! Diagnostics that point into any loaded source file, not just the one being checked.

use netsmith_lang::{ParseError, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceId(pub(crate) usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    pub source: SourceId,
    pub span: Span,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Sources(Vec<Source>);

impl Sources {
    pub(crate) fn add(&mut self, name: &str, text: &str) -> SourceId {
        self.0.push(Source {
            name: name.to_string(),
            text: text.to_string(),
        });
        SourceId(self.0.len() - 1)
    }

    pub fn get(&self, id: SourceId) -> &Source {
        &self.0[id.0]
    }

    /// How many files are loaded.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `file:line` of a span, if the file is known.
    pub fn place(&self, source: SourceId, span: Span) -> Option<String> {
        let source = self.0.get(source.0)?;
        let (line, _) = ParseError::new(span, "").line_col(&source.text);
        Some(format!("{}:{line}", source.name))
    }

    /// The text of a span, on one line, if the file is known: a statement as it was written.
    pub fn statement(&self, source: SourceId, span: Span) -> Option<String> {
        let source = self.0.get(source.0)?;
        let text = source
            .text
            .get(span.start..span.end.min(source.text.len()))?;
        Some(text.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}

impl Diagnostic {
    /// 1-based line and column of the start of the diagnostic.
    pub fn line_col(&self, sources: &Sources) -> (usize, usize) {
        ParseError::new(self.span, "").line_col(&sources.get(self.source).text)
    }

    pub fn render(&self, sources: &Sources) -> String {
        let source = sources.get(self.source);
        ParseError::new(self.span, self.message.clone()).render(&source.name, &source.text)
    }
}
