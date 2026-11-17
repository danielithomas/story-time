//! End-to-end: drive the binary against a throwaway vault shaped like the real one.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

const IDEAS: &str = "\n# Ideas\n\n## Ideas\n\n### Fiction\n\n1. Sample Story: A premise, with a catch.\n1. Other Story: Another premise.\n\n### Non-Fiction\n\n\n";

fn vault() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".obsidian")).unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    fs::create_dir(dir.path().join("scratchpad")).unwrap();
    fs::write(dir.path().join("scratchpad/ideas.md"), IDEAS).unwrap();
    dir
}

fn run(vault: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_story-time"))
        .args(args)
        .current_dir(vault)
        .env_remove("STORY_TIME_BOOKSHELF")
        .env_remove("STORY_TIME_VAULT")
        .output()
        .unwrap()
}

fn ok(vault: &Path, args: &[&str]) -> String {
    let output = run(vault, args);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "{args:?} failed\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

#[test]
fn a_project_from_idea_to_manuscript() {
    let dir = vault();
    let root = dir.path();

    let init = ok(root, &["init"]);
    assert!(init.contains("created  templates/scene.md"), "{init}");
    assert!(root.join(".obsidian/templates.json").is_file());

    let new = ok(
        root,
        &[
            "new",
            "Sample Story",
            "--structure",
            "save-the-cat",
            "--from-idea",
        ],
    );
    assert!(new.contains("Created sample-story-world/"), "{new}");
    let ideas = fs::read_to_string(root.join("scratchpad/ideas.md")).unwrap();
    assert!(
        ideas.contains("with a catch. → [[sample-story-world|Sample Story]]"),
        "{ideas}"
    );
    let book = root.join("sample-story-world/books/sample-story");
    let note = fs::read_to_string(book.join("sample-story.md")).unwrap();
    assert!(note.contains("A premise, with a catch."));
    let world = fs::read_to_string(root.join("sample-story-world/sample-story-world.md")).unwrap();
    assert!(
        world.contains("## Books\n\n- [[sample-story|Sample Story]]\n"),
        "{world}"
    );
    let flow = fs::read_to_string(book.join("sample-story-flow.md")).unwrap();
    assert!(
        flow.contains("### Catalyst\n\n*10% · around 9,000 words*"),
        "{flow}"
    );

    let added = ok(root, &["add", "scene", "Arrival"]);
    assert!(added.contains("as a chapter of its own"), "{added}");
    ok(root, &["add", "character", "Mara Chen", "-p", "sample"]);
    let note = fs::read_to_string(book.join("sample-story.md")).unwrap();
    assert!(note.contains("\n- [[arrival|Arrival]]\n"), "{note}");
    let scene = book.join("scenes/arrival.md");
    let text = fs::read_to_string(&scene)
        .unwrap()
        .replace("synopsis:", "synopsis: Mara arrives.")
        .replace("status: outline", "status: draft");
    fs::write(
        &scene,
        format!(
            "{text}\n[[mara-chen|Mara Chen]] opened the door to the hall. %% fix %%\nNobody was there.\n"
        ),
    )
    .unwrap();

    let stats = ok(root, &["stats", "sample"]);
    assert!(stats.contains("Arrival"), "{stats}");
    assert!(stats.contains("+11 words"), "today's words: {stats}");
    let today = ok(root, &["today"]);
    assert!(today.contains("+11 words"), "{today}");

    let outline = ok(root, &["outline"]);
    assert!(
        outline.contains("  Chapter 1\n    Arrival  [draft · 11 words]\n      Mara arrives.\n"),
        "{outline}"
    );

    let check = ok(root, &["check"]);
    assert!(check.contains("Nothing to report."), "{check}");
    fs::write(
        root.join("sample-story-world/bible/characters/Old Tom.md"),
        "Linked as [[Mara Chen]].\n",
    )
    .unwrap();
    let check = ok(root, &["check"]);
    assert!(
        check.contains("Obsidian can't follow them") && check.contains("Mara Chen"),
        "{check}"
    );
    assert!(
        check.contains("characters/Old Tom.md") && check.contains("old-tom.md"),
        "{check}"
    );
    fs::remove_file(root.join("sample-story-world/bible/characters/Old Tom.md")).unwrap();

    ok(root, &["add", "scene", "First Light", "--chapter", "1"]);
    ok(root, &["add", "scene", "Aside", "--unplaced"]);
    let note = fs::read_to_string(book.join("sample-story.md")).unwrap();
    assert!(
        note.contains("\n- Chapter\n  - [[arrival|Arrival]]\n  - [[first-light|First Light]]\n"),
        "{note}"
    );
    let stats = ok(root, &["stats", "sample"]);
    assert!(
        stats.contains("Unplaced") && stats.contains("1 scene · 0 words, not in the manuscript"),
        "{stats}"
    );
    let bad = run(root, &["add", "scene", "Lost", "--chapter", "7"]);
    assert!(
        String::from_utf8_lossy(&bad.stderr).contains("no chapter 7"),
        "{}",
        String::from_utf8_lossy(&bad.stderr)
    );

    let compiled = ok(root, &["compile"]);
    assert!(
        compiled.contains("1 chapter (1 scene, 11 words)"),
        "{compiled}"
    );
    let manuscript =
        fs::read_to_string(root.join(".story-time/build/sample-story-world/sample-story.md"))
            .unwrap();
    assert!(
        manuscript
            .starts_with("---\ntitle: \"Sample Story\"\n---\n\n# Chapter 1\n\nMara Chen opened"),
        "{manuscript}"
    );
    assert!(
        manuscript.contains("Mara Chen opened the door to the hall. \n\nNobody was there."),
        "{manuscript}"
    );
    assert!(!manuscript.contains("fix"));
    assert_eq!(
        fs::read_to_string(root.join(".gitignore")).unwrap(),
        ".story-time/build/\n"
    );

    let sequel = ok(root, &["new", "Second Story", "--in", "sample"]);
    assert!(
        sequel.contains("Added Second Story to sample-story-world/"),
        "{sequel}"
    );
    let which = run(root, &["outline"]);
    assert!(
        String::from_utf8_lossy(&which.stderr).contains("which book of Sample Story?"),
        "a project with two books needs one named"
    );
    let shared = run(root, &["add", "scene", "Arrival", "-b", "second"]);
    assert!(
        String::from_utf8_lossy(&shared.stderr).contains("has the same name"),
        "scenes in two books may share a name, with a warning"
    );
    assert!(
        root.join("sample-story-world/books/second-story/scenes/arrival.md")
            .is_file()
    );
    let stats = ok(root, &["stats", "sample"]);
    assert!(
        stats.contains("Second Story")
            && stats.contains("World bible of Sample Story: 1 character"),
        "{stats}"
    );

    ok(root, &["idea", "Added:", "A", "new", "premise."]);
    let ideas = fs::read_to_string(root.join("scratchpad/ideas.md")).unwrap();
    assert!(
        ideas.contains("1. Other Story: Another premise.\n1. Added: A new premise.\n"),
        "{ideas}"
    );
}

#[test]
fn scratchpads_and_research() {
    let dir = vault();
    let root = dir.path();
    ok(root, &["new", "Sample Story"]);
    ok(root, &["snapshot"]);

    let scrap = ok(
        root,
        &[
            "scrap",
            "add",
            "Four",
            "words",
            "kept",
            "here.",
            "--source",
            "dictation",
        ],
    );
    let path = scrap.trim().strip_prefix("Created ").unwrap();
    assert!(path.starts_with("scratchpad/"), "{scrap}");
    let note = fs::read_to_string(root.join(path)).unwrap();
    assert!(
        note.ends_with("source: dictation\n---\nFour words kept here.\n"),
        "{note}"
    );

    let piped = Command::new(env!("CARGO_BIN_EXE_story-time"))
        .args(["scrap", "add", "-", "-p", "sample"])
        .current_dir(root)
        .env_remove("STORY_TIME_BOOKSHELF")
        .env_remove("STORY_TIME_VAULT")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(b"Two lines\nexactly.\n")?;
            child.wait_with_output()
        })
        .unwrap();
    assert!(piped.status.success());
    let path = String::from_utf8_lossy(&piped.stdout);
    let path = path.trim().strip_prefix("Created ").unwrap();
    assert!(path.starts_with("sample-story-world/scratchpad/"), "{path}");
    assert!(
        fs::read_to_string(root.join(path))
            .unwrap()
            .ends_with("---\nTwo lines\nexactly.\n")
    );

    let today = ok(root, &["today"]);
    assert!(
        today.contains("Scratchpad") && today.contains("+4"),
        "{today}"
    );
    assert!(
        today.contains("Sample Story scratchpad") && today.contains("+3"),
        "{today}"
    );

    let research = ok(
        root,
        &[
            "research",
            "add",
            "Orbits",
            "-p",
            "sample",
            "--source",
            "https://example.com",
            "--agent",
        ],
    );
    assert!(
        research.contains("Created sample-story-world/research/orbits.md"),
        "{research}"
    );
    let note = fs::read_to_string(root.join("sample-story-world/research/orbits.md")).unwrap();
    assert!(note.contains("source: https://example.com\n"), "{note}");
    assert!(note.contains("> [!note] Added by an agent"), "{note}");

    let stats = ok(root, &["stats", "sample"]);
    assert!(
        stats.contains("Scratchpad: 1 note, 3 words · Research: 1 note"),
        "{stats}"
    );
}

