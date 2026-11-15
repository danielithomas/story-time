//! Syncing a bookshelf that keeps a history with other computers, through a git remote: a
//! repository on a service such as GitHub that each computer pulls from and pushes to.
//!
//! A sync records the bookshelf's changes, fetches the other computers' and merges them, then
//! pushes. Prose is never merged: when a note changed on both sides, this computer's version is
//! kept as it was, and the merge commit says so in a `Story-Time-Kept:` line, so every computer
//! that has the merge knows the note is waiting for the writer to choose. Choosing records a
//! `Story-Time-Decided:` line. Everything else merges as git merges it, keeping this computer's
//! side of any clash; the writing log keeps both sides' lines (`.gitattributes`).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};

use crate::config::{CONFIG_FILE, META_DIR};
use crate::git::{commit_everything, git, identity, output, run};
use crate::vault::Vault;

/// The remote a bookshelf syncs with.
const REMOTE: &str = "origin";
const KEPT: &str = "Story-Time-Kept: ";
const DECIDED: &str = "Story-Time-Decided: ";

/// The address the bookshelf syncs with, if it has one.
pub fn address(vault: &Vault) -> Option<String> {
    if !vault.root.join(".git").exists() {
        return None;
    }
    run(git(&vault.root).args(["remote", "get-url", REMOTE]))
        .ok()
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
}

/// What a sync did.
#[derive(Debug, Default)]
pub struct Synced {
    /// Files the other computers' changes rewrote, added or removed, as full paths.
    pub changed: Vec<PathBuf>,
    /// Notes changed here and on another computer: this computer's version was kept.
    pub kept: Vec<PathBuf>,
}

/// Starts syncing with the repository at `address`, which may be empty or hold this bookshelf's
/// history already. One that holds a different bookshelf is refused, as merging two would mix
/// their worlds. The first sync follows.
pub fn connect(vault: &Vault, address: &str) -> Result<Synced> {
    let root = &vault.root;
    let address = address.trim();
    if address.is_empty() {
        bail!("paste the address of the repository to sync with");
    }
    if !vault.config.keep_history {
        bail!("turn on Keep a history first: syncing shares the history with other computers");
    }
    if !root.join(".git").exists() {
        bail!(
            "this bookshelf is part of a larger git repository, so Story Time leaves syncing it \
             to you"
        );
    }
    network(address, git(root).args(["ls-remote", "--heads", address]))?;
    let set = if address_of(root).is_some() {
        "set-url"
    } else {
        "add"
    };
    run(git(root).args(["remote", set, REMOTE, address]))?;
    let connected = (|| {
        fetch(vault)?;
        let theirs = their_branch(vault)?;
        if let Some(theirs) = &theirs
            && merge_base(root, "HEAD", theirs)?.is_none()
            && run(git(root).args([
                "cat-file",
                "-e",
                &format!("{theirs}:{META_DIR}/{CONFIG_FILE}"),
            ]))
            .is_ok()
        {
            bail!(
                "that repository already holds a bookshelf. To use it on this computer, choose \
                 Open a bookshelf from another computer when Story Time starts"
            );
        }
        sync(vault)
    })();
    if connected.is_err() {
        run(git(root).args(["remote", "remove", REMOTE]))?;
    }
    connected
}

/// Stops syncing. The other computers' copies, and the repository synced with, stay as they are.
pub fn disconnect(vault: &Vault) -> Result<()> {
    run(git(&vault.root).args(["remote", "remove", REMOTE])).map(drop)
}

/// Syncs: records this computer's changes, merges the other computers', and shares the result.
/// If another computer shared something in the meantime, it merges and shares once more.
pub fn sync(vault: &Vault) -> Result<Synced> {
    let mut synced = Synced::default();
    for _ in 0..2 {
        fetch(vault)?;
        let merged = merge(vault)?;
        synced.changed.extend(merged.changed);
        synced.kept.extend(merged.kept);
        if push(vault)? {
            return Ok(synced);
        }
    }
    bail!("another computer kept sharing changes at the same moment; sync again in a minute")
}

