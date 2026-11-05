//! Creating things: a vault's setup, new projects, and new notes within a project.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{NaiveDate, NaiveDateTime};

use crate::config::{CONFIG_FILE, Config, DEFAULT_CONFIG, META_DIR};
use crate::history::History;
use crate::ideas;
use crate::ids;
use crate::links::LinkIndex;
use crate::note::{Note, PropertyValue, section, set_properties, set_section};
use crate::project::{BIBLE_DIR, BOOKS_HEADING, Book, Form, NoteKind, Project, RESERVED};
use crate::structures::{self, Structure};
use crate::templates::{self, render};
use crate::text::{slugify, unique_slug};
use crate::vault::{Vault, markdown_files};

/// The line `init` adds to the vault's `.gitignore`: compiled manuscripts are rebuilt on demand.
pub const BUILD_IGNORE: &str = ".story-time/build/";

/// The line `init` adds to the vault's `.gitattributes`: when two machines both add to the
/// writing log, git keeps both sets of lines instead of reporting a conflict.
pub const LOG_MERGE: &str = ".story-time/history.csv merge=union";

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Created(PathBuf),
    Updated(PathBuf),
    Kept(PathBuf),
}

/// Sets up a vault: config, note templates, the folder for Obsidian's Templates plugin and, in a
/// git repository, `.gitignore` and `.gitattributes` entries. Existing files are kept unless
/// `overwrite_templates` is set, which replaces only the templates. A vault in a newer format is
/// left untouched.
pub fn init_vault(root: &Path, overwrite_templates: bool) -> Result<Vec<Step>> {
    let mut steps = Vec::new();
    let config_path = root.join(META_DIR).join(CONFIG_FILE);
    steps.push(write_if_missing(&config_path, DEFAULT_CONFIG)?);
    let config = Config::load(root)?;
    config.ensure_supported()?;

    let templates_dir = root.join(&config.templates_dir);
    for template in templates::ALL {
        let path = templates_dir.join(template.file_name);
        if overwrite_templates && path.exists() {
            fs::write(&path, template.default)
                .with_context(|| format!("writing {}", path.display()))?;
            steps.push(Step::Updated(path));
        } else {
            steps.push(write_if_missing(&path, template.default)?);
        }
    }

    if root.join(".obsidian").is_dir() {
        let settings = format!(
            "{{\n  \"folder\": \"{}\"\n}}\n",
            config.templates_dir.replace('"', "\\\"")
        );
        steps.push(write_if_missing(
            &root.join(".obsidian").join("templates.json"),
            &settings,
        )?);
    }

    if root.join(".git").exists() {
        steps.push(add_line(&root.join(".gitignore"), BUILD_IGNORE)?);
        steps.push(add_line(&root.join(".gitattributes"), LOG_MERGE)?);
    }
    Ok(steps)
}

/// Adds `line` to the end of a file unless the file already has it.
fn add_line(path: &Path, line: &str) -> Result<Step> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    if existing.lines().any(|existing| existing.trim() == line) {
        return Ok(Step::Kept(path.to_path_buf()));
    }
    let separator = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    fs::write(path, format!("{existing}{separator}{line}\n"))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(if existing.is_empty() {
        Step::Created(path.to_path_buf())
    } else {
        Step::Updated(path.to_path_buf())
    })
}

fn write_if_missing(path: &Path, content: &str) -> Result<Step> {
    if path.exists() {
        return Ok(Step::Kept(path.to_path_buf()));
    }
    write_new(path, content)?;
    Ok(Step::Created(path.to_path_buf()))
}

/// Writes a file that must not already exist.
fn write_new(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("writing {}", path.display()))
}

/// A book to create: the first of a new project, or another in an existing one.
#[derive(Clone, Copy)]
pub struct NewBook<'a> {
    pub title: &'a str,
    pub form: Form,
    /// Defaults to a typical length for the form.
    pub target: Option<u64>,
    pub structure: &'static Structure,
    pub premise: Option<&'a str>,
}

/// A new project: a world, and the first book set in it.
#[derive(Clone, Copy)]
pub struct NewProject<'a> {
    /// The world's name. Defaults to the book's title.
    pub world: Option<&'a str>,
    pub book: NewBook<'a>,
}

