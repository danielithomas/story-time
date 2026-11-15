//! A bookshelf's git history: keeping one (`keep_history`), and reading it.
//!
//! Story Time runs the installed `git` program, so the writer's own git settings, SSH keys and
//! credential helpers work unchanged. A bookshelf that keeps a history is committed as a whole:
//! every file in its folder, apart from what `.gitignore` leaves out. Reading past commits copies
//! them into a temporary folder, never checking them out in the bookshelf.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{NaiveDate, NaiveDateTime};

use crate::config::{self, CONFIG_FILE, META_DIR};
use crate::history::{History, Snapshot};
use crate::note::Note;
use crate::scaffold::{BUILD_IGNORE, LOG_MERGE, add_line};
use crate::text::name_from_file;
use crate::upgrade::BACKUPS_IGNORE;
use crate::vault::Vault;

pub(crate) const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

/// Whether the bookshelf is in a git repository: its folder, or one above it, holds `.git`.
pub fn in_repository(root: &Path) -> bool {
    root.ancestors().any(|dir| dir.join(".git").exists())
}

/// Folders that cloud storage services keep in step between computers, by a folder name in the
/// path, with the service's name.
const SYNCED_FOLDERS: [(&str, &str); 7] = [
    ("OneDrive", "OneDrive"),
    ("Dropbox", "Dropbox"),
    ("iCloud Drive", "iCloud Drive"),
    ("iCloudDrive", "iCloud Drive"),
    ("Mobile Documents", "iCloud Drive"),
    ("Google Drive", "Google Drive"),
    ("GoogleDrive", "Google Drive"),
];

/// The cloud storage service whose folder holds the bookshelf, if any, going by the folders in
/// its path (`OneDrive`, `OneDrive - Company`, `Dropbox` and so on). A git repository in such a
/// folder can be damaged when the service and git write to it at once.
pub fn synced_folder(root: &Path) -> Option<&'static str> {
    root.components().find_map(|component| {
        let name = component.as_os_str().to_string_lossy();
        SYNCED_FOLDERS
            .iter()
            .find(|(folder, _)| {
                name.strip_prefix(folder)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '-']))
            })
            .map(|(_, service)| *service)
    })
}

/// A commit that recorded changes to the bookshelf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// When, on the clock of the computer that made it.
    pub at: NaiveDateTime,
    /// What changed, as its message says: "Arrival, The Landing and 2 other notes".
    pub message: String,
}

/// Starts keeping a history: sets `keep_history`, and records the bookshelf as it is, making its
/// repository first if it has none.
pub fn start(vault: &Vault) -> Result<Option<Commit>> {
    config::set_bookshelf_setting(
        &vault.root,
        "keep_history",
        Some(toml::Value::Boolean(true)),
    )?;
    let vault = Vault::open(&vault.root)?;
    prepare(&vault.root)?;
    commit(&vault)
}

/// Stops keeping a history. What was recorded stays in the bookshelf's repository.
pub fn stop(vault: &Vault) -> Result<()> {
    config::set_bookshelf_setting(&vault.root, "keep_history", None)
}

/// Records the bookshelf's changes in its history, if it keeps one, and returns the commit made.
/// Nothing is recorded when nothing but the writing log changed: the log goes with the next
/// change. In a repository that holds more than the bookshelf, only the bookshelf is committed.
pub fn commit(vault: &Vault) -> Result<Option<Commit>> {
    record(vault, false)
}

/// Records every change, even one to nothing but the writing log, as syncing needs a clean folder.
pub(crate) fn commit_everything(vault: &Vault) -> Result<Option<Commit>> {
    record(vault, true)
}