/// Records this computer's changes and fetches the other computers'. Nothing in the bookshelf
/// changes but its history, so this can take its time while the writer works.
pub fn fetch(vault: &Vault) -> Result<()> {
    let address = syncing(vault)?;
    commit_everything(vault)?;
    network(
        &address,
        git(&vault.root).args(["fetch", "--quiet", "--prune", REMOTE]),
    )?;
    Ok(())
}

/// Merges what [`fetch`] brought into the bookshelf, after recording any change made since. Quick,
/// and the only step that changes files, so front ends hold their saves while it runs.
pub fn merge(vault: &Vault) -> Result<Synced> {
    syncing(vault)?;
    let root = &vault.root;
    commit_everything(vault)?;
    let Some(theirs) = their_branch(vault)? else {
        return Ok(Synced::default());
    };
    let head = rev(root, "HEAD")?;
    let other = rev(root, &theirs)?;
    if is_ancestor(root, &other, &head)? {
        return Ok(Synced::default());
    }
    let full = |paths: Vec<String>| paths.into_iter().map(|path| root.join(path)).collect();
    if is_ancestor(root, &head, &other)? {
        run(git(root).args(["merge", "--ff-only", "--quiet", &other]))?;
        return Ok(Synced {
            changed: full(names(root, &["diff", "--name-only", "-z", &head, &other])?),
            kept: Vec::new(),
        });
    }
    let base = merge_base(root, &head, &other)?;
    let kept = changed_on_both(vault, base.as_deref(), &head, &other)?;
    let merged = (|| {
        let mut command = git(root);
        command.args(identity(vault)).args([
            "merge",
            "--no-ff",
            "--no-commit",
            "--quiet",
            "-X",
            "ours",
        ]);
        if base.is_none() {
            command.arg("--allow-unrelated-histories");
        }
        // A clash git can't settle, such as a note changed here and deleted there, stops the
        // merge with the clash marked; it's settled below. A merge that can't start says why.
        let started = output(command.arg(&other))?;
        if rev(root, "MERGE_HEAD").is_err() {
            let said = String::from_utf8_lossy(&started.stderr).into_owned();
            return Err(anyhow!(
                "the other computer's changes couldn't be merged: {}",
                said.trim()
            ));
        }
        for path in &kept {
            run(git(root).args(["checkout", &head, "--", path]))?;
        }
        // A note changed on one side and deleted on the other is kept: words are never lost.
        for path in names(root, &["diff", "--name-only", "-z", "--diff-filter=U"])? {
            if root.join(&path).exists() {
                run(git(root).args(["add", "--", &path]))?;
            } else {
                run(git(root).args(["rm", "--quiet", "--cached", "--", &path]))?;
            }
        }
        let mut message = "Synced with another computer".to_string();
        if !kept.is_empty() {
            message.push_str("\n\nChanged on both computers; this one's version was kept:\n");
            for path in &kept {
                message.push_str(&format!("{KEPT}{path}\n"));
            }
        }
        run(git(root)
            .args(identity(vault))
            .args(["commit", "--quiet", "-m", &message]))
    })();
    if let Err(error) = merged {
        let _ = run(git(root).args(["merge", "--abort"]));
        return Err(error);
    }
    Ok(Synced {
        changed: full(names(root, &["diff", "--name-only", "-z", &head, "HEAD"])?),
        kept: full(kept),
    })
}

/// Shares this computer's history. False if another computer shared something first, which must
/// be fetched and merged before this can go.
pub fn push(vault: &Vault) -> Result<bool> {
    let address = syncing(vault)?;
    let pushed =
        output(git(&vault.root).args(["push", "--porcelain", "--set-upstream", REMOTE, "HEAD"]))?;
    if pushed.status.success() {
        return Ok(true);
    }
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&pushed.stdout),
        String::from_utf8_lossy(&pushed.stderr)
    );
    if said.contains("[rejected]") || said.contains("non-fast-forward") {
        return Ok(false);
    }
    Err(explain(&address, &said))
}

