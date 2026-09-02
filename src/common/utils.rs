//! General utilities: pure text processing, math, formatting, color mapping, and Web/DOM helpers.
use std::borrow::Cow;

use crate::common::constants::NA;

/// Case-insensitive ASCII substring search without heap allocations.
pub const fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() {
        return true;
    }
    if n.len() > h.len() {
        return false;
    }
    let max_start = h.len() - n.len();
    let mut i = 0;
    while i <= max_start {
        let mut j = 0;
        let mut matches = true;
        while j < n.len() {
            if !h[i + j].eq_ignore_ascii_case(&n[j]) {
                matches = false;
                break;
            }
            j += 1;
        }
        if matches {
            return true;
        }
        i += 1;
    }
    false
}

/// Formats a string value, returning `NA` ("N/A") if empty.
pub fn format_or_na(val: &str) -> &str {
    if val.is_empty() { NA } else { val }
}

/// Formats a list of strings joined by `", "`, returning `NA` ("N/A") if empty or all strings are empty.
pub fn format_list_or_na(list: &[String]) -> Cow<'_, str> {
    if list.is_empty() || list.iter().all(|s| s.is_empty()) {
        Cow::Borrowed(NA)
    } else {
        Cow::Owned(list.join(", "))
    }
}

/// Trims surrounding whitespace and single/double quotes from a unit name string.
pub fn clean_unit_name(name: &str) -> &str {
    name.trim().trim_matches('\'').trim_matches('"')
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_contains_ignore_ascii_case() {
        assert!(contains_ignore_ascii_case(
            "AUTOMOTIVE_DESIGN",
            "automotive"
        ));
        assert!(contains_ignore_ascii_case(
            "CONFIG_CONTROL_DESIGN",
            "DESIGN"
        ));
        assert!(contains_ignore_ascii_case("AP203", "ap203"));
        assert!(contains_ignore_ascii_case("anything", ""));
        assert!(!contains_ignore_ascii_case("short", "longer_needle"));
        assert!(!contains_ignore_ascii_case("hello world", "xyz"));
    }

    #[wasm_bindgen_test]
    fn test_format_or_na() {
        assert_eq!(format_or_na(""), "N/A");
        assert_eq!(format_or_na("hello"), "hello");
    }

    #[wasm_bindgen_test]
    fn test_format_list_or_na() {
        assert_eq!(format_list_or_na(&[]), "N/A");
        assert_eq!(format_list_or_na(&[String::new(), String::new()]), "N/A");
        assert_eq!(
            format_list_or_na(&["Alice".to_string(), "Bob".to_string()]),
            "Alice, Bob"
        );
    }

    #[wasm_bindgen_test]
    fn test_clean_unit_name() {
        assert_eq!(clean_unit_name("  'MM'  "), "MM");
        assert_eq!(clean_unit_name("\"INCH\""), "INCH");
        assert_eq!(clean_unit_name("  MILLIMETRE  "), "MILLIMETRE");
    }
}
