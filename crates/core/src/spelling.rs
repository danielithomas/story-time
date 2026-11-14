//! Near-miss spellings of the world bible's names: words in the scenes that are a typo away from a
//! character's, place's or world term's name (*Marra* for *Mara*), or differ from one only in
//! capitalisation (*Mckay* for *McKay*). An ordinary spellchecker passes invented names by, so
//! this is the style sheet a copy editor would keep.
//!
//! Names are full of ordinary words ("The Binder"), so only telling words count as terms: words of
//! four or more letters, capitalised in the name. Only capitalised words in the prose are compared
//! with them, and a word the scenes also use in lower case is taken to be an ordinary word. A
//! variant that is meant, such as a nickname, stops being flagged once it's one of the note's
//! other names.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

use crate::text::edit_distance;

/// A word in the scenes that is nearly one of the world bible's names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NearMiss {
    /// The word as the scenes have it.
    pub word: String,
    /// The word of the name it is close to, such as "Mara".
    pub term: String,
    /// The note whose name it is, and that note's name.
    pub note: PathBuf,
    pub name: String,
    /// It differs from the term only in capitalisation.
    pub case_only: bool,
    /// The scenes it appears in, by file name.
    pub scenes: BTreeSet<String>,
}

/// A name in the world bible: the note it belongs to, the note's own name, and the name (its
/// title or one of its other names).
pub struct BibleName {
    pub note: PathBuf,
    pub title: String,
    pub name: String,
}

const MIN_LETTERS: usize = 4;

/// Small words that start many names ("The Binder") and say nothing about them.
const ORDINARY: [&str; 12] = [
    "the", "and", "of", "in", "on", "at", "to", "for", "from", "by", "with", "an",
];

/// Words in `scenes` (each a file name and its prose) that nearly match a word of one of `names`,
/// leaving alone the words in `known`, such as book and scene titles. Sorted by word.
pub fn near_misses(
    scenes: &[(String, String)],
    names: &[BibleName],
    known: &[String],
) -> Vec<NearMiss> {
    let mut terms: Vec<(&str, &BibleName)> = Vec::new();
    let mut exact: HashSet<&str> = known.iter().flat_map(|name| words(name)).collect();
    for name in names {
        for word in words(&name.name) {
            exact.insert(word);
            if is_term(word) && !terms.iter().any(|(term, _)| *term == word) {
                terms.push((word, name));
            }
        }
    }
    let lower_case: HashSet<String> = scenes
        .iter()
        .flat_map(|(_, prose)| words(prose))
        .filter(|word| word.chars().all(|c| !c.is_uppercase()))
        .map(str::to_lowercase)
        .collect();

    let mut found: BTreeMap<(String, String), NearMiss> = BTreeMap::new();
    for (scene, prose) in scenes {
        for word in words(prose) {
            let word = without_possessive(word);
            if exact.contains(word) || !starts_upper(word) || word.chars().all(char::is_uppercase) {
                continue;
            }
            let lower = word.to_lowercase();
            let close = terms.iter().find_map(|(term, name)| {
                let term_lower = term.to_lowercase();
                if lower == term_lower {
                    return Some((term, name, true, 0));
                }
                let first = |s: &str| s.chars().next();
                if lower_case.contains(&lower)
                    || word.chars().count() < MIN_LETTERS
                    || first(&lower) != first(&term_lower)
                {
                    return None;
                }
                let allowed = if term.chars().count() >= 8 { 2 } else { 1 };
                let distance = edit_distance(&lower, &term_lower);
                (distance <= allowed).then_some((term, name, false, distance))
            });
            if let Some((term, name, case_only, _)) = close {
                found
                    .entry((word.to_string(), term.to_string()))
                    .or_insert_with(|| NearMiss {
                        word: word.to_string(),
                        term: term.to_string(),
                        note: name.note.clone(),
                        name: name.title.clone(),
                        case_only,
                        scenes: BTreeSet::new(),
                    })
                    .scenes
                    .insert(scene.clone());
            }
        }
    }
    found.into_values().collect()
}

