//! Upgrading a vault from the layout story-time 0.1 used (format 0) to format 1, as
//! docs/bookshelf-format.md ("Versions and upgrades") describes.
//!
//! [`plan`] works out every change without touching anything, so a clash stops the upgrade before
//! it starts. [`apply`] copies the vault into `.story-time/backups/`, then makes the changes. Notes
//! are moved, not rewritten: story-time changes only the properties it owns, adds the `## Books`
//! and `## Contents` lists, and rewrites links to notes that moved. The ideas file moves exactly
//! as it is.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use chrono::{NaiveDate, NaiveDateTime};
use regex::{Captures, Regex};

use crate::config::{self, CONFIG_FILE, DEFAULT_CONFIG};
use crate::contents;
use crate::history::History;
use crate::ids;
use crate::links::{LinkIndex, Resolution};
use crate::note::{Note, PropertyValue, remove_properties, set_properties, set_section};
use crate::project::{BIBLE_DIR, BOOKS_DIR, BOOKS_HEADING, NoteKind, RESERVED, SCRATCHPAD_DIR};
use crate::scaffold::{self, add_line, write_new};
use crate::templates::{self, render};
use crate::text::{is_slug, name_from_file, slugify, strip_leading_number, unique_slug};
use crate::vault::{Vault, files_under, is_hidden, is_note, markdown_files};
use crate::words::WIKILINK;

/// The folder in `.story-time/` holding backups.
const BACKUPS_DIR: &str = "backups";

/// The line the upgrade adds to a git vault's `.gitignore`: backups stay on this machine.
pub const BACKUPS_IGNORE: &str = ".story-time/backups/";

/// Where format 1 keeps templates and the ideas file.
const TEMPLATES_DIR: &str = "templates";
const IDEAS_FILE: &str = "ideas.md";

/// The templates story-time 0.1 wrote, so copies nobody changed can give way to the new ones.
const OLD_TEMPLATES: [(&str, &str); 6] = [
    (
        "Project.md",
        include_str!("../templates/format-0/Project.md"),
    ),
    (
        "Story Flow.md",
        include_str!("../templates/format-0/Story Flow.md"),
    ),
    (
        "Chapter.md",
        include_str!("../templates/format-0/Chapter.md"),
    ),
    (
        "Character.md",
        include_str!("../templates/format-0/Character.md"),
    ),
    (
        "Location.md",
        include_str!("../templates/format-0/Location.md"),
    ),
    ("World.md", include_str!("../templates/format-0/World.md")),
];

/// The property story-time 0.1 used to name a note's project. In format 1 the folder says it.
const OLD_PROJECT_PROPERTY: &str = "project";

/// Everything an upgrade will change, worked out before anything does.
#[derive(Debug)]
pub struct Plan {
    /// Files to move, from their old path to their new one.
    pub moves: Vec<(PathBuf, PathBuf)>,
    /// story-time 0.1's templates, where nobody changed them. The new ones replace them.
    pub replaced: Vec<PathBuf>,
    /// The projects, each becoming a world with one book.
    pub worlds: Vec<NewWorld>,
    /// Property changes, by the note's new path.
    edits: HashMap<PathBuf, Edit>,
    /// The last word counts under the keys the writing log used before.
    counts: Vec<(String, u64)>,
    /// Links as they resolved before the upgrade.
    index: LinkIndex,
}

/// A project as it becomes a world: its note was the book's, so it gets a new one.
#[derive(Debug)]
pub struct NewWorld {
    /// The project's folder before the upgrade.
    pub from: PathBuf,
    pub title: String,
    /// The project note to create.
    pub note: PathBuf,
    /// The timeline to create.
    pub timeline: PathBuf,
    /// The book note: the old project note, moved.
    pub book: PathBuf,
    id: String,
    genre: Vec<String>,
    /// The book's scenes, in the order of the chapters they were, with their names.
    scenes: Vec<(PathBuf, String)>,
}