#[test]
fn helpful_errors() {
    let dir = vault();
    let root = dir.path();
    let none = run(root, &["stats", "anything"]);
    assert!(!none.status.success());
    assert!(String::from_utf8_lossy(&none.stderr).contains("no projects yet"));

    ok(root, &["new", "Other Story", "--form", "novella"]);
    ok(root, &["new", "Sample Story"]);
    let ambiguous = run(root, &["add", "scene", "One"]);
    assert!(String::from_utf8_lossy(&ambiguous.stderr).contains("which project?"));
    let missing_idea = run(root, &["new", "Missing", "--from-idea"]);
    assert!(String::from_utf8_lossy(&missing_idea.stderr).contains("no idea named \"Missing\""));
    assert!(
        !root.join("missing-world").exists(),
        "nothing is created when the idea is missing"
    );
    let bad_form = run(root, &["new", "X", "--form", "epic"]);
    assert!(!bad_form.status.success());

    let overview = ok(root, &["stats"]);
    assert!(
        overview.contains("Other Story")
            && overview.contains("novella")
            && overview.contains("30,000"),
        "{overview}"
    );
}

#[test]
fn the_example_book() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let init = ok(root, &["init", "--example"]);
    assert!(init.contains("Added example-world/"), "{init}");
    let check = ok(root, &["check"]);
    assert!(check.contains("Nothing to report."), "{check}");
    let today = ok(root, &["today"]);
    assert!(
        today.contains("0 words"),
        "its words aren't today's: {today}"
    );
    let again = run(root, &["init", "--example"]);
    assert!(!again.status.success(), "only once");
}