/// The words in some text: runs of letters, with an apostrophe inside a word (K'thar, Mara's)
/// kept as part of it.
fn words(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let start = rest.find(char::is_alphabetic)?;
        rest = &rest[start..];
        let mut end = 0;
        let mut chars = rest.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let joins = (c == '\'' || c == '’')
                && chars.peek().is_some_and(|(_, next)| next.is_alphabetic());
            if c.is_alphabetic() || joins {
                end = i + c.len_utf8();
            } else {
                break;
            }
        }
        let word = &rest[..end];
        rest = &rest[end..];
        Some(word)
    })
}

/// A word of a name that is telling enough to check the scenes against.
fn is_term(word: &str) -> bool {
    starts_upper(word)
        && word.chars().filter(|c| c.is_alphabetic()).count() >= MIN_LETTERS
        && !ORDINARY.contains(&word.to_lowercase().as_str())
}

fn starts_upper(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_uppercase)
}

/// `Mara's` as `Mara`.
fn without_possessive(word: &str) -> &str {
    word.strip_suffix("'s")
        .or_else(|| word.strip_suffix("’s"))
        .unwrap_or(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(title: &str, name: &str) -> BibleName {
        BibleName {
            note: PathBuf::from(format!("{}.md", title.to_lowercase())),
            title: title.to_string(),
            name: name.to_string(),
        }
    }

    fn found(prose: &str) -> Vec<(String, String, bool)> {
        let names = [
            name("Mara Okafor", "Mara Okafor"),
            name("Mara Okafor", "Captain Okafor"),
            name("Tom McKay", "Tom McKay"),
            name("The Binder", "The Binder"),
            name("K'thar", "K'thar"),
            name("Thessaly Reach", "Thessaly Reach"),
        ];
        let scenes = [("arrival".to_string(), prose.to_string())];
        near_misses(&scenes, &names, &["The Long Night".to_string()])
            .into_iter()
            .map(|miss| (miss.word, miss.term, miss.case_only))
            .collect()
    }

    fn miss(word: &str, term: &str, case_only: bool) -> (String, String, bool) {
        (word.to_string(), term.to_string(), case_only)
    }

    #[test]
    fn flags_words_a_typo_from_a_name() {
        assert_eq!(
            found("Marra came ashore with Okafur."),
            [
                miss("Marra", "Mara", false),
                miss("Okafur", "Okafor", false)
            ]
        );
        assert_eq!(
            found("Mraa's ship and Kthar's."),
            [miss("Kthar", "K'thar", false), miss("Mraa", "Mara", false)]
        );
        assert_eq!(
            found("Thesally Reach, and Mearra."),
            [miss("Thesally", "Thessaly", false)],
            "a long name allows two edits, a short one only one"
        );
        assert_eq!(
            found("Rech and Bonder."),
            [
                miss("Bonder", "Binder", false),
                miss("Rech", "Reach", false)
            ]
        );
        assert_eq!(
            found("Mckay and MCKAY."),
            [miss("Mckay", "McKay", true)],
            "capitals that differ"
        );
    }

    #[test]
    fn leaves_names_and_ordinary_words_alone() {
        assert!(found("Mara Okafor, Captain Okafor, Mara's and Tom McKay met K'thar.").is_empty());
        assert!(found("The binder on the left, and the Binder itself.").is_empty());
        assert!(
            found("Make it so. Many hands.").is_empty(),
            "not close enough"
        );
        assert!(
            found("Maya lent a hand. maya fruit.").is_empty(),
            "used in lower case too, so an ordinary word"
        );
        assert!(found("The Long Night fell.").is_empty(), "a known title");
        assert!(found("MARRA! said the sign.").is_empty(), "shouting");
    }
}
