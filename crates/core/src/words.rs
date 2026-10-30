//! Turning notes into manuscript text, and counting its words.
//!
//! What counts is the prose a reader would read. Excluded: frontmatter (handled by
//! [`crate::note`]), headings (chapter and scene titles), Obsidian `%% comments %%`, HTML
//! `<!-- comments -->`, callouts (`> [!note]`), fenced code blocks and embeds. Links count as the
//! text they display. Hyphenated words count once, words joined by an em dash count separately,
//! and stray punctuation such as `***` does not count.

use std::sync::LazyLock;

use regex::{Captures, Regex};

static EMBED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!\[\[[^\]]*\]\]|!\[[^\]]*\]\([^)]*\)").unwrap());
static WIKILINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|]*)(?:\|([^\]]*))?\]\]").unwrap());
static MARKDOWN_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
static LIST_MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:[-*+]|\d+[.)])\s+(?:\[.\]\s+)?").unwrap());
static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s{0,3}#{1,6}(?:\s|$)").unwrap());

/// The body of a note as manuscript Markdown: notes-to-self removed, links reduced to their text.
pub fn prose(body: &str) -> String {
    let text = strip_comments(body);
    let mut out = String::with_capacity(text.len());
    let mut fence: Option<&str> = None;
    let mut in_callout = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(marker) = fence {
            if trimmed.starts_with(marker) {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(&trimmed[..3]);
            continue;
        }
        match trimmed.strip_prefix('>') {
            Some(_) if in_callout => continue,
            Some(quoted) if quoted.trim_start().starts_with("[!") => {
                in_callout = true;
                continue;
            }
            Some(_) => {}
            None => in_callout = false,
        }
        out.push_str(&plain_text(line));
        out.push('\n');
    }
    out
}

/// Replaces links with the text they display and drops embeds.
pub fn plain_text(line: &str) -> String {
    let line = EMBED.replace_all(line, "");
    let line = WIKILINK.replace_all(&line, |c: &Captures| match c.get(2) {
        Some(alias) => alias.as_str().to_string(),
        None => link_note(&c[1]).to_string(),
    });
    MARKDOWN_LINK.replace_all(&line, "$1").into_owned()
}

/// The notes and files a body links to or embeds, by name: `[[world/Harbour#Origin|the harbour]]`
/// gives "Harbour". Links inside comments are not real links and are ignored.
pub fn link_targets(body: &str) -> Vec<String> {
    WIKILINK
        .captures_iter(&strip_comments(body))
        .map(|c| link_note(&c[1]).to_string())
        .filter(|target| !target.is_empty())
        .collect()
}

fn link_note(target: &str) -> &str {
    let note = target.split(['#', '^']).next().unwrap_or_default();
    let note = note.rsplit('/').next().unwrap_or_default().trim();
    note.strip_suffix(".md").unwrap_or(note)
}

fn strip_comments(text: &str) -> String {
    strip_between(&strip_between(text, "%%", "%%"), "<!--", "-->")
}

fn strip_between(text: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + open.len()..];
        match after_open.find(close) {
            Some(end) => rest = &after_open[end + close.len()..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

pub fn count_words(body: &str) -> usize {
    prose(body).lines().map(count_line).sum()
}

fn count_line(line: &str) -> usize {
    if HEADING.is_match(line) {
        return 0;
    }
    let line = LIST_MARKER.replace(line, "").replace("--", " ");
    line.split(|c: char| c.is_whitespace() || c == '—')
        .filter(|token| token.chars().any(char::is_alphanumeric))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_plain_words() {
        assert_eq!(count_words("The ship fell silent."), 4);
        assert_eq!(count_words("A well-known fact—obviously."), 4);
        assert_eq!(count_words("Wait -- what?"), 2);
        assert_eq!(count_words("\"Don't,\" she said. 1990 was long ago."), 7);
    }

    #[test]
    fn ignores_markup_and_notes_to_self() {
        assert_eq!(
            count_words("# Chapter One\n\n## Scene\n\n* * *\n\n---\n"),
            0
        );
        assert_eq!(count_words("#hashtag stays"), 2);
        assert_eq!(count_words("Visible %% hidden note %% text"), 2);
        assert_eq!(count_words("Before\n%%\nA long\nhidden plan\n%%\nAfter"), 2);
        assert_eq!(count_words("Seen <!-- unseen --> here"), 2);
        assert_eq!(
            count_words("Prose.\n> [!todo] Fix this\n> and this too\n\nMore prose."),
            3
        );
        assert_eq!(count_words("> A quoted letter.\n"), 3);
        assert_eq!(count_words("```\nlet x = 1;\n```\nAfter code"), 2);
        assert_eq!(count_words("1. First item\n- [x] Done thing\n- Bullet"), 5);
    }

    #[test]
    fn counts_links_as_displayed() {
        assert_eq!(count_words("[[Mara Chen]] ran."), 3);
        assert_eq!(count_words("[[Mara Chen|Mara]] ran."), 2);
        assert_eq!(count_words("See ![[map.png]] the [map](http://x.y/z)."), 3);
        assert_eq!(count_words("[[world/Harbour#Origin]]"), 1);
    }

    #[test]
    fn prose_keeps_markdown_but_drops_notes() {
        let body =
            "# One\n\n*She* ran to [[Old Port|the port]].\n%% fix later %%\n> [!note]\n> cut?\n";
        assert_eq!(prose(body), "# One\n\n*She* ran to the port.\n\n");
    }

    #[test]
    fn finds_link_targets() {
        let body = "[[Mara Chen|Mara]] met [[world/Harbour#Origin]] and ![[map.png]] and [[#Local heading]]. %% [[Planned]] %%";
        assert_eq!(link_targets(body), ["Mara Chen", "Harbour", "map.png"]);
    }
}
