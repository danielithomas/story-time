# story-time

Write, organise, measure and compile stories kept as Markdown in a folder you choose. Two front ends share one core:

- **Story Time**, a desktop app for writing (`apps/desktop`, built with Tauri)
- **`story-time`**, a command-line tool (`crates/cli`)

An Obsidian vault works as-is.

The vault is the source of truth: plain Markdown files with a few properties (YAML frontmatter). story-time reads them and writes ordinary Markdown back. It keeps no database and no format of its own, so you can stop using it at any time. The vault can live in git, on OneDrive, or anywhere else a folder can.

## Install

```sh
cargo install --path crates/cli     # installs the `story-time` binary to ~/.cargo/bin
```

Needs Rust 1.90 or later. [pandoc](https://pandoc.org) is optional; it is used only to compile to Word, EPUB, HTML or PDF (PDF also needs a LaTeX engine).

## Getting started

```sh
cd ~/stories
story-time init                                   # config, templates, Obsidian template folder
story-time new "Sample Story" --from-idea         # project folder; premise copied from the ideas file
story-time add chapter "Arrival"                  # chapters/01 Arrival.md
story-time add character "Mara Chen"              # characters/Mara Chen.md
story-time stats                                  # every project at a glance
```

### Finding the vault

Commands use the first of:

1. `--vault <path>`
2. the `STORY_TIME_VAULT` environment variable
3. the nearest folder at or above the current directory that holds `.story-time/` or `.obsidian/`
4. `vault = "~/path/to/stories"` in `~/.config/story-time/config.toml`

Option 4 makes every command work from any directory, which helps if the vault moves somewhere like `~/OneDrive/stories`.

## Commands

| Command | What it does |
|---|---|
| `init [--force]` | Writes `.story-time/config.toml`, the note templates in `Templates/`, `.obsidian/templates.json` (pointing Obsidian's Templates plugin at that folder) and, in a git repository, a `.gitignore` entry for compiled output. Existing files are kept; `--force` replaces only the templates. |
| `new "Title" [--form F] [--target N] [--structure S] [--from-idea [NAME]]` | Creates `<slug>/` with the project note, the story flow and the four folders. `--structure` lays out the flow note with beats and the word count each lands at (`three-act`, `save-the-cat`, `heros-journey`, `story-circle`, `kishotenketsu`, `freeform`). `--from-idea` copies the premise from the ideas file and appends `→ [[slug]]` to that idea. Nothing in the ideas file is removed or reworded. |
| `add chapter\|character\|location\|world "Name" [-p project]` | Creates a note from its template. Chapters are numbered after the highest existing number. |
| `stats [project]` | All projects: status, form, chapters, words, progress to target. One project: chapter table, progress, deadline pacing, length category, manuscript pages and reading time, words today, story-bible size. |
| `outline [project]` | Each chapter's status, POV, word count and synopsis, in order: a corkboard for the terminal. |
| `today` | Words written today against the daily goal, per project, plus your streak. |
| `history [--days N]` | Words written per day. |
| `snapshot` | Records word counts silently, for a cron job or git hook. |
| `idea Name: description [--non-fiction]` | Appends an idea under `### Fiction` (or `### Non-Fiction`) in the ideas file, in the list style already there. |
| `check [project]` | Story-bible gaps: links to notes that don't exist yet, character, location and world notes whose name never appears in a chapter, and chapters missing a status or synopsis. |
| `compile [project] [--format md\|docx\|epub\|html\|pdf] [-o path]` | Chapters in order as one manuscript, minus notes-to-self. Defaults to `.story-time/build/<slug>.<format>`. |

Without a project argument, commands other than `stats` use the project folder you are in, or the only project in the vault. Otherwise give its slug, title, or an unambiguous part of either (`stats edge`).

## Conventions

**Projects.** A project is any folder at the vault root that holds a note with the same name: `sample-story/sample-story.md`. Next to it are `sample-story-flow.md` and `chapters/`, `characters/`, `locations/` and `world/`.

**Properties** read by story-time (all optional, all editable in Obsidian):

| Note | Property | Meaning |
|---|---|---|
| project | `title`, `status`, `form`, `genre` | shown in `stats` |
| project | `target` | target words; `90000` or `"90,000"` |
| project | `deadline` | `YYYY-MM-DD`; `stats` works out the words a day needed |
| chapter | `status` | `outline` → `draft` → `revised` → `final` |
| chapter | `pov`, `synopsis` | shown in `stats` and `outline` |
| character, location, world | `aliases` | other names; `check` looks for these on the page |

**Chapter order** comes from the file path, comparing numbers as numbers: `2 x` before `10 y`. Subfolders such as `chapters/part-1/` work too.

**Word counts** cover the prose a reader would read. They leave out properties, headings (chapter and scene titles), `%% Obsidian comments %%`, `<!-- HTML comments -->`, callouts such as `> [!todo]`, code blocks and embeds. Links count as the text they display. Hyphenated words count once; words joined by an em dash count separately.

**Templates** in the vault's `Templates/` folder override the built-in ones, so editing a template in Obsidian changes what `add` and `new` create. Obsidian fills `{{title}}` and `{{date}}`. story-time also fills `{{project}}`, `{{project_slug}}`, `{{slug}}`, `{{form}}`, `{{target}}`, `{{premise}}`, `{{structure}}` and `{{structure_name}}`. Those placeholders stay literal if you insert the Project or Story Flow template by hand.

**The writing log** is `.story-time/history.csv`. Each time `stats`, `today`, `history` or `snapshot` runs, it appends a line for every project whose count changed. Words written on a day are the net change from the previous snapshot, so deleting words counts against the day. If you write without running a command, those words count toward the next day you do. To log every day, schedule a snapshot:

```cron
55 23 * * * $HOME/.cargo/bin/story-time --vault $HOME/stories snapshot
```

## The desktop app

Story Time is a dedicated writing program laid out like Scrivener:

- **Binder** (left): the project's overview, story flow, chapters (numbered, with status and word count), characters, locations and world notes, each with a `+` to add one.
- **Editor** (centre): a distraction-free page in Literata, with spellcheck, find and replace (Ctrl+F), and Markdown that is styled rather than shown raw. `%% notes to self %%` are dimmed.
- **Inspector** (right): the open note's properties, such as chapter status, point of view and synopsis, or a project's form, target and deadline.
- **Views:** *Board* (chapter cards with synopses), *Progress* (target, deadline pacing, last 30 days, chapters by status), *Check* (story-bible gaps, with one-click "create this note"), *Ideas* (capture ideas, or start a project from one).
- **Focus mode** (F11, Esc to leave): just the page, full screen.

Writing saves itself a moment after you stop typing. Before every save, Story Time checks that the file hasn't changed on disk since it was opened, for example in Obsidian or through OneDrive sync. If it has, nothing is overwritten: you choose which version to keep. Switching back to the window reloads a note that was edited elsewhere.

Shortcuts: Ctrl+S save now, Ctrl+1–5 switch views, Ctrl+F find, F11 focus mode.

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
├── core/            story-time-core: the library. Vault discovery, projects, notes and
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
| `VITE_START_VIEW=board` | Open straight into a view (`write`, `board`, `progress`, `check`, `ideas`, `focus`, `new-project`). |
| `VITE_SNAPSHOT_DIR=/tmp/shots` | Tour every screen and save a PNG of each, made by WebKitGTK from the window's own contents (Linux only). Along the way it types a test sentence into the open note through the editor, which exercises autosave. |
| `VITE_THEME=light` | Use the light palette whatever the system prefers. |

Point the app at a throwaway folder by giving it its own config and data directories, so your real stories and settings are untouched:

```sh
mkdir -p /tmp/st/config/story-time && echo 'vault = "/tmp/st/vault"' > /tmp/st/config/story-time/config.toml
XDG_CONFIG_HOME=/tmp/st/config XDG_DATA_HOME=/tmp/st/data VITE_SNAPSHOT_DIR=/tmp/st/shots npm run tauri dev
```

### Releases

Pushing a version tag builds the installers on GitHub (`.github/workflows/release.yml`): `.deb`, `.rpm` and `.AppImage` on Ubuntu 22.04, `.msi` and `-setup.exe` on Windows, and a `.dmg` each for Apple silicon and Intel Macs. They are attached to a draft release, which stays private until you publish it.

1. Set the new version in `Cargo.toml` (`[workspace.package]`), `apps/desktop/src-tauri/tauri.conf.json` and `apps/desktop/package.json` (`npm version 0.2.0 --no-git-tag-version` updates its lock file too), then run `cargo check` so `Cargo.lock` follows. The workflow stops if the tag and the versions disagree.
2. Commit, then tag and push: `git tag -a v0.2.0 -m "v0.2.0" && git push origin main v0.2.0`.
3. When the run finishes, check the draft release on GitHub and publish it.

Versions follow [semantic versioning](https://semver.org). Before 1.0, raise the minor version (0.2.0) for new features or any change to the vault format, configuration or CLI, and the patch version (0.1.1) for fixes. To build an existing tag again, run the Release workflow from the Actions tab and give it the tag.

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT licence ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work by you, as defined in the Apache-2.0 licence, shall be dual licensed as above, without any additional terms or conditions.
