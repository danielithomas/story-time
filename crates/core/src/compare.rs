//! Comparing two versions of a note, for a writer choosing what to keep from each. Prose changes
//! by sentence and word rather than by line, so bodies are compared sentence by sentence, and
//! each difference marks the words that differ. Properties are compared one by one.

use anyhow::{Result, bail};
use similar::{Algorithm, DiffTag, capture_diff_slices};

use crate::note::{self, Frontmatter, PropertyValue};

/// How two versions of a note differ: "here", the version on this computer, and "other".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    /// The properties set differently, in this version's order, then any only the other has.
    pub properties: Vec<PropertyDifference>,
    /// The body in order: stretches that read the same in both, and differences between them.
    pub parts: Vec<Part>,
}

impl Comparison {
    /// The differences in the body, in order.
    pub fn differences(&self) -> impl Iterator<Item = &Difference> {
        self.parts.iter().filter_map(|part| match part {
            Part::Different(difference) => Some(difference),
            Part::Same(_) => None,
        })
    }

    /// Whether the two versions read the same, properties and all.
    pub fn is_same(&self) -> bool {
        self.properties.is_empty() && self.differences().next().is_none()
    }
}

/// A property that the two versions set differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyDifference {
    pub key: String,
    /// Its value here, or `None` if it isn't set (or is blank).
    pub here: Option<PropertyValue>,
    pub other: Option<PropertyValue>,
}

/// A stretch of a note's body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Text that reads the same in both versions.
    Same(String),
    Different(Difference),
}

/// One or more sentences that read differently in the two versions. Either side can be empty,
/// for sentences only one version has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    /// This version's sentences, in pieces, marked where their words differ from the other's.
    pub here: Vec<Piece>,
    pub other: Vec<Piece>,
    /// What goes between the two when both are kept: a space within a paragraph, or the line
    /// break that separates whole paragraphs.
    joint: String,
}

impl Difference {
    pub fn here_text(&self) -> String {
        self.here.iter().map(|piece| piece.text.as_str()).collect()
    }

    pub fn other_text(&self) -> String {
        self.other.iter().map(|piece| piece.text.as_str()).collect()
    }

    /// Both versions' sentences, this one's first.
    pub fn both(&self) -> String {
        let (here, other) = (self.here_text(), self.other_text());
        if here.trim().is_empty() {
            return other;
        }
        if other.trim().is_empty() {
            return here;
        }
        let spaced = here.ends_with(char::is_whitespace) || other.starts_with(char::is_whitespace);
        let joint = if spaced { "" } else { &self.joint };
        format!("{here}{joint}{other}")
    }
}

/// Some text in one side of a difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    /// Whether these words differ from the other version's.
    pub differs: bool,
}

/// What the writer keeps at one difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Here,
    Other,
    /// This version's sentences, then the other's.
    Both,
}

/// Compares two versions of a note's text, frontmatter and all.
pub fn compare(here: &str, other: &str) -> Comparison {
    let (here_yaml, here_body) = note::split_frontmatter(here);
    let (other_yaml, other_body) = note::split_frontmatter(other);
    Comparison {
        properties: compare_properties(here_yaml, other_yaml),
        parts: compare_bodies(here_body, other_body),
    }
}

/// The note `here` with what the writer took from `other`: the other's value of each property
/// in `properties`, and at each difference in the body, what `picks` says, in order. Everything
/// else stays exactly as `here` has it.
pub fn combine(here: &str, other: &str, properties: &[String], picks: &[Pick]) -> Result<String> {
    let comparison = compare(here, other);
    if picks.len() != comparison.differences().count() {
        bail!("the two versions changed since they were compared: compare them again");
    }
    let mut content = here.to_string();
    for difference in &comparison.properties {
        if !properties.contains(&difference.key) {
            continue;
        }
        content = match &difference.other {
            Some(value) => note::set_properties(&content, &[(&difference.key, value.clone())]),
            None => note::remove_properties(&content, &[&difference.key]),
        };
    }
    let mut picks = picks.iter();
    let body: String = comparison
        .parts
        .iter()
        .map(|part| match part {
            Part::Same(text) => text.clone(),
            Part::Different(difference) => match picks.next() {
                Some(Pick::Other) => difference.other_text(),
                Some(Pick::Both) => difference.both(),
                _ => difference.here_text(),
            },
        })
        .collect();
    if body == note::split_frontmatter(&content).1 {
        return Ok(content);
    }
    Ok(note::replace_body(&content, &body))
}

