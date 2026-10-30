//! Finding the vault and the projects in it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::NaiveDateTime;

use crate::config::{self, Config, META_DIR};
use crate::history::History;
use crate::project::{Chapter, Project};
use crate::text::{natural_cmp, slugify};

#[derive(Debug, Clone)]
pub struct Vault {
    pub root: PathBuf,
    pub config: Config,
}

impl Vault {
    pub fn open(root: impl AsRef<Path>) -> Result<Vault> {
        let root = root.as_ref();
        let root = root
            .canonicalize()
            .with_context(|| format!("vault folder {} not found", root.display()))?;
        if !root.is_dir() {
            bail!("{} is not a folder", root.display());
        }
        let config = Config::load(&root)?;
        Ok(Vault { root, config })
    }

    /// Finds the vault: an explicit path wins; then the nearest folder at or above `cwd` that
    /// holds `.story-time` or `.obsidian`; then the default in the user config file.
    pub fn discover(explicit: Option<&Path>, cwd: &Path) -> Result<Vault> {
        if let Some(path) = explicit {
            return Vault::open(path);
        }
        if let Some(dir) = cwd
            .ancestors()
            .find(|dir| dir.join(META_DIR).is_dir() || dir.join(".obsidian").is_dir())
        {
            return Vault::open(dir);
        }
        if let Some(path) = config::user_default_vault()? {
            return Vault::open(path);
        }
        let user_config = config::user_config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        bail!(
            "no vault found. Run this inside your stories folder, pass --vault, set STORY_TIME_VAULT, \
             or put `vault = \"/path/to/stories\"` in {user_config}"
        )
    }

    pub fn meta_dir(&self) -> PathBuf {
        self.root.join(META_DIR)
    }

    pub fn history_path(&self) -> PathBuf {
        self.meta_dir().join("history.csv")
    }

    pub fn build_dir(&self) -> PathBuf {
        self.meta_dir().join("build")
    }

    pub fn ideas_path(&self) -> PathBuf {
        self.root.join(&self.config.ideas_file)
    }

    pub fn templates_dir(&self) -> PathBuf {
        self.root.join(&self.config.templates_dir)
    }

    /// A path relative to the vault, for display.
    pub fn relative<'a>(&self, path: &'a Path) -> &'a Path {
        path.strip_prefix(&self.root).unwrap_or(path)
    }

    /// Every folder at the vault root that holds a note of the same name, sorted by title.
    pub fn projects(&self) -> Result<Vec<Project>> {
        let mut projects = Vec::new();
        for entry in
            fs::read_dir(&self.root).with_context(|| format!("reading {}", self.root.display()))?
        {
            let path = entry?.path();
            if path.is_dir() && !is_hidden(&path) {
                projects.extend(Project::load(&path)?);
            }
        }
        projects.sort_by(|a, b| natural_cmp(&a.title(), &b.title()));
        Ok(projects)
    }

    /// Picks a project by slug, title, or an unambiguous part of either. With no query, uses the
    /// project `cwd` is inside, or the only project in the vault.
    pub fn find_project(&self, query: Option<&str>, cwd: &Path) -> Result<Project> {
        let projects = self.projects()?;
        if projects.is_empty() {
            bail!("there are no projects yet; start one with `story-time new \"Title\"`");
        }
        let names = || {
            projects
                .iter()
                .map(|p| p.slug.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let Some(query) = query else {
            if let Some(project) = projects.iter().find(|p| cwd.starts_with(&p.dir)) {
                return Ok(project.clone());
            }
            if let [only] = projects.as_slice() {
                return Ok(only.clone());
            }
            bail!("which project? Name one of: {}", names());
        };
        let wanted_slug = slugify(query);
        let wanted_title = query.to_lowercase();
        if let Some(project) = projects
            .iter()
            .find(|p| p.slug == wanted_slug || p.title().to_lowercase() == wanted_title)
        {
            return Ok(project.clone());
        }
        let matches: Vec<&Project> = projects
            .iter()
            .filter(|p| {
                p.slug.contains(&wanted_slug) || p.title().to_lowercase().contains(&wanted_title)
            })
            .collect();
        match matches.as_slice() {
            [only] => Ok((*only).clone()),
            [] => bail!("no project matches \"{query}\". Projects: {}", names()),
            several => bail!(
                "\"{query}\" matches several projects: {}",
                several
                    .iter()
                    .map(|p| p.slug.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    /// Every project with its chapters and word count.
    pub fn measure(&self) -> Result<Vec<Measured>> {
        self.projects()?
            .into_iter()
            .map(|project| {
                let chapters = project.chapters()?;
                let words = chapters.iter().map(|chapter| chapter.words as u64).sum();
                Ok(Measured {
                    project,
                    chapters,
                    words,
                })
            })
            .collect()
    }

    /// Measures every project and records the counts that changed in the writing log.
    pub fn snapshot(&self, now: NaiveDateTime) -> Result<(Vec<Measured>, History)> {
        let measured = self.measure()?;
        let path = self.history_path();
        let mut history = History::load(&path)?;
        history.record(&path, now, &counts(&measured))?;
        Ok((measured, history))
    }

    /// Every file in the vault outside hidden folders.
    pub fn files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        walk(&self.root, &mut files)?;
        Ok(files)
    }

    /// Another note with this name anywhere in the vault. Obsidian resolves links by name, so
    /// two notes sharing one make links ambiguous.
    pub fn note_named(&self, name: &str, except: &Path) -> Result<Option<PathBuf>> {
        Ok(markdown_files(&self.root)?.into_iter().find(|path| {
            path != except
                && path
                    .file_stem()
                    .is_some_and(|stem| stem.to_string_lossy().eq_ignore_ascii_case(name))
        }))
    }
}

/// A project with its chapters and total words, as measured at one moment.
#[derive(Debug, Clone)]
pub struct Measured {
    pub project: Project,
    pub chapters: Vec<Chapter>,
    pub words: u64,
}

/// Word counts by project slug, as the writing log records them.
pub fn counts(measured: &[Measured]) -> Vec<(String, u64)> {
    measured
        .iter()
        .map(|m| (m.project.slug.clone(), m.words))
        .collect()
}

/// Markdown files under `dir`, skipping hidden folders. A missing folder has none.
pub(crate) fn markdown_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk(dir, &mut files)?;
    files.retain(|path| {
        path.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    });
    Ok(files)
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    for entry in entries {
        let path = entry?.path();
        if is_hidden(&path) {
            continue;
        }
        if path.is_dir() {
            walk(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}