#[derive(Debug, Default)]
struct Edit {
    set: Vec<(&'static str, PropertyValue)>,
    remove: Vec<&'static str>,
}

/// Works out how to upgrade a vault in format 0, changing nothing.
pub fn plan(vault: &Vault) -> Result<Plan> {
    if !vault.config.needs_upgrade() {
        bail!(
            "this bookshelf is already in format {}, so there is nothing to upgrade",
            vault.config.format
        );
    }
    let root = &vault.root;
    let history = History::load(&vault.history_path())?;
    let mut planner = Planner {
        root,
        moves: Vec::new(),
        edits: HashMap::new(),
    };
    let mut replaced = Vec::new();

    // The ideas file, and any other notes in the old scratchpad.
    let old_ideas = vault.ideas_path();
    let scratchpad = root.join(SCRATCHPAD_DIR);
    if old_ideas.is_file() {
        planner.move_file(&old_ideas, scratchpad.join(IDEAS_FILE));
    }
    let old_scratchpads = folders(root, SCRATCHPAD_DIR)?;
    let mut taken = HashSet::from([IDEAS_FILE.trim_end_matches(".md").to_string()]);
    for old in &old_scratchpads {
        let notes = files_under(old)?
            .into_iter()
            .filter(|path| *path != old_ideas);
        planner.move_tree(old, notes, &scratchpad, &mut taken, |note| {
            note.frontmatter.str("type").is_none().then_some("scrap")
        })?;
    }

    // Templates: story-time's own give way to the new ones, unless they were changed.
    let old_templates = vault.templates_dir();
    if old_templates != *root {
        let templates = root.join(TEMPLATES_DIR);
        let mut taken = HashSet::new();
        for path in files_under(&old_templates)? {
            let relative = path.strip_prefix(&old_templates)?;
            let old = OLD_TEMPLATES
                .iter()
                .find(|(name, _)| relative == Path::new(name));
            let folder = templates.join(slug_dirs(relative.parent()));
            let to = match old {
                Some((_, default)) if read(&path)?.replace("\r\n", "\n") == *default => {
                    replaced.push(path);
                    continue;
                }
                Some(_) => folder.join(format!("old-{}.md", slugify(&stem(&path)))),
                None if is_note(&path) => {
                    let slug = unique_slug(&slugify(&stem(&path)), |s| taken.contains(s));
                    taken.insert(slug.clone());
                    folder.join(format!("{slug}.md"))
                }
                None => folder.join(path.file_name().context("a file has no name")?),
            };
            planner.move_file(&path, to);
        }
    }

    // Projects: folders at the root holding a note of the same name.
    let mut roots = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let path = entry?.path();
        if !is_hidden(&path) {
            roots.push(path);
        }
    }
    roots.sort();
    let is_project = |path: &PathBuf| {
        path.is_dir()
            && *path != old_templates
            && !old_scratchpads.contains(path)
            && path.join(format!("{}.md", file_name(path))).is_file()
    };
    let (projects, staying): (Vec<PathBuf>, Vec<PathBuf>) = roots.into_iter().partition(is_project);
    let mut names: HashSet<String> = staying
        .iter()
        .map(|path| file_name(path).to_lowercase())
        .collect();
    names.extend(RESERVED.map(String::from));
    let mut taken_ids = HashSet::new();
    let mut worlds = Vec::new();
    let mut counts = Vec::new();
    for dir in projects {
        let name = file_name(&dir);
        let book = slugify(&name);
        if book.is_empty() {
            bail!("{name}/ needs a letter or digit in its name to name its book after");
        }
        let slug = unique_slug(&format!("{book}-world"), |s| names.contains(s));
        names.insert(slug.clone());
        let (world, words) = planner.plan_project(&dir, &name, &slug, &book, &mut taken_ids)?;
        if history.latest(&name).is_some() {
            counts.push((name, words));
        }
        worlds.push(world);
    }

    // Nothing may land on another file.
    let lower = |path: &Path| path.to_string_lossy().to_lowercase();
    let sources: HashSet<String> = planner.moves.iter().map(|(from, _)| lower(from)).collect();
    let created = worlds
        .iter()
        .flat_map(|world| [&world.note, &world.timeline]);
    let mut targets = HashSet::new();
    for to in planner.moves.iter().map(|(_, to)| to).chain(created) {
        let shown = vault.relative(to).display();
        if !targets.insert(lower(to)) {
            bail!("two files would both become {shown}; rename one, then upgrade again");
        }
        if to.exists() && !sources.contains(&lower(to)) {
            bail!("{shown} is in the way; move or rename it, then upgrade again");
        }
    }

    Ok(Plan {
        moves: planner.moves,
        replaced,
        worlds,
        edits: planner.edits,
        counts,
        index: LinkIndex::build(vault)?,
    })
}

struct Planner<'a> {
    root: &'a Path,
    moves: Vec<(PathBuf, PathBuf)>,
    edits: HashMap<PathBuf, Edit>,
}

