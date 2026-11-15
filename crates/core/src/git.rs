//! A bookshelf's git history, for a bookshelf kept in a git repository.
//!
//! Story Time runs the `git` program and only reads the repository: a past commit is copied into
//! a temporary folder, never checked out in the bookshelf, and nothing is committed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{NaiveDate, NaiveDateTime};

use crate::config::{CONFIG_FILE, META_DIR};
use crate::history::{History, Snapshot};
use crate::vault::Vault;

const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

/// Whether the bookshelf is in a git repository: its folder, or one above it, holds `.git`.
pub fn in_repository(root: &Path) -> bool {
    root.ancestors().any(|dir| dir.join(".git").exists())
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
/// the folder, and settings that would change what git prints or copies are turned off.
fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        // Files kept with Git LFS aren't notes, so there's no need to download them.
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .args([
            "-c",
            "log.showSignature=false",
            "-c",
            "core.sparseCheckout=false",
        ]);
    command
}

fn run(command: &mut Command) -> Result<String> {
    let output = command.output().map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => anyhow!("git isn't installed, or isn't on the PATH"),
        _ => anyhow!(e).context("running git"),
    })?;
    if !output.status.success() {
        bail!("git: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn has_git() -> bool {
        Command::new("git").arg("--version").output().is_ok()
    }

    /// Runs git in `dir` as a test user, away from this computer's git settings.
    fn git_in(dir: &Path, args: &[&str], date: &str) {
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

    fn commit(repo: &Path, date: &str) {
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
        commit(&repo, "2026-03-01T09:00:00+11:00");

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
        commit(&r.repo, "2026-03-02T10:00:00+11:00");
        r.write(250);
        commit(&r.repo, "2026-03-03T09:00:00+11:00");
        r.write(400);
        commit(&r.repo, "2026-03-03T23:30:00+11:00");
        fs::write(r.repo.join("notes.md"), "Outside the bookshelf.\n").unwrap();
        commit(&r.repo, "2026-03-04T12:00:00+11:00");
        r.write(450);
        commit(&r.repo, "2026-03-05T08:00:00-05:00");
        r.write(9999);
        commit(&r.repo, "2026-03-09T08:00:00+11:00");

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
        commit(&r.repo, "2026-03-02T10:00:00+11:00");
        r.vault.snapshot(time("2026-03-02T21:00:00")).unwrap();
        r.write(300);
        commit(&r.repo, "2026-03-03T22:00:00+11:00");
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
        commit(root, "2026-03-01T10:00:00+11:00");
        fs::write(
            vault.scratchpad_dir().join("line.md"),
            "one two three four\n",
        )
        .unwrap();
        commit(root, "2026-03-02T10:00:00+11:00");
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
}
