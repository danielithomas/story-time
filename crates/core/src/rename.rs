//! Renaming a note: its name, its file name and every link to it change together, as the format
//! asks. Scenes, world-bible notes (characters, locations and world notes) and research can be
//! renamed, and so can books and projects through their own notes: their folders, story flow and
//! timeline follow. Scraps are named by when they were captured.
//!
//! A note's name is its first alias (or its `title`, if it has one). The file takes the name's
//! slug, numbered if another note in the project has it, as when a note is added. Every link in
//! the bookshelf that meant the note is rewritten to reach it, as a link Obsidian can follow; a
//! link that showed the old name shows the new one, and other link text is kept. A book's
//! contents list is links, so renaming a scene keeps its place. The ideas file and templates are
//! never edited.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::home::Home;
use crate::ideas;
use crate::links::{self, LinkIndex, Moves};
use crate::note::{
    Note, PropertyValue, replace_body, set_properties, split_frontmatter, write_atomically,
};
use crate::project::{BIBLE_DIR, Book, NoteKind, Project, RESERVED};
use crate::scaffold::{project_stems, stems};
use crate::scratchpad;
use crate::text::{name_from_file, slugify, unique_slug};
use crate::vault::{Vault, markdown_files};

/// What a rename did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    /// Where the note is now.
    pub path: PathBuf,
    /// Its name now.
    pub name: String,
    /// How many links were rewritten, and in how many notes.
    pub links: usize,
    pub notes: usize,
    /// Another note on the bookshelf with the same file name, which makes bare links to the name
    /// ambiguous in Obsidian.
    pub same_name: Option<PathBuf>,
    /// Every note that moved, from where it was to where it is now: the note itself, or for a
    /// book or world, everything in its folder.
    pub moved: Vec<(PathBuf, PathBuf)>,
}

/// Where the note at `path` belongs, if it's one that can be renamed: anything but a scrap.
fn renamable(vault: &Vault, path: &Path) -> Result<Home> {
    match Home::of(vault, path)? {
        Some(Home::Scrap { .. }) | None => bail!(
            "only scenes, characters, locations, world notes, research, books and worlds can be \
             renamed; {} isn't one",
            vault.relative(path).display()
        ),
        Some(home) => Ok(home),
    }
}

/// The notes a new name can't be shared with, who they belong to, and what they are.
fn siblings(home: &Home) -> Result<(Vec<Note>, String, &'static str)> {
    Ok(match home {
        Home::Scene { book, .. } => {
            let scenes = book.scenes()?.into_iter().map(|scene| scene.note).collect();
            (scenes, book.title(), "a scene")
        }
        Home::Bible { project, kind } => (
            project.notes(*kind)?,
            project.title(),
            match kind {
                NoteKind::Character => "a character",
                NoteKind::Location => "a location",
                _ => "a world note",
            },
        ),
        Home::Research { project, dir } => (
            scratchpad::research(dir)?,
            project
                .as_ref()
                .map_or_else(|| "The bookshelf".to_string(), Project::title),
            "research",
        ),
        Home::Scrap { .. } => unreachable!("scraps aren't renamed"),
    })
}

/// File names already taken where the note's file name must be unique.
fn taken(home: &Home) -> Result<HashSet<String>> {
    match home {
        Home::Scene { project, book } => project_stems(project, Some(book)),
        Home::Bible { project, .. }
        | Home::Research {
            project: Some(project),
            ..
        } => project_stems(project, None),
        Home::Research { project: None, dir } => stems(dir),
        Home::Scrap { .. } => unreachable!("scraps aren't renamed"),
    }
}

