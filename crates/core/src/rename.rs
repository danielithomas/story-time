//! Renaming a note: its name, its file name and every link to it change together, as the format
//! asks. Scenes, world-bible notes (characters, locations and world notes) and research can be
//! renamed; the rest are named after their project or book, or by when they were captured.
//!
//! A note's name is its first alias (or its `title`, if it has one). The file takes the name's
//! slug, numbered if another note in the project has it, as when a note is added. Every link in
//! the bookshelf that meant the note is rewritten to reach it, as a link Obsidian can follow; a
//! link that showed the old name shows the new one, and other link text is kept. A book's
//! contents list is links, so renaming a scene keeps its place. The ideas file and templates are
//! never edited.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use regex::Captures;

use crate::links::{LinkIndex, Resolution};
use crate::note::{
    Note, PropertyValue, replace_body, set_properties, split_frontmatter, write_atomically,
};
use crate::project::{Book, NoteKind, Project};
use crate::scaffold::{project_stems, stems};
use crate::scratchpad;
use crate::text::{name_from_file, slugify, unique_slug, yaml_escape};
use crate::vault::{Vault, markdown_files};
use crate::words::WIKILINK;

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
}

/// Where a note that can be renamed belongs, which decides the names it can't share.
enum Home {
    Scene {
        project: Project,
        book: Box<Book>,
    },
    Bible {
        project: Project,
        kind: NoteKind,
    },
    Research {
        project: Option<Project>,
        dir: PathBuf,
    },
}

impl Home {
    fn find(vault: &Vault, path: &Path) -> Result<Home> {
        if path.starts_with(vault.research_dir()) {
            let dir = vault.research_dir();
            return Ok(Home::Research { project: None, dir });
        }
        let project = vault
            .projects()?
            .into_iter()
            .find(|project| path.starts_with(&project.dir));
        if let Some(project) = project {
            if path.starts_with(project.research_dir()) {
                let dir = project.research_dir();
                return Ok(Home::Research {
                    project: Some(project),
                    dir,
                });
            }
            let kind = [NoteKind::Character, NoteKind::Location, NoteKind::World]
                .into_iter()
                .find(|kind| path.starts_with(project.folder(*kind)));
            if let Some(kind) = kind {
                return Ok(Home::Bible { project, kind });
            }
            let book = project
                .books()?
                .into_iter()
                .find(|book| path.starts_with(book.scenes_dir()));
            if let Some(book) = book {
                return Ok(Home::Scene {
                    project,
                    book: Box::new(book),
                });
            }
        }
        bail!(
            "only scenes, characters, locations, world notes and research can be renamed; {} \
             isn't one",
            vault.relative(path).display()
        )
    }

    /// The notes a new name can't be shared with, who they belong to, and what they are.
    fn siblings(&self) -> Result<(Vec<Note>, String, &'static str)> {
        Ok(match self {
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
        })
    }

    /// File names already taken where the note's file name must be unique.
    fn taken(&self) -> Result<std::collections::HashSet<String>> {
        match self {
            Home::Scene { project, book } => project_stems(project, Some(book)),
            Home::Bible { project, .. } => project_stems(project, None),
            Home::Research {
                project: Some(project),
                ..
            } => project_stems(project, None),
            Home::Research { project: None, dir } => stems(dir),
        }
    }
}

/// Renames the note at `path` to `name`, or with `None`, keeps its name and gives its file the
/// name's slug, which is what `check` suggests for a file name that has drifted from its name.
/// Refuses a name that another note of the same kind already has where it belongs: in the same
/// book for a scene, else in the same project (or the bookshelf's research).
pub fn rename(vault: &Vault, path: &Path, name: Option<&str>) -> Result<Renamed> {
    let note = Note::load(path)?;
    let home = Home::find(vault, path)?;
    let old_name = note.title();
    let name = name
        .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| old_name.clone());
    let slug = slugify(&name);
    if slug.is_empty() {
        bail!("the name needs at least one letter or number");
    }
    if name.contains(['[', ']', '|']) {
        bail!("a name can't contain [, ] or |, which would break links to it");
    }
    let (siblings, owner, what) = home.siblings()?;
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
        let mut taken = home.taken()?;
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
    let after = LinkIndex::build(vault)?;
    // Link text showing the old name, or the name the file was given, shows the new one.
    let old_names = [old_name.clone(), name_from_file(&note.name)];
    let relinker = Relinker {
        before: &before,
        after: &after,
        old_path: path,
        new_path: &new_path,
        old_names: &old_names,
        name: &name,
    };
    let (mut links, mut notes) = (0, 0);
    let ideas = vault.ideas_path();
    let templates = vault.templates_dir();
    for file in markdown_files(&vault.root)? {
        if file == ideas || file.starts_with(&templates) {
            continue;
        }
        let content =
            fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
        let (rewritten, count) = relinker.relink(&file, &content);
        if count > 0 {
            write_atomically(&file, &rewritten)?;
            links += count;
            notes += 1;
        }
    }
    let same_name = vault.note_named(&slug, &new_path)?;
    Ok(renamed(links, notes, same_name))
}