fn record(vault: &Vault, log_alone: bool) -> Result<Option<Commit>> {
    if !vault.config.keep_history {
        return Ok(None);
    }
    let root = &vault.root;
    if !has_repository(root)? {
        prepare(root)?;
    }
    run(git(root).args(["add", "--all", "--", "."]))?;
    let staged = run(git(root).args([
        "diff",
        "--cached",
        "--name-status",
        "-z",
        "--no-renames",
        "--relative",
    ]))?;
    let log = format!("{META_DIR}/history.csv");
    let fields: Vec<&str> = staged.split('\0').collect();
    let staged: Vec<(&str, &str)> = fields
        .chunks(2)
        .filter_map(|pair| match pair {
            [status, path] if !path.is_empty() => Some((*status, *path)),
            _ => None,
        })
        .collect();
    let changed: Vec<(&str, &str)> = staged
        .iter()
        .copied()
        .filter(|(_, path)| *path != log)
        .collect();
    let (subject, body) = if !changed.is_empty() {
        message(vault, &changed)
    } else if log_alone && !staged.is_empty() {
        ("The writing log".to_string(), None)
    } else {
        return Ok(None);
    };
    let mut command = git(root);
    command
        .args(identity(vault))
        .args(["commit", "--quiet", "-m", &subject]);
    if let Some(body) = &body {
        command.args(["-m", body]);
    }
    run(command.args(["--", "."]))?;
    last_commit(vault)
}

/// The last commit that changed the bookshelf, if it has a history.
pub fn last_commit(vault: &Vault) -> Result<Option<Commit>> {
    let root = &vault.root;
    if !has_repository(root)?
        || run(git(root).args(["rev-parse", "--verify", "--quiet", "HEAD"])).is_err()
    {
        return Ok(None);
    }
    let found = run(git(root).args([
        "log",
        "-1",
        "--format=%cd%x00%s",
        "--date=format:%Y-%m-%dT%H:%M:%S",
        "HEAD",
        "--",
        ".",
    ]))?;
    Ok(found
        .trim_end()
        .split_once('\0')
        .and_then(|(time, message)| {
            Some(Commit {
                at: NaiveDateTime::parse_from_str(time, TIME_FORMAT).ok()?,
                message: message.to_string(),
            })
        }))
}

/// Whether the bookshelf has a repository of its own, or is tracked in one around it. A
/// repository around it that doesn't track it, such as one for the files in a home folder,
/// isn't its history.
fn has_repository(root: &Path) -> Result<bool> {
    if root.join(".git").exists() {
        return Ok(true);
    }
    if !in_repository(root) {
        return Ok(false);
    }
    let tracked = run(git(root).args(["ls-files", "--", "."]))?;
    Ok(!tracked.trim().is_empty())
}

/// Makes the bookshelf a repository if it isn't one, and gives it the `.gitignore` and
/// `.gitattributes` lines story-time needs: compiled manuscripts and upgrade backups are left
/// out, and the writing log merges by keeping both computers' lines.
fn prepare(root: &Path) -> Result<()> {
    if !has_repository(root)? {
        run(git(root).args(["-c", "init.defaultBranch=main", "init", "--quiet"]))?;
    }
    add_line(&root.join(".gitignore"), BUILD_IGNORE)?;
    add_line(&root.join(".gitignore"), BACKUPS_IGNORE)?;
    add_line(&root.join(".gitattributes"), LOG_MERGE)?;
    Ok(())
}

/// The note names a commit message gives before counting the rest.
const NAMED: usize = 3;

/// A commit's message for these changes (git's status letter and a path relative to the
/// bookshelf): the names of the notes that changed, then settings or other files, with every note
/// named in the body when the subject can't name them all. A deleted note is named as it was in
/// the last commit.
fn message(vault: &Vault, changed: &[(&str, &str)]) -> (String, Option<String>) {
    let config = format!("{META_DIR}/{CONFIG_FILE}");
    let templates = format!("{}/", vault.config.templates_dir.trim_end_matches('/'));
    let mut notes: Vec<String> = Vec::new();
    let (mut settings, mut others) = (false, false);
    for &(status, path) in changed {
        let hidden = path.split('/').any(|part| part.starts_with('.'));
        if path == config {
            settings = true;
        } else if path.ends_with(".md") && !hidden && !path.starts_with(&templates) {
            let file = vault.root.join(path);
            let text = if status == "D" {
                run(git(&vault.root).args(["show", &format!("HEAD:./{path}")]))
            } else {
                fs::read_to_string(&file).map_err(Into::into)
            };
            let name = text
                .map(|text| Note::parse(&file, &text).title())
                .unwrap_or_else(|_| {
                    name_from_file(&file.file_stem().unwrap_or_default().to_string_lossy())
                });
            if !notes.contains(&name) {
                notes.push(name);
            }
        } else {
            others = true;
        }
    }
    let mut items: Vec<String> = if notes.len() > NAMED {
        let mut items = notes[..NAMED - 1].to_vec();
        items.push(format!("{} other notes", notes.len() - (NAMED - 1)));
        items
    } else {
        notes.clone()
    };
    if settings {
        items.push("settings".to_string());
    }
    if others {
        items.push("other files".to_string());
    }
    let subject = match items.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    };
    let mut chars = subject.chars();
    let subject = chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    let body = (notes.len() > NAMED).then(|| notes.join("\n"));
    (subject, body)
}