fn compare_properties(here: Option<&str>, other: Option<&str>) -> Vec<PropertyDifference> {
    let entries = |yaml: Option<&str>| -> Vec<(String, PropertyValue)> {
        yaml.and_then(|yaml| Frontmatter::parse(yaml).ok())
            .map(|frontmatter| frontmatter.entries())
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, value)| match value {
                PropertyValue::Text(text) => !text.trim().is_empty(),
                PropertyValue::List(items) => !items.is_empty(),
            })
            .collect()
    };
    let (here, other) = (entries(here), entries(other));
    let value = |entries: &[(String, PropertyValue)], key: &str| {
        entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    };
    let mut keys: Vec<&str> = here.iter().map(|(key, _)| key.as_str()).collect();
    for (key, _) in &other {
        if !keys.contains(&key.as_str()) {
            keys.push(key);
        }
    }
    keys.into_iter()
        .filter_map(|key| {
            let (here, other) = (value(&here, key), value(&other, key));
            (here != other).then(|| PropertyDifference {
                key: key.to_string(),
                here,
                other,
            })
        })
        .collect()
}

/// A stretch of sentences: the same in both versions, or not. Ranges index each side's units.
#[derive(Debug, Clone)]
struct Run {
    same: bool,
    here: std::ops::Range<usize>,
    other: std::ops::Range<usize>,
}

fn compare_bodies(here: &str, other: &str) -> Vec<Part> {
    let a = sentences(here);
    let b = sentences(other);
    let mut runs: Vec<Run> = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, &a, &b) {
        let (tag, old, new) = op.as_tag_tuple();
        let same = tag == DiffTag::Equal;
        match runs.last_mut() {
            Some(last) if !last.same && !same => {
                last.here.end = old.end;
                last.other.end = new.end;
            }
            _ => runs.push(Run {
                same,
                here: old,
                other: new,
            }),
        }
    }
    let text = |units: &[&str], range: &std::ops::Range<usize>| units[range.clone()].concat();
    // Differences a space apart, in the same paragraph, are one difference.
    let mut i = 1;
    while i + 1 < runs.len() {
        let joins = runs[i].same
            && !runs[i - 1].same
            && !runs[i + 1].same
            && a[runs[i].here.clone()]
                .iter()
                .all(|unit| unit.trim().is_empty() && !unit.contains('\n'));
        if joins {
            let next = runs.remove(i + 1);
            runs.remove(i);
            runs[i - 1].here.end = next.here.end;
            runs[i - 1].other.end = next.other.end;
        } else {
            i += 1;
        }
    }
    // A difference only in spacing matters only where it makes or removes a line or paragraph
    // break inside the text; then the sentences either side of it show where.
    let last = runs.len().saturating_sub(1);
    for i in 0..runs.len() {
        if runs[i].same {
            continue;
        }
        let (here_text, other_text) = (text(&a, &runs[i].here), text(&b, &runs[i].other));
        if !here_text.trim().is_empty() || !other_text.trim().is_empty() {
            continue;
        }
        if i == 0 || i == last || breaks(&here_text) == breaks(&other_text) {
            runs[i].same = true;
            continue;
        }
        if runs[i - 1].same && !runs[i - 1].here.is_empty() {
            runs[i - 1].here.end -= 1;
            runs[i - 1].other.end -= 1;
            runs[i].here.start -= 1;
            runs[i].other.start -= 1;
        }
        if runs[i + 1].same && !runs[i + 1].here.is_empty() {
            runs[i + 1].here.start += 1;
            runs[i + 1].other.start += 1;
            runs[i].here.end += 1;
            runs[i].other.end += 1;
        }
    }
    // Spacing made the same can leave runs of one kind side by side, or empty.
    let mut merged: Vec<Run> = Vec::new();
    for run in runs
        .into_iter()
        .filter(|run| !run.here.is_empty() || !run.other.is_empty())
    {
        match merged.last_mut() {
            Some(last) if last.same == run.same => {
                last.here.end = run.here.end;
                last.other.end = run.other.end;
            }
            _ => merged.push(run),
        }
    }
    merged
        .iter()
        .map(|run| {
            if run.same {
                return Part::Same(text(&a, &run.here));
            }
            let (here_text, other_text) = (text(&a, &run.here), text(&b, &run.other));
            let (here_pieces, other_pieces) = pieces(&here_text, &other_text);
            let before = run.here.start.checked_sub(1).map(|i| a[i]);
            let after = a.get(run.here.end).copied();
            let breaks_line = |unit: Option<&str>| unit.is_none_or(|unit| unit.contains('\n'));
            let joint = if breaks_line(before) && breaks_line(after) {
                [before, after]
                    .into_iter()
                    .flatten()
                    .find(|unit| unit.contains('\n'))
                    .unwrap_or("\n\n")
                    .to_string()
            } else {
                " ".to_string()
            };
            Part::Different(Difference {
                here: here_pieces,
                other: other_pieces,
                joint,
            })
        })
        .collect()
}