/// Renames the note at `path` to `name`, or with `None`, keeps its name and gives its file the
/// name's slug, which is what `check` suggests for a file name that has drifted from its name.
/// Refuses a name that another note of the same kind already has where it belongs: in the same
/// book for a scene, else in the same project (or the bookshelf's research).
pub fn rename(vault: &Vault, path: &Path, name: Option<&str>) -> Result<Renamed> {
    match folder_of(vault, path)? {
        Some(Folder::Project(project)) => return rename_project(vault, &project, name),
        Some(Folder::Book { project, book }) => return rename_book(vault, &project, &book, name),
        None => {}
    }
    let note = Note::load(path)?;
    let home = renamable(vault, path)?;
    let old_name = note.title();
    let (name, slug) = new_name(name, &old_name)?;
    let (siblings, owner, what) = siblings(&home)?;
    if siblings
        .iter()
        .any(|other| other.path != path && other.title().eq_ignore_ascii_case(&name))
    {
        bail!("{owner} already has {what} called {name}");
    }
    let stem = note.name.to_lowercase();
    let slug = if slug == stem {
        slug
    } else {
        let mut taken = taken(&home)?;
        taken.remove(&stem);
        unique_slug(&slug, |candidate| taken.contains(candidate))
    };
    let new_path = path.with_file_name(format!("{slug}.md"));
    let renamed = |links, notes, same_name| Renamed {
        path: new_path.clone(),
        name: name.clone(),
        links,
        notes,
        same_name,
        moved: vec![(path.to_path_buf(), new_path.clone())],
    };
    if new_path == path && note.given_name().as_deref() == Some(name.as_str()) {
        return Ok(renamed(0, 0, None));
    }
    if new_path != path && new_path.exists() {
        bail!("{} already exists", vault.relative(&new_path).display());
    }

    let before = LinkIndex::build(vault)?;
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    write_atomically(path, &named(&content, &note, &old_name, &name))?;
    if new_path != path {
        fs::rename(path, &new_path).with_context(|| {
            format!(
                "renaming {} to {}",
                vault.relative(path).display(),
                vault.relative(&new_path).display()
            )
        })?;
    }
    // Link text showing the old name, or the name the file was given, shows the new one.
    let old_names = [old_name.clone(), name_from_file(&note.name)];
    let moves = Moves::one(path, &new_path).named(&new_path, old_names.to_vec(), &name);
    let (links, notes) = links::relink(vault, &before, &moves)?;
    let same_name = vault.note_named(&slug, &new_path)?;
    Ok(renamed(links, notes, same_name))
}

/// Whether the note at `path` is one that can be renamed: a scene, a world-bible note or research.
pub fn can_rename(vault: &Vault, path: &Path) -> bool {
    renamable(vault, path).is_ok() || matches!(folder_of(vault, path), Ok(Some(_)))
}

/// The note `query` means: a path to it, from `cwd` or the bookshelf, or a name or file name that
/// one note that can be renamed has. Fails if none does, or if several do.
pub fn find(vault: &Vault, cwd: &Path, query: &str) -> Result<PathBuf> {
    for base in [cwd, vault.root.as_path()] {
        let path = base.join(query);
        if path.is_file() {
            return path
                .canonicalize()
                .with_context(|| format!("reading {}", path.display()));
        }
    }
    let wanted = query.trim().trim_end_matches(".md");
    let mut found = Vec::new();
    for path in markdown_files(&vault.root)? {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let matches = stem.eq_ignore_ascii_case(wanted)
            || Note::load(&path)?.title().eq_ignore_ascii_case(wanted);
        if matches && can_rename(vault, &path) {
            found.push(path);
        }
    }
    match found.len() {
        0 => bail!(
            "no scene, character, location, world note, research, book or world is called {wanted}"
        ),
        1 => Ok(found.remove(0)),
        _ => {
            let paths: Vec<String> = found
                .iter()
                .map(|path| vault.relative(path).display().to_string())
                .collect();
            bail!(
                "several notes are called {wanted}; give the path of the one you mean: {}",
                paths.join(", ")
            )
        }
    }
}

/// A note's text with its name changed: its `title` if it has one, else its first alias, and a
/// first heading that showed the old name.
fn named(content: &str, note: &Note, old_name: &str, name: &str) -> String {
    let content = if note.frontmatter.str("title").is_some() {
        set_properties(content, &[("title", PropertyValue::Text(name.to_string()))])
    } else {
        let mut aliases = vec![name.to_string()];
        aliases.extend(
            note.aliases()
                .into_iter()
                .skip(1)
                .filter(|alias| alias != name),
        );
        set_properties(content, &[("aliases", PropertyValue::List(aliases))])
    };
    with_heading(&content, old_name, name)
}

/// A note's text with a first heading that showed `old` showing `new` instead.
fn with_heading(content: &str, old: &str, new: &str) -> String {
    let (_, body) = split_frontmatter(content);
    let mut lines: Vec<&str> = body.lines().collect();
    let heading = format!("# {new}");
    match lines.iter().position(|line| !line.trim().is_empty()) {
        Some(i) if lines[i].trim_end() == format!("# {old}") && old != new => {
            lines[i] = &heading;
            let ending = if body.ends_with('\n') { "\n" } else { "" };
            replace_body(content, &format!("{}{ending}", lines.join("\n")))
        }
        _ => content.to_string(),
    }
}

