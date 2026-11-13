//! The record a refresh reads, parsed from LCSC's product endpoint (ADR 0039 says which and
//! what it gives). Only the fields netsmith uses are read; anything else the
//! answer holds is ignored. An answer that has not the shape the endpoint had when this was
//! written is an error that says which field is missing, so a changed endpoint is reported as
//! one and not as a part that has gone.
//!
//! JLCPCB's parts library is not read at all (ADR 0039, the 2026-11-02 decision).

use serde_json::Value;

/// The part as LCSC lists it. The manufacturer's number and the package are what the row is
/// checked against. The stock is LCSC's own and is only ever reported; it is `None` if the
/// answer has no such field. LCSC's prices are not read: nothing uses them.
#[derive(Debug, Clone, PartialEq)]
pub struct Product {
    pub lcsc: String,
    pub mpn: String,
    pub package: Option<String>,
    /// LCSC's own stock, `stockNumber`. This is not JLCPCB's stock.
    pub stock: Option<u64>,
}

fn shape(what: &str, field: &str) -> String {
    format!("{what} has no `{field}` of the kind it had when netsmith was written")
}

/// LCSC's record of `lcsc`. `Ok(None)` is the answer for a number LCSC has no record of
/// (`"result": null`).
pub fn parse_product(body: &str, lcsc: &str) -> Result<Option<Product>, String> {
    const WHAT: &str = "LCSC's product answer";
    let json: Value = serde_json::from_str(body)
        .map_err(|e| format!("{WHAT} is not JSON ({e}); it starts {}", start_of(body)))?;
    let result = json.get("result").ok_or_else(|| shape(WHAT, "result"))?;
    if result.is_null() {
        return Ok(None);
    }
    let text = |field: &str| {
        result
            .get(field)
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
    };
    let code =
        text("productCode").ok_or_else(|| shape(&format!("{WHAT} for {lcsc}"), "productCode"))?;
    if code != lcsc {
        return Err(format!("{WHAT} for {lcsc} is the record of {code}"));
    }
    Ok(Some(Product {
        lcsc: code,
        mpn: text("productModel")
            .ok_or_else(|| shape(&format!("{WHAT} for {lcsc}"), "productModel"))?,
        package: text("encapStandard"),
        // A stock that is absent or of another kind is not an error: nothing is then said.
        stock: result.get("stockNumber").and_then(Value::as_u64),
    }))
}

/// The first characters of a body that is not what was expected, for a message.
fn start_of(body: &str) -> String {
    let head: String = body.trim().chars().take(60).collect();
    format!("{head:?}")
}

/// Whether the package of the row (netsmith's name for it, `SOT-23`) is the one the supplier
/// lists. The suppliers add the pin count (`SOT-23-3L`) or name the package twice
/// (`SMA(DO-214AC)`, `DO-214AC(SMA)`), so the row's package must be the supplier's or one of its
/// words, with a trailing `-<pins>L` taken off. The manufacturer's number is the check that
/// the part is the same part; this only catches a number that was given to another package.
pub fn package_matches(row: &str, listed: &str) -> bool {
    let norm = |s: &str| s.trim().to_ascii_uppercase();
    let row = norm(row);
    let listed = norm(listed);
    if row == listed {
        return true;
    }
    listed
        .split(['(', ')', ',', '/', ' '])
        .filter(|w| !w.is_empty())
        .any(|word| word == row || strip_pins(word) == row)
}

/// `SOT-23-3L` is `SOT-23`: a trailing dash, a digit and an `L`.
fn strip_pins(word: &str) -> &str {
    let bytes = word.as_bytes();
    if let [.., b'-', d, b'L'] = bytes
        && d.is_ascii_digit()
    {
        return &word[..word.len() - 3];
    }
    word
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packages_the_suppliers_spell_differently_still_match() {
        assert!(package_matches("0402", "0402"));
        assert!(package_matches("SOT-23", "SOT-23-3L"));
        assert!(package_matches("SMA", "SMA(DO-214AC)"));
        assert!(package_matches("SMA", "DO-214AC(SMA)"));
        assert!(package_matches("sod-123", "SOD-123"));
        assert!(!package_matches("0603", "0402"));
        assert!(!package_matches("SOT-23-5", "SOT-23-3L"));
        assert!(!package_matches("SMB", "SMA(DO-214AC)"));
    }

    #[test]
    fn a_body_that_is_not_the_expected_json_names_what_is_wrong() {
        let e = parse_product("<html>blocked</html>", "C1").unwrap_err();
        assert!(
            e.contains("is not JSON") && e.contains("<html>blocked"),
            "{e}"
        );
        let e = parse_product(r#"{"code":200}"#, "C1").unwrap_err();
        assert!(e.contains("`result`"), "{e}");
        let e = parse_product(r#"{"result":{"productCode":"C1"}}"#, "C1").unwrap_err();
        assert!(e.contains("`productModel`"), "{e}");
        let e = parse_product(
            r#"{"result":{"productCode":"C2","productModel":"X"}}"#,
            "C1",
        )
        .unwrap_err();
        assert!(e.contains("is the record of C2"), "{e}");
    }

    #[test]
    fn a_stock_the_answer_lacks_is_left_out_and_not_guessed() {
        let bare = parse_product(
            r#"{"result":{"productCode":"C1","productModel":"X"}}"#,
            "C1",
        )
        .unwrap()
        .unwrap();
        assert_eq!(bare.stock, None);
        let full = parse_product(
            r#"{"result":{"productCode":"C1","productModel":"X","stockNumber":7}}"#,
            "C1",
        )
        .unwrap()
        .unwrap();
        assert_eq!(full.stock, Some(7));
    }
}
