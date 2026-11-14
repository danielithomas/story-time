//! A project is a world: `<slug>/<slug>.md`, its world bible in `bible/`, and one or more books
//! in `books/<book>/`, each with its own note, story flow and scenes.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;

use crate::contents::{self, Contents, Listing};
use crate::ids;
use crate::links::links;
use crate::note::{Note, content_hash, section};
use crate::text::{natural_cmp, slugify};
use crate::vault::markdown_files;
use crate::words;

/// Scene statuses, in the order a scene moves through them.
pub const SCENE_STATUSES: [&str; 4] = ["outline", "draft", "revised", "final"];

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
/// The folder, in the vault and in each project, holding research notes.
pub const RESEARCH_DIR: &str = "research";
/// The folder in each project holding its scratchpad. The vault's is set in its config.
pub const SCRATCHPAD_DIR: &str = "scratchpad";

/// Folders at the vault root that are never projects.
pub const RESERVED: [&str; 3] = [RESEARCH_DIR, SCRATCHPAD_DIR, "templates"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Scene,
    Character,
    Location,
    World,
}

impl NoteKind {
    pub const ALL: [NoteKind; 4] = [
        NoteKind::Scene,
        NoteKind::Character,
        NoteKind::Location,
        NoteKind::World,
    ];

    pub fn key(self) -> &'static str {
        match self {
            NoteKind::Scene => "scene",
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
            NoteKind::Scene => "scenes",
            NoteKind::Character => "characters",
            NoteKind::Location => "locations",
            NoteKind::World => "world",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            NoteKind::Scene => "a scene in a book's scenes/, listed in its contents",
            NoteKind::Character => "a character sheet in bible/characters/",
            NoteKind::Location => "a place in bible/locations/",
            NoteKind::World => "how something in the world works, in bible/world/",
        }
    }

    /// Whether notes of this kind belong to the world bible rather than to a book.
    pub fn is_bible(self) -> bool {
        self != NoteKind::Scene
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

    /// The world's chronology, `bible/<slug>-timeline.md`.
    pub fn timeline_path(&self) -> PathBuf {
        self.bible_dir().join(format!("{}-timeline.md", self.slug))
    }

    pub fn research_dir(&self) -> PathBuf {
        self.dir.join(RESEARCH_DIR)
    }

    pub fn scratchpad_dir(&self) -> PathBuf {
        self.dir.join(SCRATCHPAD_DIR)
    }

    /// What the writing log records the project's scratchpad under: the project's ID, or its
    /// folder name and `/scratchpad` if it has none.
    pub fn log_key(&self) -> String {
        self.id()
            .unwrap_or_else(|| format!("{}/{SCRATCHPAD_DIR}", self.slug))
    }

    /// Where world-bible notes of `kind` live. Scenes belong to a book instead.
    pub fn folder(&self, kind: NoteKind) -> PathBuf {
        debug_assert!(kind.is_bible(), "scenes live in a book");
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

/// A book: `books/<slug>/<slug>.md`, listing its contents, its story flow `<slug>-flow.md`, and
/// `scenes/`.
#[derive(Debug, Clone)]
pub struct Book {
    /// The folder name, which is also the book note's name.
    pub slug: String,
    pub dir: PathBuf,
    pub note: Note,
    /// The book note's content hash when it was read, so changes to its contents can be refused
    /// if the note has changed on disk since.
    pub hash: String,
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
        let content = fs::read_to_string(&note_path)
            .with_context(|| format!("reading {}", note_path.display()))?;
        Ok(Some(Book {
            note: Note::parse(&note_path, &content),
            hash: content_hash(&content),
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

    pub fn scenes_dir(&self) -> PathBuf {
        self.dir.join(NoteKind::Scene.folder())
    }

    /// Every scene in `scenes/`, placed or not, by name.
    pub fn scenes(&self) -> Result<Vec<Scene>> {
        let mut scenes = markdown_files(&self.scenes_dir())?
            .iter()
            .map(|path| {
                let note = Note::load(path)?;
                Ok(Scene {
                    words: note.words(),
                    note,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        scenes.sort_by(|a, b| natural_cmp(&a.title(), &b.title()));
        Ok(scenes)
    }

    /// The contents as written under `## Contents` in the book note.
    pub fn listing(&self) -> Listing {
        Listing::parse(&section(&self.note.body, contents::HEADING).unwrap_or_default())
    }

    /// The book's scenes in reading order, as its contents list them, and those they don't.
    pub fn contents(&self) -> Result<Contents> {
        Ok(Contents::resolve(
            &self.listing(),
            &self.dir,
            self.scenes()?,
        ))
    }
}

fn valid_id(note: &Note) -> Option<String> {
    note.frontmatter.str("id").filter(|id| ids::is_valid(id))
}

#[derive(Debug, Clone)]
pub struct Scene {
    pub note: Note,
    pub words: usize,
}

impl Scene {
    pub fn title(&self) -> String {
        self.note.title()
    }

    pub fn status(&self) -> Option<String> {
        self.note.frontmatter.str("status")
    }

    /// The point-of-view character, as plain text.
    pub fn pov(&self) -> Option<String> {
        self.plain("pov")
    }

    /// Where the scene happens, as plain text.
    pub fn location(&self) -> Option<String> {
        self.plain("location")
    }

    /// When the scene happens in story time, as written.
    pub fn when(&self) -> Option<String> {
        self.note.frontmatter.str("when")
    }

    pub fn synopsis(&self) -> Option<String> {
        self.note.frontmatter.str("synopsis")
    }

    fn plain(&self, key: &str) -> Option<String> {
        self.note
            .frontmatter
            .str(key)
            .map(|value| words::plain_text(&value))
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