/// Whether the note at `path` is one that can be renamed: a scene, a world-bible note or research.
pub fn can_rename(vault: &Vault, path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "md") && Home::find(vault, path).is_ok()
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
        if matches && Home::find(vault, &path).is_ok() {
            found.push(path);
        }
    }
    match found.len() {
        0 => bail!("no scene, character, location, world note or research is called {wanted}"),
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
    let (_, body) = split_frontmatter(&content);
    let mut lines: Vec<&str> = body.lines().collect();
    let heading = format!("# {name}");
    match lines.iter().position(|line| !line.trim().is_empty()) {
        Some(i) if lines[i].trim_end() == format!("# {old_name}") && old_name != name => {
            lines[i] = &heading;
            let ending = if body.ends_with('\n') { "\n" } else { "" };
            replace_body(&content, &format!("{}{ending}", lines.join("\n")))
        }
        _ => content,
    }
}

/// Rewrites the links that meant a renamed note.
struct Relinker<'a> {
    before: &'a LinkIndex,
    after: &'a LinkIndex,
    old_path: &'a Path,
    new_path: &'a Path,
    old_names: &'a [String],
    name: &'a str,
}

impl Relinker<'_> {
    /// `content`, the text of the note at `file`, with every link that meant the renamed note
    /// rewritten, and how many there were.
    fn relink(&self, file: &Path, content: &str) -> (String, usize) {
        // The renamed note's own links are resolved from where it was.
        let was = if file == self.new_path {
            self.old_path
        } else {
            file
        };
        let frontmatter_end = match split_frontmatter(content) {
            (Some(yaml), _) => 4 + yaml.len(),
            (None, _) => 0,
        };
        let mut count = 0;
        let rewritten = WIKILINK.replace_all(content, |c: &Captures| {
            let whole = c.get(0).expect("a match");
            let written = &c[1];
            let (target, rest) =
                written.split_at(written.find(['#', '^']).unwrap_or(written.len()));
            let meant = self.before.resolve(was, target.trim());
            if meant != Resolution::Found(self.old_path.to_path_buf()) {
                return whole.as_str().to_string();
            }
            count += 1;
            let to = format!("{}{rest}", self.after.link_target(file, self.new_path));
            let embed = content[..whole.start()].ends_with('!');
            let text = match c.get(2).map(|text| text.as_str()) {
                Some(text)
                    if self
                        .old_names
                        .iter()
                        .any(|old| text.trim().eq_ignore_ascii_case(old)) =>
                {
                    Some(self.name.to_string())
                }
                Some(text) => Some(text.to_string()),
                None if embed || to == self.name => None,
                None => Some(self.name.to_string()),
            };
            let link = match text {
                Some(text) => format!("[[{to}|{text}]]"),
                None => format!("[[{to}]]"),
            };
            if whole.start() < frontmatter_end {
                let line_start = content[..whole.start()].rfind('\n').map_or(0, |i| i + 1);
                match quoting(&content[line_start..whole.start()]) {
                    Some('"') => return yaml_escape(&link),
                    Some(_) => return link.replace('\'', "''"),
                    None => {}
                }
            }
            link
        });
        (rewritten.into_owned(), count)
    }
}

/// The quote a YAML line is inside at the end of `prefix`, if any: `"` or `'`.
fn quoting(prefix: &str) -> Option<char> {
    let mut quote = None;
    let mut last = ' ';
    let mut chars = prefix.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('"'), '\\') => {
                chars.next();
            }
            (Some('"'), '"') => quote = None,
            (Some('\''), '\'') => {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                } else {
                    quote = None;
                }
            }
            // A quote opens a scalar only at its start.
            (None, '"' | '\'') if matches!(last, ' ' | ':' | '-' | '[' | ',') => {
                quote = Some(c);
            }
            _ => {}
        }
        if !c.is_whitespace() || quote.is_some() {
            last = c;
        } else {
            last = ' ';
        }
    }
    quote
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check;
    use crate::contents::{self, Place};
    use crate::project::Form;
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
            &s.book.note.path,
            &s.project.note.path,
            &s.vault.ideas_path(),
        ] {
            assert!(rename(&s.vault, path, Some("Anything")).is_err());
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

    #[test]
    fn finds_the_quote_a_property_is_in() {
        assert_eq!(quoting("pov: \""), Some('"'));
        assert_eq!(quoting("  - '"), Some('\''));
        assert_eq!(quoting("synopsis: Mara's "), None);
        assert_eq!(quoting("pov: \"[[a]] and \\\" "), Some('"'));
        assert_eq!(quoting("pov: 'it''s "), Some('\''));
        assert_eq!(quoting("pov: \"done\" "), None);
    }
}