/// How far apart spacing puts the text either side of it: 0 on one line, 1 on the next line,
/// 2 in the next paragraph.
fn breaks(space: &str) -> usize {
    space.matches('\n').count().min(2)
}

/// Splits text into sentences and the spaces between them, which together are the whole text.
/// A sentence ends at a full stop, question or exclamation mark (and any closing quotes or
/// brackets) before a space, or at the end of a line, so headings and list items are sentences.
fn sentences(text: &str) -> Vec<&str> {
    let mut units = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if !c.is_whitespace() {
            continue;
        }
        let mut end = at + c.len_utf8();
        while let Some(&(next, c)) = chars.peek() {
            if !c.is_whitespace() {
                break;
            }
            end = next + c.len_utf8();
            chars.next();
        }
        let before = &text[start..at];
        let space = &text[at..end];
        if before.is_empty() || space.contains('\n') || ends_sentence(before) {
            if !before.is_empty() {
                units.push(before);
            }
            units.push(space);
            start = end;
        }
    }
    if start < text.len() {
        units.push(&text[start..]);
    }
    units
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end_matches(['"', '\'', '”', '’', '»', ')', ']', '*', '_'])
        .ends_with(['.', '!', '?', '…'])
}

/// Splits text into words, spaces, and each other character on its own.
fn words(text: &str) -> Vec<&str> {
    let kind = |c: char| {
        if c.is_alphanumeric() || c == '\'' || c == '’' {
            1
        } else if c.is_whitespace() {
            2
        } else {
            0
        }
    };
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut current = None;
    for (at, c) in text.char_indices() {
        let k = kind(c);
        if at > start && (k == 0 || current != Some(k)) {
            tokens.push(&text[start..at]);
            start = at;
        }
        current = Some(k);
    }
    if start < text.len() {
        tokens.push(&text[start..]);
    }
    tokens
}

