//! A rule turned off for one line: `// lint: off naming` at the end of the line, or alone on the
//! line above it. Several rules are separated by commas (`// lint: off naming, redundant`). The
//! comment is a plain comment: the checker never reads it, and the linter reads it from the
//! text, since the lexer drops comments.

use crate::Rule;

/// The words that start the comment.
const MARK: &str = "// lint: off ";

/// The rules a line's comment turns off, if it has one: what follows `// lint: off ` up to the
/// end of the line, split at commas, each a rule's name. A word that names no rule is left out
/// here and refused by [`unknown`].
fn rules_of(line: &str) -> Option<Vec<&str>> {
    let at = line.find(MARK)?;
    // A `///` or `//!` line is documentation, not a comment to the linter.
    if line[..at].ends_with('/') {
        return None;
    }
    Some(
        line[at + MARK.len()..]
            .split(',')
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .collect(),
    )
}

/// Whether the 1-based `line` of `text` has `rule` turned off: by a comment at its end, or by
/// a comment alone on the line above it.
pub fn is_off(text: &str, line: usize, rule: Rule) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    let names =
        |i: usize| -> Vec<&str> { lines.get(i).and_then(|l| rules_of(l)).unwrap_or_default() };
    if line == 0 {
        return false;
    }
    if names(line - 1).contains(&rule.name()) {
        return true;
    }
    line >= 2
        && lines[line - 2].trim_start().starts_with("//")
        && names(line - 2).contains(&rule.name())
}

/// The words of `// lint: off` comments in `text` that name no rule, each with its 1-based line:
/// a misspelt rule would turn nothing off without a word.
pub fn unknown(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .flat_map(|(i, l)| {
            rules_of(l)
                .unwrap_or_default()
                .into_iter()
                .filter(|w| Rule::from_name(w).is_none())
                .map(move |w| (i + 1, w.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_turns_a_rule_off_on_its_line_or_the_next() {
        let text = "a // lint: off naming\n// lint: off redundant, naming\nb\nc\n";
        assert!(is_off(text, 1, Rule::Naming));
        assert!(!is_off(text, 1, Rule::Redundant));
        assert!(is_off(text, 3, Rule::Redundant));
        assert!(is_off(text, 3, Rule::Naming));
        assert!(!is_off(text, 4, Rule::Naming));
    }

    #[test]
    fn a_trailing_comment_does_not_reach_the_next_line() {
        let text = "a // lint: off naming\nb\n";
        assert!(!is_off(text, 2, Rule::Naming));
    }

    #[test]
    fn a_doc_comment_is_not_read_and_a_misspelt_rule_is_named() {
        assert!(!is_off("/// lint: off naming\nb\n", 2, Rule::Naming));
        assert_eq!(
            unknown("a // lint: off namign\n"),
            [(1, "namign".to_string())]
        );
    }
}
