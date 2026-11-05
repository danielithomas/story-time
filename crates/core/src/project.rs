//! A story project: `<slug>/<slug>.md`, `<slug>/<slug>-flow.md` and four folders of notes.

use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::NaiveDate;

use crate::ids;
use crate::note::Note;
use crate::text::{leading_number, name_from_file, natural_cmp, strip_leading_number};
use crate::vault::markdown_files;
use crate::words;

/// Chapter statuses, in the order a chapter moves through them.
pub const CHAPTER_STATUSES: [&str; 4] = ["outline", "draft", "revised", "final"];

/// Project statuses, in the order a project usually moves through them.
pub const PROJECT_STATUSES: [&str; 5] = ["planning", "drafting", "revising", "complete", "shelved"];

/// Roles offered for characters.
pub const CHARACTER_ROLES: [&str; 4] = ["protagonist", "antagonist", "supporting", "minor"];

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
            NoteKind::Chapter => "a numbered chapter in chapters/",
            NoteKind::Character => "a character sheet in characters/",
            NoteKind::Location => "a place in locations/",
            NoteKind::World => "how something in the world works, in world/",
        }
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
    /// Loads the project in `dir`, or `None` if `dir` is not a project folder.
    pub fn load(dir: &Path) -> Result<Option<Project>> {
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
        Ok(Some(Project {
            note: Note::load(&note_path)?,
            dir: dir.to_path_buf(),
            slug,
        }))
    }

    pub fn title(&self) -> String {
        self.note.title()
    }

    /// The permanent ID written when the project was created, if it has a usable one.
    pub fn id(&self) -> Option<String> {
        self.note
            .frontmatter
            .str("id")
            .filter(|id| ids::is_valid(id))
    }

    /// What the writing log records this project's words under: its ID, or its folder name if
    /// it was created before projects had IDs.
    pub fn log_key(&self) -> String {
        self.id().unwrap_or_else(|| self.slug.clone())
    }

    pub fn status(&self) -> Option<String> {
        self.note.frontmatter.str("status")
    }

    pub fn form(&self) -> Option<Form> {
        Form::from_key(&self.note.frontmatter.str("form")?)
    }

    pub fn genre(&self) -> Option<String> {
        self.note.frontmatter.str("genre")
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

    pub fn folder(&self, kind: NoteKind) -> PathBuf {
        self.dir.join(kind.folder())
    }

    /// Chapters in reading order: by file path, with numbers compared as numbers.
    pub fn chapters(&self) -> Result<Vec<Chapter>> {
        let folder = self.folder(NoteKind::Chapter);
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

    /// Notes of one kind, sorted by name.
    pub fn notes(&self, kind: NoteKind) -> Result<Vec<Note>> {
        let mut notes = markdown_files(&self.folder(kind))?
            .iter()
            .map(|path| Note::load(path))
            .collect::<Result<Vec<_>>>()?;
        notes.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        Ok(notes)
    }
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
