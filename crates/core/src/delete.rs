//! Deleting a note, a chapter, a book or a world, and cutting a scene from its book to its
//! world's scratchpad.
//!
//! Deleting hands the note to a `discard` function: in the app, the system trash, so it can be
//! restored. A scene comes out of its book's contents first. Links to a deleted note stay as they
//! were and lead nowhere until it's restored, as `check` reports; `links_to` counts them first.
//!
//! Cutting moves a scene to its world's scratchpad and out of its book's contents, keeping its
//! properties, so it can go back into a book unchanged: `put_back` moves it into a book as an
//! unplaced scene. Links to it, and its own links, are rewritten where they would no longer reach
//! the same notes.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::contents::{self, Spot};
use crate::home::Home;
use crate::links::{self, LinkIndex, Moves, Resolution};
use crate::note::{Note, Saved, content_hash, save_section, section, split_frontmatter};
use crate::project::{BOOKS_HEADING, Book, Project};
use crate::vault::{Vault, markdown_files};

/// Links to the note at `path` from other notes: how many, and in how many notes.
pub fn links_to(vault: &Vault, path: &Path) -> Result<(usize, usize)> {
    let index = LinkIndex::build(vault)?;
    let meant = Resolution::Found(path.to_path_buf());
    let (mut count, mut notes) = (0, 0);
    for file in markdown_files(&vault.root)? {
        if file == path {
            continue;
        }
        let content =
            fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
        let here = links::links(&content)
            .iter()
            .filter(|link| index.resolve(&file, &link.target) == meant)
            .count();
        if here > 0 {
            count += here;
            notes += 1;
        }
    }
    Ok((count, notes))
}

/// Whether the note at `path` can be deleted: a scene, world-bible note, research or a scrap.
pub fn can_delete(vault: &Vault, path: &Path) -> bool {
    matches!(Home::of(vault, path), Ok(Some(_)))
}

/// Deletes the note at `path` by handing it to `discard`, after taking a scene out of its book's
/// contents. Returns its name.
pub fn delete(
    vault: &Vault,
    path: &Path,
    discard: impl FnOnce(&Path) -> Result<()>,
) -> Result<String> {
    let Some(home) = Home::of(vault, path)? else {
        bail!(
            "only scenes, world-bible notes, research and scraps can be deleted here; {} isn't one",
            vault.relative(path).display()
        );
    };
    let name = Note::load(path)?.title();
    if let Home::Scene { book, .. } = &home {
        contents::move_scene(vault, book, &book.hash, path, &Spot::Unplaced)?;
    }
    discard(path).with_context(|| format!("deleting {}", vault.relative(path).display()))?;
    Ok(name)
}

/// Deletes chapter `chapter` of `book`, counting from 0 across parts, from its contents, and
/// hands each of its scenes to `discard`: in the app, the system trash. Unlike taking the chapter
/// out alone ([`contents::delete_chapter`]), Story Time can't undo this, though the trash can.
/// Refused if the book note has changed since `base_hash`, the hash of the contents the writer
/// saw. Returns the chapter's heading and its scenes' names.
pub fn delete_chapter_and_scenes(
    vault: &Vault,
    book: &Book,
    base_hash: &str,
    chapter: usize,
    mut discard: impl FnMut(&Path) -> Result<()>,
) -> Result<(String, Vec<String>)> {
    if book.hash != base_hash {
        bail!(
            "{} has changed since its contents were read, so they were left as they are",
            book.title()
        );
    }
    let scenes: Vec<(PathBuf, String)> = book
        .contents()?
        .chapters()
        .nth(chapter)
        .map(|chapter| {
            let scenes = chapter.scenes.iter();
            scenes
                .map(|scene| (scene.note.path.clone(), scene.title()))
                .collect()
        })
        .unwrap_or_default();
    let deleted = contents::delete_chapter(book, base_hash, chapter)?;
    let mut names = Vec::new();
    for (path, name) in scenes {
        discard(&path).with_context(|| format!("deleting {}", vault.relative(&path).display()))?;
        names.push(name);
    }
    Ok((deleted.what, names))
}

/// Where a cut or put-back scene is now, and how many links were rewritten in how many notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relocated {
    pub path: PathBuf,
    pub name: String,
    pub links: usize,
    pub notes: usize,
}

