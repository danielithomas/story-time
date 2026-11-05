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
use crate::note::{PropertyValue, set_properties};
use crate::project::{Form, NoteKind, Project};
use crate::structures::{self, Structure};
use crate::templates::{self, render};
use crate::text::{note_file_name, slugify};
use crate::vault::Vault;

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

#[derive(Clone, Copy)]
pub struct NewProject<'a> {
    pub title: &'a str,
    pub form: Form,
    /// Defaults to a typical length for the form.
    pub target: Option<u64>,
    pub structure: &'static Structure,
    pub premise: Option<&'a str>,
}

/// Creates `<slug>/` with the project note, the story flow and the four note folders. The project
/// note gets a new permanent ID, even from a vault template that doesn't ask for one.
pub fn create_project(vault: &Vault, spec: &NewProject, today: NaiveDate) -> Result<Project> {
    let slug = slugify(spec.title);
    if slug.is_empty() {
        bail!("the title needs at least one letter or number");
    }
    let dir = vault.root.join(&slug);
    if dir.exists() {
        bail!("{} already exists", vault.relative(&dir).display());
    }
    for kind in NoteKind::ALL {
        fs::create_dir_all(dir.join(kind.folder()))
            .with_context(|| format!("creating {}", dir.display()))?;
    }
    let target = spec
        .target
        .unwrap_or(spec.form.default_target())
        .to_string();
    let date = today.format("%Y-%m-%d").to_string();
    let structure = structures::render(spec.structure, target.parse().ok());
    let taken: HashSet<String> = vault.projects()?.iter().filter_map(Project::id).collect();
    let id = ids::new_id(&taken)?;
    let vars = [
        ("id", id.as_str()),
        ("title", spec.title),
        ("project", spec.title),
        ("slug", slug.as_str()),
        ("project_slug", slug.as_str()),
        ("form", spec.form.key()),
        ("target", target.as_str()),
        ("date", date.as_str()),
        ("premise", spec.premise.unwrap_or_default()),
        ("structure", structure.as_str()),
        ("structure_name", spec.structure.key),
    ];
    let note = render(&templates::PROJECT.load(vault)?, &vars);
    write_new(
        &dir.join(format!("{slug}.md")),
        &set_properties(&note, &[("id", PropertyValue::Text(id.clone()))]),
    )?;
    write_new(
        &dir.join(format!("{slug}-flow.md")),
        &render(&templates::FLOW.load(vault)?, &vars),
    )?;
    Project::load(&dir)?.context("the project note was not created")
}

/// Starts a project: creates it, then records a zero snapshot so everything written from now on
/// counts as words written. Given the name of an idea in the ideas file, the idea's premise
/// replaces `spec.premise` and the idea is linked to the new project; nothing is created if the
/// idea can't be found.
pub fn start_project(
    vault: &Vault,
    spec: &NewProject,
    idea: Option<&str>,
    now: NaiveDateTime,
) -> Result<Project> {
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
        premise: found
            .as_ref()
            .map(|(_, idea)| idea.premise.as_str())
            .or(spec.premise),
        ..*spec
    };
    let project = create_project(vault, &spec, now.date())?;
    let history_path = vault.history_path();
    History::load(&history_path)?.record(&history_path, now, &[(project.log_key(), 0)])?;
    if let Some((content, idea)) = &found {
        fs::write(
            &ideas_path,
            ideas::link_idea(content, idea.line, &project.slug),
        )
        .with_context(|| format!("writing {}", ideas_path.display()))?;
    }
    Ok(project)
}

pub struct NewNote {
    pub path: PathBuf,
    /// Another note elsewhere in the vault with the same name, which makes links ambiguous.
    pub same_name: Option<PathBuf>,
}