#[test]
fn renames_notes_and_the_links_to_them() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    ok(root, &["init", "--example"]);
    let renamed = ok(root, &["rename", "The Writer", "The Author"]);
    assert!(
        renamed.contains(
            "Renamed The Writer to The Author: example-world/bible/characters/the-author.md."
        ),
        "{renamed}"
    );
    assert!(renamed.contains("Updated "), "{renamed}");
    let scene = "example-world/books/a-tour-of-story-time/scenes/notes-to-self.md";
    let moved = ok(root, &["rename", scene, "Notes to Myself"]);
    assert!(moved.contains("scenes/notes-to-myself.md"), "{moved}");
    let book = fs::read_to_string(
        root.join("example-world/books/a-tour-of-story-time/a-tour-of-story-time.md"),
    )
    .unwrap();
    assert!(
        book.contains("- [[notes-to-myself|Notes to Myself]]"),
        "{book}"
    );
    let check = ok(root, &["check"]);
    assert!(check.contains("Nothing to report."), "{check}");
    let outline = ok(root, &["outline"]);
    assert!(outline.contains("Notes to Myself"), "{outline}");

    let book = ok(root, &["rename", "A Tour of Story Time", "A Grand Tour"]);
    assert!(
        book.contains("example-world/books/a-grand-tour/a-grand-tour.md"),
        "{book}"
    );
    let world = ok(
        root,
        &["rename", "example-world/example-world.md", "The Example"],
    );
    assert!(world.contains("the-example/the-example.md"), "{world}");
    let check = ok(root, &["check"]);
    assert!(check.contains("Nothing to report."), "{check}");
    let outline = ok(root, &["outline"]);
    assert!(outline.contains("A Grand Tour"), "{outline}");

    let nothing = ok(root, &["rename", "the-author"]);
    assert!(nothing.contains("Nothing to rename"), "{nothing}");
    let taken = run(root, &["rename", "The Author", "The Binder"]);
    assert!(
        taken.status.success(),
        "a character may share a location's name"
    );
    let flow = "the-example/books/a-grand-tour/a-grand-tour-flow.md";
    let refused = run(root, &["rename", flow, "Other"]);
    assert!(
        !refused.status.success(),
        "a story flow is named after its book"
    );
    let unknown = run(root, &["rename", "Nobody", "Somebody"]);
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("no scene, character"));
}

