//! A project is a world: `<slug>/<slug>.md`, its world bible in `bible/`, and one or more books
//! in `books/<book>/`, each with its own note, story flow and chapters.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use chrono::NaiveDate;

use crate::ids;
use crate::links::links;
use crate::note::{Note, section};
use crate::text::{leading_number, name_from_file, natural_cmp, slugify, strip_leading_number};
use crate::vault::markdown_files;
use crate::words;

/// Chapter statuses, in the order a chapter moves through them.
pub const CHAPTER_STATUSES: [&str; 4] = ["outline", "draft", "revised", "final"];

/// Book statuses, in the order a book usually moves through them.
pub const BOOK_STATUSES: [&str; 7] = [
    "planning",
    "drafting",
    "revising",
    "complete",
    "submitted",
    "published",
    "shelved",
];

/// The project folder holding the world bible, shared by every book.
pub const BIBLE_DIR: &str = "bible";
/// The project folder holding its books.
pub const BOOKS_DIR: &str = "books";
/// The heading in a project note under which its books are listed in order.
pub const BOOKS_HEADING: &str = "Books";

/// Folders at the vault root that are never projects.
pub const RESERVED: [&str; 3] = ["research", "scratchpad", "templates"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Chapter,
    Character,
    Location,
    World,
}

impl NoteKind {
    pub const ALL: [NoteKind; 4] = [
        NoteKind::Chapter,
        NoteKind::Character,
        NoteKind::Location,
        NoteKind::World,
    ];

    pub fn key(self) -> &'static str {
        match self {
            NoteKind::Chapter => "chapter",
            NoteKind::Character => "character",
            NoteKind::Location => "location",
            NoteKind::World => "world",
        }
    }

    pub fn from_key(key: &str) -> Option<NoteKind> {
        NoteKind::ALL
            .into_iter()
            .find(|kind| kind.key().eq_ignore_ascii_case(key))
    }

    pub fn folder(self) -> &'static str {
        match self {
            NoteKind::Chapter => "chapters",
            NoteKind::Character => "characters",
            NoteKind::Location => "locations",
            NoteKind::World => "world",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            NoteKind::Chapter => "a numbered chapter in a book's chapters/",
            NoteKind::Character => "a character sheet in bible/characters/",
            NoteKind::Location => "a place in bible/locations/",
            NoteKind::World => "how something in the world works, in bible/world/",
        }
    }

    /// Whether notes of this kind belong to the world bible rather than to a book.
    pub fn is_bible(self) -> bool {
        self != NoteKind::Chapter
    }
}

/// Length categories used by the SFWA (Nebula) and Hugo awards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    Flash,
    ShortStory,
    Novelette,
    Novella,
    Novel,
}