/// A new name with its spacing tidied, or with `None` the note's own, and its slug. Refuses a name
/// with no letters or numbers, and one that would break links to the note.
fn new_name(name: Option<&str>, current: &str) -> Result<(String, String)> {
    let name = name
        .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| current.to_string());
    let slug = slugify(&name);
    if slug.is_empty() {
        bail!("the name needs at least one letter or number");
    }
    if name.contains(['[', ']', '|']) {
        bail!("a name can't contain [, ] or |, which would break links to it");
    }
    Ok((name, slug))
}

/// A project or a book, found by its own note. Renaming one renames its folder too.
enum Folder {
    Project(Project),
    Book { project: Project, book: Box<Book> },
}

fn folder_of(vault: &Vault, path: &Path) -> Result<Option<Folder>> {
    for project in vault.projects()? {
        if path == project.note.path {
            return Ok(Some(Folder::Project(project)));
        }
        if path.starts_with(&project.dir) {
            let book = project
                .books()?
                .into_iter()
                .find(|book| book.note.path == path);
            return Ok(book.map(|book| Folder::Book {
                project,
                book: Box::new(book),
            }));
        }
    }
    Ok(None)
}

/// Refuses to rename a book or world without a permanent ID: the writing log would lose it.
fn needs_id(id: Option<String>, title: &str) -> Result<()> {
    if id.is_none() {
        bail!(
            "{title} has no permanent id, so the writing log records it under its folder's name; \
             renaming it would lose that history"
        );
    }
    Ok(())
}

/// Where the note at `path`, in the folder `from`, is once `from` has moved to `to`, with the files
/// in `renamed` (paths within the folder, old and new) given their new names.
fn moved_to(path: &Path, from: &Path, to: &Path, renamed: &[(String, String)]) -> PathBuf {
    let relative = path.strip_prefix(from).unwrap_or(path);
    match renamed.iter().find(|(old, _)| Path::new(old) == relative) {
        Some((_, new)) => to.join(new),
        None => to.join(relative),
    }
}

/// Renames a book: its title and heading, its folder `books/<slug>/`, its note and story flow, and
/// every link to them or to its scenes that no longer reaches them.
fn rename_book(
    vault: &Vault,
    project: &Project,
    book: &Book,
    name: Option<&str>,
) -> Result<Renamed> {
    let old_title = book.title();
    let (title, slug) = new_name(name, &old_title)?;
    needs_id(book.id(), &old_title)?;
    if project
        .books()?
        .iter()
        .any(|other| other.dir != book.dir && other.title().eq_ignore_ascii_case(&title))
    {
        bail!("{} already has a book called {title}", project.title());
    }
    let slug = if slug == book.slug {
        slug
    } else {
        let mut taken = project_stems(project, None)?;
        taken.remove(&book.slug.to_lowercase());
        taken.remove(&format!("{}-flow", book.slug).to_lowercase());
        unique_slug(&slug, |c| {
            taken.contains(c) || taken.contains(&format!("{c}-flow"))
        })
    };
    let new_dir = book.dir.with_file_name(&slug);
    let renamed_files = [
        (format!("{}.md", book.slug), format!("{slug}.md")),
        (format!("{}-flow.md", book.slug), format!("{slug}-flow.md")),
    ];
    let flow = book.flow_path();
    let flow_title = flow
        .is_file()
        .then(|| Note::load(&flow))
        .transpose()?
        .map(|note| note.title());
    rename_folder(
        vault,
        &book.dir,
        &new_dir,
        &renamed_files,
        (&book.note, &old_title, &title),
        flow_title.map(|old| (flow, old)),
        &old_title,
        &title,
    )
}

