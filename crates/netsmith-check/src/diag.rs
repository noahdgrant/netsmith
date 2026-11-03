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
