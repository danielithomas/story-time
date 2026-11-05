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
    fs::create_dir(dir.path().join("Scratchpad")).unwrap();
    fs::write(dir.path().join("Scratchpad/Ideas.md"), IDEAS).unwrap();
    dir
}

fn run(vault: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_story-time"))
        .args(args)
        .current_dir(vault)
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
    assert!(init.contains("created  Templates/Chapter.md"), "{init}");
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
    assert!(new.contains("Created sample-story/"), "{new}");
    let ideas = fs::read_to_string(root.join("Scratchpad/Ideas.md")).unwrap();
    assert!(
        ideas.contains("with a catch. → [[sample-story]]"),
        "{ideas}"
    );
    let note = fs::read_to_string(root.join("sample-story/sample-story.md")).unwrap();
    assert!(note.contains("A premise, with a catch."));
    let flow = fs::read_to_string(root.join("sample-story/sample-story-flow.md")).unwrap();
    assert!(
        flow.contains("### Catalyst\n\n*10% · around 9,000 words*"),
        "{flow}"
    );

    ok(root, &["add", "chapter", "Arrival"]);
    ok(root, &["add", "character", "Mara Chen", "-p", "sample"]);
    let chapter = root.join("sample-story/chapters/01 Arrival.md");
    let text = fs::read_to_string(&chapter)
        .unwrap()
        .replace("synopsis:", "synopsis: Mara arrives.")
        .replace("status: outline", "status: draft");
    fs::write(
        &chapter,
        format!(
            "{text}\n[[Mara Chen]] opened the door to the hall. %% fix %%\nNobody was there.\n"
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
        outline.contains("01  Arrival  [draft · 11 words]"),
        "{outline}"
    );
    assert!(outline.contains("Mara arrives."), "{outline}");

    let check = ok(root, &["check"]);
    assert!(check.contains("Nothing to report."), "{check}");

    ok(root, &["compile"]);
    let manuscript = fs::read_to_string(root.join(".story-time/build/sample-story.md")).unwrap();
    assert!(
        manuscript.starts_with("---\ntitle: \"Sample Story\"\n---\n\n# Arrival\n"),
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

    ok(root, &["idea", "Added:", "A", "new", "premise."]);
    let ideas = fs::read_to_string(root.join("Scratchpad/Ideas.md")).unwrap();
    assert!(
        ideas.contains("1. Other Story: Another premise.\n1. Added: A new premise.\n"),
        "{ideas}"
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
    let ambiguous = run(root, &["add", "chapter", "One"]);
    assert!(String::from_utf8_lossy(&ambiguous.stderr).contains("which project?"));
    let missing_idea = run(root, &["new", "Missing", "--from-idea"]);
    assert!(String::from_utf8_lossy(&missing_idea.stderr).contains("no idea named \"Missing\""));
    assert!(
        !root.join("missing").exists(),
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
    assert!(!root.join("Templates").exists(), "nothing was written");
}