/// Adds a note from its template. Chapters are numbered after the highest existing number.
pub fn add_note(
    vault: &Vault,
    project: &Project,
    kind: NoteKind,
    name: &str,
    today: NaiveDate,
) -> Result<NewNote> {
    let title = note_file_name(name);
    if title.is_empty() {
        bail!("the name needs at least one letter or number");
    }
    let file_name = match kind {
        NoteKind::Chapter => {
            let next = project
                .chapters()?
                .iter()
                .filter_map(|chapter| chapter.number())
                .max()
                .unwrap_or(0)
                + 1;
            format!("{next:02} {title}")
        }
        _ => title.clone(),
    };
    let path = project.folder(kind).join(format!("{file_name}.md"));
    if path.exists() {
        bail!("{} already exists", vault.relative(&path).display());
    }
    let project_title = project.title();
    let date = today.format("%Y-%m-%d").to_string();
    let vars = [
        ("title", title.as_str()),
        ("project", project_title.as_str()),
        ("project_slug", project.slug.as_str()),
        ("date", date.as_str()),
    ];
    write_new(
        &path,
        &render(&templates::for_kind(kind).load(vault)?, &vars),
    )?;
    let same_name = vault.note_named(&file_name, &path)?;
    Ok(NewNote { path, same_name })
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
        assert!(dir.path().join("Templates/Character.md").is_file());
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
        assert!(!dir.path().join("Templates").exists());
        assert!(Vault::open(dir.path()).is_err());
    }

    #[test]
    fn creates_a_project_and_numbers_chapters() {
        let (_dir, vault) = vault();
        let spec = NewProject {
            title: "Sample Story",
            form: Form::Novel,
            target: None,
            structure: &THREE_ACT,
            premise: Some("A premise, with a catch."),
        };
        let project = create_project(&vault, &spec, today()).unwrap();
        assert_eq!(project.slug, "sample-story");
        assert_eq!(project.title(), "Sample Story");
        let id = project.id().expect("a new project has an ID");
        assert_eq!(project.log_key(), id);
        assert_eq!(project.target(), Some(90_000));
        assert_eq!(project.form(), Some(Form::Novel));
        assert!(project.note.body.contains("A premise, with a catch."));
        assert!(
            fs::read_to_string(project.flow_path())
                .unwrap()
                .contains("around 9,000 words")
        );
        assert!(create_project(&vault, &spec, today()).is_err());

        let first = add_note(&vault, &project, NoteKind::Chapter, "Arrival", today()).unwrap();
        let second = add_note(
            &vault,
            &project,
            NoteKind::Chapter,
            "Leaving: Home?",
            today(),
        )
        .unwrap();
        assert!(first.path.ends_with("chapters/01 Arrival.md"));
        assert!(second.path.ends_with("chapters/02 Leaving Home.md"));
        let chapters = project.chapters().unwrap();
        assert_eq!(chapters[1].title(), "Leaving Home");
        assert_eq!(chapters[1].status().as_deref(), Some("outline"));
        assert_eq!(
            chapters[1].words, 0,
            "a new chapter holds only its heading and a comment"
        );

        let mara = add_note(&vault, &project, NoteKind::Character, "Mara Chen", today()).unwrap();
        assert!(mara.same_name.is_none());
        assert!(add_note(&vault, &project, NoteKind::Character, "Mara Chen", today()).is_err());
    }

    #[test]
    fn vault_templates_override_built_ins() {
        let (dir, vault) = vault();
        let project = create_project(
            &vault,
            &NewProject {
                title: "Other Story",
                form: Form::Novella,
                target: Some(25_000),
                structure: &THREE_ACT,
                premise: None,
            },
            today(),
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("Templates")).unwrap();
        fs::write(
            dir.path().join("Templates/Location.md"),
            "---\ntype: location\n---\n# {{title}} in {{project}}\n",
        )
        .unwrap();
        let place = add_note(&vault, &project, NoteKind::Location, "The Lab", today()).unwrap();
        assert_eq!(
            fs::read_to_string(place.path).unwrap(),
            "---\ntype: location\n---\n# The Lab in Other Story\n"
        );
    }

    #[test]
    fn projects_get_ids_even_from_templates_without_one() {
        let (dir, vault) = vault();
        fs::create_dir_all(dir.path().join("Templates")).unwrap();
        fs::write(
            dir.path().join("Templates/Project.md"),
            "---\ntype: project\ntitle: \"{{title}}\"\n---\n# {{title}}\n",
        )
        .unwrap();
        let spec = NewProject {
            title: "Sample Story",
            form: Form::Novel,
            target: None,
            structure: &THREE_ACT,
            premise: None,
        };
        let now = today().and_hms_opt(9, 0, 0).unwrap();
        let project = start_project(&vault, &spec, None, now).unwrap();
        let id = project
            .id()
            .expect("the ID is added to the template's properties");
        let log = fs::read_to_string(vault.history_path()).unwrap();
        assert_eq!(
            log,
            format!("timestamp,id,words\n2026-10-30T09:00:00,{id},0\n")
        );
    }

    #[test]
    fn projects_from_before_ids_are_logged_by_folder_name() {
        let (dir, vault) = vault();
        fs::create_dir_all(dir.path().join("old-story")).unwrap();
        fs::write(
            dir.path().join("old-story/old-story.md"),
            "---\ntype: project\ntitle: Old Story\n---\n",
        )
        .unwrap();
        let project = vault.find_project(Some("old-story"), dir.path()).unwrap();
        assert_eq!(project.id(), None);
        assert_eq!(project.log_key(), "old-story");
    }
}
