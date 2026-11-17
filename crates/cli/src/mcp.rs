//! `story-time mcp`: a Model Context Protocol server on standard input and output, for an agent
//! that helps around the writing but doesn't write it. Its tools capture notes, add research,
//! and read: listing, searching and reading notes, outlines, word counts, `check` and the ideas.
//! No tool creates, changes, moves or deletes a scene, chapter, world-bible note or story flow,
//! so an agent that reaches the bookshelf only through this server can't either.
//!
//! Messages are JSON-RPC 2.0, one to a line. Each tool opens the bookshelf afresh, so changes
//! made in the app show at once.

use std::io::{self, BufRead, Write};
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use chrono::Local;
use serde_json::{Map, Value, json};
use story_time_core::Vault;
use story_time_core::scaffold::{self, NewResearch};
use story_time_core::search;

use crate::reports;

/// The protocol versions this server speaks, newest first. Its tools work the same in each.
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Story Time keeps a writer's bookshelf of Markdown notes: books made \
of scenes, each world's bible of characters, locations and world notes, research, scratchpads \
and an ideas file. It is for a human to write in. You can help around the writing: capture \
notes the writer dictates, add research with its source, and list, search and read the notes, \
outlines, word counts, check results and ideas. Nothing here creates, changes, moves or deletes \
the story; offer suggestions in your reply instead, for the writer to make.";

/// Answers the client on standard input and output until it closes.
pub fn serve(root: &Path) -> Result<()> {
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = reply(root, &line) {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// The reply to one line from the client, if it needs one: notifications don't.
pub fn reply(root: &Path, line: &str) -> Option<Value> {
    match serde_json::from_str::<Value>(line) {
        Err(e) => Some(error(Value::Null, -32700, &format!("Parse error: {e}"))),
        Ok(Value::Array(batch)) => {
            let replies: Vec<Value> = batch.iter().filter_map(|m| answer(root, m)).collect();
            (!replies.is_empty()).then_some(Value::Array(replies))
        }
        Ok(message) => answer(root, &message),
    }
}

fn answer(root: &Path, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned()?;
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A reply to a request this server never makes.
        let reply = message.get("result").is_some() || message.get("error").is_some();
        return (!reply).then(|| error(id, -32600, "Invalid request"));
    };
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    let result = match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call(root, &params),
        _ => Err((-32601, format!("Method not found: {method}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error(id, code, &message),
    })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|asked| VERSIONS.contains(asked))
        .unwrap_or(VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "story-time",
            "title": "Story Time",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": INSTRUCTIONS,
    })
}

fn tools() -> Vec<Value> {
    let reads = json!({ "readOnlyHint": true, "openWorldHint": false });
    let adds = json!({
        "readOnlyHint": false,
        "destructiveHint": false,
        "idempotentHint": false,
        "openWorldHint": false,
    });
    let text = |description: &str| json!({ "type": "string", "description": description });
    let project = |description: &str| {
        text(&format!(
            "{description} A project is a world with its books; name it by its title or file name."
        ))
    };
    let schema = |properties: Value, required: &[&str]| json!({ "type": "object", "properties": properties, "required": required });
    let tool = |name: &str, title: &str, description: &str, input: Value, hints: &Value| {
        json!({
            "name": name,
            "title": title,
            "description": description,
            "inputSchema": input,
            "annotations": hints,
        })
    };
    vec![
        tool(
            "capture_note",
            "Capture a note",
            "Keep a note in a scratchpad, such as something the writer dictates. It becomes a new \
             note named by the time, with its text exactly as given; no existing note changes. \
             The writer decides later whether to use it.",
            schema(
                json!({
                    "text": text("The note, exactly as it should be kept."),
                    "project": project("Put it in this project's scratchpad instead of the bookshelf's."),
                    "source": text("Where it came from, such as `dictation`. Defaults to `agent`."),
                }),
                &["text"],
            ),
            &adds,
        ),
        tool(
            "add_research",
            "Add research",
            "Add a research note: something true in the real world (physics, history, a \
             reference), with the source it came from. It becomes a new note, opening with a \
             callout asking the writer to check it against its source; no existing note changes.",
            schema(
                json!({
                    "name": text("What it is about: the note's name."),
                    "text": text("What it says, in Markdown."),
                    "source": text("Where it came from: a URL or citation."),
                    "project": project("Put it in this project's research instead of the bookshelf's."),
                }),
                &["name", "text", "source"],
            ),
            &adds,
        ),
        tool(
            "list_notes",
            "List notes",
            "Every note on the bookshelf, or in one project, with its path, name, other names, \
             kind (project, book, flow, scene, character, location, world, timeline, research, \
             scrap or note) and the project and book it belongs to.",
            schema(
                json!({ "project": project("Only this project's notes.") }),
                &[],
            ),
            &reads,
        ),
        tool(
            "search",
            "Search notes",
            "The notes that have every word searched for, in any case, in their text or \
             properties; a \"quoted phrase\" counts as one. Notes named by a word come first, \
             each with the first few lines the words appear in.",
            schema(
                json!({
                    "query": text("The words to look for."),
                    "project": project("Only this project's notes."),
                }),
                &["query"],
            ),
            &reads,
        ),
        tool(
            "read_note",
            "Read a note",
            "A note's full text as written, properties and all.",
            schema(
                json!({ "note": text("The note: its name, its file name, or its path on the bookshelf.") }),
                &["note"],
            ),
            &reads,
        ),
        tool(
            "outline",
            "Outline a book",
            "A book's scenes in order, part by part and chapter by chapter, with each scene's \
             status, point of view, location, time, synopsis and words, and its unplaced scenes.",
            schema(
                json!({
                    "project": project("The project (default: the only one)."),
                    "book": text("The book, by title or file name (default: the project's only one)."),
                }),
                &[],
            ),
            &reads,
        ),
        tool(
            "stats",
            "Word counts",
            "Every book's words, target and progress, or with a project, its books in detail: \
             pace against a deadline, chapters, the world bible's size and the scratchpad.",
            schema(
                json!({ "project": project("Report on this project in detail.") }),
                &[],
            ),
            &reads,
        ),
        tool(
            "today",
            "Today's words",
            "Words written today against the daily goal, by book and scratchpad, and the \
             writing streak.",
            schema(json!({}), &[]),
            &reads,
        ),
        tool(
            "check",
            "Check a world",
            "A project's world-bible gaps and loose ends: links to notes that don't exist yet, \
             notes never mentioned in a scene, contents that don't match the scenes, scenes \
             missing a status or synopsis, and spellings close to a world-bible name.",
            schema(
                json!({ "project": project("The project (default: the only one).") }),
                &[],
            ),
            &reads,
        ),
        tool(
            "list_ideas",
            "List ideas",
            "The ideas in the writer's ideas file, with the project each became.",
            schema(json!({}), &[]),
            &reads,
        ),
    ]
}

