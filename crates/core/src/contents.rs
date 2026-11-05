//! A book's contents: the list under `## Contents` in its note, which sets the order of its parts,
//! chapters and scenes.
//!
//! ```markdown
//! ## Contents
//!
//! ### Part One
//!
//! - Chapter
//!   - [[arrival|Arrival]]
//!   - [[first-light|First Light]]
//! - The Long Night
//!   - [[leaving-home|Leaving Home]]
//! - [[epilogue|Epilogue]]
//! ```
//!
//! `###` headings are parts. A list item that isn't a link is a chapter, titled by its text
//! (`Chapter` for an untitled one), and the links indented beneath it are its scenes. A link at
//! the top level is a chapter of its own. Anything else in the section, such as comments or notes
//! under a chapter, means nothing to story-time and is left as it is when the list changes.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Result, bail};
use regex::Regex;

use crate::links::{Link, links, normalise};
use crate::project::Scene;
use crate::words::{plain_text, strip_comments};

/// The heading in a book note under which its contents are listed.
pub const HEADING: &str = "Contents";

/// The text of a chapter item that has no title of its own.
pub const UNTITLED: &str = "Chapter";

static ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)([-*+]|\d+[.)])\s+(.*)$").unwrap());

/// The contents as written, with where each chapter sits among the section's lines.
#[derive(Debug, Clone, Default)]
pub struct Listing {
    pub parts: Vec<ListedPart>,
}

#[derive(Debug, Clone)]
pub struct ListedPart {
    /// `None` for chapters listed before the first part heading.
    pub title: Option<String>,
    heading: Option<usize>,
    pub chapters: Vec<ListedChapter>,
}

#[derive(Debug, Clone)]
pub struct ListedChapter {
    /// `None` for an untitled chapter.
    pub title: Option<String>,
    /// The links to its scenes, in order.
    pub scenes: Vec<Link>,
    /// A scene listed at the top level, with no chapter item of its own.
    pub own: bool,
    /// Its item and the lines indented beneath it.
    lines: Range<usize>,
}

impl Listing {
    pub fn parse(lines: &[&str]) -> Listing {
        let mut parts = vec![ListedPart {
            title: None,
            heading: None,
            chapters: Vec::new(),
        }];
        let mut base = None;
        for (i, line) in lines.iter().enumerate() {
            if let Some(title) = part_heading(line) {
                parts.push(ListedPart {
                    title,
                    heading: Some(i),
                    chapters: Vec::new(),
                });
                continue;
            }
            let Some(item) = ITEM.captures(line) else {
                continue;
            };
            let indent = width(&item[1]);
            let base = *base.get_or_insert(indent);
            let text = item[3].trim();
            let link = scene_link(text);
            let part = parts.last_mut().expect("there is always a part");
            match (part.chapters.last_mut(), link) {
                (Some(chapter), link) if indent > base => {
                    chapter.scenes.extend(link);
                    chapter.lines.end = i + 1;
                }
                (_, Some(link)) => part.chapters.push(ListedChapter {
                    title: None,
                    scenes: vec![link],
                    own: true,
                    lines: i..i + 1,
                }),
                (_, None) => part.chapters.push(ListedChapter {
                    title: chapter_title(text),
                    scenes: Vec::new(),
                    own: false,
                    lines: i..i + 1,
                }),
            }
        }
        if parts.len() > 1 && parts[0].chapters.is_empty() {
            parts.remove(0);
        }
        Listing { parts }
    }

    pub fn chapters(&self) -> impl Iterator<Item = &ListedChapter> {
        self.parts.iter().flat_map(|part| &part.chapters)
    }
}

/// The title of a `### part` heading line, or `None` for other lines; `Some(None)` for a heading
/// with no text.
fn part_heading(line: &str) -> Option<Option<String>> {
    let rest = line.trim_start().strip_prefix("###")?;
    if !(rest.is_empty() || rest.starts_with(' ')) {
        return None;
    }
    let title = plain_text(&strip_comments(rest)).trim().to_string();
    Some((!title.is_empty()).then_some(title))
}

/// The link an item starts with, which makes it a scene.
fn scene_link(text: &str) -> Option<Link> {
    if !text.starts_with("[[") {
        return None;
    }
    links(text).into_iter().next()
}

fn chapter_title(text: &str) -> Option<String> {
    let title = plain_text(&strip_comments(text)).trim().to_string();
    (!title.is_empty() && !title.eq_ignore_ascii_case(UNTITLED)).then_some(title)
}

/// Indentation in columns, with a tab as four.
fn width(indent: &str) -> usize {
    indent.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum()
}