/// Settings that name a commit's author, for a computer whose git doesn't say who is writing:
/// the bookshelf's `author`, else Story Time.
pub(crate) fn identity(vault: &Vault) -> Vec<String> {
    let set = |key: &str| {
        run(git(&vault.root).args(["config", "--get", key]))
            .is_ok_and(|value| !value.trim().is_empty())
    };
    let mut settings = Vec::new();
    if !set("user.name") {
        let name = vault
            .config
            .author
            .clone()
            .filter(|author| !author.trim().is_empty())
            .unwrap_or_else(|| "Story Time".to_string());
        settings.extend(["-c".to_string(), format!("user.name={name}")]);
    }
    if !set("user.email") {
        settings.extend([
            "-c".to_string(),
            "user.email=story-time@localhost".to_string(),
        ]);
    }
    settings
}

/// What filling in the writing log from git history found.
#[derive(Debug, Default)]
pub struct Backfill {
    /// The days read: each day with a commit that changed the bookshelf, oldest first.
    pub days: Vec<NaiveDate>,
    /// Days whose commit doesn't hold a bookshelf in this version's format, such as those from
    /// before story-time set it up or upgraded it from story-time 0.1, or before it was in this
    /// folder.
    pub skipped: usize,
    /// Each day whose words written change: the day, then the words before and after.
    pub changed: Vec<(NaiveDate, i64, i64)>,
}

/// Fills in the writing log from the bookshelf's git history, so days written without
/// story-time running still count. Each day's last commit is measured as story-time measures the
/// bookshelf now, and a count the log doesn't already have for that moment is appended, so
/// filling in again adds nothing. With `write` false, it only says what would change.
pub fn backfill(vault: &Vault, now: NaiveDateTime, write: bool) -> Result<Backfill> {
    let (days, skipped, readings) = readings(vault, now)?;
    let path = vault.history_path();
    let mut history = History::load(&path)?;
    let missing = history.missing(&readings);
    let before = history.daily(None);
    let after = history.with(&missing).daily(None);
    let changed = before
        .keys()
        .chain(after.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|day| {
            let was = before.get(day).copied().unwrap_or(0);
            let is = after.get(day).copied().unwrap_or(0);
            (was != is).then_some((*day, was, is))
        })
        .collect();
    if write {
        history.append(&path, missing)?;
    }
    Ok(Backfill {
        days,
        skipped,
        changed,
    })
}

/// The counts in each day's last commit, as the writing log records them, with the days read and
/// the number skipped.
fn readings(vault: &Vault, now: NaiveDateTime) -> Result<(Vec<NaiveDate>, usize, Vec<Snapshot>)> {
    let repository = Repository::of(&vault.root)?;
    let commits = repository.daily_commits(now)?;
    let scratch = tempfile::tempdir().context("making a temporary folder")?;
    let index = scratch.path().join("index");
    let tree = scratch.path().join("bookshelf");
    fs::create_dir(&tree).with_context(|| format!("creating {}", tree.display()))?;
    let (mut days, mut skipped, mut readings) = (Vec::new(), 0, Vec::new());
    for (at, commit) in commits {
        let counts = repository
            .read(&commit, &index, &tree)
            .and_then(|()| counts_in(&tree));
        match counts {
            Ok(counts) => {
                days.push(at.date());
                readings.extend(
                    counts
                        .into_iter()
                        .map(|(key, words)| Snapshot { at, key, words }),
                );
            }
            Err(_) => skipped += 1,
        }
    }
    Ok((days, skipped, readings))
}