/// Cuts a scene from its book to its world's scratchpad, taking it out of the book's contents.
/// Refused if another note in the world has its file name, since the scratchpad shares the
/// world's names: renaming the scene first gets round that.
pub fn cut(vault: &Vault, path: &Path) -> Result<Relocated> {
    let Some(Home::Scene { project, book }) = Home::of(vault, path)? else {
        bail!(
            "only a scene in a book can be cut; {} isn't one",
            vault.relative(path).display()
        );
    };
    let to = project.scratchpad_dir().join(file_name(path)?);
    if let Some(other) = same_name_in(&project, None, path)? {
        bail!(
            "{} already has a note called {}, at {}: rename the scene, then cut it",
            project.title(),
            stem(path),
            vault.relative(&other).display()
        );
    }
    contents::move_scene(vault, &book, &book.hash, path, &Spot::Unplaced)?;
    relocate(vault, path, &to)
}

/// Puts a scene that was cut to its world's scratchpad back into `book`, one of that world's
/// books, as an unplaced scene. Refused for any other scrap, and if `book` already has a scene by
/// that name, or another note in the world has its file name.
pub fn put_back(vault: &Vault, path: &Path, book: &Book) -> Result<Relocated> {
    let note = Note::load(path)?;
    let is_cut_scene = note.frontmatter.str("type").as_deref() == Some("scene");
    let project = match Home::of(vault, path)? {
        Some(Home::Scrap {
            project: Some(project),
            ..
        }) if is_cut_scene => project,
        _ => bail!(
            "only a scene cut to a world's scratchpad can be put back into a book; {} isn't one",
            vault.relative(path).display()
        ),
    };
    if book.project != project.slug {
        bail!("{} isn't one of {}'s books", book.title(), project.title());
    }
    let name = note.title();
    if book
        .scenes()?
        .iter()
        .any(|scene| scene.title().eq_ignore_ascii_case(&name))
    {
        bail!(
            "{} already has a scene called {name}: rename one of them, then put it back",
            book.title()
        );
    }
    if let Some(other) = same_name_in(&project, Some(book), path)? {
        bail!(
            "{} already has a note called {}, at {}: rename the scene, then put it back",
            project.title(),
            stem(path),
            vault.relative(&other).display()
        );
    }
    relocate(vault, path, &book.scenes_dir().join(file_name(path)?))
}

/// Another note in `project` with the same file name as `path`, leaving out the scenes of books
/// other than `book` (which may share names) when moving into a book.
fn same_name_in(project: &Project, book: Option<&Book>, path: &Path) -> Result<Option<PathBuf>> {
    let others: Vec<PathBuf> = match book {
        Some(book) => project
            .books()?
            .iter()
            .filter(|other| other.dir != book.dir)
            .map(Book::scenes_dir)
            .collect(),
        None => Vec::new(),
    };
    let wanted = stem(path).to_lowercase();
    Ok(markdown_files(&project.dir)?.into_iter().find(|other| {
        other != path
            && !others.iter().any(|dir| other.starts_with(dir))
            && stem(other).to_lowercase() == wanted
    }))
}

/// Moves the note at `from` to `to` and rewrites the links that would no longer reach it, or
/// that it would no longer follow.
fn relocate(vault: &Vault, from: &Path, to: &Path) -> Result<Relocated> {
    let name = Note::load(from)?.title();
    let (links, notes) = relocate_all(vault, &[(from.to_path_buf(), to.to_path_buf())])?;
    Ok(Relocated {
        path: to.to_path_buf(),
        name,
        links,
        notes,
    })
}

/// Moves notes, each from the first path to the second, then rewrites the links that would no
/// longer reach them, or that they would no longer follow, in one pass. Returns how many links
/// were rewritten, in how many notes.
fn relocate_all(vault: &Vault, moves: &[(PathBuf, PathBuf)]) -> Result<(usize, usize)> {
    if let Some((_, to)) = moves.iter().find(|(_, to)| to.exists()) {
        bail!("{} already exists", vault.relative(to).display());
    }
    let before = LinkIndex::build(vault)?;
    let mut moved = Moves::default();
    for (from, to) in moves {
        let dir = to.parent().context("a note is in a folder")?;
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        fs::rename(from, to).with_context(|| {
            format!(
                "moving {} to {}",
                vault.relative(from).display(),
                vault.relative(to).display()
            )
        })?;
        moved.to.insert(from.clone(), to.clone());
    }
    links::relink(vault, &before, &moved)
}

