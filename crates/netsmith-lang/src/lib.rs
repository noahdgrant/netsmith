//! The netsmith language front-end: source text to syntax tree.

pub mod ast;
pub mod error;
mod group;
mod lexer;
mod parser;
mod printer;

pub use error::{ParseError, Span, floor_boundary};
pub use group::expand_groups;
pub use parser::{MAX_GROUP, parse, parse_expr};

/// A number that says what the program in `src` is, apart from its plain comments: the tokens (doc
/// comments are tokens: they are what the schematic's notes say) with the line and column of each.
/// Two texts with the same number build the same design with every part, net and problem at the
/// same place, so a change to a `//` comment (not one that adds or removes a line) needs no new
/// analysis. `None` if the text does not lex.
pub fn code_fingerprint(src: &str) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let tokens = lexer::lex(src).ok()?;
    let mut starts = vec![0usize];
    starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for token in &tokens {
        // A newline stands at the end of its line, after any comment on it: its column says nothing about the
        // program (the line of the next token does).
        if token.tok == lexer::Tok::Newline {
            format!("{:?}", token.tok).hash(&mut hasher);
            continue;
        }
        let line = starts.partition_point(|&s| s <= token.span.start) - 1;
        let col = src[starts[line]..token.span.start].chars().count();
        format!("{:?}", token.tok).hash(&mut hasher);
        (line, col).hash(&mut hasher);
    }
    Some(hasher.finish())
}

#[cfg(test)]
mod fingerprint_tests {
    use super::code_fingerprint as fp;

    #[test]
    fn a_plain_comment_does_not_change_it() {
        let code = "board B {\n    .r: R // note\n}\n";
        assert_eq!(
            fp(code),
            fp("board B {\n    .r: R // another note, longer\n}\n")
        );
        assert_eq!(fp(code), fp("board B {\n    .r: R\n}\n"));
        // A comment on an empty line, and one that makes a line longer, change no position.
        assert_eq!(fp("a\nb\n"), fp("a // x\nb\n"));
        assert_eq!(fp("a\n// x\nb\n"), fp("a\n// something else\nb\n"));
    }

    #[test]
    fn a_doc_comment_does_change_it() {
        assert_ne!(fp("/// one\nboard B {}\n"), fp("/// two\nboard B {}\n"));
        assert_ne!(fp("//! one\nboard B {}\n"), fp("//! two\nboard B {}\n"));
        // A doc comment is not a plain one when it has a fourth slash: that is plain.
        assert_eq!(fp("//// one\nboard B {}\n"), fp("//// two\nboard B {}\n"));
    }

    #[test]
    fn a_line_added_or_removed_changes_it_because_everything_below_moves() {
        assert_ne!(fp("a\nb\n"), fp("a\n// new line\nb\n"));
        assert_ne!(fp("a\nb\n"), fp("a\n\nb\n"));
    }

    #[test]
    fn code_changes_it_and_so_does_its_place() {
        assert_ne!(fp("a b\n"), fp("a c\n"));
        assert_ne!(fp("a b\n"), fp("a  b\n"));
        assert_ne!(fp("a b\n"), fp("a b c\n"));
    }

    #[test]
    fn a_slash_pair_in_a_string_is_text() {
        assert_ne!(fp("a \"x//y\"\n"), fp("a \"x//z\"\n"));
    }

    #[test]
    fn text_that_does_not_lex_has_none() {
        assert_eq!(fp("a \"unterminated\n"), None);
    }
}