/// Where a new scene goes in a book's contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Left out of the contents, to be placed later.
    Unplaced,
    /// After everything else, as a chapter of its own.
    End,
    /// At the end of the chapter at this position, counting from 0 across parts.
    Chapter(usize),
}

/// The section's lines with an item for `link` added at `place`. Every other line stays as it
/// was, except that a scene listed as a chapter of its own becomes the first scene of an untitled
/// chapter when another scene joins it.
pub fn insert(lines: &[&str], place: Place, link: &str) -> Result<Vec<String>> {
    let listing = Listing::parse(lines);
    let mut out: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    let first_item = listing
        .chapters()
        .next()
        .and_then(|chapter| ITEM.captures(lines[chapter.lines.start]));
    let (indent, marker) = first_item
        .map(|item| (item[1].to_string(), item[2].to_string()))
        .unwrap_or_else(|| (String::new(), "-".to_string()));
    match place {
        Place::Unplaced => {}
        Place::End => {
            let part = listing.parts.last().expect("there is always a part");
            let item = format!("{indent}{marker} {link}");
            match part.chapters.last() {
                Some(chapter) => out.insert(chapter.lines.end, item),
                None => {
                    let start = part.heading.map_or(0, |heading| heading + 1);
                    let end = start
                        + lines[start..]
                            .iter()
                            .position(|line| part_heading(line).is_some())
                            .unwrap_or(lines.len() - start);
                    let at = (part.heading.unwrap_or(0)..end)
                        .rev()
                        .find(|&i| !lines[i].trim().is_empty())
                        .map_or(start, |i| i + 1);
                    let mut new = vec![item];
                    if at > 0 && !lines[at - 1].trim().is_empty() {
                        new.insert(0, String::new());
                    }
                    out.splice(at..at, new);
                }
            }
        }
        Place::Chapter(n) => {
            let count = listing.chapters().count();
            let Some(chapter) = listing.chapters().nth(n) else {
                bail!(
                    "there is no chapter {} in the contents, which list {count}",
                    n + 1
                );
            };
            let nested = nested_indent(lines, &listing, chapter, &indent);
            let item = format!("{nested}{marker} {link}");
            out.insert(chapter.lines.end, item);
            if chapter.own {
                let line = chapter.lines.start;
                let text = ITEM
                    .captures(lines[line])
                    .map(|item| {
                        (
                            item[1].to_string(),
                            item[2].to_string(),
                            item[3].to_string(),
                        )
                    })
                    .expect("a chapter starts with an item");
                out[line] = format!("{}{} {UNTITLED}", text.0, text.1);
                out.insert(line + 1, format!("{nested}{} {}", text.1, text.2));
            }
        }
    }
    Ok(out)
}

/// The indentation for a scene in `chapter`: that of its own scenes, else of any scene indented
/// in the section, else two spaces more than the chapter's.
fn nested_indent(lines: &[&str], listing: &Listing, chapter: &ListedChapter, base: &str) -> String {
    let indented = |range: Range<usize>| {
        lines[range].iter().find_map(|line| {
            let item = ITEM.captures(line)?;
            (width(&item[1]) > width(base)).then(|| item[1].to_string())
        })
    };
    indented(chapter.lines.clone())
        .or_else(|| listing.chapters().find_map(|c| indented(c.lines.clone())))
        .unwrap_or_else(|| format!("{base}  "))
}

/// A book's scenes in reading order, as its contents list them, and the scenes left out.
#[derive(Debug, Clone, Default)]
pub struct Contents {
    pub parts: Vec<Part>,
    /// Scenes in the book that the contents don't list, by name.
    pub unplaced: Vec<Scene>,
    /// Entries that link to no scene in the book, as written.
    pub missing: Vec<String>,
    /// Scenes listed more than once, by file name. Only the first counts.
    pub repeated: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Part {
    /// `None` for chapters listed before the first part heading, or in a book without parts.
    pub title: Option<String>,
    pub chapters: Vec<Chapter>,
}

#[derive(Debug, Clone)]
pub struct Chapter {
    /// `None` for an untitled chapter.
    pub title: Option<String>,
    /// For an untitled chapter, its number among the book's untitled chapters, from 1. Titled
    /// chapters, such as a prologue, go by their title alone.
    pub number: Option<usize>,
    pub scenes: Vec<Scene>,
}

impl Chapter {
    /// What the chapter is called on the page: its title, or "Chapter 3".
    pub fn heading(&self) -> String {
        match (&self.title, self.number) {
            (Some(title), _) => title.clone(),
            (None, Some(n)) => format!("{UNTITLED} {n}"),
            (None, None) => UNTITLED.to_string(),
        }
    }

