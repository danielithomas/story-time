# CLAUDE.md

story-time is a Rust toolkit for a writer's bookshelf, a folder of Markdown files that Obsidian can also open as a vault, so a writer who stops using Story Time loses nothing (don't assume Obsidian is open alongside it), with two front ends: the `story-time` CLI and the Story Time desktop app (Tauri, in `apps/desktop`). README.md documents commands and conventions; docs/writing-practice.md explains the reasoning behind the features and lists next steps. docs/bookshelf-format.md is the approved bookshelf format 1, which the code implements: change it only with the owner's approval, and the code with it.

Everything a person reads (the app, CLI help and messages, docs) calls the folder a bookshelf, never a vault (#30); the code calls it `Vault`.

## Commands

```sh
cargo test                   # core unit tests + end-to-end tests (crates/cli/tests/cli.rs)
cargo clippy --all-targets   # keep at zero warnings
cargo fmt
cargo clippy --workspace --all-targets      # includes the desktop crate; needs webkit2gtk dev packages
cd apps/desktop && npm run check            # svelte-check: keep at zero errors and warnings
cd apps/desktop && npm run tauri dev        # run the desktop app
```

To see the desktop screens, use the snapshot tour (README, "Checking the desktop screens") with `XDG_CONFIG_HOME`/`XDG_DATA_HOME` pointed at a throwaway vault. Don't use desktop screenshot tools such as `spectacle`: they capture whatever window has focus, which can be a private browser window. Stop the app with `pkill -x story-time-desk`; `pkill -f` patterns that appear in your own command line kill your own shell.

Releases are built by `.github/workflows/release.yml` when a `vX.Y.Z` tag is pushed (README, "Releases"). The tag must equal the version in `Cargo.toml`, `apps/desktop/package.json` and `apps/desktop/src-tauri/tauri.conf.json`. Never attach locally built binaries to a release: they embed the build machine's home-directory paths.

## Architecture

- `crates/core` (`story-time-core`) holds all behaviour. `crates/cli` (`story-time`) parses arguments and prints. `apps/desktop/src-tauri` exposes core functions as Tauri commands (`src/commands.rs`, DTOs with camelCase serde); the Svelte interface in `apps/desktop/src` calls them through `src/lib/api.ts`, with shapes mirrored in `src/lib/types.ts`. Keep those three in step. Front ends never read the vault directly; new behaviour goes in core first.
- Desktop saving (`src/lib/store.svelte.ts`): debounced autosave, serialised through one queue, with a content hash so a file changed on disk (by a sync service such as OneDrive, or on another computer) is never overwritten silently. That becomes a conflict for the writer to resolve. Commands validate that paths are Markdown files inside the vault.
- Tauri crates and `@tauri-apps/*` npm packages must share major.minor versions; the workspace `rust-version` feeds Cargo's MSRV-aware resolver, so raising Tauri may mean raising it.
- The vault's files are the only source of truth. Don't add a database or cache; anything story-time stores must be readable Markdown or simple text in `.story-time/`. What belongs to one computer rather than to the writing goes in Story Time's data folder (`vault::data_dir`, in a folder per bookshelf from `Vault::folder_in`), never in the vault: the daily backups (`backup.rs`) and Board undo history (`undo.rs`, a TOML file per book).
- Scaffolding never overwrites files: `scaffold::write_new` uses `create_new`. Changes to the ideas file only append (`ideas::add_idea`, `ideas::link_idea`). The only code that rewrites notes is the desktop `save_note` command, using `note::set_properties` (edits just the lines of the properties being set) and `note::replace_body` (keeps the frontmatter as written), and `note::save_section` for the `## Books` and `## Contents` lists. Both sit behind the hash check. The exceptions are renaming, cutting and putting back a scene (`rename.rs`, `delete.rs`), which a writer asks for: they rewrite the note's name and first heading (on a rename) and the links that would no longer reach it, reading each file fresh and changing only those links.
- `project.rs`: a `Project` is a world (project note, world bible and timeline in `bible/`, books listed under `## Books`, `research/` and `scratchpad/`); a `Book` (`books/<slug>/`) owns its status, form, target, deadline, flow and `scenes/`. Measuring and the writing log work per book (`Book::log_key`). `note::save_section` rewrites a single `## section`, and only if the file is unchanged since it was read: that is how story-time edits the lists it owns.
- `contents.rs`: a book's `## Contents`. `Listing` is the list as written, with line positions, and `insert` adds an entry while leaving every other line as it was; `Contents` resolves the entries to the book's scenes (parts, chapters, unplaced scenes, missing and repeated entries). Order comes only from this list, never from file names. `move_scene`, `move_chapter` and `delete_chapter` change it for the Board, behind the hash check; a move never removes a chapter, even one it empties.
- `scratchpad.rs`: the vault's and each project's scratchpad (`Scratchpad`, whose words the writing log records under the project's ID, or `scratchpad` for the vault's) and research notes. `Vault::counts` gives every book's and scratchpad's count, so use it for snapshots and live previews. `check` leaves scraps out: fragments may link to notes that don't exist, and captured ones are named by date. `scaffold::add_scrap` stores captured text exactly as given; never edit it afterwards.
- Names and links: files and folders are kebab-case slugs (`text::slugify`), a note's real name is its `title` or first alias (`Note::title`), and `links.rs` resolves and writes wikilinks. Write links through `LinkIndex::link` so Obsidian can follow them. `rename.rs` renames a scene, world-bible note or research note: its name, its file and every link to it together (not the ideas file or templates); books and projects can't be renamed yet. `delete.rs` deletes a note through a `discard` function (the system trash in the app; tests pass their own so nothing reaches a real trash) and cuts a scene to its world's scratchpad or puts it back. Both say where a note belongs with `home::Home` and rewrite links with `links::relink`.
- Word-counting rules live in `words.rs`, and `compile` reuses `words::prose`, so counts and manuscripts agree. Change both together and update the README's "Word counts" paragraph.
- Built-in templates are `crates/core/templates/*.md`, compiled in with `include_str!`. A vault's `templates/` folder overrides them.
- `example.rs` adds the example world (`init --example`, and Start a bookshelf in the app). Its notes are in `crates/core/example/`, laid out as on the bookshelf. They describe the app, so keep them true when features change, and passing `check` (a test does that).
- `upgrade.rs` converts a vault from story-time 0.1's layout (format 0: a config file without `format`). `plan` decides every move and edit without touching the vault and stops on any clash; `apply` backs the vault up into `.story-time/backups/` first. It never edits the ideas file or templates. `crates/core/templates/format-0/` holds 0.1's templates, so unchanged copies can be replaced. Commands refuse a format-0 vault (`Config::ensure_current`) except `upgrade`; the desktop app shows an upgrade screen instead.

## Boundaries

Never write story content (prose, plots, characters, ideas) into the vault or into tests that use the real vault. Test against temporary vaults.