/// Creates a project: `<slug>/` with the project note and the world bible's folders, and its
/// first book in `books/`. The project is named after the world; when that would give it the same
/// name as its book, the project gets a `-world` suffix (`sample-story-world/`).
pub fn create_project(
    vault: &Vault,
    spec: &NewProject,
    today: NaiveDate,
) -> Result<(Project, Book)> {
    let book_slug = slugify(spec.book.title);
    let world = spec
        .world
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .unwrap_or(spec.book.title);
    let mut slug = slugify(world);
    if slug.is_empty() || book_slug.is_empty() {
        bail!("the title needs at least one letter or number");
    }
    if slug == book_slug {
        slug.push_str("-world");
    }
    if RESERVED.contains(&slug.as_str()) {
        bail!("{slug} is a reserved folder name; choose another name for the world");
    }
    let dir = vault.root.join(&slug);
    if dir.exists() {
        bail!("{} already exists", vault.relative(&dir).display());
    }
    for kind in NoteKind::ALL.into_iter().filter(|kind| kind.is_bible()) {
        let folder = dir.join(BIBLE_DIR).join(kind.folder());
        fs::create_dir_all(&folder).with_context(|| format!("creating {}", folder.display()))?;
    }
    let mut taken = taken_ids(vault)?;
    let id = ids::new_id(&taken)?;
    taken.insert(id.clone());
    let date = today.format("%Y-%m-%d").to_string();
    let vars = [
        ("id", id.as_str()),
        ("title", world),
        ("date", date.as_str()),
    ];
    let note = render(&templates::PROJECT.load(vault)?, &vars);
    write_new(
        &dir.join(format!("{slug}.md")),
        &set_properties(&note, &[("id", PropertyValue::Text(id))]),
    )?;
    let project = Project::load(&dir)?.context("the project note was not created")?;
    let book = create_book(vault, &project, &spec.book, &taken, today)?;
    let project = Project::load(&dir)?.context("the project note disappeared")?;
    Ok((project, book))
}

/// Adds another book to a project, after the others in its `## Books` list.
pub fn add_book(
    vault: &Vault,
    project: &Project,
    spec: &NewBook,
    today: NaiveDate,
) -> Result<Book> {
    create_book(vault, project, spec, &taken_ids(vault)?, today)
}

/// Creates `books/<slug>/` with the book note, its story flow and `chapters/`, and adds the book
/// to the project note's `## Books` list.
fn create_book(
    vault: &Vault,
    project: &Project,
    spec: &NewBook,
    taken: &HashSet<String>,
    today: NaiveDate,
) -> Result<Book> {
    let slug = slugify(spec.title);
    if slug.is_empty() {
        bail!("the title needs at least one letter or number");
    }
    let stems = project_stems(project, None)?;
    let slug = unique_slug(&slug, |candidate| stems.contains(candidate));
    let dir = project.books_dir().join(&slug);
    fs::create_dir_all(dir.join(NoteKind::Chapter.folder()))
        .with_context(|| format!("creating {}", dir.display()))?;
    let note_path = dir.join(format!("{slug}.md"));
    let flow_path = dir.join(format!("{slug}-flow.md"));
    let index = LinkIndex::build(vault)?;
    let flow_link = index.link(&note_path, &flow_path, "Story flow");
    let id = ids::new_id(taken)?;
    let target = spec
        .target
        .unwrap_or(spec.form.default_target())
        .to_string();
    let date = today.format("%Y-%m-%d").to_string();
    let structure = structures::render(spec.structure, target.parse().ok());
    let project_title = project.title();
    let vars = [
        ("id", id.as_str()),
        ("title", spec.title),
        ("project", project_title.as_str()),
        ("book", spec.title),
        ("slug", slug.as_str()),
        ("form", spec.form.key()),
        ("target", target.as_str()),
        ("date", date.as_str()),
        ("premise", spec.premise.unwrap_or_default()),
        ("flow_link", flow_link.as_str()),
        ("structure", structure.as_str()),
        ("structure_name", spec.structure.key),
    ];
    let note = render(&templates::BOOK.load(vault)?, &vars);
    write_new(
        &note_path,
        &set_properties(&note, &[("id", PropertyValue::Text(id.clone()))]),
    )?;
    write_new(&flow_path, &render(&templates::FLOW.load(vault)?, &vars))?;

    let mut listed: Vec<String> = section(&project.note.body, BOOKS_HEADING)
        .unwrap_or_default()
        .into_iter()
        .map(String::from)
        .filter(|line| !line.trim().is_empty())
        .collect();
    listed.push(format!(
        "- {}",
        index.link(&project.note.path, &note_path, spec.title)
    ));
    let content = fs::read_to_string(&project.note.path)
        .with_context(|| format!("reading {}", project.note.path.display()))?;
    fs::write(
        &project.note.path,
        set_section(&content, BOOKS_HEADING, &listed),
    )
    .with_context(|| format!("writing {}", project.note.path.display()))?;
    Book::load(&dir, &project.slug)?.context("the book note was not created")
}