/// Renames a project, which is a world: its title and heading, its folder, its note and timeline,
/// and every link to them or to anything in it that no longer reaches them. A project named like
/// one of its books takes `-world`, as when it was started. Ideas that became it have a link to
/// its new name added, since the ideas file is only ever added to.
fn rename_project(vault: &Vault, project: &Project, name: Option<&str>) -> Result<Renamed> {
    let old_title = project.title();
    let (title, slug) = new_name(name, &old_title)?;
    needs_id(project.id(), &old_title)?;
    let books = project.books()?;
    for book in &books {
        needs_id(book.id(), &book.title())?;
    }
    if vault
        .projects()?
        .iter()
        .any(|other| other.dir != project.dir && other.title().eq_ignore_ascii_case(&title))
    {
        bail!("The bookshelf already has a world called {title}");
    }
    let mut slug = slug;
    if books.iter().any(|book| book.slug == slug) {
        slug.push_str("-world");
    }
    if slug != project.slug {
        if RESERVED.contains(&slug.as_str()) {
            bail!("{slug} is a reserved folder name; choose another name for the world");
        }
        let taken: HashSet<String> = fs::read_dir(&vault.root)
            .with_context(|| format!("reading {}", vault.root.display()))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_lowercase())
            .filter(|name| *name != project.slug.to_lowercase())
            .collect();
        slug = unique_slug(&slug, |c| taken.contains(c) || RESERVED.contains(&c));
    }
    let new_dir = vault.root.join(&slug);
    let timeline = project.timeline_path();
    let renamed_files = [
        (format!("{}.md", project.slug), format!("{slug}.md")),
        (
            format!("{BIBLE_DIR}/{}-timeline.md", project.slug),
            format!("{BIBLE_DIR}/{slug}-timeline.md"),
        ),
    ];
    let timeline_title = timeline
        .is_file()
        .then(|| Note::load(&timeline))
        .transpose()?
        .map(|note| note.title());
    let renamed = rename_folder(
        vault,
        &project.dir,
        &new_dir,
        &renamed_files,
        (&project.note, &old_title, &title),
        timeline_title.map(|old| (timeline, old)),
        &old_title,
        &title,
    )?;
    if slug != project.slug {
        let ideas_path = vault.ideas_path();
        if ideas_path.is_file() {
            let link = LinkIndex::build(vault)?.link(&ideas_path, &renamed.path, &title);
            let content = fs::read_to_string(&ideas_path)
                .with_context(|| format!("reading {}", ideas_path.display()))?;
            let (content, count) = ideas::link_renamed_project(&content, &project.slug, &link);
            if count > 0 {
                write_atomically(&ideas_path, &content)?;
            }
        }
    }
    Ok(renamed)
}