/// The writing log's counts for a bookshelf copied out of a commit, which must have been set up
/// by story-time in the current format.
fn counts_in(tree: &Path) -> Result<Vec<(String, u64)>> {
    if !tree.join(META_DIR).join(CONFIG_FILE).is_file() {
        bail!("not a bookshelf yet");
    }
    let vault = Vault::open(tree)?;
    vault.config.ensure_current()?;
    let measured = vault.measure()?;
    vault.counts(&measured)
}

struct Repository {
    root: PathBuf,
    git_dir: PathBuf,
    /// The bookshelf's folder in the repository, such as `stories`, or empty at its top.
    prefix: String,
}

impl Repository {
    fn of(root: &Path) -> Result<Repository> {
        if !in_repository(root) {
            bail!("this bookshelf isn't in a git repository, so there's no history to read");
        }
        let found = run(git(root).args(["rev-parse", "--absolute-git-dir", "--show-prefix"]))?;
        let mut lines = found.lines();
        let git_dir = lines
            .next()
            .context("git didn't say where the repository is")?;
        Ok(Repository {
            root: root.to_path_buf(),
            git_dir: PathBuf::from(git_dir),
            prefix: lines.next().unwrap_or("").trim_end_matches('/').to_string(),
        })
    }

    /// The last commit of each day that changed the bookshelf, oldest first, as its time and
    /// hash, along the repository's main line (each merge's first parent), so every commit read
    /// is a state the bookshelf was in. A commit's time is on the clock of the computer that made
    /// it, as the writing log's are; commits dated after `now` are left out.
    fn daily_commits(&self, now: NaiveDateTime) -> Result<Vec<(NaiveDateTime, String)>> {
        if run(git(&self.root).args(["rev-parse", "--verify", "--quiet", "HEAD"])).is_err() {
            return Ok(Vec::new());
        }
        let log = run(git(&self.root).args([
            "log",
            "--first-parent",
            "--format=%H %cd",
            "--date=format:%Y-%m-%dT%H:%M:%S",
            "HEAD",
            "--",
            ".",
        ]))?;
        let mut days: BTreeMap<NaiveDate, (NaiveDateTime, String)> = BTreeMap::new();
        // Newest first, so the first commit seen on a day is its last.
        for line in log.lines() {
            let Some((commit, time)) = line.split_once(' ') else {
                continue;
            };
            let Ok(at) = NaiveDateTime::parse_from_str(time, TIME_FORMAT) else {
                continue;
            };
            if at <= now {
                days.entry(at.date())
                    .or_insert_with(|| (at, commit.to_string()));
            }
        }
        Ok(days.into_values().collect())
    }

    /// Makes `tree` hold the bookshelf as it was at `commit`. Git keeps track of `tree` in
    /// `index`, so only what changed since the last commit read is written.
    fn read(&self, commit: &str, index: &Path, tree: &Path) -> Result<()> {
        let at = if self.prefix.is_empty() {
            format!("{commit}^{{tree}}")
        } else {
            format!("{commit}:{}", self.prefix)
        };
        run(git(&self.root)
            .env("GIT_INDEX_FILE", index)
            .arg("--git-dir")
            .arg(&self.git_dir)
            .arg("--work-tree")
            .arg(tree)
            .args(["read-tree", "--reset", "-u", &at]))
        .map(drop)
    }
}

/// `git`, run in `dir`. The variables a git hook sets are cleared, so the repository is found from
/// the folder; git never waits for a password typed in a terminal; and settings that would change
/// what git prints or copies are turned off.
pub(crate) fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_TERMINAL_PROMPT", "0")
        // Files kept with Git LFS aren't notes, so there's no need to download them.
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .args([
            "-c",
            "log.showSignature=false",
            "-c",
            "core.sparseCheckout=false",
        ]);
    // Tests run away from this computer's git settings.
    #[cfg(test)]
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join(".no-gitconfig"));
    command
}

