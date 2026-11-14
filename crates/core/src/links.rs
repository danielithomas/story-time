//! Wikilinks: reading them, finding the note a link means, and writing links that both
//! story-time and Obsidian follow.
//!
//! story-time resolves a bare link such as `[[mara-okafor]]` to the nearest note with that file
//! name: in the linking note's own folder and the folders above it, so a book's notes come before
//! the rest of its project. From inside a project it never reaches into another project. It also
//! accepts a note's alias in place of its file name, after file names at the same distance.
//!
//! Obsidian looks bare names up across the whole vault and doesn't follow aliases, so a link to a
//! name that several notes share is written with a path relative to the linking note instead.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use regex::Captures;

use crate::note::{Note, split_frontmatter, write_atomically};
use crate::text::yaml_escape;
use crate::vault::{Vault, markdown_files};
use crate::words::{WIKILINK, strip_comments};

/// A `[[target|text]]` link as written, without any `#heading` or `^block` part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub target: String,
    pub text: Option<String>,
}

impl Link {
    /// Whether the link names a note without a path, as Obsidian's "shortest path" links do.
    pub fn is_bare(&self) -> bool {
        !self.target.contains('/')
    }
}

/// The links in a note's text, leaving out any inside `%% comments %%` or HTML comments.
pub fn links(text: &str) -> Vec<Link> {
    WIKILINK
        .captures_iter(&strip_comments(text))
        .filter_map(|c| {
            let target = c[1].split(['#', '^']).next().unwrap_or_default().trim();
            (!target.is_empty()).then(|| Link {
                target: target.to_string(),
                text: c.get(2).map(|text| text.as_str().trim().to_string()),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The note or file the link means.
    Found(PathBuf),
    /// Several notes at the same distance have the name.
    Ambiguous(Vec<PathBuf>),
    Missing,
}

/// Every note and file in a vault, indexed for resolving links.
#[derive(Debug, Default)]
pub struct LinkIndex {
    root: PathBuf,
    /// Lower-cased note file names (without `.md`) and the notes with them.
    stems: HashMap<String, Vec<PathBuf>>,
    /// Lower-cased aliases and the notes with them.
    aliases: HashMap<String, Vec<PathBuf>>,
    /// Lower-cased names of other files (images, PDFs) and their paths.
    files: HashMap<String, Vec<PathBuf>>,
    projects: Vec<PathBuf>,
}

impl LinkIndex {
    pub fn build(vault: &Vault) -> Result<LinkIndex> {
        let mut index = LinkIndex {
            root: vault.root.clone(),
            projects: vault.projects()?.into_iter().map(|p| p.dir).collect(),
            ..LinkIndex::default()
        };
        for path in vault.files()? {
            let is_note = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
            if is_note {
                let note = Note::load(&path)?;
                for alias in note.aliases() {
                    index
                        .aliases
                        .entry(alias.to_lowercase())
                        .or_default()
                        .push(path.clone());
                }
                index
                    .stems
                    .entry(note.name.to_lowercase())
                    .or_default()
                    .push(path);
            } else if let Some(name) = path.file_name() {
                let name = name.to_string_lossy().to_lowercase();
                index.files.entry(name).or_default().push(path);
            }
        }
        Ok(index)
    }

    /// How many notes have this file name anywhere in the vault: what Obsidian sees for a bare
    /// link. One means Obsidian opens the right note; none or several, that it can't.
    pub fn notes_named(&self, name: &str) -> usize {
        self.stems.get(&name.to_lowercase()).map_or(0, Vec::len)
    }

    /// The note or file that `target`, linked from the note at `from`, means to story-time.
    pub fn resolve(&self, from: &Path, target: &str) -> Resolution {
        let target = target.trim();
        if target.contains('/') {
            return self.resolve_path(from, target);
        }
        let lower = target.to_lowercase();
        let has_other_extension = Path::new(&lower).extension().is_some_and(|ext| ext != "md");
        if has_other_extension {
            return match self.files.get(&lower).and_then(|paths| paths.first()) {
                Some(path) => Resolution::Found(path.clone()),
                None => Resolution::Missing,
            };
        }
        let name = lower.strip_suffix(".md").unwrap_or(&lower);
        let by_stem = self.stems.get(name).map(Vec::as_slice).unwrap_or_default();
        let by_alias = self
            .aliases
            .get(name)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let home = self.project_of(from);
        let mut scope = from.parent();
        while let Some(dir) = scope.filter(|dir| dir.starts_with(&self.root)) {
            let at_root = dir == self.root;
            let in_scope = |path: &&PathBuf| {
                path.starts_with(dir)
                    && !(at_root && home.is_some() && self.project_of(path).is_some())
            };
            for candidates in [by_stem, by_alias] {
                let mut found: Vec<PathBuf> = candidates.iter().filter(in_scope).cloned().collect();
                found.dedup();
                match found.len() {
                    0 => {}
                    1 => return Resolution::Found(found.remove(0)),
                    _ => return Resolution::Ambiguous(found),
                }
            }
            scope = dir.parent();
        }
        Resolution::Missing
    }

    /// A path link: relative to the linking note's folder, else from the vault's root.
    fn resolve_path(&self, from: &Path, target: &str) -> Resolution {
        let relative = from.parent().map(|dir| normalise(&dir.join(target)));
        let absolute = normalise(&self.root.join(target.trim_start_matches('/')));
        for base in relative.into_iter().chain([absolute]) {
            let mut note = base.clone().into_os_string();
            note.push(".md");
            for path in [PathBuf::from(note), base] {
                if self.contains(&path) {
                    return Resolution::Found(path);
                }
            }
        }
        Resolution::Missing
    }

    fn contains(&self, path: &Path) -> bool {
        let named = |map: &HashMap<String, Vec<PathBuf>>, key: Option<String>| {
            key.and_then(|key| map.get(&key))
                .is_some_and(|paths| paths.iter().any(|p| p == path))
        };
        let lower = |s: Option<&OsStr>| s.map(|s| s.to_string_lossy().to_lowercase());
        let is_note = path.extension().is_some_and(|ext| ext == "md");
        if is_note {
            named(&self.stems, lower(path.file_stem()))
        } else {
            named(&self.files, lower(path.file_name()))
        }
    }

    /// The project folder `path` is inside, if any.
    fn project_of(&self, path: &Path) -> Option<&Path> {
        self.projects
            .iter()
            .find(|dir| path.starts_with(dir))
            .map(PathBuf::as_path)
    }

    /// A link from the note at `from` to the note at `to`, showing `name`. A bare link when no
    /// other note in the vault shares the file name, else a path relative to `from`'s folder, so
    /// Obsidian opens the right note either way.
    pub fn link(&self, from: &Path, to: &Path, name: &str) -> String {
        let target = self.link_target(from, to);
        if name == target {
            format!("[[{target}]]")
        } else {
            format!("[[{target}|{name}]]")
        }
    }

    /// The part of such a link before any `|`: the file name, or a relative path when other notes
    /// share it.
    pub fn link_target(&self, from: &Path, to: &Path) -> String {
        let stem = to
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let others = self
            .stems
            .get(&stem.to_lowercase())
            .map_or(0, |paths| paths.iter().filter(|path| *path != to).count());
        match (others, from.parent()) {
            (1.., Some(dir)) => relative_path(dir, &to.with_extension("")),
            _ => stem,
        }
    }
}

/// After the note at `from` has moved to `to` (been renamed, or moved to another folder), rewrites
/// the links that no longer reach what they meant: every link to the note, and the note's own
/// links that don't reach their notes from where it is now. `before` indexes the bookshelf as it
/// was. Link text showing one of `old_names` shows `name`; other text is kept, and a link that
/// still works is left as written. The ideas file and templates are never edited. Returns how many
/// links were rewritten, in how many notes.
pub(crate) fn relink(
    vault: &Vault,
    before: &LinkIndex,
    from: &Path,
    to: &Path,
    old_names: &[String],
    name: &str,
) -> Result<(usize, usize)> {
    let relinker = Relinker {
        before,
        after: &LinkIndex::build(vault)?,
        from,
        to,
        old_names,
        name,
        titles: HashMap::new(),
    };
    let (mut links, mut notes) = (0, 0);
    let ideas = vault.ideas_path();
    let templates = vault.templates_dir();
    let mut relinker = relinker;
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
    Ok((links, notes))
}

struct Relinker<'a> {
    before: &'a LinkIndex,
    after: &'a LinkIndex,
    from: &'a Path,
    to: &'a Path,
    old_names: &'a [String],
    name: &'a str,
    /// Names of the notes the moved note links to, read as needed.
    titles: HashMap<PathBuf, Option<String>>,
}

impl Relinker<'_> {
    /// `content`, the text of the note at `file`, with its links rewritten, and how many were.
    fn relink(&mut self, file: &Path, content: &str) -> (String, usize) {
        let moved = file == self.to;
        // The moved note's links are resolved from where it was.
        let was = if moved { self.from } else { file };
        let frontmatter_end = match split_frontmatter(content) {
            (Some(yaml), _) => 4 + yaml.len(),
            (None, _) => 0,
        };
        let mut count = 0;
        let rewritten = WIKILINK.replace_all(content, |c: &Captures| {
            let whole = c.get(0).expect("a match").as_str();
            let start = c.get(0).expect("a match").start();
            let written = &c[1];
            let (target, rest) =
                written.split_at(written.find(['#', '^']).unwrap_or(written.len()));
            let target = target.trim();
            let Resolution::Found(meant) = self.before.resolve(was, target) else {
                return whole.to_string();
            };
            let to_moved = meant == self.from;
            let now = if to_moved {
                self.to.to_path_buf()
            } else {
                meant
            };
            let is_note = now.extension().is_some_and(|ext| ext == "md");
            if !is_note || !(to_moved || moved) {
                return whole.to_string();
            }
            // Obsidian follows a bare link by file name only, never by alias.
            let still_reaches = self.after.resolve(file, target) == Resolution::Found(now.clone())
                && (target.contains('/')
                    || now
                        .file_stem()
                        .is_some_and(|stem| stem.to_string_lossy().eq_ignore_ascii_case(target)));
            let new_target = if still_reaches {
                target.to_string()
            } else {
                self.after.link_target(file, &now)
            };
            let retargeted = new_target != target;
            let embed = content[..start].ends_with('!');
            let text = match c.get(2).map(|text| text.as_str()) {
                Some(text)
                    if to_moved
                        && self
                            .old_names
                            .iter()
                            .any(|old| text.trim().eq_ignore_ascii_case(old)) =>
                {
                    Some(self.name.to_string())
                }
                Some(text) => Some(text.to_string()),
                None if embed || !retargeted => None,
                // A retargeted link shows the note's name, unless the target already does.
                None => {
                    let shown = if to_moved {
                        Some(self.name.to_string())
                    } else {
                        self.title(&now)
                    };
                    shown.filter(|shown| *shown != new_target)
                }
            };
            let to = format!("{new_target}{rest}");
            let mut link = match text {
                Some(text) => format!("[[{to}|{text}]]"),
                None => format!("[[{to}]]"),
            };
            if start < frontmatter_end {
                let line_start = content[..start].rfind('\n').map_or(0, |i| i + 1);
                match quoting(&content[line_start..start]) {
                    Some('"') => link = yaml_escape(&link),
                    Some(_) => link = link.replace('\'', "''"),
                    None => {}
                }
            }
            if link == whole {
                return link;
            }
            count += 1;
            link
        });
        (rewritten.into_owned(), count)
    }

    fn title(&mut self, path: &Path) -> Option<String> {
        self.titles
            .entry(path.to_path_buf())
            .or_insert_with(|| Note::load(path).ok().map(|note| note.title()))
            .clone()
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

/// Removes `.` and `..` from a path without touching the filesystem.
pub(crate) fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// `to` written relative to the folder `from`, with forward slashes: `../characters/mara`.
fn relative_path(from: &Path, to: &Path) -> String {
    let from: Vec<Component> = from.components().collect();
    let to: Vec<Component> = to.components().collect();
    let shared = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let ups = std::iter::repeat_n("..".to_string(), from.len() - shared);
    let downs = to[shared..]
        .iter()
        .map(|c| c.as_os_str().to_string_lossy().into_owned());
    ups.chain(downs).collect::<Vec<_>>().join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(root: &Path, path: &str, content: &str) -> PathBuf {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        path
    }

    /// Two worlds that each have a Mara, and a book whose scene shares a name with another's.
    fn vault() -> (TempDir, Vault) {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::create_dir(root.join(".obsidian")).unwrap();
        for project in ["world-a", "world-b"] {
            write(
                root,
                &format!("{project}/{project}.md"),
                "---\ntype: project\n---\n",
            );
            write(
                root,
                &format!("{project}/characters/mara.md"),
                "---\naliases:\n  - Mara Okafor\n---\n",
            );
        }
        write(root, "world-a/books/one/scenes/morning.md", "");
        write(root, "world-a/books/two/scenes/morning.md", "");
        write(root, "world-a/books/two/scenes/harbour.md", "");
        write(root, "world-a/world/map.png", "");
        write(root, "world-b/locations/dock.md", "");
        write(root, "scratchpad/ideas.md", "");
        let vault = Vault::open(root).unwrap();
        (dir, vault)
    }

    #[test]
    fn reads_links() {
        let found = links(
            "See [[mara|Mara]], [[harbour#Docks]] and ![[map.png]]. %% not [[this]] %% [[#Top]]",
        );
        assert_eq!(
            found,
            [
                Link {
                    target: "mara".into(),
                    text: Some("Mara".into())
                },
                Link {
                    target: "harbour".into(),
                    text: None
                },
                Link {
                    target: "map.png".into(),
                    text: None
                },
            ]
        );
    }

    #[test]
    fn resolves_to_the_nearest_note() {
        let (dir, vault) = vault();
        let root = vault.root.clone();
        let index = LinkIndex::build(&vault).unwrap();
        let in_book_two = root.join("world-a/books/two/scenes/harbour.md");
        let found = |from: &Path, target: &str| index.resolve(from, target);

        assert_eq!(
            found(&in_book_two, "morning"),
            Resolution::Found(root.join("world-a/books/two/scenes/morning.md")),
            "the book's own scene"
        );
        assert_eq!(
            found(&in_book_two, "Mara"),
            Resolution::Found(root.join("world-a/characters/mara.md")),
            "this world's Mara, never the other world's"
        );
        assert_eq!(
            found(&in_book_two, "Mara Okafor"),
            Resolution::Found(root.join("world-a/characters/mara.md")),
            "by alias"
        );
        assert!(matches!(
            found(&root.join("world-a/world-a.md"), "morning"),
            Resolution::Ambiguous(paths) if paths.len() == 2
        ));
        assert_eq!(
            found(&in_book_two, "ideas"),
            Resolution::Found(root.join("scratchpad/ideas.md")),
            "then the vault outside any project"
        );
        assert_eq!(
            found(&in_book_two, "dock"),
            Resolution::Missing,
            "but never another project"
        );
        assert!(matches!(
            found(&root.join("scratchpad/ideas.md"), "mara"),
            Resolution::Ambiguous(_)
        ));
        assert_eq!(
            found(&root.join("scratchpad/ideas.md"), "world-b"),
            Resolution::Found(root.join("world-b/world-b.md")),
            "notes outside projects can link into them"
        );
        assert_eq!(
            found(&in_book_two, "map.png"),
            Resolution::Found(root.join("world-a/world/map.png"))
        );
        assert_eq!(
            found(&in_book_two, "../../../characters/mara"),
            Resolution::Found(root.join("world-a/characters/mara.md"))
        );
        assert_eq!(
            found(&root.join("world-a/world-a.md"), "books/one/scenes/morning"),
            Resolution::Found(root.join("world-a/books/one/scenes/morning.md"))
        );
        assert_eq!(
            found(&in_book_two, "world-b/characters/mara"),
            Resolution::Found(root.join("world-b/characters/mara.md")),
            "a path from the vault's root"
        );
        assert_eq!(found(&in_book_two, "nobody"), Resolution::Missing);
        assert_eq!(index.notes_named("MARA"), 2);
        assert_eq!(index.notes_named("harbour"), 1);
        drop(dir);
    }

    #[test]
    fn writes_links_obsidian_can_follow() {
        let (_dir, vault) = vault();
        let root = vault.root.clone();
        let index = LinkIndex::build(&vault).unwrap();
        let scene = root.join("world-a/books/two/scenes/harbour.md");
        let book = root.join("world-a/books/two/two.md");
        assert_eq!(
            index.link(&scene, &root.join("world-a/characters/mara.md"), "Mara"),
            "[[../../../characters/mara|Mara]]",
            "two worlds have a mara"
        );
        assert_eq!(
            index.link(
                &book,
                &root.join("world-a/books/two/scenes/morning.md"),
                "Morning"
            ),
            "[[scenes/morning|Morning]]"
        );
        assert_eq!(
            index.link(&book, &scene, "Harbour"),
            "[[harbour|Harbour]]",
            "the only harbour"
        );
        assert_eq!(index.link(&book, &scene, "harbour"), "[[harbour]]");
        for (from, to) in [
            (&scene, "world-a/characters/mara.md"),
            (&book, "world-a/books/two/scenes/morning.md"),
        ] {
            let link = index.link(from, &root.join(to), "Name");
            let target = &links(&link)[0].target;
            assert_eq!(
                index.resolve(from, target),
                Resolution::Found(root.join(to))
            );
        }
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