impl Planner<'_> {
    fn move_file(&mut self, from: &Path, to: PathBuf) {
        if from != to {
            self.moves.push((from.to_path_buf(), to));
        }
    }

    /// Moves a note into `folder` under a slug of its name, unique among `taken`, keeping the name
    /// in its aliases. Returns its new path and its name.
    fn move_note(
        &mut self,
        path: &Path,
        folder: &Path,
        taken: &mut HashSet<String>,
        chapter: bool,
        kind: Option<&'static str>,
    ) -> Result<(PathBuf, String)> {
        let note = Note::load(path)?;
        let name = old_name(&note, chapter);
        let slug = match slugify(&name) {
            slug if slug.is_empty() => kind.unwrap_or("note").to_string(),
            slug => slug,
        };
        let slug = unique_slug(&slug, |s| taken.contains(s));
        taken.insert(slug.clone());
        let to = folder.join(format!("{slug}.md"));
        let mut edit = Edit::default();
        if let Some(kind) =
            kind.filter(|kind| note.frontmatter.str("type").as_deref() != Some(kind))
        {
            edit.set
                .push(("type", PropertyValue::Text(kind.to_string())));
        }
        if let Some(aliases) = aliases_for(&note, &name, &slug) {
            edit.set.push(("aliases", PropertyValue::List(aliases)));
        }
        if note.frontmatter.str(OLD_PROJECT_PROPERTY).is_some() {
            edit.remove.push(OLD_PROJECT_PROPERTY);
        }
        self.edit(&to, edit);
        self.move_file(path, to.clone());
        Ok((to, name))
    }

    /// Moves the files `paths`, all under `from`, to the same places under `to`, with folder names
    /// made slugs and notes moved by [`Planner::move_note`]. `kind` gives a note's new `type`.
    fn move_tree(
        &mut self,
        from: &Path,
        paths: impl IntoIterator<Item = PathBuf>,
        to: &Path,
        taken: &mut HashSet<String>,
        kind: impl Fn(&Note) -> Option<&'static str>,
    ) -> Result<()> {
        for path in paths {
            let relative = path.strip_prefix(from)?;
            let folder = to.join(slug_dirs(relative.parent()));
            if is_note(&path) {
                let kind = kind(&Note::load(&path)?);
                self.move_note(&path, &folder, taken, false, kind)?;
            } else {
                let name = path.file_name().context("a file has no name")?;
                self.move_file(&path, folder.join(name));
            }
        }
        Ok(())
    }

    fn edit(&mut self, path: &Path, edit: Edit) {
        if !edit.set.is_empty() || !edit.remove.is_empty() {
            self.edits.insert(path.to_path_buf(), edit);
        }
    }

    /// Plans a story-time 0.1 project in `dir` as the world `slug` with the single book `book`:
    /// the project note becomes the book's note, chapters become its scenes, and characters,
    /// locations and world notes move into the world bible. Returns the world, and the words its
    /// chapters held, as story-time 0.1 counted them.
    fn plan_project(
        &mut self,
        dir: &Path,
        name: &str,
        slug: &str,
        book: &str,
        taken_ids: &mut HashSet<String>,
    ) -> Result<(NewWorld, u64)> {
        let world_dir = self.root.join(slug);
        let book_dir = world_dir.join(BOOKS_DIR).join(book);
        let old_note = dir.join(format!("{name}.md"));
        let note = Note::load(&old_note)?;
        let title = note
            .frontmatter
            .str("title")
            .or_else(|| note.heading())
            .unwrap_or_else(|| name_from_file(name));
        let mut taken: HashSet<String> = [
            slug.to_string(),
            format!("{slug}-timeline"),
            book.to_string(),
            format!("{book}-flow"),
        ]
        .into();

        let book_id = ids::new_id(taken_ids)?;
        taken_ids.insert(book_id.clone());
        let id = ids::new_id(taken_ids)?;
        taken_ids.insert(id.clone());
        let book_note = book_dir.join(format!("{book}.md"));
        let mut set = vec![
            ("type", PropertyValue::Text("book".to_string())),
            ("id", PropertyValue::Text(book_id)),
        ];
        if note.frontmatter.str("title").is_none() {
            set.push(("title", PropertyValue::Text(title.clone())));
        }
        self.edit(
            &book_note,
            Edit {
                set,
                remove: vec!["genre", OLD_PROJECT_PROPERTY],
            },
        );
        self.move_file(&old_note, book_note.clone());
        let old_flow = dir.join(format!("{name}-flow.md"));
        if old_flow.is_file() {
            let flow = book_dir.join(format!("{book}-flow.md"));
            self.edit(
                &flow,
                Edit {
                    remove: vec![OLD_PROJECT_PROPERTY],
                    ..Edit::default()
                },
            );
            self.move_file(&old_flow, flow);
        }

        let mut moved: Vec<PathBuf> = vec![old_note, old_flow];
        for kind in NoteKind::ALL.into_iter().filter(|kind| kind.is_bible()) {
            for old in folders(dir, kind.folder())? {
                let to = world_dir.join(BIBLE_DIR).join(kind.folder());
                let files = files_under(&old)?;
                moved.extend(files.iter().cloned());
                self.move_tree(&old, files, &to, &mut taken, |_| Some(kind.key()))?;
            }
        }
        let mut scenes = Vec::new();
        let mut words = 0;
        for chapters in folders(dir, "chapters")? {
            let scenes_dir = book_dir.join(NoteKind::Scene.folder());
            for path in files_under(&chapters)? {
                if is_note(&path) {
                    words += Note::load(&path)?.words() as u64;
                    let scene =
                        self.move_note(&path, &scenes_dir, &mut taken, true, Some("scene"))?;
                    scenes.push(scene);
                } else {
                    let relative = path.strip_prefix(&chapters)?;
                    let name = path.file_name().context("a file has no name")?;
                    let folder = scenes_dir.join(slug_dirs(relative.parent()));
                    self.move_file(&path, folder.join(name));
                }
                moved.push(path);
            }
        }
        let rest = files_under(dir)?
            .into_iter()
            .filter(|path| !moved.contains(path));
        self.move_tree(dir, rest, &world_dir, &mut taken, |_| None)?;

        let world = NewWorld {
            from: dir.to_path_buf(),
            note: world_dir.join(format!("{slug}.md")),
            timeline: world_dir
                .join(BIBLE_DIR)
                .join(format!("{slug}-timeline.md")),
            book: book_note,
            title,
            id,
            genre: note.frontmatter.list("genre"),
            scenes,
        };
        Ok((world, words))
    }
}

