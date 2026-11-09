//! The example a new bookshelf can start with: a world with one book whose scenes explain Story
//! Time, each an example of what it describes. Its notes are in `crates/core/example/`, laid out
//! as they are on the bookshelf.

use std::path::PathBuf;

use anyhow::{Result, bail};
use chrono::{Days, NaiveDateTime};

use crate::ids;
use crate::scaffold::{taken_ids, write_new};
use crate::vault::Vault;

/// The example's folder at the root of the bookshelf.
pub const EXAMPLE_DIR: &str = "example-world";

macro_rules! files {
    ($($path:literal),* $(,)?) => {
        [$(($path, include_str!(concat!("../example/", $path)))),*]
    };
}

/// Each note, by its path on the bookshelf.
const FILES: [(&str, &str); 15] = files![
    "example-world/example-world.md",
    "example-world/bible/example-world-timeline.md",
    "example-world/bible/characters/the-writer.md",
    "example-world/bible/locations/the-binder.md",
    "example-world/bible/world/the-writing-log.md",
    "example-world/research/standard-manuscript-format.md",
    "example-world/scratchpad/example-scrap.md",
    "example-world/books/a-tour-of-story-time/a-tour-of-story-time.md",
    "example-world/books/a-tour-of-story-time/a-tour-of-story-time-flow.md",
    "example-world/books/a-tour-of-story-time/scenes/welcome-to-the-example.md",
    "example-world/books/a-tour-of-story-time/scenes/scenes-and-chapters.md",
    "example-world/books/a-tour-of-story-time/scenes/notes-to-self.md",
    "example-world/books/a-tour-of-story-time/scenes/the-inspector-and-the-board.md",
    "example-world/books/a-tour-of-story-time/scenes/the-world-bible.md",
    "example-world/books/a-tour-of-story-time/scenes/a-scene-set-aside.md",
];

/// How long the example book's deadline is from the day it's added.
const DEADLINE_DAYS: u64 = 60;

/// Adds the example to a bookshelf, then records a snapshot, so its words are the book's starting
/// point and don't count as written that day.
pub fn add_example(vault: &Vault, now: NaiveDateTime) -> Result<PathBuf> {
    let dir = vault.root.join(EXAMPLE_DIR);
    if dir.exists() {
        bail!("{EXAMPLE_DIR} already exists on this bookshelf");
    }
    let mut taken = taken_ids(vault)?;
    let project_id = ids::new_id(&taken)?;
    taken.insert(project_id.clone());
    let book_id = ids::new_id(&taken)?;
    let today = now.date();
    let deadline = today + Days::new(DEADLINE_DAYS);
    for (path, text) in FILES {
        let text = text
            .replace("{{project_id}}", &project_id)
            .replace("{{book_id}}", &book_id)
            .replace("{{date}}", &today.to_string())
            .replace("{{deadline}}", &deadline.to_string());
        write_new(&vault.root.join(path), &text)?;
    }
    vault.snapshot(now)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check;
    use crate::scaffold::create_bookshelf;
    use chrono::NaiveDate;
    use tempfile::TempDir;

    #[test]
    fn the_example_is_a_clean_book_whose_words_are_a_starting_point() {
        let dir = TempDir::new().unwrap();
        let root = create_bookshelf(dir.path(), "Stories").unwrap();
        let vault = Vault::open(&root).unwrap();
        let now = NaiveDate::from_ymd_opt(2026, 11, 9)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        add_example(&vault, now).unwrap();

        let project = vault.find_project(Some("example"), &root).unwrap();
        assert_eq!(project.title(), "Example World");
        let report = check::check(&vault, &project).unwrap();
        assert!(report.is_clean(), "{report:#?}");

        let book = &project.books().unwrap()[0];
        assert_eq!(book.target(), Some(3000));
        let contents = book.contents().unwrap();
        assert_eq!(contents.unplaced.len(), 1, "{contents:#?}");
        assert!(contents.words() > 500, "{}", contents.words());

        let (_, history) = vault.snapshot(now).unwrap();
        assert_eq!(history.written_on(now.date(), None), 0);
        assert!(add_example(&vault, now).is_err(), "only once");
    }
}