    pub fn words(&self) -> u64 {
        self.scenes.iter().map(|scene| scene.words as u64).sum()
    }
}

impl Contents {
    /// Matches the entries in `listing` to `scenes`, the scenes in the book at `book_dir`.
    pub fn resolve(listing: &Listing, book_dir: &Path, scenes: Vec<Scene>) -> Contents {
        let paths: Vec<PathBuf> = scenes.iter().map(|scene| scene.note.path.clone()).collect();
        let aliases: Vec<Vec<String>> = scenes
            .iter()
            .map(|scene| {
                let aliases = scene.note.aliases();
                aliases.iter().map(|alias| alias.to_lowercase()).collect()
            })
            .collect();
        let mut slots: Vec<Option<Scene>> = scenes.into_iter().map(Some).collect();
        let mut contents = Contents::default();
        let mut untitled = 0;
        for listed_part in &listing.parts {
            let mut part = Part {
                title: listed_part.title.clone(),
                chapters: Vec::new(),
            };
            for listed in &listed_part.chapters {
                let number = listed.title.is_none().then(|| {
                    untitled += 1;
                    untitled
                });
                let mut chapter = Chapter {
                    title: listed.title.clone(),
                    number,
                    scenes: Vec::new(),
                };
                for link in &listed.scenes {
                    match find(&paths, &aliases, book_dir, &link.target) {
                        Some(i) => match slots[i].take() {
                            Some(scene) => chapter.scenes.push(scene),
                            None => contents.repeated.push(stem(&paths[i])),
                        },
                        None => contents.missing.push(link.target.clone()),
                    }
                }
                part.chapters.push(chapter);
            }
            contents.parts.push(part);
        }
        contents.unplaced = slots.into_iter().flatten().collect();
        contents.repeated.dedup();
        contents
    }

    pub fn chapters(&self) -> impl Iterator<Item = &Chapter> {
        self.parts.iter().flat_map(|part| &part.chapters)
    }

    /// The scenes the contents list, in reading order.
    pub fn placed(&self) -> impl Iterator<Item = &Scene> {
        self.chapters().flat_map(|chapter| &chapter.scenes)
    }

    /// Every scene in the book: those placed, in order, then those not.
    pub fn scenes(&self) -> impl Iterator<Item = &Scene> {
        self.placed().chain(&self.unplaced)
    }

    /// Words in every scene in the book, placed or not.
    pub fn words(&self) -> u64 {
        self.scenes().map(|scene| scene.words as u64).sum()
    }

    /// Words in the placed scenes: the manuscript.
    pub fn placed_words(&self) -> u64 {
        self.placed().map(|scene| scene.words as u64).sum()
    }