pub(crate) fn run(command: &mut Command) -> Result<String> {
    let output = output(command)?;
    if !output.status.success() {
        bail!("git: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Runs git and returns what it printed, whether or not it succeeded.
pub(crate) fn output(command: &mut Command) -> Result<std::process::Output> {
    command.output().map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => anyhow!(
            "keeping a history needs Git, a free program that isn't installed on this computer. \
             Install it from https://git-scm.com and try again"
        ),
        _ => anyhow!(e).context("running git"),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tempfile::TempDir;

    pub(crate) fn has_git() -> bool {
        Command::new("git").arg("--version").output().is_ok()
    }

    /// Runs git in `dir` as a test user, away from this computer's git settings.
    pub(crate) fn git_in(dir: &Path, args: &[&str], date: &str) {
        let home = dir.join(".no-home");
        let status = Command::new("git")
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", home.join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }

    fn commit_on(repo: &Path, date: &str) {
        git_in(repo, &["add", "-A"], date);
        git_in(
            repo,
            &["commit", "--quiet", "--allow-empty", "-m", date],
            date,
        );
    }

    fn time(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, TIME_FORMAT).unwrap()
    }

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    /// A repository holding a bookshelf in `stories/`, with a world whose book has one scene.
    struct Repo {
        _dir: TempDir,
        repo: PathBuf,
        vault: Vault,
        scene: PathBuf,
        key: String,
    }

    fn repo() -> Repo {
        use crate::contents::Place;
        use crate::project::Form;
        use crate::scaffold::{NewBook, NewProject, add_scene, create_project, init_vault};
        use crate::structures::THREE_ACT;

        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo");
        let root = repo.join("stories");
        fs::create_dir_all(&root).unwrap();
        git_in(&repo, &["init", "--quiet"], "2026-03-01T09:00:00+11:00");
        fs::write(repo.join("notes.md"), "Before the bookshelf.\n").unwrap();
        fs::write(
            root.join("draft.md"),
            "An Obsidian note, before story-time.\n",
        )
        .unwrap();
        commit_on(&repo, "2026-03-01T09:00:00+11:00");

        init_vault(&root, false).unwrap();
        let vault = Vault::open(&root).unwrap();
        let new = NewProject {
            world: None,
            book: NewBook {
                title: "Sample Story",
                form: Form::Novel,
                target: None,
                structure: &THREE_ACT,
                premise: None,
            },
        };
        let (project, book) = create_project(&vault, &new, day("2026-03-02")).unwrap();
        let scene = add_scene(
            &vault,
            &project,
            &book,
            "Arrival",
            Place::End,
            day("2026-03-02"),
        )
        .unwrap()
        .path;
        Repo {
            _dir: dir,
            repo,
            vault,
            scene,
            key: book.log_key(),
        }
    }

    impl Repo {
        fn write(&self, words: usize) {
            let text = fs::read_to_string(&self.scene).unwrap();
            let body = format!("# Arrival\n\n{}\n", vec!["word"; words].join(" "));
            fs::write(&self.scene, crate::note::replace_body(&text, &body)).unwrap();
        }

        fn written(&self, history: &History) -> BTreeMap<NaiveDate, i64> {
            history.daily(Some(&self.key))
        }
    }

    #[test]
    fn fills_in_days_from_each_days_last_commit() {
        if !has_git() {
            return;
        }
        let r = repo();
        r.write(100);
        commit_on(&r.repo, "2026-03-02T10:00:00+11:00");
        r.write(250);
        commit_on(&r.repo, "2026-03-03T09:00:00+11:00");
        r.write(400);
        commit_on(&r.repo, "2026-03-03T23:30:00+11:00");
        fs::write(r.repo.join("notes.md"), "Outside the bookshelf.\n").unwrap();
        commit_on(&r.repo, "2026-03-04T12:00:00+11:00");
        r.write(450);
        commit_on(&r.repo, "2026-03-05T08:00:00-05:00");
        r.write(9999);
        commit_on(&r.repo, "2026-03-09T08:00:00+11:00");

        let now = time("2026-03-06T12:00:00");
        let looked = backfill(&r.vault, now, false).unwrap();
        assert_eq!(
            looked.days,
            [day("2026-03-02"), day("2026-03-03"), day("2026-03-05")],
            "the 4th's commit didn't change the bookshelf, and the 9th's is after now"
        );
        assert_eq!(looked.skipped, 1, "the 1st's commit isn't a bookshelf yet");
        assert_eq!(
            looked.changed,
            [(day("2026-03-03"), 0, 300), (day("2026-03-05"), 0, 50)]
        );
        assert!(
            !r.vault.history_path().exists(),
            "only looking writes nothing"
        );

        let filled = backfill(&r.vault, now, true).unwrap();
        assert_eq!(filled.changed, looked.changed);
        let history = History::load(&r.vault.history_path()).unwrap();
        let written = r.written(&history);
        assert_eq!(written.get(&day("2026-03-02")), Some(&0), "a baseline");
        assert_eq!(
            written.get(&day("2026-03-03")),
            Some(&300),
            "the day's last commit"
        );
        assert_eq!(
            written.get(&day("2026-03-05")),
            Some(&50),
            "on the clock of the computer that made it"
        );

        let again = backfill(&r.vault, now, true).unwrap();
        assert!(again.changed.is_empty(), "{:?}", again.changed);
        assert_eq!(
            History::load(&r.vault.history_path()).unwrap().daily(None),
            history.daily(None)
        );
    }

    #[test]
    fn days_already_logged_keep_their_words_and_later_days_lose_the_backlog() {
        if !has_git() {
            return;
        }
        let r = repo();
        r.write(100);
        commit_on(&r.repo, "2026-03-02T10:00:00+11:00");
        r.vault.snapshot(time("2026-03-02T21:00:00")).unwrap();
        r.write(300);
        commit_on(&r.repo, "2026-03-03T22:00:00+11:00");
        r.write(350);
        r.vault.snapshot(time("2026-03-04T21:00:00")).unwrap();

        let before = r.written(&History::load(&r.vault.history_path()).unwrap());
        assert_eq!(before.get(&day("2026-03-04")), Some(&250));
        backfill(&r.vault, time("2026-03-05T09:00:00"), true).unwrap();
        let after = r.written(&History::load(&r.vault.history_path()).unwrap());
        assert_eq!(after.get(&day("2026-03-02")), Some(&0));
        assert_eq!(
            after.get(&day("2026-03-03")),
            Some(&200),
            "a day story-time missed"
        );
        assert_eq!(
            after.get(&day("2026-03-04")),
            Some(&50),
            "no longer counting the 3rd's"
        );
        assert_eq!(
            after.values().sum::<i64>(),
            before.values().sum::<i64>(),
            "no words gained or lost overall"
        );
    }

    #[test]
    fn reads_a_bookshelf_at_the_top_of_its_repository() {
        if !has_git() {
            return;
        }
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        git_in(root, &["init", "--quiet"], "2026-03-01T09:00:00+11:00");
        crate::scaffold::init_vault(root, false).unwrap();
        let vault = Vault::open(root).unwrap();
        fs::create_dir_all(vault.scratchpad_dir()).unwrap();
        fs::write(vault.scratchpad_dir().join("line.md"), "one two three\n").unwrap();
        commit_on(root, "2026-03-01T10:00:00+11:00");
        fs::write(
            vault.scratchpad_dir().join("line.md"),
            "one two three four\n",
        )
        .unwrap();
        commit_on(root, "2026-03-02T10:00:00+11:00");
        let filled = backfill(&vault, time("2026-03-03T09:00:00"), true).unwrap();
        assert_eq!(filled.days, [day("2026-03-01"), day("2026-03-02")]);
        assert_eq!(filled.changed, [(day("2026-03-02"), 0, 1)]);
    }

    #[test]
    fn says_when_there_is_no_history() {
        let dir = TempDir::new().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let error = backfill(&vault, time("2026-03-03T09:00:00"), true).unwrap_err();
        assert!(
            error.to_string().contains("isn't in a git repository"),
            "{error}"
        );
        if !has_git() {
            return;
        }
        git_in(
            dir.path(),
            &["init", "--quiet"],
            "2026-03-01T09:00:00+11:00",
        );
        let filled = backfill(&vault, time("2026-03-03T09:00:00"), true).unwrap();
        assert!(filled.days.is_empty(), "no commits yet");
    }

    /// A bookshelf, outside any repository, with a world whose book has two scenes.
    pub(crate) fn shelf(root: &Path) -> (Vault, PathBuf, PathBuf) {
        use crate::contents::Place;
        use crate::project::Form;
        use crate::scaffold::{NewBook, NewProject, add_scene, create_project, init_vault};
        use crate::structures::THREE_ACT;

        fs::create_dir_all(root).unwrap();
        init_vault(root, false).unwrap();
        let vault = Vault::open(root).unwrap();
        let new = NewProject {
            world: None,
            book: NewBook {
                title: "Sample Story",
                form: Form::Novel,
                target: None,
                structure: &THREE_ACT,
                premise: None,
            },
        };
        let (project, book) = create_project(&vault, &new, day("2026-03-02")).unwrap();
        let scene = |name| {
            let book = crate::Book::load(&book.dir, &project.slug)
                .unwrap()
                .unwrap();
            add_scene(&vault, &project, &book, name, Place::End, day("2026-03-02"))
                .unwrap()
                .path
        };
        let arrival = scene("Arrival");
        let leaving = scene("Leaving Home");
        (vault, arrival, leaving)
    }

    pub(crate) fn reopen(vault: &Vault) -> Vault {
        Vault::open(&vault.root).unwrap()
    }

    /// What git says about the bookshelf's repository, from `dir`.
    pub(crate) fn ask(dir: &Path, args: &[&str]) -> String {
        run(git(dir).args(args)).unwrap()
    }

    #[test]
    fn keeps_a_history_once_started() {
        if !has_git() {
            return;
        }
        let dir = TempDir::new().unwrap();
        let (vault, arrival, leaving) = shelf(dir.path());
        let root = &vault.root;
        assert_eq!(commit(&vault).unwrap(), None, "off until started");
        assert!(!root.join(".git").exists());
        assert_eq!(last_commit(&vault).unwrap(), None);

        let first = start(&vault).unwrap().expect("the bookshelf as it was");
        let vault = reopen(&vault);
        assert!(vault.config.keep_history);
        assert!(root.join(".git").is_dir());
        let ignored = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(ignored.contains(BUILD_IGNORE) && ignored.contains(BACKUPS_IGNORE));
        assert!(
            fs::read_to_string(root.join(".gitattributes"))
                .unwrap()
                .contains(LOG_MERGE)
        );
        assert!(
            first
                .message
                .ends_with(" other notes, settings and other files"),
            "{}",
            first.message
        );
        assert_eq!(
            ask(root, &["status", "--porcelain"]),
            "",
            "everything recorded"
        );
        assert_eq!(
            ask(root, &["log", "-1", "--format=%an <%ae>"]).trim(),
            "Story Time <story-time@localhost>",
            "this computer's git doesn't say who is writing"
        );
        let body = ask(root, &["log", "-1", "--format=%b"]);
        assert!(
            body.lines().any(|line| line == "Arrival")
                && body.lines().any(|line| line == "Leaving Home"),
            "every note is named in the body: {body}"
        );

        let text = fs::read_to_string(&arrival).unwrap();
        fs::write(&arrival, format!("{text}\nShe came ashore.\n")).unwrap();
        let edited = commit(&vault).unwrap().unwrap();
        assert_eq!(edited.message, "Arrival");
        assert_eq!(last_commit(&vault).unwrap(), Some(edited));

        vault.snapshot(time("2026-03-03T21:00:00")).unwrap();
        assert_eq!(
            commit(&vault).unwrap(),
            None,
            "the writing log waits for the next change"
        );
        fs::remove_file(&leaving).unwrap();
        fs::write(&arrival, "# Arrival\n\nShe came ashore at dawn.\n").unwrap();
        config::set_bookshelf_setting(root, "daily_goal", Some(toml::Value::Integer(800))).unwrap();
        let vault = reopen(&vault);
        let several = commit(&vault).unwrap().unwrap();
        assert_eq!(several.message, "Arrival, Leaving Home and settings");
        assert!(
            ask(root, &["show", "--stat", "--format=", "HEAD"]).contains("history.csv"),
            "the writing log went with it"
        );

        stop(&vault).unwrap();
        let vault = reopen(&vault);
        assert!(!vault.config.keep_history);
        fs::write(&arrival, "# Arrival\n\nShe came ashore at noon.\n").unwrap();
        assert_eq!(commit(&vault).unwrap(), None, "stopped");
        assert!(root.join(".git").is_dir(), "what was recorded stays");
    }

    #[test]
    fn names_a_few_notes_and_counts_the_rest() {
        if !has_git() {
            return;
        }
        let dir = TempDir::new().unwrap();
        let (vault, _, _) = shelf(dir.path());
        start(&vault).unwrap();
        let vault = reopen(&vault);
        let scenes = vault
            .root
            .join("sample-story-world/books/sample-story/scenes");
        for name in ["a", "b", "c", "d"] {
            fs::write(
                scenes.join(format!("{name}.md")),
                format!("# Scene {name}\n"),
            )
            .unwrap();
        }
        let made = commit(&vault).unwrap().unwrap();
        assert_eq!(made.message, "Scene a, Scene b and 2 other notes");
        assert_eq!(
            ask(&vault.root, &["log", "-1", "--format=%b"]).trim(),
            "Scene a\nScene b\nScene c\nScene d"
        );
    }

    #[test]
    fn commits_only_the_bookshelf_in_a_repository_that_holds_more() {
        if !has_git() {
            return;
        }
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        git_in(repo, &["init", "--quiet"], "2026-03-01T09:00:00+11:00");
        let (vault, arrival, _) = shelf(&repo.join("stories"));
        fs::write(repo.join("other.md"), "Not a story.\n").unwrap();
        commit_on(repo, "2026-03-01T10:00:00+11:00");
        assert!(
            has_repository(&vault.root).unwrap(),
            "tracked in the repository around it"
        );

        start(&vault).unwrap();
        let vault = reopen(&vault);
        assert!(
            !vault.root.join(".git").exists(),
            "no repository of its own"
        );
        fs::write(repo.join("other.md"), "Still not a story.\n").unwrap();
        git_in(repo, &["add", "other.md"], "2026-03-01T11:00:00+11:00");
        fs::write(&arrival, "# Arrival\n\nShe came ashore.\n").unwrap();
        assert_eq!(commit(&vault).unwrap().unwrap().message, "Arrival");
        assert_eq!(
            ask(repo, &["status", "--porcelain"]),
            "M  other.md\n",
            "what's outside the bookshelf is left as it was"
        );
    }

    #[test]
    fn makes_its_own_repository_inside_one_that_doesnt_track_it() {
        if !has_git() {
            return;
        }
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        git_in(home, &["init", "--quiet"], "2026-03-01T09:00:00+11:00");
        fs::write(home.join(".profile"), "export EDITOR=vi\n").unwrap();
        commit_on(home, "2026-03-01T10:00:00+11:00");
        let (vault, _, _) = shelf(&home.join("Documents/Stories"));
        assert!(!has_repository(&vault.root).unwrap());
        start(&vault).unwrap();
        assert!(vault.root.join(".git").is_dir());
        assert_eq!(
            ask(home, &["log", "--oneline"]).lines().count(),
            1,
            "home untouched"
        );
    }

    #[test]
    fn knows_cloud_storage_folders() {
        let named = |path: &str| synced_folder(Path::new(path));
        assert_eq!(named("/home/writer/OneDrive/Stories"), Some("OneDrive"));
        assert_eq!(
            named("C:/Users/writer/OneDrive - Example Ltd/Stories"),
            Some("OneDrive")
        );
        assert_eq!(
            named("/Users/writer/Library/CloudStorage/OneDrive-Personal/Stories"),
            Some("OneDrive")
        );
        assert_eq!(named("/Users/writer/Dropbox/Stories"), Some("Dropbox"));
        assert_eq!(
            named("/Users/writer/Library/Mobile Documents/com~apple~CloudDocs/Stories"),
            Some("iCloud Drive")
        );
        assert_eq!(named("/home/writer/Documents/Stories"), None);
        assert_eq!(named("/home/writer/OneDrivers/Stories"), None);
    }
}