/// Two versions of some sentences in pieces, marked where their words differ. Words the same in
/// both are marked too when they are only spacing, punctuation or one short word between
/// differences, so a rewritten phrase reads as one change rather than many.
fn pieces(here: &str, other: &str) -> (Vec<Piece>, Vec<Piece>) {
    let a = words(here);
    let b = words(other);
    // (same, here's text, other's text)
    let mut stretches: Vec<(bool, String, String)> = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, &a, &b) {
        let (tag, old, new) = op.as_tag_tuple();
        let same = tag == DiffTag::Equal;
        let (here, other) = (a[old].concat(), b[new].concat());
        match stretches.last_mut() {
            Some(last) if !last.0 && !same => {
                last.1.push_str(&here);
                last.2.push_str(&other);
            }
            _ => stretches.push((same, here, other)),
        }
    }
    for i in 1..stretches.len().saturating_sub(1) {
        if stretches[i].0 && !stretches[i - 1].0 && !stretches[i + 1].0 {
            let named: Vec<&str> = words(&stretches[i].1)
                .into_iter()
                .filter(|word| word.chars().any(char::is_alphanumeric))
                .collect();
            if named.is_empty() || (named.len() == 1 && named[0].chars().count() <= 3) {
                stretches[i].0 = false;
            }
        }
    }
    let mut here_pieces: Vec<Piece> = Vec::new();
    let mut other_pieces: Vec<Piece> = Vec::new();
    let add = |pieces: &mut Vec<Piece>, text: String, differs: bool| {
        if text.is_empty() {
            return;
        }
        match pieces.last_mut() {
            Some(last) if last.differs == differs => last.text.push_str(&text),
            _ => pieces.push(Piece { text, differs }),
        }
    };
    for (same, here, other) in stretches {
        add(&mut here_pieces, here, !same);
        add(&mut other_pieces, other, !same);
    }
    (here_pieces, other_pieces)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn differences(comparison: &Comparison) -> Vec<(String, String)> {
        comparison
            .differences()
            .map(|difference| (difference.here_text(), difference.other_text()))
            .collect()
    }

    fn marked(pieces: &[Piece]) -> String {
        pieces
            .iter()
            .map(|piece| {
                if piece.differs {
                    format!("[{}]", piece.text)
                } else {
                    piece.text.clone()
                }
            })
            .collect()
    }

    #[test]
    fn splits_text_into_sentences_and_spaces() {
        let text = "# Heading\n\nShe ran. \"Wait!\" he said… Then (quietly) left.\n- one\n- two";
        assert_eq!(
            sentences(text),
            [
                "# Heading",
                "\n\n",
                "She ran.",
                " ",
                "\"Wait!\"",
                " ",
                "he said…",
                " ",
                "Then (quietly) left.",
                "\n",
                "- one",
                "\n",
                "- two"
            ]
        );
        assert_eq!(sentences(text).concat(), text);
        assert_eq!(sentences("  lead"), ["  ", "lead"]);
        assert!(sentences("").is_empty());
    }

    #[test]
    fn finds_the_sentences_that_differ() {
        let here = "The rain came. She ran to the door. It was locked.\n\nMorning.\n";
        let other = "The rain came. She walked to the door. It was locked.\n\nMorning.\n";
        let comparison = compare(here, other);
        assert!(comparison.properties.is_empty());
        assert_eq!(
            differences(&comparison),
            [(
                "She ran to the door.".to_string(),
                "She walked to the door.".to_string()
            )]
        );
        let difference = comparison.differences().next().unwrap();
        assert_eq!(marked(&difference.here), "She [ran] to the door.");
        assert_eq!(marked(&difference.other), "She [walked] to the door.");
        assert_eq!(
            comparison.parts[0],
            Part::Same("The rain came. ".to_string())
        );
    }

    #[test]
    fn neighbouring_sentences_are_one_difference_but_paragraphs_are_not() {
        let here = "One. Two. Three.\n\nFour. Five.";
        let other = "One. Deux. Trois.\n\nQuatre. Five.";
        assert_eq!(
            differences(&compare(here, other)),
            [
                ("Two. Three.".to_string(), "Deux. Trois.".to_string()),
                ("Four.".to_string(), "Quatre.".to_string())
            ]
        );
    }

    #[test]
    fn a_rewritten_phrase_is_one_marked_change() {
        let (here, other) = pieces("He ran to a door quickly.", "He walked to a gate slowly.");
        assert_eq!(marked(&here), "He [ran] to a [door quickly].");
        assert_eq!(marked(&other), "He [walked] to a [gate slowly].");
        // One short word between changes goes with them.
        let (here, _) = pieces("He ran far.", "She ran near.");
        assert_eq!(marked(&here), "[He ran far].");
    }

    #[test]
    fn sentences_only_one_version_has() {
        let here = "First.\n\nLast.";
        let other = "First.\n\nMiddle.\n\nLast.";
        let comparison = compare(here, other);
        let found = differences(&comparison);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "");
        assert_eq!(found[0].1.trim(), "Middle.");
        assert_eq!(combine(here, other, &[], &[Pick::Other]).unwrap(), other);
        assert_eq!(combine(here, other, &[], &[Pick::Both]).unwrap(), other);
        assert_eq!(combine(here, other, &[], &[Pick::Here]).unwrap(), here);
    }

    #[test]
    fn spacing_matters_only_when_it_breaks_a_line() {
        // A newline at the end, or a double space, isn't worth asking about.
        assert!(compare("One.  Two.\n", "One. Two.").is_same());
        // A paragraph split in two is, with the sentences either side to show where.
        let comparison = compare(
            "Intro.\n\nOne. Two.\n\nEnd.",
            "Intro.\n\nOne.\n\nTwo.\n\nEnd.",
        );
        assert_eq!(
            differences(&comparison),
            [("One. Two.".to_string(), "One.\n\nTwo.".to_string())]
        );
    }

    #[test]
    fn keeps_both_versions_one_after_the_other() {
        let here = "Start. She ran. End.\n\nOld paragraph.\n\nClose.";
        let other = "Start. She walked. End.\n\nNew paragraph.\n\nClose.";
        let combined = combine(here, other, &[], &[Pick::Both, Pick::Both]).unwrap();
        assert_eq!(
            combined,
            "Start. She ran. She walked. End.\n\nOld paragraph.\n\nNew paragraph.\n\nClose."
        );
        let mixed = combine(here, other, &[], &[Pick::Other, Pick::Here]).unwrap();
        assert_eq!(mixed, "Start. She walked. End.\n\nOld paragraph.\n\nClose.");
    }

    #[test]
    fn compares_and_takes_properties() {
        let here = "---\ntype: scene\n# how far along\nstatus: draft\npov: Mara\n---\n# Arrival\n\nText.\n";
        let other =
            "---\ntype: scene\nstatus: revised\nsynopsis: She lands.\n---\n# Arrival\n\nText.\n";
        let comparison = compare(here, other);
        let keys: Vec<&str> = comparison
            .properties
            .iter()
            .map(|difference| difference.key.as_str())
            .collect();
        assert_eq!(keys, ["status", "pov", "synopsis"]);
        assert_eq!(comparison.properties[1].other, None);
        assert!(comparison.differences().next().is_none());

        // Untaken, nothing changes, comments and all.
        assert_eq!(combine(here, other, &[], &[]).unwrap(), here);
        let taken = ["status", "pov", "synopsis"].map(String::from);
        let combined = combine(here, other, &taken, &[]).unwrap();
        assert_eq!(
            combined,
            "---\ntype: scene\n# how far along\nstatus: revised\nsynopsis: She lands.\n---\n# Arrival\n\nText.\n"
        );
        assert!(compare(&combined, other).is_same());
    }

    #[test]
    fn keeps_this_version_where_nothing_is_taken() {
        let here = "---\nstatus: draft\n---\nOne. Two.\n\nThree.";
        let other = "---\nstatus: final\n---\nOne. 2.\n\nThree. Four.";
        let picks = vec![Pick::Here; compare(here, other).differences().count()];
        assert_eq!(combine(here, other, &[], &picks).unwrap(), here);
        assert!(combine(here, other, &[], &[]).is_err());
    }

    #[test]
    fn identical_versions_are_the_same() {
        let text = "---\nstatus: draft\n---\nOne.\n";
        assert!(compare(text, text).is_same());
        assert_eq!(
            compare(text, text).parts,
            [Part::Same("One.\n".to_string())]
        );
        assert!(compare("", "").parts.is_empty());
    }
}
