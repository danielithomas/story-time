//! Automatic backups: the whole bookshelf as a zip file, at most one a day, kept outside the
//! bookshelf so a synced folder doesn't upload them and git doesn't see them.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, Timelike};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::vault::{Vault, data_dir};

/// How many copies to keep: the most recent days that have one.
pub const KEEP: usize = 30;

/// Folders a copy leaves out, relative to the bookshelf: git's own history, the copy made before
/// the format 1 upgrade, and compiled manuscripts.
const LEFT_OUT: [&str; 3] = [".git", ".story-time/backups", ".story-time/build"];

/// Where this computer keeps copies of `vault`: the bookshelf's own folder in `base`, or by
/// default in `backups/` in Story Time's data folder.
pub fn folder(vault: &Vault, base: Option<&Path>) -> Result<PathBuf> {
    let base = match base {
        Some(base) => base.to_path_buf(),
        None => data_dir()?.join("backups"),
    };
    Ok(vault.folder_in(&base))
}

/// Copies the bookshelf into `folder` as `<date>.zip`, replacing that day's earlier copy, unless
/// nothing has changed since the newest copy was made. Then deletes all but the newest [`KEEP`]
/// copies. Returns the copy made, if one was.
pub fn back_up(vault: &Vault, folder: &Path, now: NaiveDateTime) -> Result<Option<PathBuf>> {
    if let Some(newest) = copies(folder)?.first() {
        let made = fs::metadata(newest)
            .and_then(|meta| meta.modified())
            .with_context(|| format!("reading {}", newest.display()))?;
        if last_change(&vault.root, folder)? <= made {
            return Ok(None);
        }
    }
    fs::create_dir_all(folder).with_context(|| format!("creating {}", folder.display()))?;
    let path = folder.join(format!("{}.zip", now.date()));
    let partial = path.with_extension("zip.partial");
    write_zip(&vault.root, folder, &partial)
        .with_context(|| format!("writing {}", partial.display()))?;
    fs::rename(&partial, &path).with_context(|| format!("writing {}", path.display()))?;
    for old in copies(folder)?.into_iter().skip(KEEP) {
        fs::remove_file(&old).with_context(|| format!("removing {}", old.display()))?;
    }
    Ok(Some(path))
}

/// Whether `folder` already holds a copy made on `day`.
pub fn has_copy(folder: &Path, day: NaiveDate) -> bool {
    folder.join(format!("{day}.zip")).is_file()
}

/// The copies in `folder`, newest first. Only files named like `2026-11-09.zip` count, so nothing
/// else that happens to be there is ever deleted.
pub fn copies(folder: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", folder.display())),
    };
    let mut copies = Vec::new();
    for entry in entries {
        let path = entry?.path();
        let dated = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".zip"))
            .is_some_and(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").is_ok());
        if dated && path.is_file() {
            copies.push(path);
        }
    }
    copies.sort_by(|a, b| b.cmp(a));
    Ok(copies)
}

/// Whether a copy leaves `path` out: the folders in [`LEFT_OUT`], and the backups folder itself
/// if it is inside the bookshelf.
fn left_out(root: &Path, folder: &Path, path: &Path) -> bool {
    path == folder || LEFT_OUT.iter().any(|out| path == root.join(out))
}

/// The latest time anything in the bookshelf changed. Folders count too, since deleting or
/// renaming a file changes its folder.
fn last_change(root: &Path, folder: &Path) -> Result<SystemTime> {
    let mut latest = SystemTime::UNIX_EPOCH;
    walk(root, root, folder, &mut |_, meta| {
        latest = latest.max(meta.modified()?);
        Ok(())
    })?;
    Ok(latest)
}

fn write_zip(root: &Path, folder: &Path, to: &Path) -> Result<()> {
    let mut zip = ZipWriter::new(File::create(to)?);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    walk(root, root, folder, &mut |path, meta| {
        let name = path
            .strip_prefix(root)
            .expect("inside the bookshelf")
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let options = match meta.modified().ok().and_then(zip_time) {
            Some(time) => options.last_modified_time(time),
            None => options,
        };
        if meta.is_dir() {
            zip.add_directory(name, options)?;
        } else {
            zip.start_file(name, options)?;
            io::copy(&mut File::open(path)?, &mut zip)?;
        }
        Ok(())
    })?;
    zip.finish()?;
    Ok(())
}