/// Starts sharing this computer's history without waiting for it to finish, for when Story Time
/// closes. Anything left is shared at the next sync.
pub fn push_in_background(vault: &Vault) -> Result<()> {
    syncing(vault)?;
    git(&vault.root)
        .args(["push", "--quiet", "--set-upstream", REMOTE, "HEAD"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("running git")?;
    Ok(())
}

/// A note waiting for the writer to choose between this computer's version and another's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    pub path: PathBuf,
    /// The commit holding the other computer's version.
    pub other: String,
}

/// Notes changed on two computers before they synced, still waiting for the writer to choose
/// which version to keep, oldest first. A note deleted since isn't waiting.
pub fn waiting(vault: &Vault) -> Result<Vec<Waiting>> {
    let root = &vault.root;
    if !root.join(".git").exists() || rev(root, "HEAD").is_err() {
        return Ok(Vec::new());
    }
    let found = run(git(root).args([
        "log",
        "--format=%H%x00%B%x01",
        "--grep=^Story-Time-",
        "HEAD",
    ]))?;
    let mut decided: HashSet<&str> = HashSet::new();
    let mut waiting: BTreeMap<&str, String> = BTreeMap::new();
    // Newest first: a note's newest line says where it stands.
    for entry in found.split('\u{1}') {
        let Some((commit, message)) = entry.trim_start().split_once('\0') else {
            continue;
        };
        for line in message.lines() {
            if let Some(path) = line.strip_prefix(DECIDED) {
                decided.insert(path.trim());
            } else if let Some(path) = line.strip_prefix(KEPT) {
                let path = path.trim();
                if !decided.contains(path) && !waiting.contains_key(path) {
                    waiting.insert(path, format!("{commit}^2"));
                }
            }
        }
    }
    let mut waiting: Vec<Waiting> = waiting
        .into_iter()
        .map(|(path, other)| Waiting {
            path: root.join(path),
            other,
        })
        .filter(|waiting| waiting.path.is_file())
        .collect();
    waiting.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(waiting)
}

/// The other computer's version of a note that is waiting, as it was written.
pub fn other_version(vault: &Vault, note: &Path) -> Result<String> {
    let found = find_waiting(vault, note)?;
    run(git(&vault.root).args([
        "show",
        &format!("{}:{}", found.other, relative(vault, note)?),
    ]))
}

/// Settles a waiting note: with `use_other`, its text becomes the other computer's version,
/// after this computer's is recorded; otherwise this computer's stays. Either way the choice is
/// recorded, so every computer stops asking.
pub fn decide(vault: &Vault, note: &Path, use_other: bool) -> Result<()> {
    let root = &vault.root;
    let found = find_waiting(vault, note)?;
    let path = relative(vault, note)?;
    commit_everything(vault)?;
    let name = crate::note::Note::load(note)?.title();
    if use_other {
        let text = run(git(root).args(["show", &format!("{}:{path}", found.other)]))?;
        fs::write(note, text).with_context(|| format!("writing {}", note.display()))?;
        run(git(root).args(["add", "--", &path]))?;
    }
    let subject = if use_other {
        format!("{name}: used the other computer's version")
    } else {
        format!("{name}: kept this computer's version")
    };
    run(git(root).args(identity(vault)).args([
        "commit",
        "--quiet",
        "--allow-empty",
        "-m",
        &subject,
        "-m",
        &format!("{DECIDED}{path}"),
    ]))
    .map(drop)
}

/// Copies a bookshelf from the repository at `address` into a new folder in `parent`, named after
/// the repository, for a second computer. Returns the folder.
pub fn clone(address: &str, parent: &Path) -> Result<PathBuf> {
    let address = address.trim();
    let name = address
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .map(|name| name.trim_end_matches(".git"))
        .filter(|name| !name.is_empty())
        .unwrap_or("Stories");
    let folder = parent.join(name);
    if folder.exists() {
        bail!(
            "{} already exists. Choose another place, or open that folder instead",
            folder.display()
        );
    }
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    network(
        address,
        git(parent)
            .args(["clone", "--quiet", "--", address])
            .arg(&folder),
    )?;
    if !folder.join(META_DIR).join(CONFIG_FILE).is_file() {
        fs::remove_dir_all(&folder).with_context(|| format!("removing {}", folder.display()))?;
        bail!("there's no bookshelf at that address");
    }
    Ok(folder)
}

/// The sync address, or why the bookshelf doesn't sync.
fn syncing(vault: &Vault) -> Result<String> {
    if !vault.config.keep_history {
        bail!("this bookshelf doesn't keep a history, so there's nothing to sync");
    }
    address(vault).ok_or_else(|| anyhow!("this bookshelf doesn't sync with another computer"))
}

fn address_of(root: &Path) -> Option<String> {
    run(git(root).args(["remote", "get-url", REMOTE])).ok()
}

/// The fetched branch to merge: the remote's branch named like this one, else, when this
/// bookshelf's history is new to it, the remote's main branch.
fn their_branch(vault: &Vault) -> Result<Option<String>> {
    let root = &vault.root;
    let branch = run(git(root).args(["symbolic-ref", "--short", "HEAD"]))?;
    let theirs = format!("{REMOTE}/{}", branch.trim());
    if rev(root, &theirs).is_ok() {
        return Ok(Some(theirs));
    }
    let remote = run(git(root).args(["branch", "--remotes", "--format=%(refname:short)"]))?;
    let branches: Vec<&str> = remote
        .lines()
        .map(str::trim)
        .filter(|name| name.starts_with(&format!("{REMOTE}/")) && *name != REMOTE)
        .filter(|name| !name.ends_with("/HEAD"))
        .collect();
    Ok(match branches.as_slice() {
        [only] => {
            // Take the remote's name for the branch, so pushing updates it.
            let name = only.trim_start_matches(&format!("{REMOTE}/"));
            run(git(root).args(["branch", "--move", name]))?;
            Some(only.to_string())
        }
        _ => None,
    })
}

/// Notes (Markdown outside hidden folders and the templates) that both sides changed since
/// `base` and that both still have, with different text: paths relative to the bookshelf.
fn changed_on_both(
    vault: &Vault,
    base: Option<&str>,
    head: &str,
    other: &str,
) -> Result<Vec<String>> {
    let root = &vault.root;
    let since = |side: &str| -> Result<BTreeSet<String>> {
        Ok(match base {
            Some(base) => names(
                root,
                &["diff", "--name-only", "-z", "--no-renames", base, side],
            )?,
            None => names(root, &["ls-tree", "-r", "--name-only", "-z", side])?,
        }
        .into_iter()
        .collect())
    };
    let (mine, theirs) = (since(head)?, since(other)?);
    let differ = run(git(root).args(["diff", "--name-status", "-z", "--no-renames", head, other]))?;
    let fields: Vec<&str> = differ.split('\0').collect();
    let templates = format!("{}/", vault.config.templates_dir.trim_end_matches('/'));
    Ok(fields
        .chunks(2)
        .filter_map(|pair| match pair {
            ["M", path] => Some(path.to_string()),
            _ => None,
        })
        .filter(|path| mine.contains(path) && theirs.contains(path))
        .filter(|path| {
            path.ends_with(".md")
                && !path.starts_with(&templates)
                && !path.split('/').any(|part| part.starts_with('.'))
        })
        .collect())
}

fn names(root: &Path, args: &[&str]) -> Result<Vec<String>> {
    Ok(run(git(root).args(args))?
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(String::from)
        .collect())
}

fn rev(root: &Path, name: &str) -> Result<String> {
    Ok(
        run(git(root).args(["rev-parse", "--verify", "--quiet", name]))?
            .trim()
            .to_string(),
    )
}

fn merge_base(root: &Path, a: &str, b: &str) -> Result<Option<String>> {
    let found = output(git(root).args(["merge-base", a, b]))?;
    Ok(found
        .status
        .success()
        .then(|| String::from_utf8_lossy(&found.stdout).trim().to_string()))
}

fn is_ancestor(root: &Path, ancestor: &str, of: &str) -> Result<bool> {
    Ok(
        output(git(root).args(["merge-base", "--is-ancestor", ancestor, of]))?
            .status
            .success(),
    )
}

fn find_waiting(vault: &Vault, note: &Path) -> Result<Waiting> {
    waiting(vault)?
        .into_iter()
        .find(|waiting| waiting.path == note)
        .ok_or_else(|| anyhow!("this note isn't waiting for a choice between two versions"))
}

/// A note's path in the repository, which is the bookshelf.
fn relative(vault: &Vault, note: &Path) -> Result<String> {
    Ok(note
        .strip_prefix(&vault.root)
        .with_context(|| format!("{} isn't in the bookshelf", note.display()))?
        .to_string_lossy()
        .replace('\\', "/"))
}

/// Runs git against the repository at `address`, giving up on a connection that stalls, and
/// explains a failure in plain words.
fn network(address: &str, command: &mut Command) -> Result<String> {
    let done = output(
        command
            .env("GIT_CONFIG_COUNT", "2")
            .env("GIT_CONFIG_KEY_0", "http.lowSpeedLimit")
            .env("GIT_CONFIG_VALUE_0", "1000")
            .env("GIT_CONFIG_KEY_1", "http.lowSpeedTime")
            .env("GIT_CONFIG_VALUE_1", "30"),
    )?;
    if done.status.success() {
        return Ok(String::from_utf8_lossy(&done.stdout).into_owned());
    }
    Err(explain(address, &String::from_utf8_lossy(&done.stderr)))
}

/// A failed sync in plain words, with what to do next.
fn explain(address: &str, said: &str) -> anyhow::Error {
    let host = host(address);
    let has = |patterns: &[&str]| patterns.iter().any(|pattern| said.contains(pattern));
    if has(&[
        "Authentication failed",
        "could not read Username",
        "could not read Password",
        "terminal prompts disabled",
        "Permission denied",
        "returned error: 401",
        "returned error: 403",
        "Host key verification failed",
    ]) {
        anyhow!(
            "couldn't sign in to {host}. Set up signing in to it on this computer (GitHub Desktop \
             or Git Credential Manager can do it for GitHub), then sync again"
        )
    } else if has(&[
        "Repository not found",
        "not found",
        "does not exist",
        "does not appear to be a git repository",
        "returned error: 404",
    ]) {
        anyhow!("nothing was found at {address}. Check the address, then try again")
    } else if has(&[
        "Could not resolve host",
        "Couldn't resolve host",
        "Could not resolve hostname",
        "Network is unreachable",
        "Connection timed out",
        "timed out",
        "Failed to connect",
        "Connection refused",
        "Operation too slow",
    ]) {
        anyhow!(
            "couldn't reach {host}. Your work is safe on this computer, and will sync when the \
             connection is back"
        )
    } else {
        let line = said
            .lines()
            .map(|line| {
                line.trim()
                    .trim_start_matches("fatal: ")
                    .trim_start_matches("error: ")
            })
            .find(|line| !line.is_empty())
            .unwrap_or("git failed without saying why");
        anyhow!("{line}")
    }
}

/// The computer an address points at: `github.com` for `https://github.com/writer/stories` or
/// `git@github.com:writer/stories.git`, or the address itself for a folder.
fn host(address: &str) -> &str {
    let rest = address.split_once("://").map_or(address, |(_, rest)| rest);
    let rest = rest.split_once('@').map_or(rest, |(_, rest)| rest);
    match rest.split_once(['/', ':']) {
        Some((host, _)) if !host.is_empty() && !address.starts_with('/') => host,
        _ => address,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git;
    use crate::git::tests::{ask, git_in, has_git, reopen, shelf};
    use tempfile::TempDir;

    /// Two computers sharing a bookshelf through a bare repository standing in for GitHub.
    struct Two {
        _dir: TempDir,
        remote: String,
        a: Vault,
        b: Vault,
        arrival_a: PathBuf,
        arrival_b: PathBuf,
    }

    fn two() -> Two {
        let dir = TempDir::new().unwrap();
        let remote = dir.path().join("stories.git");
        fs::create_dir(&remote).unwrap();
        git_in(
            &remote,
            &["init", "--quiet", "--bare"],
            "2026-03-01T09:00:00+11:00",
        );
        let remote = remote.display().to_string();
        let (a, arrival_a, _) = shelf(&dir.path().join("a/Stories"));
        git::start(&a).unwrap();
        let a = reopen(&a);
        connect(&a, &remote).unwrap();
        let b = clone(&remote, &dir.path().join("b")).unwrap();
        let b = Vault::open(b).unwrap();
        assert!(
            b.config.keep_history,
            "the setting travels with the bookshelf"
        );
        let arrival_b = b.root.join(a.relative(&arrival_a));
        Two {
            _dir: dir,
            remote,
            a,
            b,
            arrival_a,
            arrival_b,
        }
    }

    fn write(path: &Path, body: &str) {
        let text = fs::read_to_string(path).unwrap();
        fs::write(path, crate::note::replace_body(&text, body)).unwrap();
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn brings_one_computers_writing_to_the_other() {
        if !has_git() {
            return;
        }
        let t = two();
        assert_eq!(address(&t.a).as_deref(), Some(t.remote.as_str()));
        assert_eq!(address(&t.b).as_deref(), Some(t.remote.as_str()));

        write(&t.arrival_a, "# Arrival\n\nShe came ashore at dawn.\n");
        let synced = sync(&t.a).unwrap();
        assert!(synced.changed.is_empty() && synced.kept.is_empty());
        let synced = sync(&t.b).unwrap();
        assert_eq!(
            synced.changed,
            std::slice::from_ref(&t.arrival_b),
            "only the scene changed"
        );
        assert!(read(&t.arrival_b).contains("She came ashore at dawn."));
        assert!(synced.kept.is_empty());
        assert_eq!(ask(&t.b.root, &["status", "--porcelain"]), "");
    }

    #[test]
    fn keeps_this_computers_version_of_a_note_changed_on_both() {
        if !has_git() {
            return;
        }
        let t = two();
        let leaving_b =
            t.b.root
                .join("sample-story-world/books/sample-story/scenes/leaving-home.md");
        write(&t.arrival_a, "# Arrival\n\nShe came ashore at dawn.\n");
        sync(&t.a).unwrap();
        write(&t.arrival_b, "# Arrival\n\nShe came ashore at night.\n");
        write(&leaving_b, "# Leaving Home\n\nShe packed in the dark.\n");
        let synced = sync(&t.b).unwrap();
        assert_eq!(synced.kept, std::slice::from_ref(&t.arrival_b));
        assert!(
            read(&t.arrival_b).contains("at night") && !read(&t.arrival_b).contains("at dawn"),
            "prose is never merged: {}",
            read(&t.arrival_b)
        );
        let waiting_b = waiting(&t.b).unwrap();
        assert_eq!(waiting_b.len(), 1);
        assert_eq!(waiting_b[0].path, t.arrival_b);
        assert!(
            other_version(&t.b, &t.arrival_b)
                .unwrap()
                .contains("at dawn")
        );

        sync(&t.a).unwrap();
        assert!(read(&t.arrival_a).contains("at night"), "A takes the merge");
        assert!(
            read(
                &t.a.root
                    .join("sample-story-world/books/sample-story/scenes/leaving-home.md")
            )
            .contains("packed in the dark")
        );
        assert_eq!(
            waiting(&t.a).unwrap().len(),
            1,
            "every computer with the merge asks"
        );

        decide(&t.a, &t.arrival_a, true).unwrap();
        assert!(read(&t.arrival_a).contains("at dawn") && !read(&t.arrival_a).contains("at night"));
        assert!(waiting(&t.a).unwrap().is_empty());
        sync(&t.a).unwrap();
        sync(&t.b).unwrap();
        assert!(
            waiting(&t.b).unwrap().is_empty(),
            "a choice made on one clears both"
        );
        assert!(read(&t.arrival_b).contains("at dawn"));
    }

    #[test]
    fn keeping_this_version_is_remembered_too() {
        if !has_git() {
            return;
        }
        let t = two();
        write(&t.arrival_a, "# Arrival\n\nDawn.\n");
        sync(&t.a).unwrap();
        write(&t.arrival_b, "# Arrival\n\nNight.\n");
        sync(&t.b).unwrap();
        decide(&t.b, &t.arrival_b, false).unwrap();
        assert!(read(&t.arrival_b).contains("Night."));
        assert!(waiting(&t.b).unwrap().is_empty());
        assert!(
            decide(&t.b, &t.arrival_b, false).is_err(),
            "nothing left to choose"
        );
    }

    #[test]
    fn keeps_a_note_changed_on_one_computer_and_deleted_on_the_other() {
        if !has_git() {
            return;
        }
        let t = two();
        fs::remove_file(&t.arrival_a).unwrap();
        sync(&t.a).unwrap();
        write(&t.arrival_b, "# Arrival\n\nWords worth keeping.\n");
        let synced = sync(&t.b).unwrap();
        assert!(synced.kept.is_empty());
        assert!(read(&t.arrival_b).contains("Words worth keeping."));
        sync(&t.a).unwrap();
        assert!(t.arrival_a.exists(), "the changed note comes back");
    }

    #[test]
    fn the_writing_log_keeps_both_computers_lines() {
        if !has_git() {
            return;
        }
        let t = two();
        let time = |text| chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").unwrap();
        write(&t.arrival_a, "# Arrival\n\nOne two three.\n");
        t.a.snapshot(time("2026-03-02T21:00:00")).unwrap();
        sync(&t.a).unwrap();
        write(&t.arrival_b, "# Arrival\n\nFour five.\n");
        t.b.snapshot(time("2026-03-02T22:00:00")).unwrap();
        sync(&t.b).unwrap();
        let log = read(&t.b.history_path());
        assert!(
            log.contains("2026-03-02T21:00:00") && log.contains("2026-03-02T22:00:00"),
            "{log}"
        );
    }

    #[test]
    fn refuses_a_repository_holding_another_bookshelf() {
        if !has_git() {
            return;
        }
        let t = two();
        let dir = TempDir::new().unwrap();
        let (other, _, _) = shelf(&dir.path().join("Other"));
        git::start(&other).unwrap();
        let other = reopen(&other);
        let error = connect(&other, &t.remote).unwrap_err();
        assert!(
            error.to_string().contains("already holds a bookshelf"),
            "{error}"
        );
        assert_eq!(address(&other), None, "left unconnected");
    }

    #[test]
    fn says_why_a_sync_failed() {
        if !has_git() {
            return;
        }
        let dir = TempDir::new().unwrap();
        let (vault, _, _) = shelf(&dir.path().join("Stories"));
        let error = connect(&vault, "/nowhere/stories.git").unwrap_err();
        assert!(
            error.to_string().contains("Keep a history first"),
            "{error}"
        );
        git::start(&vault).unwrap();
        let vault = reopen(&vault);
        let error = connect(&vault, "/nowhere/stories.git").unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("nothing was found at /nowhere/stories.git"),
            "{error}"
        );
        assert!(
            sync(&vault)
                .unwrap_err()
                .to_string()
                .contains("doesn't sync")
        );
        let error = clone("/nowhere/stories.git", dir.path()).unwrap_err();
        assert!(
            error.to_string().starts_with("nothing was found"),
            "{error}"
        );
    }

    #[test]
    fn explains_failures_in_plain_words() {
        let said = |address, stderr| explain(address, stderr).to_string();
        assert!(
            said(
                "https://github.com/writer/stories.git",
                "fatal: could not read Username for 'https://github.com': terminal prompts disabled"
            )
            .starts_with("couldn't sign in to github.com.")
        );
        assert!(
            said(
                "git@github.com:writer/stories.git",
                "git@github.com: Permission denied (publickey)."
            )
            .starts_with("couldn't sign in to github.com.")
        );
        assert!(
            said(
                "https://gitlab.com/writer/stories",
                "fatal: unable to access 'https://gitlab.com/writer/stories/': Could not resolve host: gitlab.com"
            )
            .starts_with("couldn't reach gitlab.com.")
        );
        assert_eq!(
            said("https://example.com/x", "error: something odd\n"),
            "something odd"
        );
    }
}
