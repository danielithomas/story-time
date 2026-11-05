//! World-bible checks: gaps between what a project's chapters mention and what its notes document,
//! links Obsidian can't follow, and file names that break the naming rules.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::links::{LinkIndex, Resolution, links};
use crate::note::{Note, PropertyValue};
use crate::project::{NoteKind, Project};
use crate::text::{contains_word, is_slug, slug_matches, slugify, strip_leading_number};
use crate::vault::Vault;
use crate::words::prose;

/// The world bible: every kind of note except chapters.
pub const BIBLE: [NoteKind; 3] = [NoteKind::Character, NoteKind::Location, NoteKind::World];

/// Link targets, and the notes (by file name) that link to them.
pub type LinkSources = BTreeMap<String, BTreeSet<String>>;

#[derive(Debug, Default)]
pub struct Report {
    /// Links to no note or file at all.
    pub undocumented: LinkSources,
    /// Links that can mean more than one note. Several notes in the vault share the file name, so
    /// Obsidian may open the wrong one.
    pub ambiguous_links: LinkSources,
    /// Links that name a note by an alias rather than its file name. story-time follows them;
    /// Obsidian doesn't.
    pub alias_links: LinkSources,
    /// Bible notes whose name, title and aliases appear in no chapter. Only checked once the
    /// chapters have words in them.
    pub not_on_page: Vec<(NoteKind, String)>,
    /// Files and folders whose names aren't kebab-case, with the name each should have.
    pub misnamed: Vec<(PathBuf, String)>,
    /// Notes whose file name no longer matches the name in their properties, with the file name
    /// (including `.md`) each should have.
    pub drifted: Vec<(PathBuf, String)>,
    pub without_status: Vec<String>,
    pub without_synopsis: Vec<String>,
    pub empty_chapters: Vec<String>,
    pub unreadable_frontmatter: Vec<(PathBuf, String)>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.undocumented.is_empty()
            && self.ambiguous_links.is_empty()
            && self.alias_links.is_empty()
            && self.not_on_page.is_empty()
            && self.misnamed.is_empty()
            && self.drifted.is_empty()
            && self.without_status.is_empty()
            && self.without_synopsis.is_empty()
            && self.empty_chapters.is_empty()
            && self.unreadable_frontmatter.is_empty()
    }
}

