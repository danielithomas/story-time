//! Finding and reading notes across a bookshelf, for agents and scripts: every note listed with
//! what it is and where it belongs, notes searched for words, and one note found by its name or
//! path, to read. Nothing here changes the bookshelf.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::note::{Note, split_frontmatter};
use crate::project::{BIBLE_DIR, BOOKS_DIR, NoteKind, Project, RESEARCH_DIR, SCRATCHPAD_DIR};
use crate::scratchpad;
use crate::vault::{Vault, files_under, is_note};

/// What a note is, from where it sits on the bookshelf, and the project and book it belongs to,
/// by their slugs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// `project` (a project's own note), `book`, `flow`, `scene`, `character`, `location`,
    /// `world`, `timeline`, `research`, `scrap`, or `note` for the ideas file and anything else.
    pub kind: &'static str,
    pub project: Option<String>,
    pub book: Option<String>,
}

/// What the note at `path` is, and where it belongs.
pub fn place(vault: &Vault, path: &Path) -> Placed {
    let placed = |kind| Placed {
        kind,
        project: None,
        book: None,
    };
    if path == vault.ideas_path() {
        return placed("note");
    }
    if path.starts_with(vault.scratchpad_dir()) {
        return placed("scrap");
    }
    if path.starts_with(vault.research_dir()) {
        return placed("research");
    }
    let parts: Vec<String> = vault
        .relative(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    let kind = match parts.as_slice() {
        [slug, file] if *file == format!("{slug}.md") => "project",
        [slug, bible, file] if bible == BIBLE_DIR && *file == format!("{slug}-timeline.md") => {
            "timeline"
        }
        [_, folder, _, ..] if folder == SCRATCHPAD_DIR => "scrap",
        [_, folder, _, ..] if folder == RESEARCH_DIR => "research",
        [_, books, book, file] if books == BOOKS_DIR && *file == format!("{book}.md") => "book",
        [_, books, book, file] if books == BOOKS_DIR && *file == format!("{book}-flow.md") => {
            "flow"
        }
        [_, books, _, folder, _, ..]
            if books == BOOKS_DIR && folder == NoteKind::Scene.folder() =>
        {
            "scene"
        }
        [_, bible, folder, _, ..] if bible == BIBLE_DIR => NoteKind::ALL
            .into_iter()
            .find(|kind| kind.is_bible() && kind.folder() == folder.as_str())
            .map_or("note", NoteKind::key),
        _ => "note",
    };
    Placed {
        kind,
        project: (parts.len() > 1).then(|| parts[0].clone()),
        book: match parts.as_slice() {
            [_, books, book, _, ..] if books == BOOKS_DIR => Some(book.clone()),
            _ => None,
        },
    }
}

/// A note on the bookshelf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub path: PathBuf,
    /// Its name: its title or first alias, else its first heading or file name; for a scrap
    /// with none of those, its first few words.
    pub name: String,
    /// The other names it lists in `aliases`.
    pub aliases: Vec<String>,
    pub placed: Placed,
}

/// Every note on the bookshelf, or in one project, in the order of their paths. Templates, and
/// anything in a hidden folder, aren't notes.
pub fn notes(vault: &Vault, project: Option<&Project>) -> Result<Vec<Listed>> {
    Ok(read_notes(vault, project)?
        .into_iter()
        .map(|(listed, _)| listed)
        .collect())
}

/// Every note with its text, as [`notes`] lists them.
fn read_notes(vault: &Vault, project: Option<&Project>) -> Result<Vec<(Listed, String)>> {
    let dir = project.map_or(vault.root.as_path(), |project| project.dir.as_path());
    let templates = vault.templates_dir();
    let mut found = Vec::new();
    for path in files_under(dir)? {
        if !is_note(&path) || path.starts_with(&templates) {
            continue;
        }
        found.push(read(vault, &path)?);
    }
    Ok(found)
}

/// The note at `path`, as [`notes`] lists it, with its text.
pub fn read(vault: &Vault, path: &Path) -> Result<(Listed, String)> {
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let note = Note::parse(path, &content);
    let placed = place(vault, path);
    let name = if placed.kind == "scrap" {
        scratchpad::label(&note)
    } else {
        note.title()
    };
    let listed = Listed {
        aliases: note.aliases(),
        path: path.to_path_buf(),
        name,
        placed,
    };
    Ok((listed, content))
}

/// A note that has every word searched for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub note: Listed,
    /// How many times the words appear in it, all told.
    pub matches: usize,
    /// The first few lines they appear in, each shortened around the first word found: lines of
    /// its text first, then of its properties.
    pub excerpts: Vec<String>,
}

/// The lines of a note shown for a search.
const EXCERPTS: usize = 3;
/// How long an excerpt can be, in characters, and how much of it comes before the word found.
const EXCERPT_WIDTH: usize = 160;
const EXCERPT_LEAD: usize = 60;

