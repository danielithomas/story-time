//! Creating things: a vault's setup, new projects, and new notes within a project.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{NaiveDate, NaiveDateTime};

use crate::config::{CONFIG_FILE, Config, DEFAULT_CONFIG, META_DIR};
use crate::contents::{self, Place};
use crate::history::History;
use crate::ideas;
use crate::ids;
use crate::links::LinkIndex;
use crate::note::{
    Note, PropertyValue, Saved, content_hash, replace_body, save_section, section, set_properties,
    split_frontmatter,
};
use crate::project::{BIBLE_DIR, BOOKS_HEADING, Book, Form, NoteKind, Project, RESERVED};
use crate::scratchpad;
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
/// `overwrite_templates` is set, which replaces only the templates. A vault in another format is
/// left untouched.
pub fn init_vault(root: &Path, overwrite_templates: bool) -> Result<Vec<Step>> {
    let mut steps = Vec::new();
    let config_path = root.join(META_DIR).join(CONFIG_FILE);
    steps.push(write_if_missing(&config_path, DEFAULT_CONFIG)?);
    let config = Config::load(root)?;
    config.ensure_supported()?;
    config.ensure_current()?;

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
pub(crate) fn add_line(path: &Path, line: &str) -> Result<Step> {
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
pub(crate) fn write_new(path: &Path, content: &str) -> Result<()> {
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

/// Creates a project: `<slug>/` with the project note, the world bible's folders and timeline, and
/// its first book in `books/`. The project is named after the world; when that would give it the
/// same name as its book, the project gets a `-world` suffix (`sample-story-world/`).
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
    let note_path = dir.join(format!("{slug}.md"));
    let timeline_path = dir.join(BIBLE_DIR).join(format!("{slug}-timeline.md"));
    let timeline_link = LinkIndex::build(vault)?.link(&note_path, &timeline_path, "Timeline");
    let vars = [
        ("id", id.as_str()),
        ("title", world),
        ("project", world),
        ("date", date.as_str()),
        ("timeline_link", timeline_link.as_str()),
    ];
    let note = render(&templates::PROJECT.load(vault)?, &vars);
    write_new(
        &note_path,
        &set_properties(&note, &[("id", PropertyValue::Text(id.clone()))]),
    )?;
    write_new(
        &timeline_path,
        &render(&templates::TIMELINE.load(vault)?, &vars),
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

/// Creates `books/<slug>/` with the book note, its story flow and `scenes/`, and adds the book to
/// the project note's `## Books` list.
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
    fs::create_dir_all(dir.join(NoteKind::Scene.folder()))
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

    let content = fs::read_to_string(&project.note.path)
        .with_context(|| format!("reading {}", project.note.path.display()))?;
    let mut listed: Vec<String> = section(split_frontmatter(&content).1, BOOKS_HEADING)
        .unwrap_or_default()
        .into_iter()
        .map(String::from)
        .filter(|line| !line.trim().is_empty())
        .collect();
    listed.push(format!(
        "- {}",
        index.link(&project.note.path, &note_path, spec.title)
    ));
    let saved = save_section(
        &project.note.path,
        BOOKS_HEADING,
        &listed,
        &content_hash(&content),
    )?;
    if let Saved::Conflict { .. } = saved {
        bail!(
            "{} was created, but the project note changed on disk before the book could be \
             listed in it. Add it under ## Books.",
            vault.relative(&dir).display()
        );
    }
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
/// scenes in the project's other books are left out, since scenes in different books may share a
/// name.
fn project_stems(project: &Project, book: Option<&Book>) -> Result<HashSet<String>> {
    let others: Vec<PathBuf> = match book {
        Some(book) => project
            .books()?
            .iter()
            .filter(|other| other.dir != book.dir)
            .map(Book::scenes_dir)
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

/// Starts a project: creates it, then records a zero snapshot for its first book and its
/// scratchpad so everything written from now on counts as words written. Given the name of an idea
/// in the ideas file, the idea's premise replaces the book's premise and the idea is linked to the
/// new project; nothing is created if the idea can't be found.
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
    record_start(vault, &[book.log_key(), project.log_key()], now)?;
    if let Some((content, idea)) = &found {
        let link = LinkIndex::build(vault)?.link(&ideas_path, &project.note.path, &project.title());
        fs::write(&ideas_path, ideas::link_idea(content, idea.line, &link))
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
    record_start(vault, &[book.log_key()], now)?;
    Ok(book)
}

/// Records a zero count for new things to measure, so their first words count as written.
fn record_start(vault: &Vault, keys: &[String], now: NaiveDateTime) -> Result<()> {
    let history_path = vault.history_path();
    let counts: Vec<(String, u64)> = keys.iter().map(|key| (key.clone(), 0)).collect();
    History::load(&history_path)?.record(&history_path, now, &counts)?;
    Ok(())
}

pub struct NewNote {
    pub path: PathBuf,
    /// Another note elsewhere in the vault with the same name, which makes links ambiguous.
    pub same_name: Option<PathBuf>,
}

/// Adds a world-bible note to the project from its template. The file is named with the slug of
/// `name`, which becomes the note's first alias. Adding a name the project already has for this
/// kind of note fails; a different name whose slug is already taken in the project gets a
/// numbered slug (`maras-2`).
pub fn add_note(
    vault: &Vault,
    project: &Project,
    kind: NoteKind,
    name: &str,
    today: NaiveDate,
) -> Result<NewNote> {
    if !kind.is_bible() {
        bail!("a scene needs a book to go in");
    }
    let new = prepare(vault, project, None, kind, name, today)?;
    write_new(&new.path, &new.text)?;
    new.created(vault)
}

/// Adds a scene to `book` from its template, named as [`add_note`] names notes, and lists it in
/// the book's contents at `place`. Scenes in different books may share a name. Only the
/// `## Contents` section of the book note changes, and not if the note changed on disk meanwhile.
pub fn add_scene(
    vault: &Vault,
    project: &Project,
    book: &Book,
    name: &str,
    place: Place,
    today: NaiveDate,
) -> Result<NewNote> {
    let new = prepare(vault, project, Some(book), NoteKind::Scene, name, today)?;
    let content = fs::read_to_string(&book.note.path)
        .with_context(|| format!("reading {}", book.note.path.display()))?;
    let lines = section(split_frontmatter(&content).1, contents::HEADING).unwrap_or_default();
    let link = LinkIndex::build(vault)?.link(&book.note.path, &new.path, &new.name);
    let listed = contents::insert(&lines, place, &link)?;
    write_new(&new.path, &new.text)?;
    if place != Place::Unplaced {
        let saved = save_section(
            &book.note.path,
            contents::HEADING,
            &listed,
            &content_hash(&content),
        )?;
        if let Saved::Conflict { .. } = saved {
            bail!(
                "{} was created, but the book note changed on disk before the scene could be \
                 listed in its contents, so it is unplaced",
                vault.relative(&new.path).display()
            );
        }
    }
    new.created(vault)
}

/// Adds a scrap to a scratchpad: the project's, or with `None` the vault's. It is named by the time
/// of capture (`2026-11-05-2109.md`, or `2026-11-05-2109-2.md` if the vault already has one),
/// takes its properties from the template, with `source` set when given, and holds `text` exactly
/// as given.
pub fn add_scrap(
    vault: &Vault,
    project: Option<&Project>,
    text: &str,
    source: Option<&str>,
    now: NaiveDateTime,
) -> Result<NewNote> {
    let dir = project.map_or_else(|| vault.scratchpad_dir(), Project::scratchpad_dir);
    // Unique across the vault, so a link to a captured note is never ambiguous in Obsidian.
    let taken = stems(&vault.root)?;
    let slug = unique_slug(&now.format("%Y-%m-%d-%H%M").to_string(), |candidate| {
        taken.contains(candidate)
    });
    let path = dir.join(format!("{slug}.md"));
    let project_title = project.map(Project::title).unwrap_or_default();
    let date = now.format("%Y-%m-%d").to_string();
    let vars = [
        ("title", slug.as_str()),
        ("project", project_title.as_str()),
        ("date", date.as_str()),
    ];
    let mut note = render(&templates::SCRAP.load(vault)?, &vars);
    if let Some(source) = source.map(str::trim).filter(|source| !source.is_empty()) {
        note = set_properties(
            &note,
            &[("source", PropertyValue::Text(source.to_string()))],
        );
    }
    if !text.is_empty() {
        let end = if text.ends_with('\n') { "" } else { "\n" };
        note = replace_body(&note, &format!("{text}{end}"));
    }
    write_new(&path, &note)?;
    Prepared {
        name: slug.clone(),
        path,
        slug,
        text: note,
    }
    .created(vault)
}

/// A research note to add.
#[derive(Clone, Copy)]
pub struct NewResearch<'a> {
    /// What it is about: the note's name.
    pub name: &'a str,
    /// Where it came from: a URL or citation.
    pub source: Option<&'a str>,
    /// What it says, in place of the template's prompt.
    pub text: Option<&'a str>,
    /// Added by an agent: the note opens with a callout saying so, so the writer knows to check it
    /// against its source.
    pub by_agent: bool,
}

/// The callout that opens a research note an agent added.
pub const AGENT_CALLOUT: &str =
    "> [!note] Added by an agent\n> Check it against its source before the story relies on it.";

/// Adds a research note: to the project's `research/`, or with `None` the vault's. It is named
/// as [`add_note`] names notes.
pub fn add_research(
    vault: &Vault,
    project: Option<&Project>,
    research: &NewResearch,
    today: NaiveDate,
) -> Result<NewNote> {
    let name = research
        .name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let slug = slugify(&name);
    if slug.is_empty() {
        bail!("the name needs at least one letter or number");
    }
    let dir = project.map_or_else(|| vault.research_dir(), Project::research_dir);
    if scratchpad::research(&dir)?
        .iter()
        .any(|note| note.title().eq_ignore_ascii_case(&name))
    {
        let owner = project.map_or_else(|| "The bookshelf".to_string(), Project::title);
        bail!("{owner} already has research called {name}");
    }
    let taken = match project {
        Some(project) => project_stems(project, None)?,
        None => stems(&dir)?,
    };
    let slug = unique_slug(&slug, |candidate| taken.contains(candidate));
    let path = dir.join(format!("{slug}.md"));
    let project_title = project.map(Project::title).unwrap_or_default();
    let date = today.format("%Y-%m-%d").to_string();
    let vars = [
        ("title", name.as_str()),
        ("project", project_title.as_str()),
        ("date", date.as_str()),
    ];
    let mut note = render(&templates::RESEARCH.load(vault)?, &vars);
    if let Some(text) = research.text.filter(|text| !text.trim().is_empty()) {
        note = replace_body(&note, &format!("# {name}\n\n{}\n", text.trim_end()));
    }
    if research.by_agent {
        note = opening_with(&note, AGENT_CALLOUT);
    }
    if let Some(source) = research.source.map(str::trim).filter(|s| !s.is_empty()) {
        note = set_properties(
            &note,
            &[("source", PropertyValue::Text(source.to_string()))],
        );
    }
    let text = with_first_alias(&path, &note, &name);
    write_new(&path, &text)?;
    Prepared {
        path,
        name,
        slug,
        text,
    }
    .created(vault)
}

/// A note with `block` at the start of its body, after its title heading if it opens with one.
fn opening_with(content: &str, block: &str) -> String {
    let (_, body) = split_frontmatter(content);
    let lines: Vec<&str> = body.lines().collect();
    let first = lines.iter().position(|line| !line.trim().is_empty());
    let at = first
        .filter(|&i| lines[i].starts_with("# "))
        .map_or(0, |i| i + 1);
    let mut out: Vec<&str> = lines[..at].to_vec();
    if at > 0 {
        out.push("");
    }
    out.extend(block.lines());
    let rest: Vec<&str> = lines[at..]
        .iter()
        .copied()
        .skip_while(|line| line.trim().is_empty())
        .collect();
    if !rest.is_empty() {
        out.push("");
        out.extend(rest);
    }
    replace_body(content, &(out.join("\n") + "\n"))
}

/// Lower-cased file names of the notes in `dir`.
fn stems(dir: &Path) -> Result<HashSet<String>> {
    Ok(markdown_files(dir)?
        .iter()
        .filter_map(|path| path.file_stem())
        .map(|stem| stem.to_string_lossy().to_lowercase())
        .collect())
}

/// A note about to be created: where, and its text.
struct Prepared {
    path: PathBuf,
    name: String,
    slug: String,
    text: String,
}

impl Prepared {
    fn created(self, vault: &Vault) -> Result<NewNote> {
        let same_name = vault.note_named(&self.slug, &self.path)?;
        Ok(NewNote {
            path: self.path,
            same_name,
        })
    }
}

fn prepare(
    vault: &Vault,
    project: &Project,
    book: Option<&Book>,
    kind: NoteKind,
    name: &str,
    today: NaiveDate,
) -> Result<Prepared> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let slug = slugify(&name);
    if slug.is_empty() {
        bail!("the name needs at least one letter or number");
    }
    let (folder, existing, owner) = match book {
        Some(book) => {
            let scenes = book.scenes()?.into_iter().map(|scene| scene.note).collect();
            (book.scenes_dir(), scenes, book.title())
        }
        None => (project.folder(kind), project.notes(kind)?, project.title()),
    };
    if existing
        .iter()
        .any(|note| note.title().eq_ignore_ascii_case(&name))
    {
        bail!("{owner} already has a {} called {name}", kind.key());
    }
    let taken = project_stems(project, book)?;
    let slug = unique_slug(&slug, |candidate| taken.contains(candidate));
    let path = folder.join(format!("{slug}.md"));
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
    Ok(Prepared {
        text: with_first_alias(&path, &note, &name),
        path,
        name,
        slug,
    })
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
    use crate::Scene;
    use crate::scratchpad::Scratchpad;
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
        let timeline = fs::read_to_string(project.timeline_path()).unwrap();
        assert!(timeline.starts_with("---\ntype: timeline\n---\n# Sample Story timeline\n"));
        assert!(
            project
                .note
                .body
                .contains("- [[sample-story-world-timeline|Timeline]]: ")
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
    fn adds_scenes_to_books_and_notes_to_the_world_bible() {
        let (_dir, vault) = vault();
        let (project, first) =
            create_project(&vault, &standalone("Sample Story"), today()).unwrap();
        let second = add_book(&vault, &project, &book("Second Story"), today()).unwrap();
        let scene = |book: &Book, name, place| {
            let book = Book::load(&book.dir, &project.slug).unwrap().unwrap();
            add_scene(&vault, &project, &book, name, place, today()).unwrap()
        };
        let arrival = scene(&first, "Arrival", Place::End);
        let leaving = scene(&first, "Leaving: Home?", Place::End);
        scene(&first, "First Light", Place::Chapter(0));
        scene(&first, "Fragment", Place::Unplaced);
        assert!(
            arrival
                .path
                .ends_with("books/sample-story/scenes/arrival.md")
        );
        assert!(
            leaving
                .path
                .ends_with("books/sample-story/scenes/leaving-home.md")
        );
        let first = Book::load(&first.dir, &project.slug).unwrap().unwrap();
        assert_eq!(
            section(&first.note.body, contents::HEADING).unwrap()[1..],
            [
                "%% The book in reading order. A list item that isn't a link is a chapter, with its scenes indented beneath it as links; a link on its own is a chapter by itself. ### headings make parts. %%",
                "",
                "- Chapter",
                "  - [[arrival|Arrival]]",
                "  - [[first-light|First Light]]",
                "- [[leaving-home|Leaving: Home?]]",
                "",
            ]
        );
        let contents = first.contents().unwrap();
        let placed: Vec<String> = contents.placed().map(Scene::title).collect();
        assert_eq!(placed, ["Arrival", "First Light", "Leaving: Home?"]);
        assert_eq!(contents.unplaced[0].title(), "Fragment");
        let leaving = &contents.chapters().nth(1).unwrap().scenes[0];
        assert_eq!(leaving.status().as_deref(), Some("outline"));
        assert_eq!(
            leaving.words, 0,
            "a new scene holds only its heading and a comment"
        );

        let morning = scene(&second, "Arrival", Place::End);
        assert!(
            morning
                .path
                .ends_with("books/second-story/scenes/arrival.md"),
            "scenes in different books may share a name"
        );
        assert!(morning.same_name.is_some());
        let second = Book::load(&second.dir, &project.slug).unwrap().unwrap();
        assert!(
            second.note.body.contains("- [[scenes/arrival|Arrival]]"),
            "a shared name is linked by its path"
        );
        assert_eq!(second.contents().unwrap().placed().count(), 1);
        assert!(
            add_scene(&vault, &project, &first, "Arrival", Place::End, today()).is_err(),
            "a book's scene names are unique"
        );
        assert!(add_scene(&vault, &project, &first, "Lost", Place::Chapter(5), today()).is_err());
        assert!(!first.scenes_dir().join("lost.md").exists());
        assert!(add_note(&vault, &project, NoteKind::Scene, "Lost", today()).is_err());

        let mara = add_note(&vault, &project, NoteKind::Character, "Mara Chen", today()).unwrap();
        assert!(
            mara.path
                .ends_with("sample-story-world/bible/characters/mara-chen.md")
        );
        assert!(mara.same_name.is_none());
        assert!(add_note(&vault, &project, NoteKind::Character, "Mara Chen", today()).is_err());
        let place = add_note(&vault, &project, NoteKind::Location, "Mara Chen", today()).unwrap();
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
        let place = add_note(&vault, &project, NoteKind::Location, "The Lab", today()).unwrap();
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
            format!(
                "timestamp,id,words\n2026-10-30T09:00:00,{id},0\n2026-10-30T09:00:00,{},0\n",
                project.log_key()
            ),
            "the book and the project's scratchpad start from nothing"
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
        assert_eq!(project.log_key(), "old-world/scratchpad");
    }

    #[test]
    fn captures_scraps_as_given() {
        let (dir, vault) = vault();
        let (project, _) = create_project(&vault, &standalone("Sample Story"), today()).unwrap();
        let now = today().and_hms_opt(21, 9, 0).unwrap();
        let text = "  A line, as dictated.\n\n- and a list  ";
        let first = add_scrap(&vault, None, text, Some("dictation"), now).unwrap();
        assert_eq!(
            first.path,
            vault.root.join("scratchpad/2026-10-30-2109.md"),
            "named by the time of capture"
        );
        assert_eq!(
            fs::read_to_string(&first.path).unwrap(),
            format!(
                "---\ntype: scrap\ncreated: 2026-10-30\ntags: []\nsource: dictation\n---\n{text}\n"
            )
        );
        let second = add_scrap(&vault, None, "Another.", None, now).unwrap();
        assert!(second.path.ends_with("scratchpad/2026-10-30-2109-2.md"));
        let empty = add_scrap(&vault, Some(&project), "", None, now).unwrap();
        assert_eq!(
            empty.path,
            dir.path()
                .join("sample-story-world/scratchpad/2026-10-30-2109-3.md"),
            "unique in the vault, for Obsidian"
        );
        assert_eq!(
            fs::read_to_string(&empty.path).unwrap(),
            "---\ntype: scrap\ncreated: 2026-10-30\ntags: []\n---\n"
        );

        fs::write(
            vault.ideas_path(),
            "### Fiction\n\n1. Four words of idea.\n",
        )
        .unwrap();
        let counts = vault.counts(&vault.measure().unwrap()).unwrap();
        let count = |key: &str| counts.iter().find(|(k, _)| k == key).map(|(_, w)| *w);
        assert_eq!(
            count("scratchpad"),
            Some(12),
            "both scraps and the ideas file"
        );
        assert_eq!(count(&project.log_key()), Some(0));
        let scratchpad = Scratchpad::of_vault(&vault).unwrap();
        assert_eq!(
            scratchpad
                .notes
                .iter()
                .map(|note| note.name.as_str())
                .collect::<Vec<_>>(),
            ["2026-10-30-2109-2", "2026-10-30-2109"],
            "newest first, without the ideas file"
        );
    }

    #[test]
    fn adds_research_with_its_source() {
        let (_dir, vault) = vault();
        let (project, _) = create_project(&vault, &standalone("Sample Story"), today()).unwrap();
        let research = NewResearch {
            name: "Orbital Mechanics",
            source: Some("https://example.com/orbits"),
            text: None,
            by_agent: false,
        };
        let added = add_research(&vault, Some(&project), &research, today()).unwrap();
        assert!(
            added
                .path
                .ends_with("sample-story-world/research/orbital-mechanics.md")
        );
        let note = Note::load(&added.path).unwrap();
        assert_eq!(note.frontmatter.str("type").as_deref(), Some("research"));
        assert_eq!(note.title(), "Orbital Mechanics");
        assert_eq!(
            note.frontmatter.str("source").as_deref(),
            Some("https://example.com/orbits")
        );
        assert!(note.body.contains("%% What is true in the real world"));
        assert!(
            add_research(&vault, Some(&project), &research, today()).is_err(),
            "the project already has it"
        );

        let by_agent = NewResearch {
            text: Some("What the source says.\n"),
            by_agent: true,
            ..research
        };
        let added = add_research(&vault, None, &by_agent, today()).unwrap();
        assert_eq!(added.path, vault.root.join("research/orbital-mechanics.md"));
        let note = Note::load(&added.path).unwrap();
        assert_eq!(
            note.body,
            format!("# Orbital Mechanics\n\n{AGENT_CALLOUT}\n\nWhat the source says.\n")
        );
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