/// Every project's and book's ID, so a new one can avoid them.
fn taken_ids(vault: &Vault) -> Result<HashSet<String>> {
    let mut taken = HashSet::new();
    for project in vault.projects()? {
        taken.extend(project.id());
        for book in project.books()? {
            taken.extend(book.id());
        }
    }
    Ok(taken)
}

/// Lower-cased file names of the project's notes, which must be unique within it. With `book`,
/// chapters in the project's other books are left out, since chapters in different books may
/// share a name.
fn project_stems(project: &Project, book: Option<&Book>) -> Result<HashSet<String>> {
    let others: Vec<PathBuf> = match book {
        Some(book) => project
            .books()?
            .iter()
            .filter(|other| other.dir != book.dir)
            .map(Book::chapters_dir)
            .collect(),
        None => Vec::new(),
    };
    Ok(markdown_files(&project.dir)?
        .iter()
        .filter(|path| !others.iter().any(|dir| path.starts_with(dir)))
        .filter_map(|path| path.file_stem())
        .map(|stem| stem.to_string_lossy().to_lowercase())
        .collect())
}

/// Starts a project: creates it, then records a zero snapshot for its first book so everything
/// written from now on counts as words written. Given the name of an idea in the ideas file, the
/// idea's premise replaces the book's premise and the idea is linked to the new project; nothing
/// is created if the idea can't be found.
pub fn start_project(
    vault: &Vault,
    spec: &NewProject,
    idea: Option<&str>,
    now: NaiveDateTime,
) -> Result<(Project, Book)> {
    let ideas_path = vault.ideas_path();
    let found = match idea {
        None => None,
        Some(name) => {
            let content = fs::read_to_string(&ideas_path)
                .with_context(|| format!("reading {}", ideas_path.display()))?;
            let idea = ideas::find_idea(&content, name).with_context(|| {
                format!(
                    "no idea named \"{name}\" in {}",
                    vault.relative(&ideas_path).display()
                )
            })?;
            Some((content, idea))
        }
    };
    let spec = NewProject {
        book: NewBook {
            premise: found
                .as_ref()
                .map(|(_, idea)| idea.premise.as_str())
                .or(spec.book.premise),
            ..spec.book
        },
        ..*spec
    };
    let (project, book) = create_project(vault, &spec, now.date())?;
    record_start(vault, &book, now)?;
    if let Some((content, idea)) = &found {
        fs::write(
            &ideas_path,
            ideas::link_idea(content, idea.line, &project.slug),
        )
        .with_context(|| format!("writing {}", ideas_path.display()))?;
    }
    Ok((project, book))
}

/// Adds a book to a project and records a zero snapshot for it.
pub fn start_book(
    vault: &Vault,
    project: &Project,
    spec: &NewBook,
    now: NaiveDateTime,
) -> Result<Book> {
    let book = add_book(vault, project, spec, now.date())?;
    record_start(vault, &book, now)?;
    Ok(book)
}

fn record_start(vault: &Vault, book: &Book, now: NaiveDateTime) -> Result<()> {
    let history_path = vault.history_path();
    History::load(&history_path)?.record(&history_path, now, &[(book.log_key(), 0)])?;
    Ok(())
}

pub struct NewNote {
    pub path: PathBuf,
    /// Another note elsewhere in the vault with the same name, which makes links ambiguous.
    pub same_name: Option<PathBuf>,
}