/// What deleting a book did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedBook {
    pub title: String,
    /// How many of its scenes moved to its world's scratchpad: none if they were deleted too.
    pub kept: usize,
}

/// Deletes a book from its world. By default its scenes move to the world's scratchpad first,
/// keeping their properties, with links to them following; with `with_scenes` they go too. Then
/// the book's folder (its note, story flow and anything else in it) is handed to `discard`: in the
/// app, the system trash. Last, the book comes out of the world's `## Books` list. Refused before
/// anything changes if a scene shares a file name with another note in the world, which the
/// scratchpad couldn't hold beside it.
pub fn delete_book(
    vault: &Vault,
    project: &Project,
    book: &Book,
    with_scenes: bool,
    discard: impl FnOnce(&Path) -> Result<()>,
) -> Result<DeletedBook> {
    let title = book.title();
    let mut kept = 0;
    if !with_scenes {
        let scenes = book.scenes()?;
        let others: Vec<PathBuf> = markdown_files(&project.dir)?
            .into_iter()
            .filter(|path| !path.starts_with(&book.dir))
            .collect();
        let mut moves = Vec::new();
        for scene in &scenes {
            let path = &scene.note.path;
            let wanted = stem(path).to_lowercase();
            if let Some(other) = others
                .iter()
                .find(|other| stem(other).to_lowercase() == wanted)
            {
                bail!(
                    "{title}'s scene {} has the same file name as {}, so it can't join the \
                     world's scratchpad: rename one of them, or delete the scenes too",
                    scene.title(),
                    vault.relative(other).display()
                );
            }
            moves.push((
                path.clone(),
                project.scratchpad_dir().join(file_name(path)?),
            ));
        }
        relocate_all(vault, &moves)?;
        kept = moves.len();
    }
    discard(&book.dir)
        .with_context(|| format!("deleting {}", vault.relative(&book.dir).display()))?;
    unlist(vault, project, &book.slug)?;
    Ok(DeletedBook { title, kept })
}

/// Takes the book with file name `slug` out of the world's `## Books` list, leaving every other
/// line as it was.
fn unlist(vault: &Vault, project: &Project, slug: &str) -> Result<()> {
    let path = &project.note.path;
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let Some(lines) = section(split_frontmatter(&content).1, BOOKS_HEADING) else {
        return Ok(());
    };
    let lists_it = |line: &str| {
        let line = line.trim_start();
        (line.starts_with(['-', '*', '+']) || line.starts_with(|c: char| c.is_ascii_digit()))
            && links::links(line).first().is_some_and(|link| {
                let stem = link.target.rsplit('/').next().unwrap_or_default();
                stem.trim_end_matches(".md").eq_ignore_ascii_case(slug)
            })
    };
    let kept: Vec<String> = lines
        .iter()
        .filter(|line| !lists_it(line))
        .map(|line| line.to_string())
        .collect();
    if kept.len() == lines.len() {
        return Ok(());
    }
    match save_section(path, BOOKS_HEADING, &kept, &content_hash(&content))? {
        Saved::Written { .. } => Ok(()),
        Saved::Conflict { .. } => bail!(
            "{} changed on disk while the book was being deleted: take it out of ## Books there",
            vault.relative(path).display()
        ),
    }
}

/// Deletes a world, handing its whole folder to `discard` in one piece: in the app, the system
/// trash, from where it can be restored as it was. Returns its title.
pub fn delete_project(
    vault: &Vault,
    project: &Project,
    discard: impl FnOnce(&Path) -> Result<()>,
) -> Result<String> {
    let title = project.title();
    discard(&project.dir)
        .with_context(|| format!("deleting {}", vault.relative(&project.dir).display()))?;
    Ok(title)
}