/// What story-time 0.1 called a note: a chapter by its title, else its file name without the
/// number; any other note by its file name. A file name that is already a slug gives way to the
/// note's heading.
fn old_name(note: &Note, chapter: bool) -> String {
    let stem = if chapter {
        strip_leading_number(&note.name).trim()
    } else {
        note.name.as_str()
    };
    let title = chapter
        .then(|| note.frontmatter.str("title").or_else(|| note.heading()))
        .flatten();
    title
        .or_else(|| is_slug(stem).then(|| note.heading()).flatten())
        .unwrap_or_else(|| name_from_file(stem))
}

/// The aliases a note moved to `slug` needs to keep its name: the name first, then any it had.
/// `None` if it needs no change.
fn aliases_for(note: &Note, name: &str, slug: &str) -> Option<Vec<String>> {
    let aliases = note.aliases();
    if aliases.first().map(String::as_str) == Some(name)
        || (aliases.is_empty() && name_from_file(slug) == name)
    {
        return None;
    }
    let mut list = vec![name.to_string()];
    list.extend(aliases.into_iter().filter(|alias| alias != name));
    Some(list)
}

/// A folder path with each folder's name made a slug.
fn slug_dirs(path: Option<&Path>) -> PathBuf {
    path.into_iter()
        .flat_map(Path::components)
        .map(|part| {
            let name = part.as_os_str().to_string_lossy();
            match slugify(&name) {
                slug if slug.is_empty() => name.into_owned(),
                slug => slug,
            }
        })
        .collect()
}

