//! Small text helpers shared across the crate.

use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

/// Turns a name into a file or folder name: lower-case ASCII letters and digits with a hyphen
/// between words. Accents become the plain letter and other punctuation is dropped, so
/// "Sample Story" becomes "sample-story" and "K'thar" becomes "kthar".
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in deunicode::deunicode(name).chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if matches!(c, '\'' | '`' | '"') {
            continue;
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_string()
}

/// Whether `name` is already a slug: what [`slugify`] would make of it.
pub fn is_slug(name: &str) -> bool {
    !name.is_empty() && slugify(name) == name
}

/// The first of `slug`, `slug-2`, `slug-3` and so on that `taken` says is free.
pub fn unique_slug(slug: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(slug) {
        return slug.to_string();
    }
    (2..)
        .map(|n| format!("{slug}-{n}"))
        .find(|candidate| !taken(candidate))
        .expect("an unbounded range always finds a free slug")
}

/// Whether `stem` is `slug`, or `slug` with the number [`unique_slug`] adds.
pub fn slug_matches(stem: &str, slug: &str) -> bool {
    stem == slug
        || stem
            .strip_prefix(slug)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// A readable name for a note that has none of its own, from its file name: a slug such as
/// "the-long-night" reads as "The long night"; any other file name is used as it is.
pub fn name_from_file(stem: &str) -> String {
    if !is_slug(stem) {
        return stem.to_string();
    }
    let words = stem.replace('-', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => words,
    }
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
    fn more_slugs() {
        assert_eq!(slugify("K'thar"), "kthar");
        assert_eq!(slugify("Ōkami"), "okami");
        assert_eq!(slugify("Café Noël, 1984"), "cafe-noel-1984");
        assert_eq!(slugify("Captain: Reyes / #1"), "captain-reyes-1");
        assert!(is_slug("the-long-night"));
        assert!(!is_slug("The Long Night"));
        assert!(!is_slug("01 Arrival"));
        assert!(!is_slug(""));
    }

    #[test]
    fn numbered_slugs() {
        let taken = ["maras", "maras-2"];
        assert_eq!(unique_slug("maras", |s| taken.contains(&s)), "maras-3");
        assert_eq!(unique_slug("ada", |s| taken.contains(&s)), "ada");
        assert!(slug_matches("maras", "maras"));
        assert!(slug_matches("maras-2", "maras"));
        assert!(!slug_matches("maras-two", "maras"));
        assert!(!slug_matches("maras-", "maras"));
        assert!(!slug_matches("mara", "maras"));
    }

    #[test]
    fn names_from_file_names() {
        assert_eq!(name_from_file("the-long-night"), "The long night");
        assert_eq!(name_from_file("The Long Night"), "The Long Night");
        assert_eq!(name_from_file("1984"), "1984");
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
