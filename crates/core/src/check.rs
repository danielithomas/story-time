//! Story-bible checks: gaps between what the chapters mention and what the notes document.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

use anyhow::Result;

use crate::note::Note;
use crate::project::{NoteKind, Project};
use crate::text::contains_word;
use crate::vault::Vault;
use crate::words::{link_targets, prose};

/// The story bible: every kind of note except chapters.
pub const BIBLE: [NoteKind; 3] = [NoteKind::Character, NoteKind::Location, NoteKind::World];

#[derive(Debug, Default)]
pub struct Report {
    /// Link targets with no note or file of that name, and the notes linking to them.
    pub undocumented: BTreeMap<String, BTreeSet<String>>,
    /// Bible notes whose name, title and aliases appear in no chapter. Only checked once the
    /// chapters have words in them.
    pub not_on_page: Vec<(NoteKind, String)>,
    pub without_status: Vec<String>,
    pub without_synopsis: Vec<String>,
    pub empty_chapters: Vec<String>,
    pub unreadable_frontmatter: Vec<(PathBuf, String)>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.undocumented.is_empty()
            && self.not_on_page.is_empty()
            && self.without_status.is_empty()
            && self.without_synopsis.is_empty()
            && self.empty_chapters.is_empty()
            && self.unreadable_frontmatter.is_empty()
    }
}

pub fn check(vault: &Vault, project: &Project) -> Result<Report> {
    let known = known_names(vault)?;
    let chapters = project.chapters()?;
    let mut bible = Vec::new();
    for kind in BIBLE {
        bible.extend(project.notes(kind)?.into_iter().map(|note| (kind, note)));
    }
    let flow = project.flow_path();
    let flow = if flow.is_file() {
        Some(Note::load(&flow)?)
    } else {
        None
    };

    let mut report = Report::default();
    let notes = chapters
        .iter()
        .map(|c| &c.note)
        .chain([&project.note])
        .chain(flow.iter())
        .chain(bible.iter().map(|(_, note)| note));
    for note in notes {
        if let Some(error) = &note.frontmatter_error {
            report
                .unreadable_frontmatter
                .push((note.path.clone(), error.clone()));
        }
        for target in link_targets(&note.body) {
            if !known.contains(&target.to_lowercase()) {
                report
                    .undocumented
                    .entry(target)
                    .or_default()
                    .insert(note.name.clone());
            }
        }
    }

    if chapters.iter().any(|chapter| chapter.words > 0) {
        let page = chapters
            .iter()
            .map(|chapter| prose(&chapter.note.body))
            .collect::<Vec<_>>()
            .join("\n");
        // Linked from a chapter, or the chapter's point of view: either way, on the page.
        let linked: HashSet<String> = chapters
            .iter()
            .flat_map(|c| link_targets(&c.note.body).into_iter().chain(c.pov()))
            .map(|t| t.to_lowercase())
            .collect();
        for (kind, note) in &bible {
            let mut names = vec![note.name.clone(), note.title()];
            names.extend(note.aliases());
            if !names
                .iter()
                .any(|name| linked.contains(&name.to_lowercase()) || contains_word(&page, name))
            {
                report.not_on_page.push((*kind, note.name.clone()));
            }
        }
    }

    for chapter in &chapters {
        let name = &chapter.note.name;
        if chapter.status().is_none() {
            report.without_status.push(name.clone());
        }
        if chapter.synopsis().is_none() {
            report.without_synopsis.push(name.clone());
        }
        if chapter.words == 0 {
            report.empty_chapters.push(name.clone());
        }
    }
    Ok(report)
}

/// Lower-cased names that links can resolve to: note names, their aliases, and other files' names.
fn known_names(vault: &Vault) -> Result<HashSet<String>> {
    let mut known = HashSet::new();
    for path in vault.files()? {
        let is_note = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if is_note {
            let note = Note::load(&path)?;
            known.insert(note.name.to_lowercase());
            known.extend(note.aliases().iter().map(|alias| alias.to_lowercase()));
        } else if let Some(name) = path.file_name() {
            known.insert(name.to_string_lossy().to_lowercase());
        }
    }
    Ok(known)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Form;
    use crate::scaffold::{NewProject, add_note, create_project};
    use crate::structures::FREEFORM;
    use chrono::NaiveDate;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn finds_gaps_both_ways() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".obsidian")).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        let spec = NewProject {
            title: "Other Story",
            form: Form::ShortStory,
            target: None,
            structure: &FREEFORM,
            premise: None,
        };
        let project = create_project(&vault, &spec, today).unwrap();
        for (kind, name) in [
            (NoteKind::Character, "Ada Okafor"),
            (NoteKind::Character, "Old Tom"),
            (NoteKind::Location, "The Array"),
            (NoteKind::World, "Orbital Mechanics"),
        ] {
            add_note(&vault, &project, kind, name, today).unwrap();
        }
        let ada = project.folder(NoteKind::Character).join("Ada Okafor.md");
        let text = fs::read_to_string(&ada)
            .unwrap()
            .replace("aliases: []", "aliases: [Ada]");
        fs::write(&ada, text).unwrap();
        let chapter = add_note(&vault, &project, NoteKind::Chapter, "Signal", today)
            .unwrap()
            .path;
        fs::write(&chapter, "---\nstatus: draft\nsynopsis: Ada hears it.\n---\nAda walked to [[The Array]] to meet [[Captain Reyes]].\n").unwrap();
        add_note(&vault, &project, NoteKind::Chapter, "Empty", today).unwrap();

        let report = check(&vault, &project).unwrap();
        assert_eq!(
            report.undocumented.keys().collect::<Vec<_>>(),
            ["Captain Reyes"]
        );
        assert_eq!(
            report.not_on_page,
            [
                (NoteKind::Character, "Old Tom".to_string()),
                (NoteKind::World, "Orbital Mechanics".to_string())
            ]
        );
        assert_eq!(report.without_synopsis, ["02 Empty"]);
        assert!(report.without_status.is_empty());
        assert_eq!(report.empty_chapters, ["02 Empty"]);
        assert!(!report.is_clean());

        let pov = add_note(&vault, &project, NoteKind::Chapter, "Tom's View", today)
            .unwrap()
            .path;
        fs::write(&pov, "---\npov: \"[[Old Tom]]\"\n---\nThe array hummed.\n").unwrap();
        let report = check(&vault, &project).unwrap();
        assert_eq!(
            report.not_on_page,
            [(NoteKind::World, "Orbital Mechanics".to_string())],
            "a POV character is on the page"
        );
    }
}