/// The folders in `dir` called `name`, in any case, in order.
fn folders(dir: &Path, name: &str) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() && file_name(&path).eq_ignore_ascii_case(name) {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

/// Upgrades the vault as `plan` says, after copying it into `.story-time/backups/`. Returns the
/// backup's folder.
pub fn apply(vault: &Vault, plan: &Plan, now: NaiveDateTime) -> Result<PathBuf> {
    let root = &vault.root;
    let backup = backup(vault, now)?;
    let history_path = vault.history_path();
    History::load(&history_path)?.record(&history_path, now, &plan.counts)?;

    for path in &plan.replaced {
        fs::remove_file(path).with_context(|| format!("removing {}", path.display()))?;
    }
    move_all(vault, plan)?;
    for (path, edit) in &plan.edits {
        let content = read(path)?;
        let mut edited = remove_properties(&content, &edit.remove);
        if !edit.set.is_empty() {
            edited = set_properties(&edited, &edit.set);
        }
        if edited != content {
            fs::write(path, edited).with_context(|| format!("writing {}", path.display()))?;
        }
    }

    let config_path = vault.meta_dir().join(CONFIG_FILE);
    let config = match fs::read_to_string(&config_path) {
        Ok(text) => config::upgraded_config(&text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => DEFAULT_CONFIG.to_string(),
        Err(e) => return Err(e).with_context(|| format!("reading {}", config_path.display())),
    };
    fs::write(&config_path, config)
        .with_context(|| format!("writing {}", config_path.display()))?;
    point_obsidian_templates(root, &vault.config.templates_dir)?;
    scaffold::init_vault(root, false)?;
    if root.join(".git").exists() {
        add_line(&root.join(".gitignore"), BACKUPS_IGNORE)?;
    }

    let vault = Vault::open(root)?;
    let index = LinkIndex::build(&vault)?;
    for world in &plan.worlds {
        create_world(&index, world, now.date())?;
    }
    rewrite_links(&vault, plan)?;
    vault.snapshot(now)?;
    Ok(backup)
}

/// Copies the vault, apart from git's history, compiled manuscripts and earlier backups, into a
/// new folder in `.story-time/backups/`.
fn backup(vault: &Vault, now: NaiveDateTime) -> Result<PathBuf> {
    let backups = vault.meta_dir().join(BACKUPS_DIR);
    let dir = backups.join(now.format("%Y-%m-%d-%H%M%S").to_string());
    if dir.exists() {
        bail!("{} already exists", vault.relative(&dir).display());
    }
    let skip = [vault.root.join(".git"), backups, vault.build_dir()];
    copy_tree(&vault.root, &dir, &skip)?;
    Ok(dir)
}

fn copy_tree(from: &Path, to: &Path, skip: &[PathBuf]) -> Result<()> {
    fs::create_dir_all(to).with_context(|| format!("creating {}", to.display()))?;
    for entry in fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let path = entry.path();
        if skip.contains(&path) {
            continue;
        }
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&path, &target, skip)?;
        } else if kind.is_file() {
            fs::copy(&path, &target).with_context(|| format!("copying {}", path.display()))?;
        }
    }
    Ok(())
}

/// Moves every file in two steps, out to `.story-time/upgrading/` and then into place, with the
/// folders left empty removed in between. A name that changes only in case then changes on
/// systems that don't tell case apart too.
fn move_all(vault: &Vault, plan: &Plan) -> Result<()> {
    let staging = vault.meta_dir().join("upgrading");
    fs::create_dir_all(&staging).with_context(|| format!("creating {}", staging.display()))?;
    for (i, (from, _)) in plan.moves.iter().enumerate() {
        fs::rename(from, staging.join(i.to_string()))
            .with_context(|| format!("moving {}", from.display()))?;
    }
    let mut vacated: Vec<PathBuf> = plan
        .moves
        .iter()
        .map(|(from, _)| from)
        .chain(&plan.replaced)
        .filter_map(|path| {
            let relative = path.strip_prefix(&vault.root).ok()?;
            let top = relative.components().next()?;
            (relative.components().count() > 1).then(|| vault.root.join(top))
        })
        .collect();
    vacated.sort();
    vacated.dedup();
    for dir in vacated {
        remove_empty(&dir)?;
    }
    for (i, (_, to)) in plan.moves.iter().enumerate() {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::rename(staging.join(i.to_string()), to)
            .with_context(|| format!("moving a file to {}", to.display()))?;
    }
    fs::remove_dir(&staging).with_context(|| format!("removing {}", staging.display()))
}

/// Removes the empty folders in and under `dir`.
fn remove_empty(dir: &Path) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            remove_empty(&path)?;
        }
    }
    // Fails, as it should, if the folder still holds anything.
    let _ = fs::remove_dir(dir);
    Ok(())
}

