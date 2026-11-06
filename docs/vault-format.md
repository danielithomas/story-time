# The vault format

**Status: approved 2026-11-05, implemented in story-time 0.2.** `story-time upgrade` converts a vault from the unversioned layout of story-time 0.1.

This document is the contract between story-time and a writer's vault: what goes where, what each note holds, and which parts story-time reads and writes. It is format **1**. Before story-time 1.0, any change to this document raises the minor version; after 1.0, the major version.

The decisions behind it are recorded in [GitHub issues](https://github.com/danielithomas/story-time/issues?q=label%3Avault-format) #1 to #5, with #6, #7 and #24 for the format version, IDs and scratchpads. Other `#` numbers below refer to issues too.

## Principles

- **The files are the only source of truth.** There is no database or cache. Everything story-time stores is Markdown with YAML properties, or plain text for its settings and log. If story-time isn't available, a person or another program can still read and use everything (#4).
- **Obsidian is a supported second editor.** Everything opens and edits correctly in Obsidian without plugins. story-time never requires Obsidian, and leaves Obsidian syntax it doesn't understand untouched (#4).
- **story-time changes as little as possible.** Creating a note never overwrites a file. Otherwise story-time rewrites only the parts it owns: the properties it sets, the `## Books` and `## Contents` sections, and links when a note is renamed. It checks a file hasn't changed on disk before writing it.
- **A note's folder decides which project and book it belongs to.** Notes carry no property naming their project or book, so moving a note can't leave one stale (#5).

## Layout

```
vault/                              one vault per git repository
├── .story-time/
│   ├── config.toml                 settings for this vault
│   ├── history.csv                 the writing log
│   └── build/                      compiled manuscripts (not committed)
├── .gitattributes
├── .gitignore
├── templates/                      note templates, shared with Obsidian
├── research/                       research not tied to one world
├── scratchpad/                     the vault's scratchpad
│   ├── ideas.md                    the ideas file
│   └── 2026-11-05-2109.md          a captured note
└── sample-story-world/             a project: one world
    ├── sample-story-world.md       the project note, listing its books
    ├── bible/                      the world bible, shared by every book
    │   ├── sample-story-world-timeline.md
    │   ├── characters/
    │   ├── locations/
    │   └── world/
    ├── research/                   background gathered from outside sources
    ├── scratchpad/                 fragments that belong to this world
    └── books/
        └── sample-story/           a book
            ├── sample-story.md     the book note, with its contents
            ├── sample-story-flow.md
            └── scenes/
```

- **A project is a world**: one or more books that share a world bible (#3). Books in different worlds are different projects.
- **A project is a folder at the vault root holding a note with the same name** and `type: project`. `research/`, `scratchpad/`, `templates/` and folders starting with a dot are never projects. Other files and folders, such as a readme, are left alone.
- **Every project keeps `books/`**, even with a single book, so a sequel is just a second folder and nothing moves. The app hides the `books/` level when a project has only one book.
- **Each project folder is self-contained.** Links between its notes use only paths inside the project, so a project can later move into a repository of its own with its history (`git filter-repo --subdirectory-filter <project>`).
- All projects start in one vault. Ideas, the daily goal and the writing streak span projects.

## Names

**Every folder and file name is lowercase kebab-case**: `books/sample-story/`, `scenes/the-long-night.md`, `characters/mara-okafor.md`. There are no spaces, and no names that differ only in case, which Windows and macOS can't tell apart.

- **A note's real name lives in the note.** Projects and books have a `title`. Every other note's name is the first entry in its `aliases`. story-time shows that name everywhere and searches the prose for it; Obsidian's link suggestions find the note by it. A note without one is named after its file: `the-long-night` reads as "The long night".
- **A slug is made from the name**: lower-case ASCII letters and digits, with a hyphen between words. Accents become the plain letter and other punctuation is dropped, so "K'thar" becomes `kthar` and "Ōkami" becomes `okami`. Plain ASCII avoids the different ways macOS and git store accented letters.
- **Collisions.** When two names give the same slug ("Mara's" and "Maras"), story-time suggests a numbered one (`maras-2`).
- **Unique within a project.** Within a project, every file name is unique, except that scenes in different books may share one: two books can each have a scene called `morning`. Unrelated projects may reuse any name.
- **Project and book.** A project note and a book note can't share a name. When a standalone book's project would take the book's name, the project gets a `-world` suffix: `sample-story-world/` holding `books/sample-story/`.
- **The timeline** is named after its project, `sample-story-world-timeline.md`, as a flow note is named after its book.
- **Captured notes** are named by the date and time of capture, `2026-11-05-2109.md`, so captures from different devices never collide.
- **Renaming** a note changes its name, its file name and every link to it together.

## Links

- Links are wikilinks with the note's name as their text: `[[mara-okafor|Mara Okafor]]`.
- **Resolving a link.** story-time looks for the file name in the same book first, then the rest of the project, then the vault outside any project. In story-time, `[[mara-okafor]]` always means this world's Mara.
- **For Obsidian**, which looks names up across the whole vault, a link to a name that also exists elsewhere in the vault is written as a path relative to the note: `[[../../../bible/characters/mara-okafor|Mara Okafor]]`. This is the form Obsidian writes with its "Relative path to file" setting, and it keeps working if the project moves to a repository of its own.
- Adding a note whose name exists elsewhere makes existing bare links to the other note ambiguous in Obsidian. story-time warns when that happens, and `check` lists them.

## Notes and properties

Every note story-time manages has a `type`. Properties are flat, and each name has one type across the vault, as Obsidian requires: `target` is a number and `created` a date in every note.

| Note | Where | Properties |
|---|---|---|
| Project | `<project>/<project>.md` | `type: project`, `id`, `title`, `genre`, `created`, `tags` |
| Book | `books/<book>/<book>.md` | `type: book`, `id`, `title`, `status`, `form`, `target`, `deadline`, `created`, `tags` |
| Story flow | `books/<book>/<book>-flow.md` | `type: flow`, `structure` |
| Scene | `books/<book>/scenes/` | `type: scene`, `aliases`, `status`, `pov`, `location`, `when`, `synopsis`, `tags`; optionally `goal`, `conflict`, `outcome` |
| Character | `bible/characters/` | `type: character`, `aliases`, `tags` |
| Location | `bible/locations/` | `type: location`, `aliases`, `tags` |
| World | `bible/world/` | `type: world`, `category`, `aliases`, `tags` |
| Timeline | `bible/<project>-timeline.md` | `type: timeline` |
| Research | `research/`, `<project>/research/` | `type: research`, `aliases`, `source`, `tags` |
| Scrap | `scratchpad/`, `<project>/scratchpad/` | `type: scrap`, `created`; optionally `aliases`, `source`, `used`, `tags` |

| Property | Type | Meaning |
|---|---|---|
| `type` | text | The note kind, as in the table above. |
| `id` | text | Eight random lower-case letters and digits, starting with a letter so YAML never reads one as a number, such as `k3x9q2mw`. Written when the note is created and never changed, so the writing log survives a rename (#7). |
| `title` | text | The project's or book's name, used on title pages. |
| `aliases` | list | The note's name first, then any other names it goes by. `check` looks for all of them in the prose. |
| `genre` | list | Free text, such as `space opera`. |
| `status` | text | For a book: `planning`, `drafting`, `revising`, `complete`, `submitted`, `published` or `shelved`. For a scene: `outline`, `draft`, `revised` or `final`. |
| `form` | text | `short-story`, `novelette`, `novella` or `novel`. story-time suggests one from the target. |
| `target` | number | Target length in words. `"90,000"` is also read, but story-time writes `90000`. |
| `deadline` | date | `YYYY-MM-DD`. story-time works out the words a day needed to meet it. |
| `created` | date | `YYYY-MM-DD`. |
| `structure` | text | The beat sheet the flow follows: `three-act`, `save-the-cat`, `heros-journey`, `story-circle`, `kishotenketsu` or `freeform`. |
| `pov` | text | A link to the character whose point of view the scene is told from. |
| `location` | text | A link to the location where the scene happens. |
| `when` | text | When the scene happens in story time, in any form, such as `Year 3, day 12` or `three days after the landing`. How story-time reads it is left to the timeline work (#21). |
| `synopsis` | text | One or two sentences, shown on the Board and in `outline`. |
| `goal`, `conflict`, `outcome` | text | For scenes planned that way. Nothing requires them: not every scene has to turn. |
| `category` | text | Free text, such as technology, culture, history, organisation, species or language. |
| `source` | text | Where a note came from: `dictation` for a captured note, or a URL or citation for research. |
| `used` | list | Links to the scenes a scrap led to. |
| `tags` | list | Free for the writer; story-time doesn't add any. |

All properties except `type` may be missing or empty. A note body is free Markdown, except for the sections described under [Order](#order-books-and-contents).

**Notes to self** go in `%% comments %%` or callouts (`> [!todo] check the physics`). Both stay out of word counts and manuscripts.

## Order: books and contents

Order is written as a Markdown list of links, readable and clickable in any editor (#1). Nothing is numbered in file names, so moving a scene, chapter or part renames nothing.

**The project note** lists its books in order under a `## Books` heading:

```markdown
## Books

- [[sample-story|Sample Story]]
```

**The book note** lists its chapters, and the scenes in each, under a `## Contents` heading:

```markdown
## Contents

### Part One

- Chapter
  - [[arrival|Arrival]]
  - [[first-light|First Light]]
- The Long Night
  - [[leaving-home|Leaving Home]]
```

- The section runs from its heading to the next heading at the same level. The rest of the note is free for notes about the book.
- **Scenes** are list items that are links.
- **Chapters** are list items that aren't links, with their scenes nested beneath. The item's text is the chapter's title, or `Chapter` for an untitled one. A scene listed at the top level is a chapter of its own, untitled.
- **Numbering.** Untitled chapters are numbered in order when compiled and in the app: Chapter 1, Chapter 2. A titled chapter, such as `Prologue` or `The Long Night`, goes by its title and isn't counted, so a book can read Prologue, Chapter 1, Chapter 2, Epilogue.
- **Parts** are `###` headings inside the section. They are optional, and the app shows them only in books that use them (#2).
- **Unplaced scenes.** A scene in `scenes/` that isn't in the list is shown as unplaced, not reported as an error. It joins the list when it's placed.
- story-time rewrites only this section when something moves.

**Compiling** a book follows the contents: a heading for each part and chapter, and a scene break (`***`) between the scenes of a chapter. Manuscripts are written to `.story-time/build/`.

## The world bible and research

**The world bible**, `bible/`, holds what is true in the story's world: its characters, locations, world notes (technology, culture, history and the like) and timeline. The story must stay consistent with it, and every book in the project shares it.

**The timeline** is the world's chronology: the order things happened in, where a book's flow note is the order its story is told in. Scenes appear on it through their `when`. The timeline note holds what no scene shows, such as backstory, history, births and deaths, as a plain Markdown list (`- Year 3, day 12: the landing`) that reads as a chronology without story-time. Its exact format, and any invented calendar, are left to the timeline work (#21).

**Research** holds what is true in the real world: physics, history, references, gathered from outside sources and used to build or enrich the story. The story may draw on it or knowingly break it. A project's research lives in `<project>/research/`; research useful to several worlds lives in the vault's `research/`. One note per subject, with its `source` (a URL or citation). Agents may add research notes (#23); story-time opens each with a callout saying an agent added it, so the writer knows to check it against its source. Research may also hold files that aren't Markdown, such as PDFs and images, which story-time leaves alone. Large files make a git repository slow to clone, so link to them where you can.

## Scratchpads and the ideas file

A scratchpad is a writer's notebook: fragments kept until they're useful, which may be years later (#24).

- **The vault's scratchpad**, `scratchpad/`, holds fragments not tied to any world. **A project's scratchpad**, `<project>/scratchpad/`, holds fragments that belong to the world but not yet to a book.
- **One note per fragment**, with `type: scrap`.
- **Cutting a scene** moves it to its project's scratchpad. It keeps its properties, so it can go back into a book unchanged.
- **Using a scrap**: move it into a book as an unplaced scene, or keep it and add a link to the scene it led to in `used`.
- **Captured notes**, for example dictated through an agent (#23), arrive as new scraps with a `source`. Their text is stored exactly as received, and agents never edit, move or delete them.

**The ideas file**, `scratchpad/ideas.md`, is the quick list of one-line ideas. Ideas are list items under `###` headings such as `### Fiction`, written as `Name: description`. story-time only adds to it. Starting a project from an idea appends a link to the idea's line, `→ [[sample-story-world|Sample Story]]`, and nothing is ever removed.

## Word counts

- **What counts** is the prose a reader would read. Properties, headings, `%% comments %%`, `<!-- HTML comments -->`, callouts, code blocks and embeds are left out. Links count as the text they display. Hyphenated words count once; words joined by an em dash count separately.
- **A book's count** covers all its scenes, placed or unplaced. Compiled manuscripts, and their page and reading-time estimates, include only placed scenes.
- **A scratchpad's count** goes toward the daily goal but toward no book's target.
- **The world bible and research** aren't counted: they're about the story, not the story.
- **Words written in a day** are the net change across the whole vault, so moving a scene between a book and a scratchpad doesn't count as writing.

## story-time's own files

**`.story-time/config.toml`** holds the vault's settings:

```toml
format = 1                            # this document's version
daily_goal = 500                      # words a day
words_per_page = 250                  # manuscript estimates
reading_wpm = 250
ideas_file = "scratchpad/ideas.md"
scratchpad = "scratchpad"             # the vault's scratchpad folder
templates_dir = "templates"
# author = "Your Name"                # printed on compiled manuscripts
# spelling = ["en_AU"]                # the editor's spellcheck languages
```

Settings added by later features, such as git (#22), are documented with them.

**`.story-time/history.csv`** is the writing log. It is append-only and never rewritten. Each row is `timestamp,id,words`: a local time (`2026-11-05T21:09:00`), what was measured, and its word count at that moment.

- What was measured is a book's `id`, a project's `id` for the project's scratchpad, or `scratchpad` for the vault's.
- A row is added whenever a count has changed since the last row for it.
- Logs started before format 1 have the header `timestamp,project,words` and project folder names in the second column. Those rows stay readable.

**`.story-time/build/`** holds compiled manuscripts and is listed in `.gitignore`.

**`.gitattributes`** holds `.story-time/history.csv merge=union`, so when two machines both add to the log, git keeps both sets of lines instead of reporting a conflict.

**`templates/`** holds one template per note kind: `project.md`, `book.md`, `story-flow.md`, `scene.md`, `character.md`, `location.md`, `world.md`, `timeline.md`, `research.md`, `scrap.md`. A template there overrides story-time's built-in one. `story-time init` points Obsidian's Templates feature at the same folder (`.obsidian/templates.json`), and the README lists the placeholders story-time fills.

## Versions and upgrades

- `story-time init` writes `format = 1` (#6).
- **A vault without a `format` key** uses the unversioned layout from before this document. story-time offers to upgrade it after making a backup. The upgrade renames files and folders to kebab-case, with each note's name kept in `aliases`, and moves the vault-level folders and files to `scratchpad/`, `scratchpad/ideas.md` and `templates/`. It moves each project's characters, locations and world notes into `bible/`, and turns the project into a single-book project whose chapters become scenes, listed in the book's contents in their old order.
- **A vault with a newer format** than story-time knows is refused with a clear message, never edited.

## What `check` reports

- Links to notes that don't exist yet, and characters, locations and world notes that never appear in the prose.
- Links in `## Books` or `## Contents` to notes that don't exist, and scenes listed more than once.
- Scenes without a status or synopsis.
- Bare links that Obsidian can't resolve to a single note.
- File and folder names that aren't kebab-case (for example notes created in Obsidian), and slugs that no longer match the note's name. story-time offers to rename them.
