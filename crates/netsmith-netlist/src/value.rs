//! What counts as the same value when `--loose-values` is given.

/// The key two values are compared by. Strictly it is the text, trimmed. Loosely, a value
/// that reads as a number with an SI prefix and an optional unit is its number, so `4k7`,
/// `4.7K` and `4.7kΩ` agree, as do `100n`, `0.1u` and `100nF`; any other text is compared
/// without case or spaces.
pub fn normalize_value(value: &str, loose: bool) -> String {
    let value = value.trim();
    if !loose {
        return value.to_string();
    }
    let compact: String = value.split_whitespace().collect();
    match parse_number(&compact) {
        Some(v) => format!("#{v:.5e}"),
        None => compact.to_lowercase(),
    }
}

/// `4k7`, `4.7k`, `100nF`, `0R`, `10 Ohm`: the number they stand for.
fn parse_number(s: &str) -> Option<f64> {
    let digits = |s: &str| s.chars().take_while(|c| c.is_ascii_digit()).count();
    let int = digits(s);
    let mut rest = &s[int..];
    let mut number = s[..int].to_string();
    if int == 0 && !rest.starts_with('.') {
        return None;
    }
    if let Some(after) = rest.strip_prefix('.') {
        let frac = digits(after);
        number.push('.');
        number.push_str(&after[..frac]);
        rest = &after[frac..];
    }
    let mut scale = 1.0;
    // A prefix letter; `R` is the decimal point of a bare ohm value (`4R7`). Case matters
    // only for `m` (milli) against `M` (mega).
    let prefix = rest.chars().next().and_then(|c| {
        Some(match c {
            'p' | 'P' => 1e-12,
            'n' | 'N' => 1e-9,
            'u' | 'U' | '\u{b5}' | '\u{3bc}' => 1e-6,
            'm' => 1e-3,
            'k' | 'K' => 1e3,
            'M' => 1e6,
            'G' => 1e9,
            'R' | 'r' => 1.0,
            _ => return None,
        })
    });
    if let Some(p) = prefix {
        let c = rest.chars().next()?;
        // `Meg` and `meg` are mega too.
        rest = &rest[c.len_utf8()..];
        scale = p;
        if (c == 'm' || c == 'M') && rest.to_lowercase().starts_with("eg") {
            scale = 1e6;
            rest = &rest[2..];
        }
        let frac = digits(rest);
        if frac > 0 {
            if number.contains('.') {
                return None;
            }
            number.push('.');
            number.push_str(&rest[..frac]);
            rest = &rest[frac..];
        }
    }
    let unit = rest.to_lowercase();
    if ![
        "", "f", "h", "v", "a", "w", "ohm", "ohms", "\u{3a9}", "\u{3c9}",
    ]
    .contains(&unit.as_str())
    {
        return None;
    }
    number.parse::<f64>().ok().map(|n| n * scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: &str, b: &str) -> bool {
        normalize_value(a, true) == normalize_value(b, true)
    }

    #[test]
    fn strictly_only_surrounding_space_is_ignored() {
        assert_eq!(normalize_value(" 4k7 ", false), "4k7");
        assert_ne!(
            normalize_value("4k7", false),
            normalize_value("4.7k", false)
        );
    }

    #[test]
    fn loosely_resistances_agree() {
        assert!(same("4k7", "4.7k"));
        assert!(same("4.7K", "4.7 kOhm"));
        assert!(same("4.7k\u{3a9}", "4700"));
        assert!(same("0R", "0"));
        assert!(same("4R7", "4.7"));
        assert!(same("1M", "1000k"));
        assert!(same("1Meg", "1M"));
        assert!(!same("1M", "1m"));
        assert!(!same("10k", "100k"));
    }

    #[test]
    fn loosely_capacitances_agree() {
        assert!(same("100n", "0.1u"));
        assert!(same("100nF", "100n"));
        assert!(same("1u", "1\u{b5}F"));
        assert!(!same("100n", "10n"));
    }

    #[test]
    fn other_text_is_compared_without_case_or_spaces() {
        assert!(same("LED", "led"));
        assert!(same("STM32 F103", "stm32f103"));
        assert!(!same("LED", "LED2"));
    }
}