/// Moves the folder `from` to `to` (when they differ), renaming the files in `renamed_files`
/// there, and renames its own note `main` (with its old and new names) and a companion note named
/// after it (a story flow or timeline: its path and old name, which starts with the old title).
/// Then rewrites the links that no longer reach what they meant.
#[allow(clippy::too_many_arguments)]
fn rename_folder(
    vault: &Vault,
    from: &Path,
    to: &Path,
    renamed_files: &[(String, String)],
    main: (&Note, &str, &str),
    companion: Option<(PathBuf, String)>,
    old_title: &str,
    title: &str,
) -> Result<Renamed> {
    let (note, old_name, name) = main;
    if to != from && to.exists() {
        bail!("{} already exists", vault.relative(to).display());
    }
    let notes = markdown_files(from)?;
    let before = LinkIndex::build(vault)?;

    // The names first, while every note is where it was.
    let content = fs::read_to_string(&note.path)
        .with_context(|| format!("reading {}", note.path.display()))?;
    write_atomically(&note.path, &named(&content, note, old_name, name))?;
    let companion = match companion {
        Some((path, old)) => {
            let new = match old.strip_prefix(old_title) {
                Some(rest) => format!("{title}{rest}"),
                None => old.clone(),
            };
            let content =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            write_atomically(&path, &with_heading(&content, &old, &new))?;
            Some((path, old, new))
        }
        None => None,
    };

    let mut moves = Moves::default();
    for path in &notes {
        let now = moved_to(path, from, to, renamed_files);
        moves.to.insert(path.clone(), now);
    }
    if to != from {
        fs::rename(from, to).with_context(|| {
            format!(
                "renaming {} to {}",
                vault.relative(from).display(),
                vault.relative(to).display()
            )
        })?;
        for (old, new) in renamed_files {
            let (old, new) = (to.join(old), to.join(new));
            if old.is_file() && old != new {
                fs::rename(&old, &new)
                    .with_context(|| format!("renaming {}", vault.relative(&old).display()))?;
            }
        }
    }
    let note_now = moves.to[&note.path].clone();
    let old_names = vec![old_name.to_string(), name_from_file(&note.name)];
    moves = moves.named(&note_now, old_names, name);
    if let Some((path, old, new)) = companion {
        let now = moves.to[&path].clone();
        moves = moves.named(&now, vec![old], &new);
    }
    let (links, changed) = links::relink(vault, &before, &moves)?;
    let stem = note_now
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    Ok(Renamed {
        same_name: vault.note_named(&stem, &note_now)?,
        path: note_now,
        name: name.to_string(),
        links,
        notes: changed,
        moved: moves.to.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check;
    use crate::contents::{self, Place};
    use crate::project::{Book, Form};
    use crate::scaffold::{NewBook, NewProject, add_note, add_scene, create_project};
    use crate::structures::THREE_ACT;
    use chrono::NaiveDate;
    use tempfile::TempDir;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 14).unwrap()
    }

    fn project(vault: &Vault, title: &str) -> (Project, Book) {
        let new = NewProject {
            world: None,
            book: NewBook {
                title,
                form: Form::Novel,
                target: None,
                structure: &THREE_ACT,
                premise: None,
            },
        };
        create_project(vault, &new, today()).unwrap()
    }

    /// A world with Mara, linked from a scene in every way a writer might, and another world
    /// with a Mara of its own.
    struct Shelf {
        _dir: TempDir,
        vault: Vault,
        project: Project,
        book: Book,
        mara: PathBuf,
        arrival: PathBuf,
    }

    fn shelf() -> Shelf {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".obsidian")).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let (project, book) = project(&vault, "Sample Story");
        let mara = add_note(&vault, &project, NoteKind::Character, "Mara", today())
            .unwrap()
            .path;
        let scene = |name: &str| {
            let book = Book::load(&book.dir, &project.slug).unwrap().unwrap();
            add_scene(&vault, &project, &book, name, Place::End, today())
                .unwrap()
                .path
        };
        let arrival = scene("Arrival");
        scene("Leaving Home");
        let content = fs::read_to_string(&arrival).unwrap();
        let content = set_properties(
            &content,
            &[("pov", PropertyValue::Text("[[mara|Mara]]".into()))],
        );
        let body = "# Arrival\n\n[[mara|Mara]] came ashore. [[mara|She]] knew [[mara]] \
                    by sight, as in [[mara#Early life|Mara]]. ![[mara]]\n";
        fs::write(&arrival, replace_body(&content, body)).unwrap();
        fs::create_dir_all(vault.ideas_path().parent().unwrap()).unwrap();
        fs::write(vault.ideas_path(), "- [[mara]]\n").unwrap();
        Shelf {
            _dir: dir,
            vault,
            project,
            book,
            mara,
            arrival,
        }
    }

    #[test]
    fn renames_the_note_its_file_and_every_link_to_it() {
        let s = shelf();
        let renamed = rename(&s.vault, &s.mara, Some("  Mara   Okafor ")).unwrap();
        let new = s.mara.with_file_name("mara-okafor.md");
        assert_eq!(renamed.path, new);
        assert_eq!(renamed.name, "Mara Okafor");
        assert_eq!((renamed.links, renamed.notes), (6, 1));
        assert!(!s.mara.exists());
        let note = Note::load(&new).unwrap();
        assert_eq!(note.aliases(), ["Mara Okafor"]);
        assert!(note.body.starts_with("# Mara Okafor\n"), "{}", note.body);

        let scene = fs::read_to_string(&s.arrival).unwrap();
        assert!(
            scene.contains("pov: \"[[mara-okafor|Mara Okafor]]\""),
            "{scene}"
        );
        assert!(
            scene.contains(
                "[[mara-okafor|Mara Okafor]] came ashore. [[mara-okafor|She]] knew \
                 [[mara-okafor|Mara Okafor]] by sight, as in \
                 [[mara-okafor#Early life|Mara Okafor]]. ![[mara-okafor]]"
            ),
            "{scene}"
        );
        assert_eq!(
            fs::read_to_string(s.vault.ideas_path()).unwrap(),
            "- [[mara]]\n",
            "the ideas file is only ever added to"
        );
        let report = check::check(&s.vault, &s.project).unwrap();
        assert!(
            report.undocumented.is_empty() && report.drifted.is_empty(),
            "{report:?}"
        );
    }

    #[test]
    fn keeps_a_scene_in_its_place() {
        let s = shelf();
        let renamed = rename(&s.vault, &s.arrival, Some("The Landing")).unwrap();
        assert!(renamed.path.ends_with("scenes/the-landing.md"));
        let book = Book::load(&s.book.dir, &s.project.slug).unwrap().unwrap();
        let contents = book.contents().unwrap();
        let placed: Vec<String> = contents.placed().map(|scene| scene.title()).collect();
        assert_eq!(placed, ["The Landing", "Leaving Home"]);
        assert!(contents.unplaced.is_empty());
        assert!(
            book.note.body.contains("- [[the-landing|The Landing]]\n"),
            "{}",
            book.note.body
        );
        let lines = crate::note::section(&book.note.body, contents::HEADING).unwrap();
        assert!(lines.iter().all(|line| !line.contains("arrival")));
    }

    #[test]
    fn refuses_a_name_its_kind_already_has() {
        let s = shelf();
        add_note(&s.vault, &s.project, NoteKind::Character, "Ada", today()).unwrap();
        let taken = rename(&s.vault, &s.mara, Some("ada")).unwrap_err();
        assert_eq!(
            taken.to_string(),
            "Sample Story already has a character called ada"
        );
        assert!(rename(&s.vault, &s.arrival, Some("Leaving home")).is_err());
        assert!(rename(&s.vault, &s.mara, Some("Mara [the second]")).is_err());
        assert!(rename(&s.vault, &s.mara, Some("!!")).is_err());
        assert!(s.mara.exists(), "nothing changes when a rename is refused");

        let renamed = rename(&s.vault, &s.arrival, Some("Ada")).unwrap();
        assert!(
            renamed.path.ends_with("scenes/ada-2.md"),
            "a scene may share a character's name, with a numbered file name"
        );
        for path in [
            s.book.flow_path(),
            s.project.timeline_path(),
            s.vault.ideas_path(),
        ] {
            assert!(
                rename(&s.vault, &path, Some("Anything")).is_err(),
                "{} is named after its book or world",
                path.display()
            );
        }
    }

    #[test]
    fn writes_links_obsidian_can_follow_when_another_world_shares_the_name() {
        let s = shelf();
        let (other, _) = project(&s.vault, "Other Story");
        add_note(
            &s.vault,
            &other,
            NoteKind::Character,
            "Mara Okafor",
            today(),
        )
        .unwrap();
        let renamed = rename(&s.vault, &s.mara, Some("Mara Okafor")).unwrap();
        assert!(renamed.same_name.is_some());
        let scene = fs::read_to_string(&s.arrival).unwrap();
        assert!(
            scene.contains("[[../../../bible/characters/mara-okafor|Mara Okafor]] came ashore"),
            "{scene}"
        );
    }

    #[test]
    fn gives_a_drifted_file_its_name() {
        let s = shelf();
        let content = fs::read_to_string(&s.mara).unwrap();
        let content = set_properties(
            &content,
            &[(
                "aliases",
                PropertyValue::List(vec!["Mara \"Ghost\" Okafor".into(), "Ghost".into()]),
            )],
        );
        fs::write(&s.mara, content).unwrap();
        let renamed = rename(&s.vault, &s.mara, None).unwrap();
        assert!(renamed.path.ends_with("characters/mara-ghost-okafor.md"));
        let note = Note::load(&renamed.path).unwrap();
        assert_eq!(note.aliases(), ["Mara \"Ghost\" Okafor", "Ghost"]);
        let scene = Note::load(&s.arrival).unwrap();
        assert_eq!(
            scene.frontmatter.str("pov").as_deref(),
            Some("[[mara-ghost-okafor|Mara \"Ghost\" Okafor]]"),
            "a link in a quoted property stays readable"
        );
        assert!(scene.frontmatter_error.is_none());

        let again = rename(&s.vault, &renamed.path, None).unwrap();
        assert_eq!((again.links, again.notes), (0, 0), "nothing left to do");
    }

    fn clean(vault: &Vault) {
        for project in vault.projects().unwrap() {
            let report = check::check(vault, &project).unwrap();
            assert!(
                report.undocumented.is_empty()
                    && report.broken_entries.is_empty()
                    && report.drifted.is_empty()
                    && report.misnamed.is_empty(),
                "{report:?}"
            );
        }
    }

    #[test]
    fn renames_a_book_with_its_folder_flow_and_scenes() {
        let s = shelf();
        let id = s.book.id();
        let renamed = rename(&s.vault, &s.book.note.path, Some("The Long Voyage")).unwrap();
        let dir = s.project.books_dir().join("the-long-voyage");
        assert_eq!(renamed.path, dir.join("the-long-voyage.md"));
        assert!(!s.book.dir.exists());
        let book = Book::load(&dir, &s.project.slug).unwrap().unwrap();
        assert_eq!(
            (book.title(), book.id()),
            ("The Long Voyage".to_string(), id)
        );
        assert!(
            book.note.body.starts_with("# The Long Voyage\n"),
            "{}",
            book.note.body
        );
        assert!(
            book.note
                .body
                .contains("[[the-long-voyage-flow|Story flow]]")
        );
        let flow = Note::load(&book.flow_path()).unwrap();
        assert!(
            flow.body.starts_with("# The Long Voyage: story flow\n"),
            "{}",
            flow.body
        );
        let placed: Vec<String> = book
            .contents()
            .unwrap()
            .placed()
            .map(|s| s.title())
            .collect();
        assert_eq!(placed, ["Arrival", "Leaving Home"]);
        let world = Note::load(&s.project.note.path).unwrap();
        assert!(
            world.body.contains("- [[the-long-voyage|The Long Voyage]]"),
            "{}",
            world.body
        );
        clean(&s.vault);

        let (other, _) = project(&s.vault, "Other Story");
        crate::scaffold::add_book(
            &s.vault,
            &other,
            &NewBook {
                title: "Second",
                form: Form::Novel,
                target: None,
                structure: &THREE_ACT,
                premise: None,
            },
            today(),
        )
        .unwrap();
        let first = other.books().unwrap().remove(0);
        assert!(
            rename(&s.vault, &first.note.path, Some("second")).is_err(),
            "a title its world's books already have"
        );
        let without_id = fs::read_to_string(&first.note.path)
            .unwrap()
            .replace("id: ", "old_id: ");
        fs::write(&first.note.path, without_id).unwrap();
        let refused = rename(&s.vault, &first.note.path, Some("Fresh"))
            .unwrap_err()
            .to_string();
        assert!(refused.contains("no permanent id"), "{refused}");
    }

    #[test]
    fn renames_a_world_with_its_timeline_and_the_ideas_that_became_it() {
        let s = shelf();
        let ideas =
            "1. Sample Story: A premise. → [[sample-story-world|Sample Story]]\n- [[mara]]\n";
        fs::write(s.vault.ideas_path(), ideas).unwrap();
        let renamed = rename(&s.vault, &s.project.note.path, Some("The Deep")).unwrap();
        let dir = s.vault.root.join("the-deep");
        assert_eq!(renamed.path, dir.join("the-deep.md"));
        assert!(!s.project.dir.exists());
        let project = Project::load(&dir).unwrap().unwrap();
        assert_eq!(project.title(), "The Deep");
        assert!(
            project.note.body.contains("[[the-deep-timeline|Timeline]]"),
            "{}",
            project.note.body
        );
        let timeline = Note::load(&project.timeline_path()).unwrap();
        assert!(
            timeline.body.starts_with("# The Deep timeline\n"),
            "{}",
            timeline.body
        );
        assert_eq!(
            project.books().unwrap()[0]
                .contents()
                .unwrap()
                .placed()
                .count(),
            2
        );
        assert_eq!(
            fs::read_to_string(s.vault.ideas_path()).unwrap(),
            "1. Sample Story: A premise. → [[sample-story-world|Sample Story]] → [[the-deep|The Deep]]\n- [[mara]]\n",
            "the idea gains a link to the new name, and nothing else changes"
        );
        clean(&s.vault);

        let back = rename(&s.vault, &renamed.path, Some("Sample Story")).unwrap();
        assert!(
            back.path
                .ends_with("sample-story-world/sample-story-world.md"),
            "a world named like its book takes -world"
        );
        assert!(
            rename(&s.vault, &back.path, Some("Research")).is_err(),
            "a reserved name"
        );
        project_named(&s.vault, "Other Story");
        assert!(
            rename(&s.vault, &back.path, Some("other story")).is_err(),
            "another world's title"
        );
        clean(&s.vault);
    }

    fn project_named(vault: &Vault, title: &str) -> (Project, Book) {
        project(vault, title)
    }
}
