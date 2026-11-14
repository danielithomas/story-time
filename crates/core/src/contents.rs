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
//! under a chapter, means nothing to story-time and is left as it is when the list changes. Lines
//! indented beneath a scene's entry or a chapter's item go with it when it moves.

use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;

use crate::links::{Link, LinkIndex, links, normalise};
use crate::note::{Saved, content_hash, save_section, section, split_frontmatter};
use crate::project::{Book, Scene};
use crate::vault::Vault;
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
    /// The line of each scene's entry, beside `scenes`.
    entries: Vec<usize>,
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
            let part = parts.last_mut().expect("there is always a part");
            let Some(item) = ITEM.captures(line) else {
                // Text indented beneath a chapter's items, such as a note about a scene.
                if let Some(base) = base
                    && !line.trim().is_empty()
                    && width(indent_of(line)) > base
                    && let Some(chapter) = part.chapters.last_mut()
                {
                    chapter.lines.end = i + 1;
                }
                continue;
            };
            let indent = width(&item[1]);
            let base = *base.get_or_insert(indent);
            let text = item[3].trim();
            let link = scene_link(text);
            match (part.chapters.last_mut(), link) {
                (Some(chapter), link) if indent > base => {
                    if let Some(link) = link {
                        chapter.scenes.push(link);
                        chapter.entries.push(i);
                    }
                    chapter.lines.end = i + 1;
                }
                (_, Some(link)) => part.chapters.push(ListedChapter {
                    title: None,
                    scenes: vec![link],
                    entries: vec![i],
                    own: true,
                    lines: i..i + 1,
                }),
                (_, None) => part.chapters.push(ListedChapter {
                    title: chapter_title(text),
                    scenes: Vec::new(),
                    entries: Vec::new(),
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

    /// For each of the matcher's scenes, the lines of the entries that link to it, in order.
    fn entries(&self, matcher: &Matcher) -> Vec<Vec<usize>> {
        let mut found = vec![Vec::new(); matcher.paths.len()];
        for chapter in self.chapters() {
            for (link, &line) in chapter.scenes.iter().zip(&chapter.entries) {
                if let Some(i) = matcher.find(&link.target) {
                    found[i].push(line);
                }
            }
        }
        found
    }

    /// The indentation and list marker of the first chapter's item, which new top-level items
    /// copy.
    fn style(&self, lines: &[&str]) -> (String, String) {
        self.chapters()
            .next()
            .and_then(|chapter| ITEM.captures(lines[chapter.lines.start]))
            .map(|item| (item[1].to_string(), item[2].to_string()))
            .unwrap_or_else(|| (String::new(), "-".to_string()))
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

/// Whether a line is a scene's entry.
fn is_entry(line: &str) -> bool {
    ITEM.captures(line)
        .is_some_and(|item| scene_link(item[3].trim()).is_some())
}

fn chapter_title(text: &str) -> Option<String> {
    let title = plain_text(&strip_comments(text)).trim().to_string();
    (!title.is_empty() && !title.eq_ignore_ascii_case(UNTITLED)).then_some(title)
}

fn indent_of(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

/// Indentation in columns, with a tab as four.
fn width(indent: &str) -> usize {
    indent.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum()
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn owned(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| line.to_string()).collect()
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

/// Where a moved scene goes in its book's contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spot {
    /// Just before this scene, in its chapter.
    Before(PathBuf),
    /// At the end of the chapter at this position, counting from 0 across parts.
    Chapter(usize),
    /// After everything else, as a chapter of its own.
    End,
    /// Out of the contents, to be placed later.
    Unplaced,
}

/// Where a moved chapter goes, with its scenes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapterSpot {
    /// Just before the chapter at this position, counting from 0 across parts, in its part.
    Before(usize),
    /// Just after the chapter at this position, in its part.
    After(usize),
    /// First in the part at this position, counting from 0: into it, if it has no chapters yet.
    Part(usize),
}

/// Where lines go among the section's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// Just before the entry on this line, in its chapter.
    Before(usize),
    /// At the end of the chapter at this position, counting from 0 across parts.
    Chapter(usize),
    /// After everything else, as a chapter of its own.
    End,
}

/// Lines that move together: an entry or a chapter, with the lines indented beneath it.
struct Block {
    /// The indentation of its first line, which the others are indented against.
    indent: String,
    lines: Vec<String>,
}

impl Block {
    fn new(lines: &[&str], range: Range<usize>) -> Block {
        Block {
            indent: indent_of(lines[range.start]).to_string(),
            lines: owned(&lines[range]),
        }
    }

    /// Its lines, indented as they would be with the first at `indent`.
    fn at(&self, indent: &str) -> Vec<String> {
        self.lines
            .iter()
            .map(|line| {
                if is_blank(line) {
                    line.clone()
                } else if let Some(rest) = line.strip_prefix(self.indent.as_str()) {
                    format!("{indent}{rest}")
                } else {
                    format!("{indent}{}", line.trim_start())
                }
            })
            .collect()
    }
}

/// The section's lines with an item for `link` added at `place`. Every other line stays as it
/// was, except that a scene listed as a chapter of its own becomes the first scene of an untitled
/// chapter when another scene joins it.
pub fn insert(lines: &[&str], place: Place, link: &str) -> Result<Vec<String>> {
    let target = match place {
        Place::Unplaced => return Ok(owned(lines)),
        Place::End => Target::End,
        Place::Chapter(n) => Target::Chapter(n),
    };
    insert_link(lines, target, link)
}

fn insert_link(lines: &[&str], target: Target, link: &str) -> Result<Vec<String>> {
    let (_, marker) = Listing::parse(lines).style(lines);
    let block = Block {
        indent: String::new(),
        lines: vec![format!("{marker} {link}")],
    };
    place(lines, target, &block)
}

/// The section's lines with `block` added at `target`, indented to fit there.
fn place(lines: &[&str], target: Target, block: &Block) -> Result<Vec<String>> {
    let listing = Listing::parse(lines);
    let mut out = owned(lines);
    let (base, _) = listing.style(lines);
    match target {
        Target::End => {
            let part = listing.parts.last().expect("there is always a part");
            let new = block.at(&base);
            match part.chapters.last() {
                Some(chapter) => {
                    out.splice(chapter.lines.end..chapter.lines.end, new);
                }
                None => {
                    let (at, new) = into_empty_part(lines, part.heading, new);
                    out.splice(at..at, new);
                }
            }
        }
        Target::Chapter(n) => {
            let count = listing.chapters().count();
            let Some(chapter) = listing.chapters().nth(n) else {
                bail!(
                    "there is no chapter {} in the contents, which list {count}",
                    n + 1
                );
            };
            let nested = nested_indent(lines, &listing, chapter, &base);
            let end = chapter.lines.end;
            out.splice(end..end, block.at(&nested));
            if chapter.own {
                let start = chapter.lines.start;
                let head = start..entry_end(lines, start);
                let mut new = vec![untitled_item(lines[start])];
                new.extend(Block::new(lines, head.clone()).at(&nested));
                out.splice(head, new);
            }
        }
        Target::Before(line) => {
            let Some(chapter) = listing
                .chapters()
                .find(|chapter| chapter.entries.contains(&line))
            else {
                bail!("no scene is listed on line {} of the contents", line + 1);
            };
            if chapter.own && line == chapter.lines.start {
                let nested = nested_indent(lines, &listing, chapter, &base);
                let head = line..entry_end(lines, line);
                let mut new = vec![untitled_item(lines[line])];
                new.extend(block.at(&nested));
                new.extend(Block::new(lines, head.clone()).at(&nested));
                out.splice(head, new);
            } else {
                out.splice(line..line, block.at(indent_of(lines[line])));
            }
        }
    }
    Ok(out)
}

/// The item of an untitled chapter, in the style of the item on `line`. A scene listed as a
/// chapter of its own is put beneath one when another scene joins it.
fn untitled_item(line: &str) -> String {
    let item = ITEM.captures(line).expect("a chapter starts with an item");
    format!("{}{} {UNTITLED}", &item[1], &item[2])
}

/// Where lines go in a part that has no chapters yet, the part with its heading on line
/// `heading` (`None` for the start of the section): after whatever it holds, with a blank line
/// between them and anything else.
fn into_empty_part(
    lines: &[&str],
    heading: Option<usize>,
    mut new: Vec<String>,
) -> (usize, Vec<String>) {
    let start = heading.map_or(0, |heading| heading + 1);
    let end = start
        + lines[start..]
            .iter()
            .position(|line| part_heading(line).is_some())
            .unwrap_or(lines.len() - start);
    let at = (heading.unwrap_or(0)..end)
        .rev()
        .find(|&i| !is_blank(lines[i]))
        .map_or(start, |i| i + 1);
    if at > 0 && !is_blank(lines[at - 1]) {
        new.insert(0, String::new());
    }
    if at < lines.len() && !is_blank(lines[at]) {
        new.push(String::new());
    }
    (at, new)
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

/// Where the entry on line `from` ends: after the lines indented beneath it, up to the next
/// scene.
fn entry_end(lines: &[&str], from: usize) -> usize {
    let indent = width(indent_of(lines[from]));
    let mut end = from + 1;
    for (i, line) in lines.iter().enumerate().skip(from + 1) {
        if is_blank(line) {
            continue;
        }
        if width(indent_of(line)) <= indent || part_heading(line).is_some() || is_entry(line) {
            break;
        }
        end = i + 1;
    }
    end
}

/// After lines were taken out at `at`, removes the blank line before it if a blank line or the
/// end of the section follows, so the list isn't left with a double gap. Returns whether it did.
fn close_gap(out: &mut Vec<String>, at: usize) -> bool {
    let gap = at > 0 && is_blank(&out[at - 1]) && out.get(at).is_none_or(|line| is_blank(line));
    if gap {
        out.remove(at - 1);
    }
    gap
}

/// The section's lines with the scene listed on line `from` moved to `to`, or with `None`, out
/// of the contents. Its chapter stays, even if that leaves it empty: a scene listed as a chapter
/// of its own leaves an untitled chapter's item in its place.
fn move_entry(lines: &[&str], from: usize, to: Option<Target>) -> Result<Vec<String>> {
    let listing = Listing::parse(lines);
    let chapters: Vec<&ListedChapter> = listing.chapters().collect();
    let Some((c, chapter)) = chapters
        .iter()
        .enumerate()
        .find(|(_, chapter)| chapter.entries.contains(&from))
    else {
        bail!("no scene is listed on line {} of the contents", from + 1);
    };
    let k = chapter
        .entries
        .iter()
        .position(|&line| line == from)
        .expect("the chapter lists it");
    let last = k + 1 == chapter.entries.len();
    let alone = chapter.entries.len() == 1 && chapter.title.is_none();
    let already = match to {
        Some(Target::Before(line)) => line == from || chapter.entries.get(k + 1) == Some(&line),
        Some(Target::Chapter(n)) => n == c && last,
        Some(Target::End) => alone && c + 1 == chapters.len(),
        None => false,
    };
    if already {
        return Ok(owned(lines));
    }

    let end = entry_end(lines, from);
    let block = Block::new(lines, from..end);
    let range = from..end;
    let replacement = if chapter.own && from == chapter.lines.start {
        vec![untitled_item(lines[from])]
    } else {
        Vec::new()
    };

    let mut out = owned(lines);
    // How far the lines after the removed ones move up. Every chapter stays where it was.
    let mut shift = range.len() - replacement.len();
    let emptied = replacement.is_empty();
    out.splice(range.clone(), replacement);
    if emptied && close_gap(&mut out, range.start) {
        shift += 1;
    }
    let Some(to) = to else {
        return Ok(out);
    };
    let to = match to {
        Target::Before(line) if line >= range.end => Target::Before(line - shift),
        to => to,
    };
    let borrowed: Vec<&str> = out.iter().map(String::as_str).collect();
    place(&borrowed, to, &block)
}

/// The section's lines with chapter `from`, counting from 0 across parts, moved with everything
/// beneath it to `to`.
fn move_chapter_lines(lines: &[&str], from: usize, to: ChapterSpot) -> Result<Vec<String>> {
    let listing = Listing::parse(lines);
    let chapters: Vec<(usize, &ListedChapter)> = listing
        .parts
        .iter()
        .enumerate()
        .flat_map(|(p, part)| part.chapters.iter().map(move |chapter| (p, chapter)))
        .collect();
    let count = chapters.len();
    let chapter_at = |n: usize| {
        chapters.get(n).copied().ok_or_else(|| {
            anyhow!(
                "there is no chapter {} in the contents, which list {count}",
                n + 1
            )
        })
    };
    let (part, moving) = chapter_at(from)?;
    let in_part = |n: usize| chapters.get(n).is_some_and(|(p, _)| *p == part);
    let indent = |chapter: &ListedChapter| indent_of(lines[chapter.lines.start]).to_string();
    let block = Block::new(lines, moving.lines.clone());
    // Where it goes, as it will be indented there, and whether it is there already.
    let (at, new, already) = match to {
        ChapterSpot::Before(n) => {
            let (_, chapter) = chapter_at(n)?;
            let already = n == from || (n == from + 1 && in_part(n));
            (chapter.lines.start, block.at(&indent(chapter)), already)
        }
        ChapterSpot::After(n) => {
            let (_, chapter) = chapter_at(n)?;
            let already = n == from || (n + 1 == from && in_part(n));
            (chapter.lines.end, block.at(&indent(chapter)), already)
        }
        ChapterSpot::Part(p) => {
            let Some(target) = listing.parts.get(p) else {
                bail!(
                    "there is no part {} in the contents, which have {}",
                    p + 1,
                    listing.parts.len()
                );
            };
            match target.chapters.first() {
                Some(first) => (
                    first.lines.start,
                    block.at(&indent(first)),
                    first.lines == moving.lines,
                ),
                None => {
                    let new = block.at(&indent(moving));
                    let (at, new) = into_empty_part(lines, target.heading, new);
                    (at, new, false)
                }
            }
        }
    };
    if already {
        return Ok(owned(lines));
    }
    let range = moving.lines.clone();
    if range.contains(&at) {
        bail!("a chapter can't go inside itself");
    }
    // A list with blank lines between its chapters keeps them around the moved one too. A part
    // with no chapters has them already.
    let spaced = is_loose(lines, &listing)
        && !matches!(to, ChapterSpot::Part(p) if listing.parts[p].chapters.is_empty());
    let mut out = owned(lines);
    let put = |out: &mut Vec<String>, mut new: Vec<String>| {
        if spaced {
            if at > 0 && !is_blank(&out[at - 1]) {
                new.insert(0, String::new());
            }
            if at < out.len() && !is_blank(&out[at]) {
                new.push(String::new());
            }
        }
        out.splice(at..at, new);
    };
    if at >= range.end {
        put(&mut out, new);
        out.splice(range.clone(), []);
        close_gap(&mut out, range.start);
    } else {
        out.splice(range.clone(), []);
        close_gap(&mut out, range.start);
        put(&mut out, new);
    }
    Ok(out)
}

/// Whether the list leaves a blank line between chapters, which a moved chapter keeps to.
fn is_loose(lines: &[&str], listing: &Listing) -> bool {
    listing.parts.iter().any(|part| {
        part.chapters.windows(2).any(|pair| {
            lines[pair[0].lines.end..pair[1].lines.start]
                .iter()
                .any(|line| is_blank(line))
        })
    })
}

/// A change to a book's contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    /// The book note's content hash after the change.
    pub hash: String,
    /// The section's lines as they were before it, which [`restore`] puts back.
    pub previous: Vec<String>,
}

/// Moves one of `book`'s scenes to `to` in its contents: within its chapter, into another, out of
/// the contents, or, for an unplaced scene, into them. The lines indented beneath its entry, such
/// as a note about it, go with it. Only the `## Contents` section changes, and nothing is written
/// if the book note no longer has the content `base_hash` was taken from. `None` means the scene
/// was there already.
pub fn move_scene(
    vault: &Vault,
    book: &Book,
    base_hash: &str,
    scene: &Path,
    to: &Spot,
) -> Result<Option<Moved>> {
    let content = read_unchanged(book, base_hash, "nothing was moved")?;
    let lines = section(split_frontmatter(&content).1, HEADING).unwrap_or_default();
    let scenes = book.scenes()?;
    let matcher = Matcher::new(&book.dir, &scenes);
    let entries = Listing::parse(&lines).entries(&matcher);
    let index = |path: &Path| {
        scenes
            .iter()
            .position(|scene| scene.note.path == path)
            .ok_or_else(|| anyhow!("{} isn't a scene in {}", path.display(), book.title()))
    };
    let moving = index(scene)?;
    let name = scenes[moving].title();
    if entries[moving].len() > 1 {
        bail!(
            "{name} is listed more than once in the contents of {}: remove the extra entries, \
             then move it",
            book.title()
        );
    }
    let target = match to {
        Spot::Before(other) => {
            let other = index(other)?;
            let Some(&line) = entries[other].first() else {
                bail!("{} isn't in the contents", scenes[other].title());
            };
            Some(Target::Before(line))
        }
        Spot::Chapter(n) => Some(Target::Chapter(*n)),
        Spot::End => Some(Target::End),
        Spot::Unplaced => None,
    };
    let listed = match (entries[moving].first(), target) {
        (Some(&from), target) => move_entry(&lines, from, target)?,
        (None, Some(target)) => {
            let link = LinkIndex::build(vault)?.link(&book.note.path, scene, &name);
            insert_link(&lines, target, &link)?
        }
        (None, None) => return Ok(None),
    };
    write_moved(book, &content, &lines, listed)
}

/// Moves one of `book`'s chapters, counting from 0 across parts, to `to` in its contents, with
/// its scenes and everything else beneath its item. Like [`move_scene`], it changes only the
/// `## Contents` section, and only if the book note is unchanged since `base_hash`.
pub fn move_chapter(
    book: &Book,
    base_hash: &str,
    chapter: usize,
    to: ChapterSpot,
) -> Result<Option<Moved>> {
    let content = read_unchanged(book, base_hash, "nothing was moved")?;
    let lines = section(split_frontmatter(&content).1, HEADING).unwrap_or_default();
    let listed = move_chapter_lines(&lines, chapter, to)?;
    write_moved(book, &content, &lines, listed)
}

/// Puts `book`'s contents back as they were before a move, from [`Moved::previous`], unless the
/// book note has changed since `base_hash` was taken from it. Returns the note's new hash.
pub fn restore(book: &Book, base_hash: &str, previous: &[String]) -> Result<String> {
    match save_section(&book.note.path, HEADING, previous, base_hash)? {
        Saved::Written { hash, .. } => Ok(hash),
        Saved::Conflict { .. } => Err(changed(book, "the move wasn't undone")),
    }
}

fn read_unchanged(book: &Book, base_hash: &str, outcome: &str) -> Result<String> {
    let path = &book.note.path;
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if content_hash(&content) != base_hash {
        return Err(changed(book, outcome));
    }
    Ok(content)
}

fn write_moved(
    book: &Book,
    content: &str,
    previous: &[&str],
    listed: Vec<String>,
) -> Result<Option<Moved>> {
    if listed
        .iter()
        .map(String::as_str)
        .eq(previous.iter().copied())
    {
        return Ok(None);
    }
    match save_section(&book.note.path, HEADING, &listed, &content_hash(content))? {
        Saved::Written { hash, .. } => Ok(Some(Moved {
            hash,
            previous: owned(previous),
        })),
        Saved::Conflict { .. } => Err(changed(book, "nothing was moved")),
    }
}

fn changed(book: &Book, outcome: &str) -> anyhow::Error {
    anyhow!(
        "the contents of {} changed on disk since they were read, so {outcome}",
        book.title()
    )
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
        let matcher = Matcher::new(book_dir, &scenes);
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
                    match matcher.find(&link.target) {
                        Some(i) => match slots[i].take() {
                            Some(scene) => chapter.scenes.push(scene),
                            None => contents.repeated.push(stem(&matcher.paths[i])),
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

/// Finds the scene a contents entry links to, among a book's scenes.
struct Matcher<'a> {
    book_dir: &'a Path,
    paths: Vec<PathBuf>,
    aliases: Vec<Vec<String>>,
}

impl<'a> Matcher<'a> {
    fn new(book_dir: &'a Path, scenes: &[Scene]) -> Matcher<'a> {
        Matcher {
            book_dir,
            paths: scenes.iter().map(|scene| scene.note.path.clone()).collect(),
            aliases: scenes
                .iter()
                .map(|scene| {
                    let aliases = scene.note.aliases();
                    aliases.iter().map(|alias| alias.to_lowercase()).collect()
                })
                .collect(),
        }
    }

    /// The scene `target` names: by path from the book's folder or the vault's root, else by file
    /// name, else by alias.
    fn find(&self, target: &str) -> Option<usize> {
        let target = target.trim();
        let lower = |path: &Path| path.to_string_lossy().to_lowercase();
        if target.contains('/') {
            let root = self.book_dir.ancestors().nth(3).unwrap_or(self.book_dir);
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
            return [wanted(self.book_dir), wanted(root)]
                .iter()
                .find_map(|wanted| self.paths.iter().position(|path| lower(path) == *wanted));
        }
        let name = target.to_lowercase();
        let name = name.strip_suffix(".md").unwrap_or(&name);
        self.paths
            .iter()
            .position(|path| stem(path).to_lowercase() == name)
            .or_else(|| {
                self.aliases
                    .iter()
                    .position(|names| names.iter().any(|alias| alias == name))
            })
    }
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

    const BOOK: &str = "\
%% The book in order. %%

### Part One

- Chapter
  - [[arrival|Arrival]]
    - needs a stronger ending
  - [[first-light|First Light]]
- The Long Night
  - [[leaving-home|Leaving Home]]
  - a note about this chapter
- [[interlude|Interlude]]

### Part Two

- Chapter
  - [[coda|Coda]]";

    fn line_of(text: &str, scene: &str) -> usize {
        let link = format!("[[{scene}");
        text.lines().position(|line| line.contains(&link)).unwrap()
    }

    fn before(text: &str, scene: &str) -> Option<Target> {
        Some(Target::Before(line_of(text, scene)))
    }

    fn moved(text: &str, scene: &str, to: Option<Target>) -> String {
        move_entry(&lines(text), line_of(text, scene), to)
            .unwrap()
            .join("\n")
    }

    #[test]
    fn moves_scenes_within_and_between_chapters() {
        assert_eq!(
            moved(BOOK, "first-light", before(BOOK, "arrival")),
            BOOK.replace(
                "  - [[arrival|Arrival]]\n    - needs a stronger ending\n  - [[first-light|First Light]]\n",
                "  - [[first-light|First Light]]\n  - [[arrival|Arrival]]\n    - needs a stronger ending\n"
            ),
            "the note beneath a scene goes with it"
        );
        assert_eq!(
            moved(BOOK, "arrival", Some(Target::Chapter(1))),
            BOOK.replace("  - [[arrival|Arrival]]\n    - needs a stronger ending\n", "")
                .replace(
                    "  - a note about this chapter\n",
                    "  - a note about this chapter\n  - [[arrival|Arrival]]\n    - needs a stronger ending\n"
                ),
            "at the end of the chapter, after its notes"
        );
        assert_eq!(
            moved(BOOK, "leaving-home", before(BOOK, "interlude")),
            BOOK.replace("  - [[leaving-home|Leaving Home]]\n", "")
                .replace(
                    "- [[interlude|Interlude]]\n",
                    "- Chapter\n  - [[leaving-home|Leaving Home]]\n  - [[interlude|Interlude]]\n"
                ),
            "a chapter of its own becomes an untitled chapter, and a titled one stays when emptied"
        );
        assert_eq!(
            moved(BOOK, "arrival", Some(Target::End)),
            format!(
                "{}\n- [[arrival|Arrival]]\n  - needs a stronger ending",
                BOOK.replace(
                    "  - [[arrival|Arrival]]\n    - needs a stronger ending\n",
                    ""
                )
            ),
            "as a chapter of its own, indented to fit"
        );
    }

    #[test]
    fn keeps_the_chapters_a_move_empties() {
        assert_eq!(
            moved(BOOK, "coda", None),
            BOOK.replace("\n  - [[coda|Coda]]", ""),
            "an untitled chapter stays without its last scene"
        );
        assert_eq!(
            moved(BOOK, "interlude", Some(Target::Chapter(0))),
            BOOK.replace("- [[interlude|Interlude]]\n", "- Chapter\n")
                .replace(
                    "  - [[first-light|First Light]]\n",
                    "  - [[first-light|First Light]]\n  - [[interlude|Interlude]]\n"
                ),
            "a chapter of its own stays as an untitled chapter"
        );
        assert_eq!(
            moved(BOOK, "interlude", Some(Target::Chapter(3))),
            format!(
                "{}\n  - [[interlude|Interlude]]",
                BOOK.replace("- [[interlude|Interlude]]\n", "- Chapter\n")
            )
        );
        assert_eq!(
            moved("- [[a]]\n  - [[b]]\n- [[c]]", "a", None),
            "- Chapter\n  - [[b]]\n- [[c]]",
            "a chapter of its own keeps the scenes beneath it"
        );
        assert_eq!(
            moved("- One\n  - [[a]]\n\n  - [[b]]\n\n  - [[c]]", "b", None),
            "- One\n  - [[a]]\n\n  - [[c]]",
            "without leaving two blank lines"
        );
    }

    #[test]
    fn leaves_scenes_that_are_already_there() {
        for (scene, to) in [
            ("arrival", before(BOOK, "arrival")),
            ("arrival", before(BOOK, "first-light")),
            ("first-light", Some(Target::Chapter(0))),
            ("coda", Some(Target::End)),
        ] {
            assert_eq!(moved(BOOK, scene, to), BOOK, "{scene} to {to:?}");
        }
        assert_ne!(moved(BOOK, "interlude", Some(Target::End)), BOOK);
        assert!(move_entry(&lines(BOOK), 0, None).is_err());
    }

    fn chapter_moved(text: &str, from: usize, to: ChapterSpot) -> String {
        move_chapter_lines(&lines(text), from, to)
            .unwrap()
            .join("\n")
    }

    #[test]
    fn moves_chapters_with_their_scenes() {
        let first = "- Chapter\n  - [[arrival|Arrival]]\n    - needs a stronger ending\n  - [[first-light|First Light]]\n";
        let night =
            "- The Long Night\n  - [[leaving-home|Leaving Home]]\n  - a note about this chapter\n";
        let interlude = "- [[interlude|Interlude]]\n";
        let one = |order: &str| BOOK.replace(&format!("{first}{night}{interlude}"), order);
        assert_eq!(
            chapter_moved(BOOK, 1, ChapterSpot::Before(0)),
            one(&format!("{night}{first}{interlude}"))
        );
        assert_eq!(
            chapter_moved(BOOK, 0, ChapterSpot::After(2)),
            one(&format!("{night}{interlude}{first}"))
        );
        assert_eq!(
            chapter_moved(BOOK, 0, ChapterSpot::Part(1)),
            one(&format!("{night}{interlude}"))
                .replace("### Part Two\n\n", &format!("### Part Two\n\n{first}"))
        );
        assert_eq!(
            chapter_moved(BOOK, 2, ChapterSpot::Before(3)),
            one(&format!("{first}{night}")).replace(
                "### Part Two\n\n",
                "### Part Two\n\n- [[interlude|Interlude]]\n"
            ),
            "into the next part"
        );
        assert_eq!(
            chapter_moved(BOOK, 3, ChapterSpot::After(2)),
            BOOK.replace("\n\n- Chapter\n  - [[coda|Coda]]", "")
                .replace(
                    interlude,
                    &format!("{interlude}- Chapter\n  - [[coda|Coda]]\n")
                ),
            "out of the last part, which stays"
        );
        for (from, to) in [
            (0, ChapterSpot::Before(0)),
            (0, ChapterSpot::Before(1)),
            (1, ChapterSpot::After(0)),
            (0, ChapterSpot::Part(0)),
            (3, ChapterSpot::After(3)),
        ] {
            assert_eq!(chapter_moved(BOOK, from, to), BOOK, "{from} to {to:?}");
        }
        assert!(move_chapter_lines(&lines(BOOK), 4, ChapterSpot::Before(0)).is_err());
        assert!(move_chapter_lines(&lines(BOOK), 0, ChapterSpot::Part(2)).is_err());
    }

    #[test]
    fn moves_chapters_into_empty_parts_and_keeps_the_spacing() {
        assert_eq!(
            chapter_moved(
                "### One\n\n- [[a]]\n- [[b]]\n\n### Two\n",
                1,
                ChapterSpot::Part(1)
            ),
            "### One\n\n- [[a]]\n\n### Two\n\n- [[b]]"
        );
        assert_eq!(
            chapter_moved(
                "### One\n\n### Two\n\n- [[a]]\n- [[b]]",
                1,
                ChapterSpot::Part(0)
            ),
            "### One\n\n- [[b]]\n\n### Two\n\n- [[a]]"
        );
        assert_eq!(
            chapter_moved(
                "%% x %%\n\n- [[a]]\n\n- [[b]]\n\n- [[c]]",
                0,
                ChapterSpot::After(2)
            ),
            "%% x %%\n\n- [[b]]\n\n- [[c]]\n\n- [[a]]",
            "blank lines between chapters stay between them"
        );
        assert_eq!(
            chapter_moved(
                "%% x %%\n\n- [[a]]\n\n- [[b]]\n\n- [[c]]",
                2,
                ChapterSpot::Before(0)
            ),
            "%% x %%\n\n- [[c]]\n\n- [[a]]\n\n- [[b]]"
        );
    }
}