pub fn check(vault: &Vault, project: &Project) -> Result<Report> {
    let index = LinkIndex::build(vault)?;
    let books = project.books()?;
    let mut chapters = Vec::new();
    let mut planning = Vec::new();
    for book in &books {
        chapters.extend(book.chapters()?);
        planning.push(book.note.clone());
        let flow = book.flow_path();
        if flow.is_file() {
            planning.push(Note::load(&flow)?);
        }
    }
    let mut bible = Vec::new();
    for kind in BIBLE {
        bible.extend(project.notes(kind)?.into_iter().map(|note| (kind, note)));
    }

    let mut report = Report::default();
    let notes = chapters
        .iter()
        .map(|c| &c.note)
        .chain([&project.note])
        .chain(planning.iter())
        .chain(bible.iter().map(|(_, note)| note));
    for note in notes {
        if let Some(error) = &note.frontmatter_error {
            report
                .unreadable_frontmatter
                .push((note.path.clone(), error.clone()));
        }
        for link in note_links(note) {
            let list = match index.resolve(&note.path, &link.target) {
                Resolution::Missing => &mut report.undocumented,
                Resolution::Ambiguous(_) => &mut report.ambiguous_links,
                Resolution::Found(_) if !link.is_bare() || has_other_extension(&link.target) => {
                    continue;
                }
                Resolution::Found(_) => {
                    let name = link.target.strip_suffix(".md").unwrap_or(&link.target);
                    match index.notes_named(name) {
                        0 => &mut report.alias_links,
                        1 => continue,
                        _ => &mut report.ambiguous_links,
                    }
                }
            };
            list.entry(link.target)
                .or_default()
                .insert(note.name.clone());
        }
    }

    if chapters.iter().any(|chapter| chapter.words > 0) {
        let page = chapters
            .iter()
            .map(|chapter| prose(&chapter.note.body))
            .collect::<Vec<_>>()
            .join("\n");
        // Linked from a chapter, or the chapter's point of view: either way, on the page.
        let linked: HashSet<PathBuf> = chapters
            .iter()
            .flat_map(|c| {
                note_links(&c.note)
                    .into_iter()
                    .map(|link| index.resolve(&c.note.path, &link.target))
            })
            .filter_map(|resolution| match resolution {
                Resolution::Found(path) => Some(path),
                _ => None,
            })
            .collect();
        for (kind, note) in &bible {
            let mut names = vec![note.title()];
            names.extend(note.aliases());
            if !linked.contains(&note.path) && !names.iter().any(|name| contains_word(&page, name))
            {
                report.not_on_page.push((*kind, note.name.clone()));
            }
        }
    }

    report.misnamed = misnamed(&project.dir)?;
    let all_notes = chapters
        .iter()
        .map(|c| (Some(NoteKind::Chapter), &c.note))
        .chain([(None, &project.note)])
        .chain(books.iter().map(|book| (None, &book.note)))
        .chain(bible.iter().map(|(kind, note)| (Some(*kind), note)));
    for (kind, note) in all_notes {
        if let Some(name) = drifted(note, kind, project) {
            report.drifted.push((note.path.clone(), name));
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

/// Links in a note's body and in its properties, such as a chapter's `pov`.
fn note_links(note: &Note) -> Vec<crate::links::Link> {
    let mut found = links(&note.body);
    for (_, value) in note.frontmatter.entries() {
        let texts = match value {
            PropertyValue::Text(text) => vec![text],
            PropertyValue::List(items) => items,
        };
        for text in texts {
            found.extend(links(&text));
        }
    }
    found
}

fn has_other_extension(target: &str) -> bool {
    Path::new(target)
        .extension()
        .is_some_and(|ext| !ext.eq_ignore_ascii_case("md"))
}

/// Files and folders under `dir`, and `dir` itself, whose names aren't kebab-case.
fn misnamed(dir: &Path) -> Result<Vec<(PathBuf, String)>> {
    let mut found = Vec::new();
    if let Some(name) = suggested_name(dir) {
        found.push((dir.to_path_buf(), name));
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(found),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.sort();
    for path in paths {
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if path.is_dir() {
            found.extend(misnamed(&path)?);
        } else if let Some(name) = suggested_name(&path) {
            found.push((path, name));
        }
    }
    Ok(found)
}

/// The kebab-case name for a file or folder whose name isn't, keeping a file's extension.
fn suggested_name(path: &Path) -> Option<String> {
    let (stem, extension) = if path.is_dir() {
        (path.file_name()?, None)
    } else {
        (path.file_stem()?, path.extension())
    };
    let stem = stem.to_string_lossy();
    let extension = extension.map(|ext| ext.to_string_lossy().into_owned());
    let lowercase_extension = extension
        .as_ref()
        .is_none_or(|ext| *ext == ext.to_lowercase());
    if is_slug(&stem) && lowercase_extension {
        return None;
    }
    let slug = slugify(&stem);
    (!slug.is_empty()).then(|| match extension {
        Some(ext) => format!("{slug}.{}", ext.to_lowercase()),
        None => slug,
    })
}

/// The file name (with `.md`) a kebab-case note should have, if its name has changed since the
/// file was named. Chapters keep their number, and a project may carry the `-world` suffix.
fn drifted(note: &Note, kind: Option<NoteKind>, project: &Project) -> Option<String> {
    if !is_slug(&note.name) {
        return None;
    }
    let slug = slugify(&note.given_name()?);
    if slug.is_empty() {
        return None;
    }
    let (prefix, stem) = match kind {
        Some(NoteKind::Chapter) => {
            let rest = strip_leading_number(&note.name);
            (&note.name[..note.name.len() - rest.len()], rest)
        }
        _ => ("", note.name.as_str()),
    };
    let is_project = kind.is_none() && note.path == project.note.path;
    let matches =
        slug_matches(stem, &slug) || (is_project && slug_matches(stem, &format!("{slug}-world")));
    (!matches).then(|| format!("{prefix}{slug}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Form;
    use crate::scaffold::{NewBook, NewProject, add_note, create_project};
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
            world: None,
            book: NewBook {
                title: "Other Story",
                form: Form::ShortStory,
                target: None,
                structure: &FREEFORM,
                premise: None,
            },
        };
        let (project, book) = create_project(&vault, &spec, today).unwrap();
        for (kind, name) in [
            (NoteKind::Character, "Ada Okafor"),
            (NoteKind::Character, "Old Tom"),
            (NoteKind::Location, "The Array"),
            (NoteKind::World, "Orbital Mechanics"),
        ] {
            add_note(&vault, &project, None, kind, name, today).unwrap();
        }
        let ada = project.folder(NoteKind::Character).join("ada-okafor.md");
        let text = fs::read_to_string(&ada)
            .unwrap()
            .replace("  - \"Ada Okafor\"\n", "  - \"Ada Okafor\"\n  - Ada\n");
        fs::write(&ada, text).unwrap();
        let chapter = add_note(
            &vault,
            &project,
            Some(&book),
            NoteKind::Chapter,
            "Signal",
            today,
        )
        .unwrap()
        .path;
        fs::write(&chapter, "---\nstatus: draft\nsynopsis: Ada hears it.\n---\nAda walked to [[the-array|The Array]] to meet [[Captain Reyes]] at [[Orbital Mechanics]].\n").unwrap();
        add_note(
            &vault,
            &project,
            Some(&book),
            NoteKind::Chapter,
            "Empty",
            today,
        )
        .unwrap();

        let report = check(&vault, &project).unwrap();
        assert_eq!(
            report.undocumented.keys().collect::<Vec<_>>(),
            ["Captain Reyes"]
        );
        assert_eq!(
            report.alias_links.keys().collect::<Vec<_>>(),
            ["Orbital Mechanics"],
            "story-time follows an alias; Obsidian needs the file name"
        );
        assert_eq!(
            report.not_on_page,
            [(NoteKind::Character, "old-tom".to_string())]
        );
        assert_eq!(report.without_synopsis, ["02-empty"]);
        assert!(report.without_status.is_empty());
        assert_eq!(report.empty_chapters, ["02-empty"]);
        assert!(report.misnamed.is_empty() && report.drifted.is_empty());
        assert!(!report.is_clean());

        let pov = add_note(
            &vault,
            &project,
            Some(&book),
            NoteKind::Chapter,
            "Tom's View",
            today,
        )
        .unwrap()
        .path;
        fs::write(
            &pov,
            "---\npov: \"[[old-tom|Old Tom]]\"\n---\nThe array hummed.\n",
        )
        .unwrap();
        let report = check(&vault, &project).unwrap();
        assert!(
            report.not_on_page.is_empty(),
            "a POV character is on the page"
        );
    }

    #[test]
    fn reports_names_and_links_obsidian_cannot_follow() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".obsidian")).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        let spec = |title| NewProject {
            world: None,
            book: NewBook {
                title,
                form: Form::ShortStory,
                target: None,
                structure: &FREEFORM,
                premise: None,
            },
        };
        let (project, book) = create_project(&vault, &spec("Sample Story"), today).unwrap();
        let (other, _) = create_project(&vault, &spec("Other Story"), today).unwrap();
        for project in [&project, &other] {
            add_note(
                &vault,
                project,
                None,
                NoteKind::Character,
                "Ada Okafor",
                today,
            )
            .unwrap();
        }
        let chapter = add_note(
            &vault,
            &project,
            Some(&book),
            NoteKind::Chapter,
            "Signal",
            today,
        )
        .unwrap()
        .path;
        fs::write(&chapter, "Ada and [[ada-okafor|her]] sister.\n").unwrap();
        let characters = project.folder(NoteKind::Character);
        fs::write(
            characters.join("Old Tom.md"),
            "---\naliases: [Old Tom]\n---\n",
        )
        .unwrap();
        fs::write(project.folder(NoteKind::World).join("Map.PNG"), "").unwrap();
        let ada = characters.join("ada-okafor.md");
        let text = fs::read_to_string(&ada)
            .unwrap()
            .replace("Ada Okafor", "Ada Mensah");
        fs::write(&ada, text).unwrap();

        let report = check(&vault, &project).unwrap();
        assert_eq!(
            report.ambiguous_links.keys().collect::<Vec<_>>(),
            ["ada-okafor"],
            "story-time knows which Ada; Obsidian sees two"
        );
        assert_eq!(
            report.misnamed,
            [
                (characters.join("Old Tom.md"), "old-tom.md".to_string()),
                (
                    project.folder(NoteKind::World).join("Map.PNG"),
                    "map.png".to_string()
                ),
            ]
        );
        assert_eq!(report.drifted, [(ada, "ada-mensah.md".to_string())]);
    }
}