static TEMPLATES_FOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""folder"\s*:\s*"([^"]*)""#).unwrap());

/// Points Obsidian's Templates plugin at the new templates folder, if it used the old one.
fn point_obsidian_templates(root: &Path, old: &str) -> Result<()> {
    let path = root.join(".obsidian").join("templates.json");
    let Ok(settings) = fs::read_to_string(&path) else {
        return Ok(());
    };
    let pointed = TEMPLATES_FOLDER.replace(&settings, |c: &Captures| {
        if c[1].trim_matches('/') == old.trim_matches('/') {
            format!("\"folder\": \"{TEMPLATES_DIR}\"")
        } else {
            c[0].to_string()
        }
    });
    if pointed != settings {
        fs::write(&path, pointed.as_ref())
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// Writes a world's project note and timeline from the built-in templates, lists its book, and
/// lists the book's scenes in its contents, each as a chapter titled as it was.
fn create_world(index: &LinkIndex, world: &NewWorld, today: NaiveDate) -> Result<()> {
    let date = today.format("%Y-%m-%d").to_string();
    let timeline_link = index.link(&world.note, &world.timeline, "Timeline");
    let vars = [
        ("id", world.id.as_str()),
        ("title", world.title.as_str()),
        ("project", world.title.as_str()),
        ("date", date.as_str()),
        ("timeline_link", timeline_link.as_str()),
    ];
    let mut note = render(templates::PROJECT.default, &vars);
    if !world.genre.is_empty() {
        note = set_properties(
            &note,
            &[("genre", PropertyValue::List(world.genre.clone()))],
        );
    }
    let book = format!("- {}", index.link(&world.note, &world.book, &world.title));
    write_new(&world.note, &set_section(&note, BOOKS_HEADING, &[book]))?;
    write_new(&world.timeline, &render(templates::TIMELINE.default, &vars))?;
    let world_dir = world
        .note
        .parent()
        .context("a project note has no folder")?;
    for kind in NoteKind::ALL.into_iter().filter(|kind| kind.is_bible()) {
        let folder = world_dir.join(BIBLE_DIR).join(kind.folder());
        fs::create_dir_all(&folder).with_context(|| format!("creating {}", folder.display()))?;
    }

    let book_dir = world.book.parent().context("a book note has no folder")?;
    let scenes = book_dir.join(NoteKind::Scene.folder());
    fs::create_dir_all(&scenes).with_context(|| format!("creating {}", scenes.display()))?;
    let lines: Vec<String> = world
        .scenes
        .iter()
        .flat_map(|(path, name)| {
            [
                format!("- {name}"),
                format!("  - {}", index.link(&world.book, path, name)),
            ]
        })
        .collect();
    let content = read(&world.book)?;
    fs::write(
        &world.book,
        set_section(&content, contents::HEADING, &lines),
    )
    .with_context(|| format!("writing {}", world.book.display()))
}

/// Rewrites the links that no longer reach the note they meant, so they follow it to where it
/// moved. The ideas file and templates are left as they are.
fn rewrite_links(vault: &Vault, plan: &Plan) -> Result<()> {
    let moved: HashMap<&Path, &Path> = plan
        .moves
        .iter()
        .map(|(from, to)| (from.as_path(), to.as_path()))
        .collect();
    let came_from: HashMap<&Path, &Path> = plan
        .moves
        .iter()
        .map(|(from, to)| (to.as_path(), from.as_path()))
        .collect();
    let index = LinkIndex::build(vault)?;
    let ideas = vault.ideas_path();
    let templates = vault.templates_dir();
    let mut names: HashMap<PathBuf, String> = HashMap::new();
    for path in markdown_files(&vault.root)? {
        if path == ideas || path.starts_with(&templates) {
            continue;
        }
        let was = came_from.get(path.as_path()).copied().unwrap_or(&path);
        let content = read(&path)?;
        let mut relink = |c: &Captures| -> Result<Option<String>> {
            let written = &c[1];
            let (target, rest) =
                written.split_at(written.find(['#', '^']).unwrap_or(written.len()));
            let target = target.trim();
            let Resolution::Found(meant) = plan.index.resolve(was, target) else {
                return Ok(None);
            };
            let now_at = moved
                .get(meant.as_path())
                .map_or(meant, |to| to.to_path_buf());
            // Obsidian follows a bare link by file name only, never by alias.
            let followed = index.resolve(&path, target) == Resolution::Found(now_at.clone())
                && (target.contains('/')
                    || stem(Path::new(target)).eq_ignore_ascii_case(&stem(&now_at)));
            if followed || !is_note(&now_at) {
                return Ok(None);
            }
            let to = format!("{}{rest}", index.link_target(&path, &now_at));
            let embed = content[..c.get(0).map_or(0, |m| m.start())].ends_with('!');
            let text = match c.get(2) {
                Some(text) => text.as_str().to_string(),
                None if embed => return Ok(Some(format!("[[{to}]]"))),
                None => match names.get(&now_at) {
                    Some(name) => name.clone(),
                    None => {
                        let name = Note::load(&now_at)?.title();
                        names.insert(now_at.clone(), name.clone());
                        name
                    }
                },
            };
            Ok(Some(format!("[[{to}|{text}]]")))
        };
        let mut failed = None;
        let rewritten = WIKILINK.replace_all(&content, |c: &Captures| match relink(c) {
            Ok(Some(link)) => link,
            Ok(None) => c[0].to_string(),
            Err(e) => {
                failed.get_or_insert(e);
                c[0].to_string()
            }
        });
        if let Some(e) = failed {
            return Err(e);
        }
        if rewritten != content {
            fs::write(&path, rewritten.as_ref())
                .with_context(|| format!("writing {}", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check;
    use chrono::NaiveDate;
    use tempfile::TempDir;

    fn write(root: &Path, path: &str, content: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn read(root: &Path, path: &str) -> String {
        fs::read_to_string(root.join(path)).unwrap()
    }

    const IDEAS: &str = "# Ideas\n\n### Fiction\n- Sample Story: a premise → [[sample-story]]\n";

    /// A vault as story-time 0.1 left it, with one project.
    fn old_vault() -> TempDir {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir(root.join(".git")).unwrap();
        write(
            root,
            ".obsidian/templates.json",
            "{\n  \"folder\": \"Templates\"\n}\n",
        );
        write(
            root,
            ".story-time/config.toml",
            "daily_goal = 700\nideas_file = \"Scratchpad/Ideas.md\"\ntemplates_dir = \"Templates\"\n",
        );
        write(
            root,
            ".story-time/history.csv",
            "timestamp,project,words\n2026-11-01T10:00:00,sample-story,3\n",
        );
        write(root, "Readme.md", "See [[Ideas]] and [[Mara Okafor]].\n");
        write(root, "Scratchpad/Ideas.md", IDEAS);
        write(root, "Scratchpad/Loose Thought.md", "Kept for later.\n");
        for (name, default) in OLD_TEMPLATES {
            write(root, &format!("Templates/{name}"), default);
        }
        write(
            root,
            "Templates/Character.md",
            "---\ntype: character\n---\n# Changed\n",
        );
        write(root, "Templates/Daily Note.md", "# {{date}}\n");
        write(
            root,
            "sample-story/sample-story.md",
            "---\ntype: project\ntitle: \"Sample Story\"\nstatus: drafting\ngenre: space opera\ntarget: 90000\n---\n# Sample Story\n\n- [[sample-story-flow|Story flow]]\n",
        );
        write(
            root,
            "sample-story/sample-story-flow.md",
            "---\ntype: flow\nproject: \"[[sample-story]]\"\n---\n- [[01 Arrival]], then [[02 The Long Night|night]]\n",
        );
        write(
            root,
            "sample-story/chapters/01 Arrival.md",
            "---\ntype: chapter\nproject: \"[[sample-story]]\"\nstatus: draft\npov: \"[[Mara Okafor]]\"\n---\n# Arrival\n\nShe landed.\n",
        );
        write(
            root,
            "sample-story/chapters/02 The Long Night.md",
            "---\ntype: chapter\n---\n# The Long Night\n\nSee [[Mara Okafor#Voice]] and ![[map.png]].\n",
        );
        write(
            root,
            "sample-story/chapters/10 Epilogue.md",
            "# Epilogue\n\nThe end.\n",
        );
        write(
            root,
            "sample-story/characters/Mara Okafor.md",
            "---\ntype: character\nproject: \"[[sample-story]]\"\naliases:\n  - Mara\n---\n# Mara Okafor\n\n## Voice\n",
        );
        write(root, "sample-story/locations/map.png", "not really a png");
        dir
    }

    #[test]
    fn upgrades_a_vault_from_story_time_0_1() {
        let dir = old_vault();
        let root = dir.path();
        let vault = Vault::open(root).unwrap();
        let now = NaiveDate::from_ymd_opt(2026, 11, 6)
            .unwrap()
            .and_hms_opt(15, 30, 0)
            .unwrap();
        let plan = plan(&vault).unwrap();
        assert_eq!(plan.replaced.len(), 5, "every template but the changed one");
        assert_eq!(plan.worlds.len(), 1);
        let backup = apply(&vault, &plan, now).unwrap();
        let root = &vault.root;

        // Backed up as it was.
        assert_eq!(
            fs::read_to_string(backup.join("Scratchpad/Ideas.md")).unwrap(),
            IDEAS
        );
        assert!(backup.join("Templates/Project.md").is_file());

        // The vault's own files.
        assert_eq!(read(root, "scratchpad/ideas.md"), IDEAS, "moved as it was");
        assert!(!root.join("Scratchpad").exists() && !root.join("Templates").exists());
        assert_eq!(
            read(root, "scratchpad/loose-thought.md"),
            "---\ntype: scrap\naliases:\n  - Loose Thought\n---\nKept for later.\n"
        );
        assert_eq!(
            read(root, "templates/project.md"),
            templates::PROJECT.default
        );
        assert_eq!(
            read(root, "templates/old-character.md"),
            "---\ntype: character\n---\n# Changed\n"
        );
        assert_eq!(read(root, "templates/daily-note.md"), "# {{date}}\n");
        assert!(read(root, ".obsidian/templates.json").contains("\"folder\": \"templates\""));
        assert!(read(root, ".gitignore").contains(BACKUPS_IGNORE));
        assert_eq!(
            read(root, "Readme.md"),
            "See [[Ideas]] and [[mara-okafor|Mara Okafor]].\n"
        );
        let vault = Vault::open(root).unwrap();
        assert_eq!(vault.config.format, 1);
        assert_eq!(vault.config.daily_goal, 700);

        // The project, now a world with one book.
        let projects = vault.projects().unwrap();
        let [project] = projects.as_slice() else {
            panic!("one project: {projects:?}")
        };
        assert_eq!(project.slug, "sample-story-world");
        assert_eq!(project.title(), "Sample Story");
        assert_eq!(project.genre(), ["space opera"]);
        assert!(project.timeline_path().is_file());
        let books = project.books().unwrap();
        let [book] = books.as_slice() else {
            panic!("one book: {books:?}")
        };
        assert_eq!(book.slug, "sample-story");
        assert_eq!(book.status().as_deref(), Some("drafting"));
        assert_eq!(book.target(), Some(90000));
        assert!(book.id().is_some());
        assert!(book.note.frontmatter.str("genre").is_none());
        let contents = book.contents().unwrap();
        let chapters: Vec<String> = contents.chapters().map(|c| c.heading()).collect();
        assert_eq!(chapters, ["Arrival", "The Long Night", "Epilogue"]);
        assert_eq!(
            read(
                root,
                "sample-story-world/books/sample-story/sample-story-flow.md"
            ),
            "---\ntype: flow\n---\n- [[arrival|Arrival]], then [[the-long-night|night]]\n"
        );
        assert_eq!(
            read(
                root,
                "sample-story-world/books/sample-story/scenes/arrival.md"
            ),
            "---\ntype: scene\nstatus: draft\npov: \"[[mara-okafor|Mara Okafor]]\"\n---\n# Arrival\n\nShe landed.\n"
        );
        assert_eq!(
            read(
                root,
                "sample-story-world/books/sample-story/scenes/the-long-night.md"
            ),
            "---\ntype: scene\naliases:\n  - The Long Night\n---\n# The Long Night\n\nSee [[mara-okafor#Voice|Mara Okafor]] and ![[map.png]].\n"
        );
        assert_eq!(
            read(root, "sample-story-world/bible/characters/mara-okafor.md"),
            "---\ntype: character\naliases:\n  - Mara Okafor\n  - Mara\n---\n# Mara Okafor\n\n## Voice\n"
        );
        assert!(
            root.join("sample-story-world/bible/locations/map.png")
                .is_file()
        );
        assert!(!root.join("sample-story").exists());
        let report = check::check(&vault, project).unwrap();
        assert!(report.undocumented.is_empty(), "{:?}", report.undocumented);
        assert!(report.alias_links.is_empty(), "{:?}", report.alias_links);
        assert!(
            report.broken_entries.is_empty(),
            "{:?}",
            report.broken_entries
        );
        assert!(report.misnamed.is_empty(), "{:?}", report.misnamed);
        assert!(report.drifted.is_empty(), "{:?}", report.drifted);

        // The writing log: a last count under the old key, and starting ones under the new.
        let log = read(root, ".story-time/history.csv");
        let rows: Vec<&str> = log.lines().skip(2).collect();
        let book_id = book.id().unwrap();
        assert_eq!(
            rows,
            [
                "2026-11-06T15:30:00,sample-story,8".to_string(),
                format!("2026-11-06T15:30:00,{book_id},8"),
                "2026-11-06T15:30:00,scratchpad,8".to_string(),
                format!("2026-11-06T15:30:00,{},0", project.log_key()),
            ]
        );
        let history = History::load(&vault.history_path()).unwrap();
        assert_eq!(
            history.written_on(now.date(), None),
            5,
            "words written since the last count, and none for the move"
        );

        let message = super::plan(&vault).unwrap_err().to_string();
        assert!(message.contains("already in format 1"), "{message}");
    }

    #[test]
    fn stops_before_a_clash() {
        let dir = old_vault();
        write(dir.path(), "templates/daily-note.md", "Another template.\n");
        let vault = Vault::open(dir.path()).unwrap();
        let message = plan(&vault).unwrap_err().to_string();
        assert!(
            message.contains("templates/daily-note.md is in the way"),
            "{message}"
        );
    }
}