fn file_name(path: &Path) -> Result<&std::ffi::OsStr> {
    path.file_name().context("a note has a file name")
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check;
    use crate::contents::Place;
    use crate::note::{PropertyValue, set_properties};
    use crate::project::{Form, NoteKind};
    use crate::scaffold::{
        NewBook, NewProject, add_book, add_note, add_scene, add_scrap, create_project,
    };
    use crate::structures::THREE_ACT;
    use chrono::NaiveDate;
    use std::cell::Cell;
    use tempfile::TempDir;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 14).unwrap()
    }

    fn new_book(title: &str) -> NewBook<'_> {
        NewBook {
            title,
            form: Form::Novel,
            target: None,
            structure: &THREE_ACT,
            premise: None,
        }
    }

    fn project(vault: &Vault, title: &str) -> (Project, Book) {
        let new = NewProject {
            world: None,
            book: new_book(title),
        };
        create_project(vault, &new, today()).unwrap()
    }

    fn reload(book: &Book) -> Book {
        Book::load(&book.dir, &book.project).unwrap().unwrap()
    }

    fn scene(vault: &Vault, project: &Project, book: &Book, name: &str) -> PathBuf {
        add_scene(vault, project, &reload(book), name, Place::End, today())
            .unwrap()
            .path
    }

    fn placed(book: &Book) -> Vec<String> {
        let contents = reload(book).contents().unwrap();
        contents.placed().map(|scene| scene.title()).collect()
    }

    fn vault() -> (TempDir, Vault) {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("shelf/.obsidian")).unwrap();
        let vault = Vault::open(dir.path().join("shelf")).unwrap();
        (dir, vault)
    }

    #[test]
    fn deletes_to_the_trash_and_out_of_the_contents() {
        let (dir, vault) = vault();
        let (project, book) = project(&vault, "Sample Story");
        let arrival = scene(&vault, &project, &book, "Arrival");
        scene(&vault, &project, &book, "Leaving Home");
        let mara = add_note(&vault, &project, NoteKind::Character, "Mara", today())
            .unwrap()
            .path;
        let pov = PropertyValue::Text("[[mara|Mara]]".into());
        let text = set_properties(&fs::read_to_string(&arrival).unwrap(), &[("pov", pov)]);
        fs::write(&arrival, text).unwrap();
        assert_eq!(links_to(&vault, &mara).unwrap(), (1, 1));

        let trash = dir.path().join("trash");
        fs::create_dir(&trash).unwrap();
        let to_trash = |path: &Path| {
            fs::rename(path, trash.join(path.file_name().unwrap())).map_err(Into::into)
        };
        assert_eq!(delete(&vault, &mara, to_trash).unwrap(), "Mara");
        assert!(!mara.exists() && trash.join("mara.md").exists());
        let report = check::check(&vault, &project).unwrap();
        assert!(
            report.undocumented.contains_key("mara"),
            "the link to it leads nowhere until it's restored"
        );

        assert_eq!(delete(&vault, &arrival, to_trash).unwrap(), "Arrival");
        assert!(trash.join("arrival.md").exists());
        assert_eq!(placed(&book), ["Leaving Home"]);
        assert!(!reload(&book).note.body.contains("arrival"));

        let called = Cell::new(false);
        for path in [
            &book.note.path,
            &project.note.path,
            &book.flow_path(),
            &vault.ideas_path(),
        ] {
            let result = delete(&vault, path, |_| {
                called.set(true);
                Ok(())
            });
            assert!(result.is_err(), "{}", path.display());
        }
        assert!(
            !called.get(),
            "nothing is discarded when a delete is refused"
        );
        let leaving = book.scenes_dir().join("leaving-home.md");
        assert!(delete(&vault, &leaving, |_| bail!("no trash here")).is_err());
        assert!(leaving.exists());
    }

    #[test]
    fn cuts_a_scene_to_the_scratchpad_and_puts_it_back() {
        let (_dir, vault) = vault();
        let (project, one) = project(&vault, "Sample Story");
        let two = add_book(&vault, &project, &new_book("Second Story"), today()).unwrap();
        let arrival = scene(&vault, &project, &one, "Arrival");
        let morning = scene(&vault, &project, &one, "Morning");
        scene(&vault, &project, &two, "Morning");
        // Two worlds have a Mara, so a link to this one is a path, which a move must follow.
        let mara = add_note(&vault, &project, NoteKind::Character, "Mara", today())
            .unwrap()
            .path;
        let (other, _) = project_named(&vault, "Other Story");
        add_note(&vault, &other, NoteKind::Character, "Mara", today()).unwrap();
        let pov = LinkIndex::build(&vault)
            .unwrap()
            .link(&arrival, &mara, "Mara");
        assert_eq!(pov, "[[../../../bible/characters/mara|Mara]]");
        let text = set_properties(
            &fs::read_to_string(&arrival).unwrap(),
            &[("pov", PropertyValue::Text(pov))],
        );
        fs::write(&arrival, text).unwrap();
        let flow = one.flow_path();
        fs::write(&flow, "See [[arrival|Arrival]].\n").unwrap();

        let cut_one = cut(&vault, &arrival).unwrap();
        let in_scratchpad = project.scratchpad_dir().join("arrival.md");
        assert_eq!(cut_one.path, in_scratchpad);
        assert_eq!(placed(&one), ["Morning"]);
        let note = Note::load(&in_scratchpad).unwrap();
        assert_eq!(
            note.frontmatter.str("type").as_deref(),
            Some("scene"),
            "it keeps its properties"
        );
        assert_eq!(
            note.frontmatter.str("pov").as_deref(),
            Some("[[../bible/characters/mara|Mara]]"),
            "its own links still reach their notes"
        );
        let index = LinkIndex::build(&vault).unwrap();
        assert_eq!(
            index.resolve(&flow, "arrival"),
            Resolution::Found(in_scratchpad.clone())
        );
        assert_eq!(
            fs::read_to_string(&flow).unwrap(),
            "See [[arrival|Arrival]].\n"
        );

        let clash = cut(&vault, &morning).unwrap_err().to_string();
        assert!(
            clash.contains("already has a note called morning"),
            "{clash}"
        );
        assert!(morning.exists());

        let back = put_back(&vault, &in_scratchpad, &reload(&two)).unwrap();
        assert_eq!(back.path, two.scenes_dir().join("arrival.md"));
        let contents = reload(&two).contents().unwrap();
        let unplaced: Vec<String> = contents.unplaced.iter().map(|s| s.title()).collect();
        assert_eq!(unplaced, ["Arrival"]);
        let note = Note::load(&back.path).unwrap();
        assert_eq!(
            note.frontmatter.str("pov").as_deref(),
            Some("[[../../../bible/characters/mara|Mara]]")
        );

        let scrap = add_scrap(
            &vault,
            Some(&project),
            "A line.",
            None,
            today().and_hms_opt(9, 0, 0).unwrap(),
        )
        .unwrap()
        .path;
        assert!(
            put_back(&vault, &scrap, &reload(&one)).is_err(),
            "only a cut scene goes back"
        );
        let again = cut(&vault, &back.path).unwrap();
        let (_, elsewhere) = project_named(&vault, "Third Story");
        assert!(
            put_back(&vault, &again.path, &elsewhere).is_err(),
            "only into its own world"
        );
    }

    #[test]
    fn deletes_a_chapter_keeping_its_scenes_or_with_them() {
        let (dir, vault) = vault();
        let (project, book) = project(&vault, "Sample Story");
        for name in ["Arrival", "Leaving Home", "Coda"] {
            scene(&vault, &project, &book, name);
        }
        let first_light = add_scene(
            &vault,
            &project,
            &reload(&book),
            "First Light",
            Place::Chapter(0),
            today(),
        )
        .unwrap()
        .path;
        assert_eq!(
            placed(&book),
            ["Arrival", "First Light", "Leaving Home", "Coda"]
        );

        // Taking the first chapter out leaves its scenes in the book, and can be undone.
        let before = reload(&book);
        let deleted = contents::delete_chapter(&before, &before.hash, 0).unwrap();
        assert_eq!(deleted.what, "Chapter 1");
        assert_eq!(placed(&book), ["Leaving Home", "Coda"]);
        let unplaced: Vec<String> = reload(&book)
            .contents()
            .unwrap()
            .unplaced
            .iter()
            .map(|scene| scene.title())
            .collect();
        assert_eq!(unplaced, ["Arrival", "First Light"]);
        assert!(
            contents::delete_chapter(&before, &before.hash, 0).is_err(),
            "not from stale contents"
        );
        contents::restore(&reload(&book), &deleted.hash, &deleted.previous).unwrap();
        assert_eq!(
            placed(&book),
            ["Arrival", "First Light", "Leaving Home", "Coda"]
        );

        // With its scenes, they go to the trash too.
        let trash = dir.path().join("trash");
        fs::create_dir(&trash).unwrap();
        let to_trash = |path: &Path| {
            fs::rename(path, trash.join(path.file_name().unwrap())).map_err(Into::into)
        };
        let now = reload(&book);
        assert!(delete_chapter_and_scenes(&vault, &now, "stale", 0, to_trash).is_err());
        let (heading, names) =
            delete_chapter_and_scenes(&vault, &now, &now.hash, 0, to_trash).unwrap();
        assert_eq!(
            (heading.as_str(), names),
            (
                "Chapter 1",
                vec!["Arrival".to_string(), "First Light".to_string()]
            )
        );
        assert!(!first_light.exists() && trash.join("first-light.md").exists());
        assert_eq!(placed(&book), ["Leaving Home", "Coda"]);
        assert!(reload(&book).contents().unwrap().unplaced.is_empty());
        assert!(contents::delete_chapter(&reload(&book), &reload(&book).hash, 9).is_err());
    }

    #[test]
    fn deletes_a_book_keeping_its_scenes_or_with_them() {
        let (dir, vault) = vault();
        let (project, one) = project(&vault, "Sample Story");
        let two = add_book(&vault, &project, &new_book("Second Story"), today()).unwrap();
        let three = add_book(&vault, &project, &new_book("Third Story"), today()).unwrap();
        let arrival = scene(&vault, &project, &two, "Arrival");
        scene(&vault, &project, &two, "Leaving Home");
        let mara = add_note(&vault, &project, NoteKind::Character, "Mara", today())
            .unwrap()
            .path;
        fs::write(
            &mara,
            "---\naliases: [Mara]\n---\nShe arrives in [[arrival|Arrival]].\n",
        )
        .unwrap();
        let trash = dir.path().join("trash");
        fs::create_dir(&trash).unwrap();
        let to_trash = |path: &Path| {
            fs::rename(path, trash.join(path.file_name().unwrap())).map_err(Into::into)
        };
        let reloaded = || Project::load(&project.dir).unwrap().unwrap();

        let deleted = delete_book(&vault, &project, &reload(&two), false, to_trash).unwrap();
        assert_eq!(
            deleted,
            DeletedBook {
                title: "Second Story".into(),
                kept: 2
            }
        );
        assert!(trash.join("second-story").is_dir() && !two.dir.exists());
        let kept = project.scratchpad_dir().join("arrival.md");
        assert!(kept.exists() && project.scratchpad_dir().join("leaving-home.md").exists());
        assert_eq!(
            LinkIndex::build(&vault).unwrap().resolve(&mara, "arrival"),
            Resolution::Found(kept),
            "links to its scenes follow them"
        );
        let books: Vec<String> = reloaded()
            .books()
            .unwrap()
            .iter()
            .map(Book::title)
            .collect();
        assert_eq!(books, ["Sample Story", "Third Story"]);
        assert!(
            !reloaded().note.body.contains("second-story"),
            "out of ## Books"
        );
        assert!(arrival.parent().unwrap().parent().unwrap() == two.dir);

        // A scene that would clash in the scratchpad stops it before anything changes: books may
        // share scene names, but the scratchpad shares the world's.
        scene(&vault, &project, &one, "Morning");
        let morning = scene(&vault, &project, &three, "Morning");
        let clash = delete_book(&vault, &project, &reload(&three), false, |_| {
            bail!("not reached")
        })
        .unwrap_err()
        .to_string();
        assert!(clash.contains("same file name"), "{clash}");
        assert!(morning.exists());
        assert!(three.dir.exists());
        let gone = delete_book(&vault, &project, &reload(&three), true, to_trash).unwrap();
        assert_eq!(gone.kept, 0);
        assert!(trash.join("third-story/scenes/morning.md").exists());

        let books: Vec<String> = reloaded()
            .books()
            .unwrap()
            .iter()
            .map(Book::title)
            .collect();
        assert_eq!(books, ["Sample Story"]);
        assert_eq!(
            delete_project(&vault, &reloaded(), to_trash).unwrap(),
            "Sample Story"
        );
        assert!(!project.dir.exists() && trash.join(project.dir.file_name().unwrap()).is_dir());
        assert!(one.dir.starts_with(&project.dir));
        assert!(vault.projects().unwrap().is_empty());
    }

    fn project_named(vault: &Vault, title: &str) -> (Project, Book) {
        project(vault, title)
    }
}
