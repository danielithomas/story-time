//! Small text helpers shared across the crate.

use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

/// Turns a title into a folder name: "Sample Story" becomes "sample-story".
pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if c == '\'' || c == '’' {
            continue;
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_string()
}

/// Removes characters that Obsidian or common filesystems reject in note names.
pub fn note_file_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(
                    c,
                    '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']'
                )
        })
        .collect();
    let joined = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.trim_start_matches('.').trim().to_string()
}

/// Formats a number with thousands separators: 90000 becomes "90,000".
pub fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if n < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Compares strings the way people expect file names to sort: "2 Arrival" before "10 Departure".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                match take_number(&mut a).cmp(&take_number(&mut b)) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            (Some(x), Some(y)) => {
                match x.to_lowercase().cmp(y.to_lowercase()) {
                    Ordering::Equal => {}
                    other => return other,
                }
                a.next();
                b.next();
            }
        }
    }
}

fn take_number(chars: &mut Peekable<Chars>) -> u128 {
    let mut n: u128 = 0;
    while let Some(d) = chars.peek().and_then(|c| c.to_digit(10)) {
        n = n.saturating_mul(10).saturating_add(u128::from(d));
        chars.next();
    }
    n
}

/// The number a file name starts with, if any: "03 Arrival" gives 3.
pub fn leading_number(name: &str) -> Option<u32> {
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// A file name without its ordering prefix: "03 - Arrival" gives "Arrival".
pub fn strip_leading_number(name: &str) -> &str {
    let rest = name.trim_start_matches(|c: char| c.is_ascii_digit());
    if rest.len() == name.len() {
        return name;
    }
    let rest =
        rest.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '.' | ':'));
    if rest.is_empty() { name } else { rest }
}

/// Whether `needle` occurs in `haystack` as a whole word or phrase ("Mara" matches "Mara's", not "Marana").
pub fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    haystack.match_indices(needle).any(|(i, _)| {
        let before = haystack[..i].chars().next_back();
        let after = haystack[i + needle.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// Escapes a value for use inside a double-quoted YAML string.
pub fn yaml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("Sample Story"), "sample-story");
        assert_eq!(slugify("  The Hero's Return!  "), "the-heros-return");
        assert_eq!(
            slugify("API: Cross-Platform Notes"),
            "api-cross-platform-notes"
        );
        assert_eq!(slugify("!!!"), "");
    }

    #[test]
    fn note_names() {
        assert_eq!(note_file_name("Captain: Reyes / #1"), "Captain Reyes 1");
        assert_eq!(note_file_name("  .hidden  name "), "hidden name");
    }

    #[test]
    fn numbers() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(90000), "90,000");
        assert_eq!(thousands(-1234567), "-1,234,567");
    }

    #[test]
    fn natural_order() {
        let mut names = vec![
            "10 Departure",
            "2 Arrival",
            "01 Opening",
            "Epilogue",
            "1 Prologue",
        ];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            names,
            [
                "01 Opening",
                "1 Prologue",
                "2 Arrival",
                "10 Departure",
                "Epilogue"
            ]
        );
    }

    #[test]
    fn chapter_prefixes() {
        assert_eq!(leading_number("03 Arrival"), Some(3));
        assert_eq!(leading_number("Arrival"), None);
        assert_eq!(strip_leading_number("03 - Arrival"), "Arrival");
        assert_eq!(strip_leading_number("Chapter 3"), "Chapter 3");
        assert_eq!(strip_leading_number("1984"), "1984");
    }

    #[test]
    fn whole_words() {
        assert!(contains_word("Mara's ship", "Mara"));
        assert!(contains_word("said Mara.", "Mara"));
        assert!(!contains_word("Marana", "Mara"));
        assert!(contains_word("the Old Port at dusk", "Old Port"));
        assert!(!contains_word("anything", ""));
    }
}
