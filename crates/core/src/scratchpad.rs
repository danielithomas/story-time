//! Scratchpads and research: notes kept beside the story rather than in it.
//!
//! A scratchpad holds fragments until they're useful: the vault's (`scratchpad/` by default) holds
//! fragments not tied to any world, and each project's `scratchpad/` those that belong to its
//! world but not yet to a book. Their words count towards the daily goal but towards no book.
//! Research holds what is true in the real world, in the vault's `research/` and in each
//! project's; it isn't counted.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::note::Note;
use crate::project::Project;
use crate::text::natural_cmp;
use crate::vault::{Vault, markdown_files};
use crate::words;

/// What the writing log records the vault's scratchpad under.
pub const VAULT_KEY: &str = "scratchpad";

/// How many words of its text name a scrap that has no name of its own.
const LABEL_WORDS: usize = 6;

/// A scratchpad and the notes in it.
#[derive(Debug, Clone)]
pub struct Scratchpad {
    /// The project whose scratchpad it is, or `None` for the vault's.
    pub project: Option<Project>,
    pub dir: PathBuf,
    /// Its notes, newest first. The ideas file isn't one of them, though its words count.
    pub notes: Vec<Note>,
    /// Words in every note in it, the ideas file included.
    pub words: u64,
}

impl Scratchpad {
    pub fn of_vault(vault: &Vault) -> Result<Scratchpad> {
        Scratchpad::load(None, vault.scratchpad_dir(), Some(&vault.ideas_path()))
    }

    pub fn of_project(project: &Project) -> Result<Scratchpad> {
        Scratchpad::load(Some(project.clone()), project.scratchpad_dir(), None)
    }

    fn load(project: Option<Project>, dir: PathBuf, ideas: Option<&Path>) -> Result<Scratchpad> {
        let mut notes = markdown_files(&dir)?
            .iter()
            .map(|path| Note::load(path))
            .collect::<Result<Vec<_>>>()?;
        let words = notes.iter().map(|note| note.words() as u64).sum();
        notes.retain(|note| Some(note.path.as_path()) != ideas);
        notes.sort_by(newest_first);
        Ok(Scratchpad {
            project,
            dir,
            notes,
            words,
        })
    }

    /// What the writing log records its words under: the project's ID, or `scratchpad` for the
    /// vault's.
    pub fn log_key(&self) -> String {
        self.project
            .as_ref()
            .map_or_else(|| VAULT_KEY.to_string(), Project::log_key)
    }

    /// "Scratchpad", or "Sample Story scratchpad" for a project's.
    pub fn title(&self) -> String {
        match &self.project {
            Some(project) => format!("{} scratchpad", project.title()),
            None => "Scratchpad".to_string(),
        }
    }
}

/// By `created`, newest first, then by file name, latest first, which orders captured notes
/// (named by the time of capture) by time.
fn newest_first(a: &Note, b: &Note) -> Ordering {
    let created = |note: &Note| note.frontmatter.date("created");
    created(b)
        .cmp(&created(a))
        .then_with(|| natural_cmp(&b.name, &a.name))
}

/// What to call a scrap: its name or heading, else the first words of its text.
pub fn label(note: &Note) -> String {
    if let Some(name) = note.given_name().or_else(|| note.heading()) {
        return name;
    }
    let prose = words::prose(&note.body);
    let text: Vec<&str> = prose
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .flat_map(str::split_whitespace)
        .take(LABEL_WORDS + 1)
        .collect();
    match text.len() {
        0 => "Untitled scrap".to_string(),
        n if n > LABEL_WORDS => format!("{}…", text[..LABEL_WORDS].join(" ")),
        _ => text.join(" "),
    }
}

/// The research notes in `dir`, by name.
pub fn research(dir: &Path) -> Result<Vec<Note>> {
    let mut notes = markdown_files(dir)?
        .iter()
        .map(|path| Note::load(path))
        .collect::<Result<Vec<_>>>()?;
    notes.sort_by(|a, b| natural_cmp(&a.title(), &b.title()));
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_scraps_by_their_text() {
        let note = |text: &str| Note::parse(Path::new("2026-11-05-2109.md"), text);
        assert_eq!(
            label(&note("---\ntype: scrap\n---\nOne two three.\n")),
            "One two three."
        );
        assert_eq!(
            label(&note("%% aside %%\nOne two three four five six seven\n")),
            "One two three four five six…"
        );
        assert_eq!(label(&note("# A heading\nText\n")), "A heading");
        assert_eq!(
            label(&note("---\naliases:\n  - Named\n---\nText\n")),
            "Named"
        );
        assert_eq!(label(&note("---\ntype: scrap\n---\n")), "Untitled scrap");
    }
}
