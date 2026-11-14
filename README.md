# story-time

Write, organise, measure and compile stories kept as Markdown in a folder you choose. Two front ends share one core:

- **Story Time**, a desktop app for writing (`apps/desktop`, built with Tauri)
- **`story-time`**, a command-line tool (`crates/cli`)

Your stories live on a **bookshelf**: a folder of plain Markdown files with a few properties (YAML frontmatter). The files are the source of truth. story-time reads them and writes ordinary Markdown back. It keeps no database and no format of its own, so you can stop using it at any time. A bookshelf can live in git, on OneDrive, or anywhere else a folder can.

Obsidian can open a bookshelf as a vault, and an existing Obsidian vault works as a bookshelf.

## Install

Download the installer for your system from the [releases](https://github.com/danielithomas/story-time/releases). That is all most people need: the desktop app does everything the command does.

The `story-time` command is attached to each release too, as an archive for each system: unpack it and put `story-time` in a folder on your PATH. Or build it from source:

```sh
cargo install --path crates/cli     # installs the `story-time` binary to ~/.cargo/bin
```

Building needs Rust 1.90 or later. [pandoc](https://pandoc.org) is optional; it is used only to compile to Word, EPUB, HTML or PDF (PDF also needs a LaTeX engine).

## Getting started

```sh
cd ~/stories
story-time init                                   # config, templates, Obsidian template folder
story-time new "Sample Story" --from-idea         # a project (a world) with its first book; premise from the ideas file
story-time add scene "Arrival"                    # books/sample-story/scenes/arrival.md, listed in the book's contents
story-time add character "Mara Chen"              # bible/characters/mara-chen.md
story-time new "Second Story" --in sample-story   # another book in the same world
story-time stats                                  # every book at a glance
```

### Finding the bookshelf

Commands use the first of:

1. `--bookshelf <path>`
2. the `STORY_TIME_BOOKSHELF` environment variable
3. the nearest folder at or above the current directory that holds `.story-time/` or `.obsidian/`
4. `bookshelf = "~/path/to/stories"` in `~/.config/story-time/config.toml`

Option 4 makes every command work from any directory, which helps if the bookshelf moves somewhere like `~/OneDrive/stories`.

## Commands

| Command | What it does |
|---|---|
| `init [--force] [--example]` | Writes `.story-time/config.toml`, the note templates in `templates/`, `.obsidian/templates.json` (pointing Obsidian's Templates plugin at that folder) and, in a git repository, a `.gitignore` entry for compiled output and a `.gitattributes` entry so git merges the writing log from two machines instead of reporting a conflict. Existing files are kept; `--force` replaces only the templates. `--example` adds `example-world/`, a world with a short book whose scenes explain how story-time works, and logs its words as a starting point so they don't count as written. Delete the folder when you're done with it. |
| `new "Title" [--world W \| --in PROJECT] [--form F] [--target N] [--structure S] [--from-idea [NAME]]` | Starts a book. On its own, creates a project: the world, with its project note and world bible (`bible/characters/`, `bible/locations/`, `bible/world/` and the timeline, `bible/<project>-timeline.md`), and the book in `books/<slug>/` with its book note, story flow and `scenes/`. The world takes the book's title unless `--world` names it; when the two would share a folder name, the project's gets `-world` (`sample-story-world/`). `--in` adds the book to an existing project instead. `--structure` lays out the flow note with beats and the word count each lands at (`three-act`, `save-the-cat`, `heros-journey`, `story-circle`, `kishotenketsu`, `freeform`). `--from-idea` copies the premise from the ideas file and appends a link to the new project, `→ [[sample-story-world|Sample Story]]`, to that idea. Nothing in the ideas file is removed or reworded. |
| `add scene\|character\|location\|world "Name" [-p project] [-b book] [--chapter N \| --unplaced]` | Creates a note from its template: a scene in a book, or a character, location or world note in the project's world bible. It is named with the kebab-case form of the name, with the name itself as its first alias. A new scene is listed at the end of the book's contents as a chapter of its own; `--chapter N` adds it to the end of chapter N instead, and `--unplaced` leaves it out of the contents. |
| `stats [project] [--json]` | All books: world, status, form, scenes, words, progress to target. One project: for each of its books, its scenes chapter by chapter, progress, deadline pacing, length category, manuscript pages and reading time, and words today; then the size of its world bible, scratchpad and research. |
| `outline [project] [-b book] [--json]` | Each scene's status, POV, location, time, word count and synopsis, chapter by chapter, then the unplaced scenes: a corkboard for the terminal. |
| `today [--json]` | Words written today against the daily goal, per book and scratchpad, plus your streak. |
| `history [--days N] [--json]` | Words written per day. |
| `snapshot` | Records word counts silently, for a cron job or git hook. |
| `ideas [--json]` | Lists the ideas in the ideas file by heading, with the project each became. |
| `idea Name: description [--non-fiction]` | Appends an idea under `### Fiction` (or `### Non-Fiction`) in the ideas file, in the list style already there. |
| `scrap add TEXT\|- [-p project] [--source S]` | Keeps a fragment as a new note in the bookshelf's scratchpad, or with `-p` a project's, named by the time of capture (`2026-11-05-2109.md`). The text is stored exactly as given; `-` reads it from standard input. `--source` records where it came from, such as `dictation`. |
| `research add "Subject" [-p project] [--source S] [--text TEXT\|-] [--agent]` | Adds a research note to the bookshelf's `research/`, or with `-p` a project's, named like other notes. `--source` is a URL or citation. `--text` gives the note's text instead of the template's prompt (`-` reads it from standard input). `--agent` marks a note an agent wrote: it opens with a callout asking you to check it against its source. |
| `check [project] [--json]` | World-bible gaps across all of a project's books: links to notes that don't exist yet, character, location and world notes whose name never appears in a scene, and scenes missing a status or synopsis. Spellings close to a world-bible name: a capitalised word in the scenes a typo away from a word of a name (*Marra* for *Mara*: one edit, or two for a name of eight letters or more), or differing from one only in its capitals (*Mckay* for *McKay*). A word the scenes also use in lower case counts as an ordinary word, and a spelling that's meant stops being flagged once it's one of the note's aliases. Also entries in `## Books` or `## Contents` that aren't a book or scene there, scenes listed twice, links Obsidian can't follow to a single note, and file names that aren't kebab-case or no longer match the note's name (`rename` fixes a note's). |
| `rename NOTE ["New name"]` | Renames a scene, character, location, world note or research note: its name, its file name and every `[[link]]` to it across the bookshelf change together, so a scene keeps its place in its book's contents. Renaming a book's or world's own note renames the book or world: its folder, its story flow or timeline, and the links into it follow, and an idea that became the world gains a link to its new name, since the ideas file is only added to. A world named like one of its books takes `-world`, and a book or world without a permanent `id` is refused, since its writing log would lose track of it. `NOTE` is its name, file name or path. A link that showed the old name shows the new one; other link text is kept. A name another note of the same kind already has (in the same book, for a scene) is refused, and a file name another note in the project has gets a number (`mara-2.md`). Without a new name, the note keeps its name and its file is renamed to match, as `check` suggests. The ideas file and templates are never edited. |
| `backup` | Backs the bookshelf up now, as a zip beside the copies the desktop app makes: `<date>.zip` in a folder named after the bookshelf, in Story Time's data folder (`~/.local/share/story-time/backups/` on Linux) or the `backup_folder` set in `~/.config/story-time/config.toml`. Does nothing if nothing has changed since the last copy. See "Backups" below. |
| `compile [project] [-b book] [--format md\|docx\|epub\|html\|pdf] [-o path]` | The scenes a book's contents list, in order, as one manuscript minus notes-to-self: a heading for each part and chapter, and `***` between scenes. Unplaced scenes are left out. Defaults to `.story-time/build/<project>/<book>.<format>`. `--format docx` gives standard manuscript format for sending to editors and agents (after William Shunn's guide): Times New Roman 12-point, double-spaced, one-inch margins, indented paragraphs, your name and `contact` lines top left of the first page with the word count top right, then the title and byline, and `Surname / KEYWORD / page` at the top of every other page. A novel or novella gets a title page of its own and each chapter starts a new page a third of the way down; a shorter piece starts beneath its title, with a centred `#` between scenes. The paper is A4, or US Letter in the United States and Canada (set `paper` to choose). |
| `upgrade [--dry-run]` | Converts a bookshelf from story-time 0.1's layout to the current one, after copying it into `.story-time/backups/<date-time>/`. `--dry-run` lists the changes without making them. See "Upgrading from 0.1" below. |

**JSON.** `stats`, `outline`, `today`, `history`, `check` and `ideas` take `--json` and print what they report as JSON instead, for scripts and other programs: Obsidian's Shell Commands plugin, n8n, a dashboard. Keys are snake_case and stay stable, a value that isn't set is `null`, dates are `YYYY-MM-DD`, paths are relative to the bookshelf, and links in properties are given as the names they show (`"pov": "Mara Okafor"`). For example, `story-time stats --json` gives `{"date", "daily_goal", "written_today", "streak", "words", "books": [...]}`, each book with its `project`, `slug`, `title`, `id`, `status`, `form`, `target`, `deadline`, `scenes`, `words`, `placed_scenes`, `placed_words`, `progress` (0 to 1, or more once the target is reached) and `written_today`; `outline --json` gives the book's `parts`, each with `chapters` and their `scenes`, then the `unplaced` scenes; `check --json` gives each list `check` prints and `clean`.

Without a project argument, commands other than `stats`, `scrap` and `research` use the project folder you are in, or the only project on the bookshelf; `scrap` and `research` use the bookshelf's own folders unless given `-p`. Otherwise give its slug, title, or an unambiguous part of either (`stats sample`). Books work the same way with `-b`: the book folder you are in, else the project's only book.

## Conventions

**Projects and books.** A project is a world: a folder at the root of the bookshelf holding a note with the same name and `type: project`, such as `sample-world/sample-world.md`. Its world bible sits in `bible/` (`characters/`, `locations/`, `world/` and a timeline note) and is shared by every book set in it. Each book is a folder in `books/` with a note of the same name, a story flow (`<book>-flow.md`) and `scenes/`. The project note lists its books in order under `## Books`; story-time adds each new book there. `research`, `scratchpad` and `templates` are never projects. `docs/bookshelf-format.md` describes the full layout.

**Scratchpads, research and ideas.** A scratchpad is a writer's notebook, one note per fragment (`type: scrap`): the bookshelf's `scratchpad/` for fragments not tied to any world, and a project's `scratchpad/` for those that belong to its world but not yet to a book. Research (`type: research`, with its `source`) is what's true in the real world: a project's in `<project>/research/`, research for several worlds in the bookshelf's `research/`. The ideas file, `scratchpad/ideas.md` by default, is the quick list of one-line ideas; story-time only adds to it.

**Properties** read by story-time (all optional, all editable in Obsidian):

| Note | Property | Meaning |
|---|---|---|
| project, book | `id` | written once when the note is created and never changed; the writing log records each book by its `id` |
| project, book | `title` | the world's or the book's name |
| project | `genre` | a list, shown in `stats` |
| book | `status` | `planning` → `drafting` → `revising` → `complete` → `submitted` → `published`, or `shelved` |
| book | `form` | `short-story`, `novelette`, `novella` or `novel` |
| book | `target` | target words; `90000` or `"90,000"` |
| book | `deadline` | `YYYY-MM-DD`; `stats` works out the words a day needed |
| scene | `status` | `outline` → `draft` → `revised` → `final` |
| scene | `pov`, `location` | links to a character and a location, shown in `stats`, `outline` and the Board |
| scene | `when`, `synopsis` | when it happens in story time, in any form, and a sentence or two about it |
| scrap | `created`, `source`, `used` | when it was kept, where it came from (such as `dictation`), and links to the scenes it led to |
| research | `source` | a URL or citation |
| any note | `aliases` | the first is the note's name, the rest other names it goes by; `check` looks for all of them on the page |

**Names.** Files and folders are named in lowercase kebab-case (`mara-chen.md`), so there are no spaces to quote and no names that differ only in case. A note's real name is its `title` (projects and books) or its first alias (everything else), and that is what story-time shows. Links are written `[[mara-chen|Mara Chen]]`: Obsidian follows the file name and shows the name. story-time resolves a bare link to the nearest note with that file name, in the linking note's project before anywhere else, and also accepts an alias; when a file name exists in more than one place, write the link with a path relative to the note (`[[../../../bible/characters/mara-chen|Mara Chen]]`) so Obsidian opens the right one. To rename a note, use `story-time rename` or the name in the app's inspector, so its file name and the links to it follow.

**Contents.** A book's order is the list under `## Contents` in its note, so moving a scene renames nothing:

```markdown
## Contents

### Part One

- Chapter
  - [[arrival|Arrival]]
  - [[first-light|First Light]]
- The Long Night
  - [[leaving-home|Leaving Home]]
```

A list item that isn't a link is a chapter, titled by its text (`Chapter` for an untitled one), with its scenes indented beneath it as links; a link on its own is a chapter by itself. `###` headings make parts. Untitled chapters are numbered in order ("Chapter 1"); titled ones, such as a prologue, go by their title. Scenes in `scenes/` that the list leaves out are *unplaced*: they count toward the book's words but not its manuscript. story-time only ever rewrites this section, and only when the note hasn't changed on disk.

**Word counts** cover the prose a reader would read, in scenes and scratchpads. Scratchpad words count toward the daily goal but toward no book; the world bible and research aren't counted. Counts leave out properties, headings (chapter and scene titles), `%% Obsidian comments %%`, `<!-- HTML comments -->`, callouts such as `> [!todo]`, code blocks and embeds. Links count as the text they display. Hyphenated words count once; words joined by an em dash count separately.

**Templates** in the bookshelf's `templates/` folder override the built-in ones, so editing a template in Obsidian changes what `add` and `new` create. Obsidian fills `{{title}}` and `{{date}}`. story-time also fills `{{id}}`, `{{project}}`, `{{book}}`, `{{slug}}`, `{{form}}`, `{{target}}`, `{{premise}}`, `{{flow_link}}`, `{{timeline_link}}`, `{{structure}}` and `{{structure_name}}`. Those placeholders stay literal if you insert the project or story-flow template by hand.

**The bookshelf format.** `docs/bookshelf-format.md` specifies this layout, format 1, and `format = 1` in `.story-time/config.toml` records it. story-time refuses to open a bookshelf whose `format` is newer than it understands, rather than misread or change it. A config file without `format` comes from story-time 0.1: commands stop and ask you to run `story-time upgrade`. A folder with no config file is new, and `init` sets it up.

**Upgrading from 0.1.** `story-time upgrade` (or the desktop app, when it opens such a bookshelf) first copies the bookshelf, apart from `.git/` and compiled output, into `.story-time/backups/<date-time>/`, which `.gitignore` then leaves out of git. Then it:

- moves the ideas file to `scratchpad/ideas.md` exactly as it is, and other notes in `Scratchpad/` to `scratchpad/` as scraps;
- replaces 0.1's templates with the new ones in `templates/`. A template you changed is kept as `templates/old-<name>.md`, and any of your own move across under a kebab-case name;
- turns each project into a world with one book. `sample-story/` becomes `sample-story-world/`: the project note becomes the book's note in `books/sample-story/` (with the story flow), each chapter becomes a scene listed in the book's contents as a chapter titled as before, in the same order, and characters, locations and world notes move into `bible/`. A new project note lists the book, and a timeline note is added;
- renames notes to kebab-case, keeping each note's name as its first alias, and rewrites links to notes that moved so Obsidian still follows them. The ideas file and templates are never edited;
- updates `config.toml` (keeping your settings and comments) and points Obsidian's Templates plugin at `templates/`;
- records a last count in the writing log under each project's old name and a first one under each book's new `id`, so no words are lost or counted twice.

Nothing is changed if a file is in the way of a move; the upgrade says which. To undo an upgrade, put the backup's contents back.

**The writing log** is `.story-time/history.csv`. Each time `stats`, `today`, `history` or `snapshot` runs, it appends a line for every book and scratchpad whose count changed, recorded under the book's `id` so renaming the book or its project keeps its history. A project's scratchpad is recorded under the project's `id`, and the bookshelf's as `scratchpad`. (A book without an `id` is recorded as `<project>/<book>`, a project's scratchpad as `<project>/scratchpad`.) Moving a scene into a scratchpad doesn't count as writing or as words lost. Words written on a day are the net change from the previous snapshot, so deleting words counts against the day. If you write without running a command, those words count toward the next day you do. To log every day, schedule a snapshot:

```cron
55 23 * * * $HOME/.cargo/bin/story-time --bookshelf $HOME/stories snapshot
```

## The desktop app

Story Time is a dedicated writing program laid out like Scrivener. The first time it opens, it offers to start a bookshelf, by default `Stories` in your Documents folder, with the example book from `init --example` unless you untick it, or to open one you already have. Before setting up a folder that already holds other files, it says what it will add and asks. A new book opens on an empty first scene, ready to write in; the new-book dialog needs only a title, with the world, form, target and structure under **More options**.


- **Binder** (left): the open book's overview and story flow; its contents, part by part and chapter by chapter, with each scene's status and word count; its unplaced scenes; then the world bible: the project's overview, timeline, characters, locations and world notes; then the world's research and scratchpad, and the general research and scratchpad that belong to no world, with the ideas file. A `+` adds a scene at the end, to a chapter, or unplaced, a world-bible or research note, or an empty scrap. With more than one book in a project, the top bar picks the book; **+ New** adds another book set in the same world, or starts a new project.
- **Editor** (centre): a distraction-free page in Literata, with spellcheck, find and replace (Ctrl+F), and Markdown that is styled rather than shown raw. `%% notes to self %%` are dimmed.
- **Inspector** (right): the open note's properties, such as a scene's place in the book, status, point of view, location, time and synopsis, a book's status, form, target and deadline, a world's genres, or a research note's source. Changing a scene's, world-bible note's or research note's name renames it, as `story-time rename` does: its file name and every link to it follow, and the message says how many links changed. A book's or world's **Title** renames the book or world, folder and all. A book's overview has **Delete book…**: its scenes move to the world's scratchpad, keeping their properties so they can go back into a book, and the rest of the book goes to the system trash (tick **Delete its scenes too** to send them with it). A world's note has **Delete world…**, which sends the whole world to the trash in one piece, so restoring it brings it back as it was. Both ask you to type `DELETE`, and Story Time can't undo them, though the trash can. Below the properties, **Delete…** moves a scene, world-bible note, research note or scrap to the system trash, where it can be restored, after saying how many links will lead nowhere; a deleted scene comes out of its book's contents. **Cut to the scratchpad** moves a scene to its world's scratchpad and out of the contents, keeping its properties, and **Put back** returns it to the open book as an unplaced scene. Any other scrap, in the world's scratchpad or the bookshelf's, can be **used as a scene** in the open book: it asks for a name if the scrap has none, and the scrap's text stays exactly as it is. A scrap that only led to a scene can stay in the scratchpad with a `used` link to it, chosen under **Led to a scene**, so the notebook keeps its history. Links to it, and its own, follow it either way.
- **Views:** *Board* (scene cards with synopses, grouped by chapter, to reorder by dragging), *Progress* (target, deadline pacing, last 30 days, scenes by status), *Check* (world-bible gaps, with one-click "create this note", spellings close to a bible name with "add to its other names" for one that's meant, and Rename for a file name that no longer matches its note's name), *Ideas* (capture ideas, or start a project from one).
- **Focus mode** (F11, Esc to leave): just the page, full screen, with typewriter scrolling: the line you're writing stays in the middle of the window, so your eyes stay in one place. Settings chooses whether that happens in focus mode, always, or never.
- **Settings** (click the bookshelf's name in the top bar, or Ctrl+,): the bookshelf's daily goal, author name and spelling languages, the contact details and paper for manuscripts, saved in its `.story-time/config.toml` with the file's comments kept; where the bookshelf is, opening another or starting a new one; and, for this computer, backups, the colours (the system's, light or dark) and typewriter scrolling.

**Reordering on the Board.** Drag a scene's card to move it within its chapter, into another chapter, into a new chapter at the end, or to Unplaced, and drag an unplaced scene into the book. Drag a chapter by its heading to move it with all its scenes, into another part if you like. With a card or a chapter's handle selected, Alt and the arrow keys move it one place. A chapter that a move leaves empty stays, ready for other scenes, until you delete it: **Delete** beside a chapter's heading (or the Delete key on its handle) takes it out of the contents with anything noted beneath it, and its scenes stay in the book as unplaced scenes; Undo puts it back. Ticking **Delete its scenes too** sends them to the system trash as well, once you type `DELETE`, and that can't be undone in Story Time. An empty chapter goes at once. Each move rewrites only the book's contents list, renaming nothing; Undo in the message that follows, or Ctrl+Z on the Board, puts it back. Each book keeps its last 100 moves to undo, one at a time, even after Story Time closes: they belong to this computer, so they're kept in Story Time's data folder beside the backups, not in the bookshelf. If the book note changed elsewhere since the Board showed it, nothing moves and the Board catches up; if it changed since a move, that move can no longer be undone, so the change is never overwritten.

Writing saves itself a moment after you stop typing. Before every save, Story Time checks that the file hasn't changed on disk since it was opened, for example through a sync service such as OneDrive, or on another computer. If it has, nothing is overwritten: you choose which version to keep. Switching back to the window reloads a note that was edited elsewhere.

**Backups.** The app backs the open bookshelf up by itself: once a day while it's open and again when it closes, unless nothing has changed. Each copy is a zip of the whole bookshelf, apart from `.git/` and compiled output, kept outside the bookshelf so a synced folder doesn't upload it and git doesn't see it. A later copy replaces that day's earlier one, and the copies from the 30 most recent days that have one are kept. To restore, unzip what you need. Settings for each computer, in the app's Settings or `~/.config/story-time/config.toml`, turn automatic backups off (`backup = false`) or put them elsewhere (`backup_folder = "..."`), such as a USB drive or another cloud folder, so one lost disk doesn't take both.

Shortcuts: Ctrl+S save now, Ctrl+1–5 switch views, Ctrl+, settings, Ctrl+F find, F11 focus mode, Ctrl+Z on the Board to undo a move.

### Running it

The first time on Debian or Ubuntu, install the system webview's development files:

```sh
sudo apt install pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libssl-dev \
  libxdo-dev libayatana-appindicator3-dev libdbus-1-dev
```

Then:

```sh
cd apps/desktop
npm install
npm run tauri dev      # run with live reload
npm run tauri build    # build installers (.deb, .rpm, .AppImage) into target/release/bundle
```

On Windows, install the [Tauri prerequisites](https://tauri.app/start/prerequisites/) and run the same npm commands to get an `.msi` installer.

Spellcheck uses the system dictionaries (hunspell on Linux). Set the language per folder in `.story-time/config.toml`, for example `spelling = ["en_AU"]`; the default is the system language.

## Architecture

```
crates/
├── core/            story-time-core: the library. Finding the bookshelf, projects, notes and
│                    properties, word counting, templates, structures, scaffolding, ideas,
│                    history, checks, compile.
└── cli/             story-time: the command-line front end. Argument parsing and printing.
apps/desktop/
├── src-tauri/       story-time-desktop: Tauri commands wrapping the core (src/commands.rs).
└── src/             The interface: Svelte 5 + TypeScript, CodeMirror 6 for the editor.
```

All behaviour lives in `core`, and each front end only presents it. The desktop app's Rust side is a thin layer of commands, and the interface calls them through `src/lib/api.ts`. A web version for other devices would be one more front end: a server crate (for example axum) serving the same Svelte interface.

[docs/writing-practice.md](docs/writing-practice.md) covers the writing practices behind the features, and ideas for what to build next.

## Development

```sh
cargo test                   # core and CLI: unit tests, plus end-to-end tests in crates/cli/tests
cargo clippy --all-targets
cargo fmt
cargo check --workspace      # also the desktop crate (needs the system packages above)
(cd apps/desktop && npm run check)   # type-check the interface
```

Plain `cargo` commands cover only the core and CLI (see `default-members` in `Cargo.toml`), so they work on machines without the webview libraries.

### Checking the desktop screens

Development builds (`npm run tauri dev`) read three optional variables, handled in `apps/desktop/src/lib/dev.ts`. Release builds leave that file out.

| Variable | Effect |
|---|---|
| `VITE_START_VIEW=board` | Open straight into a view (`write`, `board`, `progress`, `check`, `ideas`, `settings`, `focus`, `new-project`, `new-book`). |
| `VITE_SNAPSHOT_DIR=/tmp/shots` | Tour every screen and save a PNG of each, made by WebKitGTK from the window's own contents (Linux only). With no bookshelf set, it shows the screens before one opens: welcome, starting a bookshelf, and asking before setting up a folder. Along the way it types a test sentence into the open note through the editor, which exercises autosave, and on the Board drags the second scene before the first and undoes it, then starts dragging the last chapter and cancels. |
| `VITE_THEME=light` | Use the light palette whatever the system prefers. |

Point the app at a throwaway folder by giving it its own config and data directories, so your real stories and settings are untouched:

```sh
mkdir -p /tmp/st/config/story-time && echo 'bookshelf = "/tmp/st/shelf"' > /tmp/st/config/story-time/config.toml
XDG_CONFIG_HOME=/tmp/st/config XDG_DATA_HOME=/tmp/st/data VITE_SNAPSHOT_DIR=/tmp/st/shots npm run tauri dev
```

The tour leaves the app open; close it (`pkill -x story-time-desk`) before running another, or two tours will type into the same note. If the screens come out unstyled and the dev server logs `failed to load virtual css module`, WebKit served components from its cache of an earlier session: give it an empty cache directory with `XDG_CACHE_HOME`.

### Releases

Pushing a version tag builds the installers on GitHub (`.github/workflows/release.yml`): `.deb`, `.rpm` and `.AppImage` on Ubuntu 22.04, `.msi` and `-setup.exe` on Windows, and a `.dmg` each for Apple silicon and Intel Macs. They are attached to a draft release, which stays private until you publish it.

1. Set the new version in `Cargo.toml` (`[workspace.package]`), `apps/desktop/src-tauri/tauri.conf.json` and `apps/desktop/package.json` (`npm version 0.2.0 --no-git-tag-version` updates its lock file too), then run `cargo check` so `Cargo.lock` follows. The workflow stops if the tag and the versions disagree.
2. Commit, then tag and push: `git tag -a v0.2.0 -m "v0.2.0" && git push origin main v0.2.0`.
3. When the run finishes, check the draft release on GitHub and publish it.

Versions follow [semantic versioning](https://semver.org). Before 1.0, raise the minor version (0.2.0) for new features or any change to the bookshelf format, configuration or CLI, and the patch version (0.1.1) for fixes. To build an existing tag again, run the Release workflow from the Actions tab and give it the tag.

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT licence ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work by you, as defined in the Apache-2.0 licence, shall be dual licensed as above, without any additional terms or conditions.