/// The notes on the bookshelf, or in one project, that have every word of `query` (in any case;
/// a "quoted phrase" counts as one), in their text or properties. Notes whose name has one of
/// the words come first, then those where they appear most.
pub fn search(vault: &Vault, query: &str, project: Option<&Project>) -> Result<Vec<Found>> {
    let terms = terms(query);
    if terms.is_empty() {
        bail!("there is nothing to search for");
    }
    let mut found: Vec<(bool, Found)> = Vec::new();
    for (note, content) in read_notes(vault, project)? {
        let text = fold(&content);
        if !terms.iter().all(|term| text.contains(term.as_str())) {
            continue;
        }
        let matches = terms
            .iter()
            .map(|term| text.matches(term.as_str()).count())
            .sum();
        let names = std::iter::once(&note.name).chain(&note.aliases);
        let named = names
            .map(|name| fold(name))
            .any(|name| terms.iter().any(|term| name.contains(term.as_str())));
        // The text before the properties, leaving out a line that only gives the note's name.
        let (properties, body) = split_frontmatter(&content);
        let name = fold(&note.name);
        let excerpts = body
            .lines()
            .filter(|line| fold(line.trim_start_matches('#').trim()) != name)
            .chain(properties.unwrap_or_default().lines())
            .filter_map(|line| excerpt(line, &terms))
            .take(EXCERPTS)
            .collect();
        found.push((
            named,
            Found {
                note,
                matches,
                excerpts,
            },
        ));
    }
    found.sort_by(|(a_named, a), (b_named, b)| {
        b_named
            .cmp(a_named)
            .then(b.matches.cmp(&a.matches))
            .then_with(|| a.note.path.cmp(&b.note.path))
    });
    Ok(found.into_iter().map(|(_, found)| found).collect())
}

/// Text in lower case, a character for a character, so positions in it match the original's.
fn fold(text: &str) -> String {
    text.chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

/// The words of a search, in lower case: each "quoted phrase" as one.
fn terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for (i, part) in query.split('"').enumerate() {
        let quoted = i % 2 == 1;
        let pieces: Vec<&str> = if quoted {
            vec![part.trim()]
        } else {
            part.split_whitespace().collect()
        };
        for piece in pieces {
            let term = fold(piece);
            if !term.is_empty() && !terms.contains(&term) {
                terms.push(term);
            }
        }
    }
    terms
}

/// A line that has one of the terms, trimmed, and shortened around the first one if it's long.
fn excerpt(line: &str, terms: &[String]) -> Option<String> {
    let line = line.trim();
    let folded = fold(line);
    let at = terms
        .iter()
        .filter_map(|term| folded.find(term.as_str()))
        .min()?;
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= EXCERPT_WIDTH {
        return Some(line.to_string());
    }
    let at = folded[..at].chars().count();
    let start = at.saturating_sub(EXCERPT_LEAD);
    let end = (start + EXCERPT_WIDTH).min(chars.len());
    let start = end.saturating_sub(EXCERPT_WIDTH).min(start);
    let text: String = chars[start..end].iter().collect();
    let before = if start > 0 { "…" } else { "" };
    let after = if end < chars.len() { "…" } else { "" };
    Some(format!("{before}{}{after}", text.trim()))
}