/// Adds a note from its template: a chapter to `book`, or a world-bible note to the project. The
/// file is named with the slug of `name`, which becomes the note's first alias, and chapters are
/// numbered after the highest existing number. Adding a name the project already has for this
/// kind of note fails; a different name whose slug is already taken in the project gets a
/// numbered slug (`maras-2`).
pub fn add_note(
    vault: &Vault,
    project: &Project,
    book: Option<&Book>,
    kind: NoteKind,
    name: &str,
    today: NaiveDate,
) -> Result<NewNote> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let slug = slugify(&name);
    if slug.is_empty() {
        bail!("the name needs at least one letter or number");
    }
    let (folder, existing, base) = match (kind, book) {
        (NoteKind::Chapter, None) => bail!("a chapter needs a book to go in"),
        (NoteKind::Chapter, Some(book)) => {
            let chapters = book.chapters()?;
            let next = chapters
                .iter()
                .filter_map(|chapter| chapter.number())
                .max()
                .unwrap_or(0)
                + 1;
            let notes = chapters.into_iter().map(|c| c.note).collect();
            (book.chapters_dir(), notes, format!("{next:02}-{slug}"))
        }
        (kind, _) => (project.folder(kind), project.notes(kind)?, slug),
    };
    if existing
        .iter()
        .any(|note| note.title().eq_ignore_ascii_case(&name))
    {
        bail!(
            "{} already has a {} called {name}",
            book.filter(|_| kind == NoteKind::Chapter)
                .map_or_else(|| project.title(), Book::title),
            kind.key()
        );
    }
    let taken = project_stems(project, book.filter(|_| kind == NoteKind::Chapter))?;
    let file_name = unique_slug(&base, |candidate| taken.contains(candidate));
    let path = folder.join(format!("{file_name}.md"));
    let project_title = project.title();
    let book_title = book.map(Book::title).unwrap_or_default();
    let date = today.format("%Y-%m-%d").to_string();
    let vars = [
        ("title", name.as_str()),
        ("project", project_title.as_str()),
        ("book", book_title.as_str()),
        ("date", date.as_str()),
    ];
    let note = render(&templates::for_kind(kind).load(vault)?, &vars);
    write_new(&path, &with_first_alias(&path, &note, &name))?;
    let same_name = vault.note_named(&file_name, &path)?;
    Ok(NewNote { path, same_name })
}