    /// Whether the book is divided into parts.
    pub fn has_parts(&self) -> bool {
        self.parts.iter().any(|part| part.title.is_some())
    }
}

/// The scene a contents entry links to: by path from the book's folder or the vault's root, else
/// by file name, else by alias.
fn find(
    paths: &[PathBuf],
    aliases: &[Vec<String>],
    book_dir: &Path,
    target: &str,
) -> Option<usize> {
    let target = target.trim();
    let lower = |path: &Path| path.to_string_lossy().to_lowercase();
    if target.contains('/') {
        let root = book_dir.ancestors().nth(3).unwrap_or(book_dir);
        let wanted = |base: &Path| {
            let path = normalise(&base.join(target.trim_start_matches('/')));
            let is_note = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
            let mut file = lower(&path);
            if !is_note {
                file.push_str(".md");
            }
            file
        };
        return [wanted(book_dir), wanted(root)]
            .iter()
            .find_map(|wanted| paths.iter().position(|path| lower(path) == *wanted));
    }
    let name = target.to_lowercase();
    let name = name.strip_suffix(".md").unwrap_or(&name);
    paths
        .iter()
        .position(|path| stem(path).to_lowercase() == name)
        .or_else(|| {
            aliases
                .iter()
                .position(|names| names.iter().any(|alias| alias == name))
        })
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;

    const LISTED: &str = "\
### Part One

- Chapter
  - [[arrival|Arrival]]
  - [[first-light|First Light]]
- The Long Night
  - [[leaving-home|Leaving Home]]
  - a note about this chapter
%% a comment %%

### Part Two

- [[epilogue|Epilogue]]
- Chapter
* [[scenes/coda]]";

    fn lines(text: &str) -> Vec<&str> {
        text.lines().collect()
    }

    #[test]
    fn reads_parts_chapters_and_scenes() {
        let listing = Listing::parse(&lines(LISTED));
        let parts: Vec<_> = listing.parts.iter().map(|p| p.title.as_deref()).collect();
        assert_eq!(parts, [Some("Part One"), Some("Part Two")]);
        let chapters: Vec<_> = listing
            .chapters()
            .map(|c| {
                let scenes: Vec<&str> = c.scenes.iter().map(|s| s.target.as_str()).collect();
                (c.title.as_deref(), c.own, scenes)
            })
            .collect();
        assert_eq!(
            chapters,
            [
                (None, false, vec!["arrival", "first-light"]),
                (Some("The Long Night"), false, vec!["leaving-home"]),
                (None, true, vec!["epilogue"]),
                (None, false, vec![]),
                (None, true, vec!["scenes/coda"]),
            ]
        );
        let flat = Listing::parse(&lines("- [[a]]\n- [[b]]\n  - [[c]]\n"));
        assert_eq!(flat.parts.len(), 1);
        assert_eq!(flat.parts[0].title, None);
        assert_eq!(flat.chapters().count(), 2, "c joins b's chapter");
    }

    fn scene(dir: &Path, name: &str, aliases: &str) -> Scene {
        let path = dir.join("scenes").join(format!("{name}.md"));
        let note = Note::parse(
            &path,
            &format!("---\naliases: [{aliases}]\n---\nOne two.\n"),
        );
        Scene { words: 2, note }
    }

    #[test]
    fn places_scenes_and_finds_the_rest() {
        let book = Path::new("/vault/world/books/book");
        let scenes = ["arrival", "first-light", "leaving-home", "coda", "fragment"]
            .map(|name| scene(book, name, ""))
            .to_vec();
        let mut scenes = scenes;
        scenes.push(scene(book, "the-end", "Epilogue"));
        let text = format!("{LISTED}\n- [[missing-scene]]\n- [[arrival]]\n");
        let contents = Contents::resolve(&Listing::parse(&lines(&text)), book, scenes);
        let placed: Vec<&str> = contents.placed().map(|s| s.note.name.as_str()).collect();
        assert_eq!(
            placed,
            ["arrival", "first-light", "leaving-home", "the-end", "coda"]
        );
        let headings: Vec<String> = contents.chapters().map(Chapter::heading).collect();
        assert_eq!(
            headings,
            [
                "Chapter 1",
                "The Long Night",
                "Chapter 2",
                "Chapter 3",
                "Chapter 4",
                "Chapter 5",
                "Chapter 6"
            ]
        );
        assert_eq!(contents.missing, ["missing-scene"]);
        assert_eq!(contents.repeated, ["arrival"]);
        let unplaced: Vec<&str> = contents
            .unplaced
            .iter()
            .map(|s| s.note.name.as_str())
            .collect();
        assert_eq!(unplaced, ["fragment"]);
        assert_eq!(contents.words(), 12);
        assert_eq!(contents.placed_words(), 10);
        assert!(contents.has_parts());
    }

    fn inserted(text: &str, place: Place) -> String {
        insert(&lines(text), place, "[[new|New]]")
            .unwrap()
            .join("\n")
    }

    #[test]
    fn adds_scenes_where_asked_and_leaves_the_rest() {
        assert_eq!(inserted("", Place::End), "- [[new|New]]");
        assert_eq!(
            inserted("\n%% The book in order. %%\n", Place::End),
            "\n%% The book in order. %%\n\n- [[new|New]]"
        );
        assert_eq!(
            inserted(LISTED, Place::End),
            format!("{LISTED}\n- [[new|New]]"),
            "at the end of the last part, with the list's own marker"
        );
        assert_eq!(
            inserted(LISTED, Place::Chapter(1)),
            LISTED.replace(
                "  - a note about this chapter\n",
                "  - a note about this chapter\n  - [[new|New]]\n"
            )
        );
        assert_eq!(
            inserted(LISTED, Place::Chapter(2)),
            LISTED.replace(
                "- [[epilogue|Epilogue]]\n",
                "- Chapter\n  - [[epilogue|Epilogue]]\n  - [[new|New]]\n"
            ),
            "a chapter of its own becomes an untitled chapter"
        );
        assert_eq!(
            inserted("- One\n\t- [[a]]\n- Two\n", Place::Chapter(1)),
            "- One\n\t- [[a]]\n- Two\n\t- [[new|New]]",
            "indented as the other scenes are"
        );
        assert_eq!(
            inserted("### Part One\n\n- [[a]]\n\n### Part Two\n", Place::End),
            "### Part One\n\n- [[a]]\n\n### Part Two\n\n- [[new|New]]"
        );
        assert_eq!(inserted(LISTED, Place::Unplaced), LISTED);
        assert!(insert(&lines(LISTED), Place::Chapter(9), "[[x]]").is_err());
    }
}