/// Visits every file and folder under `dir` that a copy includes, without following links.
fn walk(
    root: &Path,
    dir: &Path,
    folder: &Path,
    visit: &mut dyn FnMut(&Path, &fs::Metadata) -> Result<()>,
) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.is_symlink() || left_out(root, folder, &path) {
            continue;
        }
        visit(&path, &meta)?;
        if meta.is_dir() {
            walk(root, &path, folder, visit)?;
        }
    }
    Ok(())
}

fn zip_time(time: SystemTime) -> Option<zip::DateTime> {
    let time: DateTime<Local> = time.into();
    zip::DateTime::from_date_and_time(
        u16::try_from(time.year()).ok()?,
        time.month() as u8,
        time.day() as u8,
        time.hour() as u8,
        time.minute() as u8,
        time.second() as u8,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scaffold::create_bookshelf;
    use std::time::Duration;
    use tempfile::TempDir;

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 9)
            .unwrap()
            .and_hms_opt(18, 0, 0)
            .unwrap()
    }

    fn names(zip: &Path) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(File::open(zip).unwrap()).unwrap();
        (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().unwrap().to_string())
            .collect()
    }

    /// Makes the newest copy look an hour old, so a change made now is newer.
    fn age(copy: &Path) {
        let hour_ago = SystemTime::now() - Duration::from_secs(3600);
        File::options()
            .write(true)
            .open(copy)
            .unwrap()
            .set_modified(hour_ago)
            .unwrap();
    }

    #[test]
    fn copies_the_bookshelf_when_it_has_changed() {
        let dir = TempDir::new().unwrap();
        let root = create_bookshelf(dir.path(), "Stories").unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref").unwrap();
        fs::create_dir_all(root.join(".story-time/build")).unwrap();
        fs::write(root.join(".story-time/build/book.md"), "built").unwrap();
        fs::create_dir_all(root.join("world/bible/characters")).unwrap();
        fs::write(root.join("world/world.md"), "# World\n").unwrap();
        let vault = Vault::open(&root).unwrap();
        let folder = dir.path().join("backups");

        let copy = back_up(&vault, &folder, now())
            .unwrap()
            .expect("a first copy");
        assert_eq!(copy, folder.join("2026-11-09.zip"));
        let names = names(&copy);
        for name in [
            ".story-time/config.toml",
            "templates/scene.md",
            "world/world.md",
            "world/bible/characters/",
        ] {
            assert!(names.iter().any(|n| n == name), "{name} in {names:?}");
        }
        assert!(
            !names
                .iter()
                .any(|n| n.starts_with(".git/") || n.starts_with(".story-time/build")),
            "{names:?}"
        );

        assert_eq!(back_up(&vault, &folder, now()).unwrap(), None, "unchanged");
        age(&copy);
        fs::remove_file(root.join("world/world.md")).unwrap();
        let again = back_up(&vault, &folder, now()).unwrap();
        assert_eq!(again, Some(copy.clone()), "a deletion is a change");
        assert_eq!(copies(&folder).unwrap(), vec![copy], "one copy a day");
        assert!(has_copy(&folder, now().date()));
    }

    #[test]
    fn keeps_the_newest_copies_and_nothing_else_is_touched() {
        let dir = TempDir::new().unwrap();
        let root = create_bookshelf(dir.path(), "Stories").unwrap();
        let vault = Vault::open(&root).unwrap();
        let folder = dir.path().join("backups");
        fs::create_dir_all(&folder).unwrap();
        let first = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        for day in first.iter_days().take(KEEP + 3) {
            fs::write(folder.join(format!("{day}.zip")), "").unwrap();
        }
        fs::write(folder.join("readme.txt"), "mine").unwrap();
        age(&copies(&folder).unwrap()[0]);

        back_up(&vault, &folder, now()).unwrap().expect("a copy");
        let kept = copies(&folder).unwrap();
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept[0], folder.join("2026-11-09.zip"));
        assert!(!folder.join("2026-09-01.zip").exists());
        assert!(folder.join("readme.txt").exists());
    }

    #[test]
    fn bookshelves_with_the_same_name_back_up_apart() {
        let dir = TempDir::new().unwrap();
        let one = create_bookshelf(&dir.path().join("a"), "Stories").unwrap();
        let two = create_bookshelf(&dir.path().join("b"), "Stories").unwrap();
        let base = dir.path().join("backups");
        let one = folder(&Vault::open(one).unwrap(), Some(&base)).unwrap();
        let two = folder(&Vault::open(two).unwrap(), Some(&base)).unwrap();
        assert_ne!(one, two);
        assert!(
            one.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("Stories-")
        );
    }
}
