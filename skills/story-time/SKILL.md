---
name: story-time
description: Help a writer around their Story Time bookshelf, a folder of Markdown notes for their novels and stories, without writing the story. Use when the writer asks you to take down or dictate a note, add research with its source, or find, read, outline or count what's on their bookshelf. Never create, edit, move or delete their scenes, chapters, world-bible notes or story flows.
license: MIT OR Apache-2.0
compatibility: Needs the story-time command-line program with the notes, search and show commands.
---

# Story Time

Story Time is for a human to write in. You can help around the writing, but not write it:

- **Capture** notes the writer dictates or asks you to keep.
- **Add research**: something true in the real world, with the source it came from.
- **Find and read** what's on the bookshelf, and report outlines, word counts, `check` results and ideas.

Use only the `story-time` commands below. Don't create, edit, move, rename or delete any file on the bookshelf yourself, and don't run its other commands (`new`, `add`, `rename`, `compile`, `idea`, `sync`, `upgrade` and so on). If the writer wants the story changed, say what you would change in your reply and leave the change to them.

## The bookshelf

`story-time` finds the bookshelf from the folder you're in or the writer's settings. Pass `--bookshelf PATH` (or set `STORY_TIME_BOOKSHELF`) to name it.

- A **project** is a world: a folder with its own note, its world bible in `bible/` (`characters/`, `locations/`, `world/` and a timeline), and its books in `books/`.
- A **book** is a folder with its own note, a story flow (`<book>-flow.md`) and `scenes/`. The order of its scenes is the `## Contents` list in its note, not the file names.
- **Research** is in `research/`, the bookshelf's or a project's. **Scratchpads** are notebooks of fragments, in `scratchpad/`. The ideas file is `scratchpad/ideas.md`.
- Each note's name is its `title` or first alias. Files are kebab-case, and links are `[[file-name|Shown Name]]`.

## Capturing a note

```sh
printf '%s' "$TEXT" | story-time scrap add - --source dictation [-p PROJECT]
```

The note becomes a new scrap in the bookshelf's scratchpad, or with `-p` a project's, named by the time. Its text is stored exactly as you give it, so pass the writer's words as they said them. Reading from standard input (`-`) keeps line breaks and quotes intact. `--source` says where it came from: `dictation`, or `agent` for something you wrote down yourself.

## Adding research

```sh
printf '%s' "$TEXT" | story-time research add "Subject" --source "URL or citation" --text - --agent [-p PROJECT]
```

Always pass `--agent` and a real `--source`. The note opens with a callout asking the writer to check it against its source, because a summary can be wrong. Keep to what the source says.

## Reading

Every command here only reads, apart from recording word counts in the writing log. Add `--json` for output to parse. Its keys stay stable, and paths are relative to the bookshelf.

| Command | What it gives |
|---|---|
| `story-time notes [-p PROJECT] --json` | Every note: `path`, `name`, `aliases`, `kind` (`project`, `book`, `flow`, `scene`, `character`, `location`, `world`, `timeline`, `research`, `scrap` or `note`), `project`, `book`. |
| `story-time search WORDS... [-p PROJECT] --json` | Notes with every word, in any case. A `"quoted phrase"` counts as one word. Results come best first, each with `matches` and `excerpts`. |
| `story-time show NOTE [--json]` | A note as written, found by name, file name or path. `--json` splits it into `properties` and `body`. |
| `story-time outline [PROJECT] [-b BOOK] --json` | A book's scenes in order, with status, point of view, location, time, synopsis and words. |
| `story-time stats [PROJECT] --json` | Words, targets and progress for every book, or one project in detail. |
| `story-time today --json` | Words written today, the daily goal and the streak. |
| `story-time check [PROJECT] --json` | World-bible gaps: missing notes, names never on the page, near-miss spellings, loose ends. |
| `story-time ideas --json` | The ideas file's ideas, with the project each became. |

Without `PROJECT`, `outline` and `check` use the only project, and `notes` and `search` cover the whole bookshelf. A project can be named by its title, its folder or part of either.

## Through MCP instead

`story-time mcp` offers the same things as Model Context Protocol tools: `capture_note`, `add_research`, `list_notes`, `search`, `read_note`, `outline`, `stats`, `today`, `check` and `list_ideas`. When those tools are available, use them. They are the boundary the writer chose, and they have no tool that changes the story.