impl Form {
    pub const ALL: [Form; 5] = [
        Form::Flash,
        Form::ShortStory,
        Form::Novelette,
        Form::Novella,
        Form::Novel,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Form::Flash => "flash",
            Form::ShortStory => "short-story",
            Form::Novelette => "novelette",
            Form::Novella => "novella",
            Form::Novel => "novel",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Form::Flash => "flash fiction",
            Form::ShortStory => "short story",
            Form::Novelette => "novelette",
            Form::Novella => "novella",
            Form::Novel => "novel",
        }
    }

    /// Accepts the key ("short-story") or the label ("short story").
    pub fn from_key(key: &str) -> Option<Form> {
        let key = key.trim();
        Form::ALL.into_iter().find(|form| {
            form.key().eq_ignore_ascii_case(key) || form.label().eq_ignore_ascii_case(key)
        })
    }

    /// Word range of the category, as text.
    pub fn range(self) -> &'static str {
        match self {
            Form::Flash => "under 1,000 words",
            Form::ShortStory => "1,000–7,500 words",
            Form::Novelette => "7,500–17,500 words",
            Form::Novella => "17,500–40,000 words",
            Form::Novel => "40,000+ words",
        }
    }

    /// A typical target for a new project of this form.
    pub fn default_target(self) -> u64 {
        match self {
            Form::Flash => 1_000,
            Form::ShortStory => 5_000,
            Form::Novelette => 12_000,
            Form::Novella => 30_000,
            Form::Novel => 90_000,
        }
    }

    /// The category a piece of this length falls into.
    pub fn for_length(words: u64) -> Form {
        match words {
            0..1_000 => Form::Flash,
            1_000..7_500 => Form::ShortStory,
            7_500..17_500 => Form::Novelette,
            17_500..40_000 => Form::Novella,
            _ => Form::Novel,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    /// The folder name, which is also the project note's name.
    pub slug: String,
    pub dir: PathBuf,
    pub note: Note,
}

impl Project {
    /// Loads the project in `dir`, or `None` if `dir` is not a project folder: one at the vault
    /// root, not a reserved name, holding a note of the same name with `type: project`.
    pub fn load(dir: &Path) -> Result<Option<Project>> {
        let Some(slug) = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            return Ok(None);
        };
        let note_path = dir.join(format!("{slug}.md"));
        if RESERVED.contains(&slug.as_str()) || !note_path.is_file() {
            return Ok(None);
        }
        let note = Note::load(&note_path)?;
        if note.frontmatter.str("type").as_deref() != Some("project") {
            return Ok(None);
        }
        Ok(Some(Project {
            note,
            dir: dir.to_path_buf(),
            slug,
        }))
    }

    pub fn title(&self) -> String {
        self.note.title()
    }

    /// The permanent ID written when the project was created, if it has a usable one.
    pub fn id(&self) -> Option<String> {
        valid_id(&self.note)
    }

    pub fn genre(&self) -> Vec<String> {
        self.note.frontmatter.list("genre")
    }

    pub fn bible_dir(&self) -> PathBuf {
        self.dir.join(BIBLE_DIR)
    }

    pub fn books_dir(&self) -> PathBuf {
        self.dir.join(BOOKS_DIR)
    }

    /// Where world-bible notes of `kind` live. Chapters belong to a book instead.
    pub fn folder(&self, kind: NoteKind) -> PathBuf {
        debug_assert!(kind.is_bible(), "chapters live in a book");
        self.bible_dir().join(kind.folder())
    }

    /// World-bible notes of one kind, sorted by name.
    pub fn notes(&self, kind: NoteKind) -> Result<Vec<Note>> {
        let mut notes = markdown_files(&self.folder(kind))?
            .iter()
            .map(|path| Note::load(path))
            .collect::<Result<Vec<_>>>()?;
        notes.sort_by(|a, b| natural_cmp(&a.title(), &b.title()));
        Ok(notes)
    }

    /// The project's books: those listed under `## Books` in the project note, in that order,
    /// then any others in `books/`, by title.
    pub fn books(&self) -> Result<Vec<Book>> {
        let mut books = Vec::new();
        let dirs = match std::fs::read_dir(self.books_dir()) {
            Ok(entries) => entries
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<Vec<_>>>()?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        for dir in dirs {
            if dir.is_dir() {
                books.extend(Book::load(&dir, &self.slug)?);
            }
        }
        let listed = self.listed_books();
        let position = |book: &Book| listed.iter().position(|slug| *slug == book.slug);
        books.sort_by(|a, b| match (position(a), position(b)) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => natural_cmp(&a.title(), &b.title()),
        });
        Ok(books)
    }

    /// The file names of the book notes linked under `## Books`, in order.
    fn listed_books(&self) -> Vec<String> {
        let lines = section(&self.note.body, BOOKS_HEADING).unwrap_or_default();
        links(&lines.join("\n"))
            .into_iter()
            .filter_map(|link| {
                let stem = link.target.rsplit('/').next()?.trim_end_matches(".md");
                Some(stem.to_lowercase())
            })
            .collect()
    }

    /// Picks a book by slug, title, or an unambiguous part of either. With no query, uses the
    /// book `cwd` is inside, or the project's only book.
    pub fn find_book(&self, query: Option<&str>, cwd: &Path) -> Result<Book> {
        let books = self.books()?;
        if books.is_empty() {
            bail!("{} has no books yet", self.title());
        }
        let names = || {
            books
                .iter()
                .map(|b| b.slug.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let Some(query) = query else {
            if let Some(book) = books.iter().find(|b| cwd.starts_with(&b.dir)) {
                return Ok(book.clone());
            }
            if let [only] = books.as_slice() {
                return Ok(only.clone());
            }
            bail!("which book of {}? Name one of: {}", self.title(), names());
        };
        let wanted_slug = slugify(query);
        let wanted_title = query.to_lowercase();
        if let Some(book) = books
            .iter()
            .find(|b| b.slug == wanted_slug || b.title().to_lowercase() == wanted_title)
        {
            return Ok(book.clone());
        }
        let matches: Vec<&Book> = books
            .iter()
            .filter(|b| {
                b.slug.contains(&wanted_slug) || b.title().to_lowercase().contains(&wanted_title)
            })
            .collect();
        match matches.as_slice() {
            [only] => Ok((*only).clone()),
            [] => bail!(
                "no book of {} matches \"{query}\". Books: {}",
                self.title(),
                names()
            ),
            several => bail!(
                "\"{query}\" matches several books: {}",
                several
                    .iter()
                    .map(|b| b.slug.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

/// A book: `books/<slug>/<slug>.md`, its story flow `<slug>-flow.md`, and `chapters/`.
#[derive(Debug, Clone)]
pub struct Book {
    /// The folder name, which is also the book note's name.
    pub slug: String,
    pub dir: PathBuf,
    pub note: Note,
    /// The slug of the project the book belongs to.
    pub project: String,
}

impl Book {
    /// Loads the book in `dir`, or `None` if `dir` holds no note of the same name.
    pub fn load(dir: &Path, project: &str) -> Result<Option<Book>> {
        let Some(slug) = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            return Ok(None);
        };
        let note_path = dir.join(format!("{slug}.md"));
        if !note_path.is_file() {
            return Ok(None);
        }
        Ok(Some(Book {
            note: Note::load(&note_path)?,
            dir: dir.to_path_buf(),
            slug,
            project: project.to_string(),
        }))
    }

    pub fn title(&self) -> String {
        self.note.title()
    }

    /// The permanent ID written when the book was created, if it has a usable one.
    pub fn id(&self) -> Option<String> {
        valid_id(&self.note)
    }

    /// What the writing log records this book's words under: its ID, or its project and folder
    /// names if it has none.
    pub fn log_key(&self) -> String {
        self.id()
            .unwrap_or_else(|| format!("{}/{}", self.project, self.slug))
    }

    pub fn status(&self) -> Option<String> {
        self.note.frontmatter.str("status")
    }

    pub fn form(&self) -> Option<Form> {
        Form::from_key(&self.note.frontmatter.str("form")?)
    }

    pub fn target(&self) -> Option<u64> {
        self.note
            .frontmatter
            .number("target")
            .filter(|target| *target > 0)
    }

    pub fn deadline(&self) -> Option<NaiveDate> {
        self.note.frontmatter.date("deadline")
    }

    pub fn flow_path(&self) -> PathBuf {
        self.dir.join(format!("{}-flow.md", self.slug))
    }

    pub fn chapters_dir(&self) -> PathBuf {
        self.dir.join(NoteKind::Chapter.folder())
    }

    /// Chapters in reading order: by file path, with numbers compared as numbers.
    pub fn chapters(&self) -> Result<Vec<Chapter>> {
        let folder = self.chapters_dir();
        let mut chapters = markdown_files(&folder)?
            .into_iter()
            .map(|path| {
                let note = Note::load(&path)?;
                let relative = path
                    .strip_prefix(&folder)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                Ok(Chapter {
                    words: note.words(),
                    relative,
                    note,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        chapters.sort_by(|a, b| natural_cmp(&a.relative, &b.relative));
        Ok(chapters)
    }
}

fn valid_id(note: &Note) -> Option<String> {
    note.frontmatter.str("id").filter(|id| ids::is_valid(id))
}

#[derive(Debug, Clone)]
pub struct Chapter {
    pub note: Note,
    /// Path within `chapters/`, which sets the reading order.
    pub relative: String,
    pub words: usize,
}

impl Chapter {
    pub fn number(&self) -> Option<u32> {
        leading_number(&self.note.name)
    }

    /// The note's name ([`Note::title`]), made from the file name without its number if it has
    /// none of its own.
    pub fn title(&self) -> String {
        self.note
            .given_name()
            .or_else(|| self.note.heading())
            .unwrap_or_else(|| name_from_file(strip_leading_number(&self.note.name)))
    }

    pub fn status(&self) -> Option<String> {
        self.note.frontmatter.str("status")
    }

    pub fn pov(&self) -> Option<String> {
        self.note
            .frontmatter
            .str("pov")
            .map(|pov| words::plain_text(&pov))
    }

    pub fn synopsis(&self) -> Option<String> {
        self.note.frontmatter.str("synopsis")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_by_length() {
        assert_eq!(Form::for_length(0), Form::Flash);
        assert_eq!(Form::for_length(7_499), Form::ShortStory);
        assert_eq!(Form::for_length(7_500), Form::Novelette);
        assert_eq!(Form::for_length(39_999), Form::Novella);
        assert_eq!(Form::for_length(120_000), Form::Novel);
        assert_eq!(Form::from_key("Short Story"), Some(Form::ShortStory));
        assert_eq!(Form::from_key("epic"), None);
    }
}
