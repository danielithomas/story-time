//! Where a note belongs on the bookshelf, which decides what can be done with it: renaming,
//! deleting, cutting a scene to the scratchpad. A project's or book's own notes (its overview,
//! story flow and timeline), the ideas file and templates belong to none of these.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::project::{Book, NoteKind, Project};
use crate::vault::Vault;

pub(crate) enum Home {
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
    /// A note in a scratchpad: the bookshelf's (with no project) or a world's.
    Scrap {
        project: Option<Project>,
    },
}

impl Home {
    /// Where the note at `path` belongs, or `None` if it's none of these.
    pub(crate) fn of(vault: &Vault, path: &Path) -> Result<Option<Home>> {
        if path.extension().is_none_or(|ext| ext != "md") || path == vault.ideas_path() {
            return Ok(None);
        }
        if path.starts_with(vault.research_dir()) {
            let dir = vault.research_dir();
            return Ok(Some(Home::Research { project: None, dir }));
        }
        if path.starts_with(vault.scratchpad_dir()) {
            return Ok(Some(Home::Scrap { project: None }));
        }
        let Some(project) = vault
            .projects()?
            .into_iter()
            .find(|project| path.starts_with(&project.dir))
        else {
            return Ok(None);
        };
        if path.starts_with(project.research_dir()) {
            let dir = project.research_dir();
            let project = Some(project);
            return Ok(Some(Home::Research { project, dir }));
        }
        if path.starts_with(project.scratchpad_dir()) {
            let project = Some(project);
            return Ok(Some(Home::Scrap { project }));
        }
        let kind = [NoteKind::Character, NoteKind::Location, NoteKind::World]
            .into_iter()
            .find(|kind| path.starts_with(project.folder(*kind)));
        if let Some(kind) = kind {
            return Ok(Some(Home::Bible { project, kind }));
        }
        let book = project
            .books()?
            .into_iter()
            .find(|book| path.starts_with(book.scenes_dir()));
        Ok(book.map(|book| Home::Scene {
            project,
            book: Box::new(book),
        }))
    }
}