fn call(root: &Path, params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602, "Missing tool name".to_string()))?;
    let known = tools()
        .iter()
        .any(|tool| tool["name"].as_str() == Some(name));
    if !known {
        return Err((-32602, format!("Unknown tool: {name}")));
    }
    let none = Map::new();
    let args = params
        .get("arguments")
        .and_then(Value::as_object)
        .unwrap_or(&none);
    let (text, failed) = match run(root, name, args) {
        Ok(text) => (text, false),
        Err(e) => (format!("{e:#}"), true),
    };
    Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": failed }))
}

/// Runs a tool, returning what it says: a sentence for one that adds a note, a note's text, or
/// JSON as the command's `--json` prints it.
fn run(root: &Path, tool: &str, args: &Map<String, Value>) -> Result<String> {
    let vault = Vault::open(root)?;
    vault.config.ensure_current()?;
    let now = Local::now().naive_local();
    let cwd = vault.root.as_path();
    let optional = |key: &str| -> Result<Option<&str>> {
        match args.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) if value.trim().is_empty() => Ok(None),
            Some(Value::String(value)) => Ok(Some(value.as_str())),
            Some(_) => bail!("{key} must be text"),
        }
    };
    let required = |key: &str| optional(key)?.ok_or_else(|| anyhow!("{key} is needed"));
    let project = optional("project")?;
    let in_project = || {
        project
            .map(|query| vault.find_project(Some(query), cwd))
            .transpose()
    };
    let path = |path: &Path| vault.relative(path).display().to_string();
    let report = match tool {
        "capture_note" => {
            let text = required("text")?;
            let source = optional("source")?.unwrap_or("agent");
            let added =
                scaffold::add_scrap(&vault, in_project()?.as_ref(), text, Some(source), now)?;
            return Ok(format!("Kept the note as {}.", path(&added.path)));
        }
        "add_research" => {
            let research = NewResearch {
                name: required("name")?,
                source: Some(required("source")?),
                text: Some(required("text")?),
                by_agent: true,
            };
            let added =
                scaffold::add_research(&vault, in_project()?.as_ref(), &research, now.date())?;
            let mut said = format!("Added {}.", path(&added.path));
            if let Some(other) = added.same_name {
                said.push_str(&format!(
                    " {} has the same name, so links by that name may go to either note.",
                    path(&other)
                ));
            }
            return Ok(said);
        }
        "read_note" => {
            let found = search::find(&vault, cwd, required("note")?)?;
            return Ok(search::read(&vault, &found)?.1);
        }
        "list_notes" => reports::notes(&vault, cwd, project)?,
        "search" => reports::search(&vault, cwd, required("query")?, project)?,
        "outline" => reports::outline(&vault, cwd, project, optional("book")?)?,
        "stats" => reports::stats(&vault, cwd, project, now)?,
        "today" => reports::today(&vault, now)?,
        "check" => reports::check(&vault, cwd, project)?,
        "list_ideas" => reports::ideas(&vault)?,
        _ => bail!("there is no tool called {tool}"),
    };
    Ok(serde_json::to_string(&report)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn shelf() -> TempDir {
        let dir = TempDir::new().unwrap();
        story_time_core::scaffold::init_vault(dir.path(), false).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        story_time_core::example::add_example(&vault, Local::now().naive_local()).unwrap();
        dir
    }

    fn ask(root: &Path, method: &str, params: Value) -> Value {
        let line = json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params });
        reply(root, &line.to_string()).expect("a request is answered")
    }

    fn call_tool(root: &Path, name: &str, arguments: Value) -> (String, bool) {
        let reply = ask(
            root,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        let result = &reply["result"];
        (
            result["content"][0]["text"].as_str().unwrap().to_string(),
            result["isError"].as_bool().unwrap(),
        )
    }

    #[test]
    fn starts_by_agreeing_a_version() {
        let dir = shelf();
        let reply = ask(
            dir.path(),
            "initialize",
            json!({ "protocolVersion": "2025-03-26" }),
        );
        assert_eq!(reply["id"], 7);
        assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(reply["result"]["serverInfo"]["name"], "story-time");
        let reply = ask(
            dir.path(),
            "initialize",
            json!({ "protocolVersion": "1999-01-01" }),
        );
        assert_eq!(reply["result"]["protocolVersion"], VERSIONS[0]);
        let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert_eq!(
            reply_to(dir.path(), &note),
            None,
            "notifications get no reply"
        );
        assert_eq!(ask(dir.path(), "ping", json!({}))["result"], json!({}));
        assert_eq!(
            ask(dir.path(), "resources/list", json!({}))["error"]["code"],
            -32601
        );
        let garbled = super::reply(dir.path(), "{not json").unwrap();
        assert_eq!(garbled["error"]["code"], -32700);
    }

    fn reply_to(root: &Path, message: &Value) -> Option<Value> {
        reply(root, &message.to_string())
    }

    #[test]
    fn offers_no_tool_that_changes_the_story() {
        let names: Vec<String> = tools()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            names,
            [
                "capture_note",
                "add_research",
                "list_notes",
                "search",
                "read_note",
                "outline",
                "stats",
                "today",
                "check",
                "list_ideas"
            ]
        );
        for tool in tools() {
            let name = tool["name"].as_str().unwrap();
            let read_only = tool["annotations"]["readOnlyHint"].as_bool().unwrap();
            assert_eq!(read_only, !["capture_note", "add_research"].contains(&name));
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
        let dir = shelf();
        let reply = ask(
            dir.path(),
            "tools/call",
            json!({ "name": "rename", "arguments": {} }),
        );
        assert_eq!(reply["error"]["code"], -32602);
    }

    #[test]
    fn captures_notes_and_research_as_new_notes() {
        let dir = shelf();
        let (said, failed) = call_tool(
            dir.path(),
            "capture_note",
            json!({ "text": "  Spoken aloud, kept as is.  ", "source": "dictation" }),
        );
        assert!(!failed, "{said}");
        let path = said
            .strip_prefix("Kept the note as ")
            .and_then(|rest| rest.strip_suffix('.'))
            .unwrap();
        let kept = std::fs::read_to_string(dir.path().join(path)).unwrap();
        assert!(kept.contains("source: dictation"));
        assert!(kept.contains("  Spoken aloud, kept as is.  "));

        let (said, failed) = call_tool(
            dir.path(),
            "add_research",
            json!({ "name": "Tides", "text": "Two a day.", "source": "https://example.org/tides" }),
        );
        assert!(!failed, "{said}");
        let path = said.strip_prefix("Added ").unwrap().trim_end_matches('.');
        let research = std::fs::read_to_string(dir.path().join(path)).unwrap();
        assert!(research.contains("Added by an agent"));
        assert!(research.contains("https://example.org/tides"));

        let (said, failed) = call_tool(dir.path(), "add_research", json!({ "name": "Tides" }));
        assert!(failed);
        assert!(said.contains("needed"), "{said}");
    }

    #[test]
    fn reads_and_searches_the_bookshelf() {
        let dir = shelf();
        let (found, failed) = call_tool(dir.path(), "search", json!({ "query": "chapters" }));
        assert!(!failed, "{found}");
        let found: Value = serde_json::from_str(&found).unwrap();
        assert_eq!(found["results"][0]["name"], "Scenes and Chapters");
        let (text, failed) = call_tool(
            dir.path(),
            "read_note",
            json!({ "note": "Scenes and Chapters" }),
        );
        assert!(!failed);
        assert!(text.contains("# Scenes and Chapters"));
        let (said, failed) = call_tool(
            dir.path(),
            "read_note",
            json!({ "note": "../../etc/passwd" }),
        );
        assert!(failed, "{said}");
        let (notes, failed) = call_tool(dir.path(), "list_notes", json!({}));
        assert!(!failed);
        assert!(
            serde_json::from_str::<Value>(&notes).unwrap()["notes"]
                .as_array()
                .unwrap()
                .len()
                > 5
        );
        for tool in ["outline", "stats", "today", "check", "list_ideas"] {
            let (said, failed) = call_tool(dir.path(), tool, json!({}));
            assert!(!failed, "{tool}: {said}");
            serde_json::from_str::<Value>(&said).unwrap();
        }
    }
}