/// A note's text with `name` as its first alias, keeping any other aliases the template gave it.
fn with_first_alias(path: &Path, note: &str, name: &str) -> String {
    let aliases = Note::parse(path, note).aliases();
    if aliases.first().map(String::as_str) == Some(name) {
        return note.to_string();
    }
    let mut list = vec![name.to_string()];
    list.extend(aliases.into_iter().filter(|alias| alias != name));
    set_properties(note, &[("aliases", PropertyValue::List(list))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structures::THREE_ACT;
    use tempfile::TempDir;

    fn vault() -> (TempDir, Vault) {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".obsidian")).unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        (dir, vault)
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 30).unwrap()
    }

    #[test]
    fn init_is_idempotent() {
        let (dir, _) = vault();
        let first = init_vault(dir.path(), false).unwrap();
        assert!(
            first.iter().all(|step| matches!(step, Step::Created(_))),
            "{first:?}"
        );
        assert!(dir.path().join("templates/character.md").is_file());
        assert_eq!(
            fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
            ".story-time/build/\n"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join(".gitattributes")).unwrap(),
            ".story-time/history.csv merge=union\n"
        );
        let second = init_vault(dir.path(), false).unwrap();
        assert!(
            second.iter().all(|step| matches!(step, Step::Kept(_))),
            "{second:?}"
        );
    }

    #[test]
    fn init_leaves_newer_vaults_alone() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(META_DIR)).unwrap();
        fs::write(
            dir.path().join(META_DIR).join(CONFIG_FILE),
            format!("format = {}\n", crate::config::FORMAT + 1),
        )
        .unwrap();
        assert!(init_vault(dir.path(), false).is_err());
        assert!(!dir.path().join("templates").exists());
        assert!(Vault::open(dir.path()).is_err());
    }

    fn book(title: &str) -> NewBook<'_> {
        NewBook {
            title,
            form: Form::Novel,
            target: None,
            structure: &THREE_ACT,
            premise: None,
        }
    }

    fn standalone(title: &str) -> NewProject<'_> {
        NewProject {
            world: None,
            book: book(title),
        }
    }

    #[test]
    fn creates_a_world_with_its_first_book() {
        let (dir, vault) = vault();
        let spec = NewProject {
            world: None,
            book: NewBook {
                premise: Some("A premise, with a catch."),
                ..book("Sample Story")
            },
        };
        let (project, first) = create_project(&vault, &spec, today()).unwrap();
        assert_eq!(
            project.slug, "sample-story-world",
            "the world steps aside for the book"
        );
        assert_eq!(project.title(), "Sample Story");
        assert!(
            dir.path()
                .join("sample-story-world/bible/characters")
                .is_dir()
        );
        assert_eq!(first.slug, "sample-story");
        assert!(first.dir.ends_with("sample-story-world/books/sample-story"));
        assert_eq!(first.title(), "Sample Story");
        assert_eq!(first.target(), Some(90_000));
        assert_eq!(first.form(), Some(Form::Novel));
        assert_eq!(first.status().as_deref(), Some("planning"));
        assert!(first.note.body.contains("A premise, with a catch."));
        assert!(first.note.body.contains("[[sample-story-flow|Story flow]]"));
        assert!(
            fs::read_to_string(first.flow_path())
                .unwrap()
                .contains("around 9,000 words")
        );
        let (project_id, book_id) = (project.id().unwrap(), first.id().unwrap());
        assert_ne!(project_id, book_id);
        assert_eq!(first.log_key(), book_id);
        assert!(create_project(&vault, &spec, today()).is_err());

        let second = add_book(&vault, &project, &book("Second Story"), today()).unwrap();
        let project = Project::load(&project.dir).unwrap().unwrap();
        assert_eq!(
            section(&project.note.body, BOOKS_HEADING).unwrap(),
            [
                "",
                "- [[sample-story|Sample Story]]",
                "- [[second-story|Second Story]]",
                "",
            ]
        );
        let books = project.books().unwrap();
        assert_eq!(
            books.iter().map(|b| b.slug.as_str()).collect::<Vec<_>>(),
            ["sample-story", "second-story"]
        );
        assert_eq!(
            project.find_book(Some("second"), dir.path()).unwrap().dir,
            second.dir
        );
        assert!(project.find_book(None, dir.path()).is_err(), "which book?");
        assert_eq!(
            project.find_book(None, &second.dir).unwrap().slug,
            "second-story"
        );
    }

    #[test]
    fn names_a_world_apart_from_its_book() {
        let (_dir, vault) = vault();
        let spec = NewProject {
            world: Some("The Long Night"),
            book: book("Arrival"),
        };
        let (project, book) = create_project(&vault, &spec, today()).unwrap();
        assert_eq!(project.slug, "the-long-night");
        assert_eq!(project.title(), "The Long Night");
        assert_eq!(book.slug, "arrival");
        let reserved = NewProject {
            world: Some("Research"),
            ..spec
        };
        assert!(create_project(&vault, &reserved, today()).is_err());
    }

    #[test]
    fn adds_chapters_to_books_and_notes_to_the_world_bible() {
        let (_dir, vault) = vault();
        let (project, first) =
            create_project(&vault, &standalone("Sample Story"), today()).unwrap();
        let second = add_book(&vault, &project, &book("Second Story"), today()).unwrap();
        let chapter = |book: &Book, name| {
            add_note(
                &vault,
                &project,
                Some(book),
                NoteKind::Chapter,
                name,
                today(),
            )
            .unwrap()
        };
        let arrival = chapter(&first, "Arrival");
        let leaving = chapter(&first, "Leaving: Home?");
        assert!(
            arrival
                .path
                .ends_with("books/sample-story/chapters/01-arrival.md")
        );
        assert!(
            leaving
                .path
                .ends_with("books/sample-story/chapters/02-leaving-home.md")
        );
        let chapters = first.chapters().unwrap();
        assert_eq!(
            chapters[1].title(),
            "Leaving: Home?",
            "the name keeps its punctuation"
        );
        assert_eq!(chapters[1].status().as_deref(), Some("outline"));
        assert_eq!(
            chapters[1].words, 0,
            "a new chapter holds only its heading and a comment"
        );
        assert!(
            chapter(&second, "Arrival")
                .path
                .ends_with("books/second-story/chapters/01-arrival.md"),
            "chapters in different books may share a name"
        );
        assert!(add_note(&vault, &project, None, NoteKind::Chapter, "Lost", today()).is_err());

        let mara = add_note(
            &vault,
            &project,
            None,
            NoteKind::Character,
            "Mara Chen",
            today(),
        )
        .unwrap();
        assert!(
            mara.path
                .ends_with("sample-story-world/bible/characters/mara-chen.md")
        );
        assert!(mara.same_name.is_none());
        assert!(
            add_note(
                &vault,
                &project,
                None,
                NoteKind::Character,
                "Mara Chen",
                today()
            )
            .is_err()
        );
        let place = add_note(
            &vault,
            &project,
            None,
            NoteKind::Location,
            "Mara Chen",
            today(),
        )
        .unwrap();
        assert!(
            place.path.ends_with("bible/locations/mara-chen-2.md"),
            "file names are unique within the project"
        );
        let note = fs::read_to_string(&mara.path).unwrap();
        assert!(
            !note.contains("project:") && !note.contains("role:"),
            "{note}"
        );
    }

    #[test]
    fn vault_templates_override_built_ins() {
        let (dir, vault) = vault();
        let (project, _) = create_project(&vault, &standalone("Other Story"), today()).unwrap();
        fs::create_dir_all(dir.path().join("templates")).unwrap();
        fs::write(
            dir.path().join("templates/location.md"),
            "---\ntype: location\n---\n# {{title}} in {{project}}\n",
        )
        .unwrap();
        let place = add_note(
            &vault,
            &project,
            None,
            NoteKind::Location,
            "The Lab",
            today(),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(place.path).unwrap(),
            "---\ntype: location\naliases:\n  - The Lab\n---\n# The Lab in Other Story\n",
            "the name is added as the first alias"
        );
    }

    #[test]
    fn ids_and_book_lists_even_from_templates_without_them() {
        let (dir, vault) = vault();
        fs::create_dir_all(dir.path().join("templates")).unwrap();
        for name in ["project", "book"] {
            fs::write(
                dir.path().join(format!("templates/{name}.md")),
                format!("---\ntype: {name}\ntitle: \"{{{{title}}}}\"\n---\n# {{{{title}}}}\n"),
            )
            .unwrap();
        }
        let now = today().and_hms_opt(9, 0, 0).unwrap();
        let (project, book) =
            start_project(&vault, &standalone("Sample Story"), None, now).unwrap();
        assert!(project.id().is_some());
        assert!(
            project
                .note
                .body
                .ends_with("## Books\n\n- [[sample-story|Sample Story]]\n")
        );
        let id = book
            .id()
            .expect("the ID is added to the template's properties");
        let log = fs::read_to_string(vault.history_path()).unwrap();
        assert_eq!(
            log,
            format!("timestamp,id,words\n2026-10-30T09:00:00,{id},0\n")
        );
    }

    #[test]
    fn books_without_ids_are_logged_by_project_and_folder() {
        let (dir, vault) = vault();
        fs::create_dir_all(dir.path().join("old-world/books/old-story")).unwrap();
        fs::write(
            dir.path().join("old-world/old-world.md"),
            "---\ntype: project\ntitle: Old World\n---\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("old-world/books/old-story/old-story.md"),
            "---\ntype: book\ntitle: Old Story\n---\n",
        )
        .unwrap();
        let project = vault.find_project(Some("old-world"), dir.path()).unwrap();
        let book = project.find_book(None, dir.path()).unwrap();
        assert_eq!(book.id(), None);
        assert_eq!(book.log_key(), "old-world/old-story");
    }

    #[test]
    fn only_project_notes_make_projects() {
        let (dir, vault) = vault();
        for (folder, kind) in [("notes", "note"), ("scratchpad", "project")] {
            fs::create_dir_all(dir.path().join(folder)).unwrap();
            fs::write(
                dir.path().join(format!("{folder}/{folder}.md")),
                format!("---\ntype: {kind}\n---\n"),
            )
            .unwrap();
        }
        assert!(vault.projects().unwrap().is_empty());
    }
}
