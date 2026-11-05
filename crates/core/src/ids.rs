//! Permanent IDs for projects and books, so the writing log survives a rename.
//!
//! An ID is eight random lower-case letters and digits, starting with a letter so YAML never
//! reads it as a number. It is written into the note when the note is created and never changed.

use std::collections::HashSet;

use anyhow::{Context, Result};

const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const ALPHANUMERIC: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
const LEN: usize = 8;

/// A new random ID that isn't one of `taken`.
pub fn new_id(taken: &HashSet<String>) -> Result<String> {
    loop {
        let id = random_id()?;
        if !taken.contains(&id) {
            return Ok(id);
        }
    }
}

fn random_id() -> Result<String> {
    let mut id = String::with_capacity(LEN);
    id.push(random_char(LETTERS)?);
    while id.len() < LEN {
        id.push(random_char(ALPHANUMERIC)?);
    }
    Ok(id)
}

/// A uniformly random character from `alphabet`, rejecting the bytes that would favour some.
fn random_char(alphabet: &[u8]) -> Result<char> {
    let limit = 256 - 256 % alphabet.len();
    loop {
        let mut byte = [0u8];
        getrandom::fill(&mut byte).context("the system's random number generator failed")?;
        if usize::from(byte[0]) < limit {
            return Ok(char::from(alphabet[usize::from(byte[0]) % alphabet.len()]));
        }
    }
}

/// Whether `id` can serve as an ID. Hand-edited IDs are accepted if they are short and safe to
/// write in the writing log: letters, digits, hyphens and underscores.
pub fn is_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_eight_characters_starting_with_a_letter() {
        let taken = HashSet::new();
        for _ in 0..200 {
            let id = new_id(&taken).unwrap();
            assert_eq!(id.len(), LEN, "{id}");
            assert!(id.starts_with(|c: char| c.is_ascii_lowercase()), "{id}");
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                "{id}"
            );
            assert!(is_valid(&id));
        }
    }

    #[test]
    fn validates_hand_edited_ids() {
        assert!(is_valid("k3x9q2mw"));
        assert!(is_valid("my-book_2"));
        assert!(!is_valid(""));
        assert!(!is_valid("has,comma"));
        assert!(!is_valid("{{id}}"));
        assert!(!is_valid(&"a".repeat(65)));
    }
}