/// The note `query` means: a path to it, from `cwd` or the bookshelf, or the name, another name
/// or the file name of one note. Only notes that [`notes`] lists can be found, so nothing outside
/// the bookshelf, in a hidden folder or among the templates.
pub fn find(vault: &Vault, cwd: &Path, query: &str) -> Result<PathBuf> {
    let listed = notes(vault, None)?;
    for base in [cwd, vault.root.as_path()] {
        let path = base.join(query);
        // A path to a file elsewhere is answered as any other name that isn't a note's, so
        // nothing is said about what lies outside the bookshelf.
        if let Ok(path) = path.canonicalize()
            && listed.iter().any(|note| note.path == path)
        {
            return Ok(path);
        }
    }
    let wanted = fold(query.trim().trim_end_matches(".md"));
    let found: Vec<&Listed> = listed
        .iter()
        .filter(|note| {
            let stem = note.path.file_stem().unwrap_or_default().to_string_lossy();
            fold(&stem) == wanted
                || fold(&note.name) == wanted
                || note.aliases.iter().any(|alias| fold(alias) == wanted)
        })
        .collect();
    match found.as_slice() {
        [] => bail!("no note on this bookshelf is called {}", query.trim()),
        [note] => Ok(note.path.clone()),
        _ => {
            let paths: Vec<String> = found
                .iter()
                .map(|note| vault.relative(&note.path).display().to_string())
                .collect();
            bail!(
                "several notes are called {}; give the path of the one you mean: {}",
                query.trim(),
                paths.join(", ")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::example;
    use chrono::NaiveDate;
    use tempfile::TempDir;

    fn shelf() -> (TempDir, Vault) {
        let dir = TempDir::new().unwrap();
        crate::scaffold::init_vault(dir.path(), false).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let now = NaiveDate::from_ymd_opt(2026, 11, 16)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        example::add_example(&vault, now).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        (dir, vault)
    }

    #[test]
    fn lists_every_note_with_what_it_is() {
        let (_dir, vault) = shelf();
        let now = NaiveDate::from_ymd_opt(2026, 11, 16)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap();
        let scrap =
            crate::scaffold::add_scrap(&vault, None, "A loose thought.", Some("dictation"), now)
                .unwrap();
        let listed = notes(&vault, None).unwrap();
        let loose = listed.iter().find(|note| note.path == scrap.path).unwrap();
        assert_eq!(
            loose.placed,
            Placed {
                kind: "scrap",
                project: None,
                book: None
            }
        );
        assert_eq!(loose.name, "A loose thought.");
        let kind_of = |name: &str| {
            listed
                .iter()
                .find(|note| note.name == name)
                .map(|note| note.placed.kind)
        };
        assert_eq!(kind_of("Scenes and Chapters"), Some("scene"));
        assert_eq!(kind_of("The Writer"), Some("character"));
        assert!(
            listed
                .iter()
                .all(|note| !note.path.starts_with(vault.templates_dir())),
            "templates aren't notes"
        );
        let scene = listed
            .iter()
            .find(|note| note.name == "Scenes and Chapters")
            .unwrap();
        assert!(scene.placed.project.is_some() && scene.placed.book.is_some());
        let project = vault.projects().unwrap().remove(0);
        let theirs = notes(&vault, Some(&project)).unwrap();
        assert!(!theirs.is_empty() && theirs.len() < listed.len());
        assert!(
            theirs
                .iter()
                .all(|note| note.path.starts_with(&project.dir))
        );
    }

    #[test]
    fn finds_notes_with_every_word() {
        let (_dir, vault) = shelf();
        let found = search(&vault, "chapters CONTENTS", None).unwrap();
        assert!(!found.is_empty());
        assert_eq!(
            found[0].note.name, "Scenes and Chapters",
            "a note named by a word comes first"
        );
        for hit in &found {
            let text = fs::read_to_string(&hit.note.path).unwrap().to_lowercase();
            assert!(text.contains("chapters") && text.contains("contents"));
            assert!(!hit.excerpts.is_empty() && hit.excerpts.len() <= EXCERPTS);
        }
        let chapters = &found[0];
        assert!(
            chapters
                .excerpts
                .iter()
                .all(|line| !line.starts_with("type:") && line != "# Scenes and Chapters"),
            "the text comes before the properties, and not the name: {:?}",
            chapters.excerpts
        );
        let by_property = search(&vault, "\"Day 1, morning\"", None).unwrap();
        let scene = by_property
            .iter()
            .find(|hit| hit.note.name == "Scenes and Chapters")
            .unwrap();
        assert!(
            scene.excerpts[0].starts_with("when:"),
            "a property is shown when only it has the words: {:?}",
            scene.excerpts
        );
        assert!(search(&vault, "zebra contents", None).unwrap().is_empty());
        assert!(search(&vault, "  ", None).is_err());
    }

    #[test]
    fn a_quoted_phrase_is_one_term() {
        assert_eq!(terms(r#"Mara "the Landing" mara"#), ["mara", "the landing"]);
        let (_dir, vault) = shelf();
        let phrase = search(&vault, "\"no files of their own\"", None).unwrap();
        assert_eq!(phrase.len(), 1);
        assert_eq!(phrase[0].note.name, "Scenes and Chapters");
    }

    #[test]
    fn shortens_long_lines_around_the_word() {
        let line = format!("{} needle {}", "a".repeat(300), "b".repeat(300));
        let shown = excerpt(&line, &["needle".to_string()]).unwrap();
        assert!(shown.starts_with('…') && shown.ends_with('…'));
        assert!(shown.contains("needle"));
        assert!(shown.chars().count() <= EXCERPT_WIDTH + 2);
        assert_eq!(
            excerpt("Short line.", &["line".to_string()]).unwrap(),
            "Short line."
        );
        assert_eq!(excerpt("Nothing here.", &["needle".to_string()]), None);
    }

    #[test]
    fn finds_a_note_by_name_or_path_but_only_on_the_bookshelf() {
        let (dir, vault) = shelf();
        let by_name = find(&vault, dir.path(), "scenes and chapters").unwrap();
        assert!(by_name.ends_with("scenes-and-chapters.md"));
        let relative = vault.relative(&by_name).to_path_buf();
        assert_eq!(
            find(&vault, Path::new("/"), &relative.to_string_lossy()).unwrap(),
            by_name
        );
        assert_eq!(
            find(&vault, dir.path(), "scenes-and-chapters").unwrap(),
            by_name
        );
        assert!(find(&vault, dir.path(), "no such note").is_err());
        let outside = TempDir::new().unwrap();
        fs::write(outside.path().join("secret.md"), "# Secret\n").unwrap();
        let escape = format!(
            "../{}/secret.md",
            outside.path().file_name().unwrap().to_string_lossy()
        );
        let refused = find(&vault, dir.path(), &escape).unwrap_err().to_string();
        assert!(
            refused.starts_with("no note on this bookshelf is called"),
            "{refused}"
        );
        assert!(
            find(
                &vault,
                dir.path(),
                &outside.path().join("secret.md").to_string_lossy()
            )
            .is_err()
        );
        assert!(find(&vault, dir.path(), "templates/scene.md").is_err());
    }
}
