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