#[test]
fn reports_as_json() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    ok(root, &["init", "--example"]);
    let json = |args: &[&str]| -> serde_json::Value {
        let out = ok(root, args);
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("{args:?}: {e}\n{out}"))
    };
    let stats = json(&["stats", "--json"]);
    let book = &stats["books"][0];
    assert_eq!(book["title"], "A Tour of Story Time");
    assert_eq!(book["project"]["slug"], "example-world");
    assert_eq!(book["form"], "short-story");
    assert!(book["words"].as_u64().unwrap() > 0);
    assert_eq!(stats["daily_goal"], 500);

    let world = json(&["stats", "example-world", "--json"]);
    assert_eq!(world["world_bible"]["characters"], 1);
    assert_eq!(world["books"][0]["parts"][0]["title"], "Writing");

    let outline = json(&["outline", "--json"]);
    let scene = &outline["parts"][0]["chapters"][0]["scenes"][0];
    assert_eq!(scene["title"], "Welcome to the Example");
    assert_eq!(
        scene["pov"], "The Writer",
        "a link shows as the name it shows"
    );
    assert_eq!(outline["unplaced"][0]["title"], "A Scene Set Aside");

    assert_eq!(json(&["check", "--json"])["clean"], true);
    assert_eq!(json(&["today", "--json"])["written"], 0);
    assert_eq!(
        json(&["history", "--days", "3", "--json"])["days"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    ok(root, &["idea", "Sample Story: A premise."]);
    let ideas = json(&["ideas", "--json"]);
    assert_eq!(ideas["ideas"][0]["name"], "Sample Story");
    assert_eq!(ideas["ideas"][0]["project"], serde_json::Value::Null);
    assert!(ok(root, &["ideas"]).contains("Sample Story: A premise."));
}

// Only Linux honours XDG_DATA_HOME, so elsewhere this would write to the real data folder.
#[cfg(target_os = "linux")]
#[test]
fn backs_up_outside_the_bookshelf() {
    let dir = vault();
    let data = TempDir::new().unwrap();
    let backup = || {
        let output = Command::new(env!("CARGO_BIN_EXE_story-time"))
            .arg("backup")
            .current_dir(dir.path())
            .env("XDG_DATA_HOME", data.path())
            .env("XDG_CONFIG_HOME", data.path())
            .env_remove("STORY_TIME_BOOKSHELF")
            .env_remove("STORY_TIME_VAULT")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let first = backup();
    let shelf = dir.path().file_name().unwrap().to_string_lossy();
    assert!(
        first.starts_with(&format!(
            "Backed up to {}/story-time/backups/{shelf}-",
            data.path().display()
        )),
        "{first}"
    );
    assert!(backup().starts_with("Nothing has changed"));
}

#[test]
fn finds_the_bookshelf_by_its_old_names_too() {
    let dir = vault();
    let shelf = dir.path().to_str().unwrap();
    // Run from elsewhere, with no user config to fall back on.
    let elsewhere = TempDir::new().unwrap();
    for (args, var) in [
        (vec!["--bookshelf", shelf], None),
        (vec!["--vault", shelf], None),
        (vec![], Some("STORY_TIME_BOOKSHELF")),
        (vec![], Some("STORY_TIME_VAULT")),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_story-time"));
        command
            .args(&args)
            .arg("today")
            .current_dir(elsewhere.path())
            .env("XDG_CONFIG_HOME", elsewhere.path())
            .env_remove("STORY_TIME_BOOKSHELF")
            .env_remove("STORY_TIME_VAULT");
        if let Some(var) = var {
            command.env(var, shelf);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{args:?} {var:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn refuses_a_vault_in_a_newer_format() {
    let dir = vault();
    let root = dir.path();
    fs::create_dir(root.join(".story-time")).unwrap();
    fs::write(root.join(".story-time/config.toml"), "format = 99\n").unwrap();
    for args in [&["stats"][..], &["init"][..]] {
        let output = run(root, args);
        assert!(!output.status.success(), "{args:?} should fail");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("format 99"), "{stderr}");
        assert!(stderr.contains("Update story-time"), "{stderr}");
    }
    assert!(!root.join("templates").exists(), "nothing was written");
}

#[test]
fn upgrades_a_vault_from_story_time_0_1() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir(root.join(".obsidian")).unwrap();
    fs::write(
        root.join(".obsidian/templates.json"),
        "{\n  \"folder\": \"Templates\"\n}\n",
    )
    .unwrap();
    fs::create_dir(root.join(".story-time")).unwrap();
    fs::write(
        root.join(".story-time/config.toml"),
        "ideas_file = \"Scratchpad/Ideas.md\"\ntemplates_dir = \"Templates\"\n",
    )
    .unwrap();
    fs::create_dir(root.join("Scratchpad")).unwrap();
    fs::write(root.join("Scratchpad/Ideas.md"), IDEAS).unwrap();
    fs::create_dir(root.join("Templates")).unwrap();
    let old_templates = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/templates/format-0");
    for entry in fs::read_dir(old_templates).unwrap() {
        let path = entry.unwrap().path();
        fs::copy(
            &path,
            root.join("Templates").join(path.file_name().unwrap()),
        )
        .unwrap();
    }

    for args in [&["stats"][..], &["init"][..]] {
        let output = run(root, args);
        assert!(!output.status.success(), "{args:?} should fail");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("story-time upgrade"), "{stderr}");
    }

    let dry_run = ok(root, &["upgrade", "--dry-run"]);
    assert!(
        dry_run.contains("move    Scratchpad/Ideas.md → scratchpad/ideas.md")
            && dry_run.contains("replace Templates/Chapter.md")
            && dry_run.contains("Nothing was changed"),
        "{dry_run}"
    );
    assert!(root.join("Scratchpad/Ideas.md").is_file());

    let upgraded = ok(root, &["upgrade"]);
    assert!(upgraded.contains("Upgraded."), "{upgraded}");
    assert_eq!(
        fs::read_to_string(root.join("scratchpad/ideas.md")).unwrap(),
        IDEAS
    );
    assert!(root.join("templates/scene.md").is_file());
    let config = fs::read_to_string(root.join(".story-time/config.toml")).unwrap();
    assert!(
        config.starts_with("# The layout of this bookshelf"),
        "{config}"
    );
    assert!(config.contains("format = 1"), "{config}");
    ok(root, &["idea", "Third Story: one more"]);
    let today = ok(root, &["today"]);
    assert!(
        today.contains("Scratchpad  +4"),
        "the new idea counts: {today}"
    );
    let again = run(root, &["upgrade"]);
    assert!(String::from_utf8_lossy(&again.stderr).contains("already in format 1"));
}

/// Runs git in `dir` as a test user, away from this computer's git settings, with commits dated
/// `date`.
fn git(dir: &Path, args: &[&str], date: &str) {
    let output = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", dir.join(".no-gitconfig"))
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
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fills_in_the_writing_log_from_git() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let commit = |date: &str| {
        git(root, &["add", "-A"], date);
        git(root, &["commit", "--quiet", "-m", date], date);
    };
    git(root, &["init", "--quiet"], "2025-01-06T09:00:00+00:00");
    ok(root, &["init"]);
    ok(root, &["new", "Sample Story"]);
    ok(root, &["add", "scene", "Arrival"]);
    commit("2025-01-06T20:00:00+00:00");
    let scene = root.join("sample-story-world/books/sample-story/scenes/arrival.md");
    let mut text = fs::read_to_string(&scene).unwrap();
    text.push_str("\nShe came ashore at dawn, and nobody saw her.\n");
    fs::write(&scene, &text).unwrap();
    commit("2025-01-07T21:00:00+00:00");
    // As though story-time never measured the bookshelf: the commands above logged today's counts.
    fs::remove_file(root.join(".story-time/history.csv")).unwrap();

    let looked = ok(root, &["backfill", "--dry-run"]);
    assert!(
        looked.contains("Read 2 days of git history, from 6 January to 7 January 2025."),
        "{looked}"
    );
    assert!(
        looked.contains("Tue  7 Jan 2025        ·       +9"),
        "{looked}"
    );
    assert!(looked.contains("Nothing was added (--dry-run)"), "{looked}");
    let filled = ok(root, &["backfill"]);
    assert!(
        filled.contains("Filled in the writing log: 1 day changed."),
        "{filled}"
    );
    let again = ok(root, &["backfill"]);
    assert!(again.contains("already has everything"), "{again}");
    let log = fs::read_to_string(root.join(".story-time/history.csv")).unwrap();
    assert!(log.contains("2025-01-07T21:00:00,"), "{log}");

    let elsewhere = TempDir::new().unwrap();
    ok(elsewhere.path(), &["init"]);
    let refused = run(elsewhere.path(), &["backfill"]);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("isn't in a git repository"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
}

#[test]
fn keeps_a_history() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let story_time = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_story-time"))
            .args(args)
            .current_dir(root)
            .env_remove("STORY_TIME_BOOKSHELF")
            .env_remove("STORY_TIME_VAULT")
            .env("GIT_CONFIG_GLOBAL", root.join(".no-gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap()
    };
    ok(root, &["init"]);
    ok(root, &["new", "Sample Story"]);
    let off = story_time(&["sync"]);
    assert!(!off.status.success());
    assert!(
        String::from_utf8_lossy(&off.stderr).contains("doesn't keep a history"),
        "{}",
        String::from_utf8_lossy(&off.stderr)
    );

    let config = root.join(".story-time/config.toml");
    let text = fs::read_to_string(&config).unwrap();
    assert!(
        text.contains("# keep_history = true"),
        "documented in the config file"
    );
    fs::write(
        &config,
        text.replace("# keep_history = true", "keep_history = true"),
    )
    .unwrap();
    let first = story_time(&["sync"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(root.join(".git").is_dir(), "the repository is made");
    let said = String::from_utf8_lossy(&first.stdout);
    assert!(
        said.starts_with("Recorded: ") && said.ends_with(", settings and other files.\n"),
        "{said}"
    );
    let again = story_time(&["sync"]);
    assert_eq!(
        String::from_utf8_lossy(&again.stdout),
        "Nothing new to record.\n"
    );
}

#[test]
fn lists_searches_and_shows_notes() {
    let dir = vault();
    let root = dir.path();
    ok(root, &["init", "--example"]);

    let notes = ok(root, &["notes"]);
    assert!(notes.contains("scene"), "{notes}");
    assert!(notes.contains("Scenes and Chapters"), "{notes}");
    assert!(
        !notes.contains("templates/"),
        "templates aren't notes: {notes}"
    );
    let listed: serde_json::Value = serde_json::from_str(&ok(root, &["notes", "--json"])).unwrap();
    let scene = listed["notes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|note| note["name"] == "Scenes and Chapters")
        .unwrap();
    assert_eq!(scene["kind"], "scene");
    assert!(scene["book"].is_string());

    let found = ok(root, &["search", "chapters", "contents"]);
    assert!(found.contains("Scenes and Chapters  (scene · "), "{found}");
    let found: serde_json::Value = serde_json::from_str(&ok(
        root,
        &["search", "\"no files of their own\"", "--json"],
    ))
    .unwrap();
    assert_eq!(found["results"].as_array().unwrap().len(), 1);
    assert!(ok(root, &["search", "zebra"]).contains("No note has all of: zebra"));

    let shown = ok(root, &["show", "Scenes and Chapters"]);
    let path = scene["path"].as_str().unwrap();
    assert_eq!(shown, fs::read_to_string(root.join(path)).unwrap());
    let shown: serde_json::Value =
        serde_json::from_str(&ok(root, &["show", path, "--json"])).unwrap();
    assert_eq!(shown["properties"]["status"], "revised");
    assert!(
        shown["body"]
            .as_str()
            .unwrap()
            .starts_with("# Scenes and Chapters")
    );
    let outside = run(root, &["show", "../outside.md"]);
    assert!(!outside.status.success());
}

#[test]
fn serves_agents_over_mcp() {
    use std::io::Write;
    use std::process::Stdio;

    let dir = vault();
    let root = dir.path();
    ok(root, &["init", "--example"]);
    let before = ok(root, &["notes", "--json"]);

    let mut server = Command::new(env!("CARGO_BIN_EXE_story-time"))
        .args(["mcp", "--bookshelf"])
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search","arguments":{"query":"chapters"}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"capture_note","arguments":{"text":"Said aloud.","source":"dictation"}}}"#,
    ];
    let mut stdin = server.stdin.take().unwrap();
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let output = server.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        replies.len(),
        4,
        "the notification gets no reply: {replies:?}"
    );
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), 10);
    let found: serde_json::Value =
        serde_json::from_str(replies[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(found["results"][0]["name"], "Scenes and Chapters");
    let kept = replies[3]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(kept.starts_with("Kept the note as scratchpad/"), "{kept}");

    let after: serde_json::Value = serde_json::from_str(&ok(root, &["notes", "--json"])).unwrap();
    let before: serde_json::Value = serde_json::from_str(&before).unwrap();
    assert_eq!(
        after["notes"].as_array().unwrap().len(),
        before["notes"].as_array().unwrap().len() + 1,
        "only the captured note is new"
    );
}
