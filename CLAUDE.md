# CLAUDE.md

story-time is a Rust toolkit for a writer's Markdown (Obsidian) vault, with two front ends: the `story-time` CLI and the Story Time desktop app (Tauri, in `apps/desktop`). README.md documents commands and conventions; docs/writing-practice.md explains the reasoning behind the features and lists next steps. docs/vault-format.md is the approved vault format 1 (not yet implemented): change code to match it, and change it only with the owner's approval.

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
- Desktop saving (`src/lib/store.svelte.ts`): debounced autosave, serialised through one queue, with a content hash so a file changed on disk (Obsidian, OneDrive) is never overwritten silently. That becomes a conflict for the writer to resolve. Commands validate that paths are Markdown files inside the vault.
- Tauri crates and `@tauri-apps/*` npm packages must share major.minor versions; the workspace `rust-version` feeds Cargo's MSRV-aware resolver, so raising Tauri may mean raising it.
- The vault's files are the only source of truth. Don't add a database or cache; anything story-time stores must be readable Markdown or simple text in `.story-time/`.
- Scaffolding never overwrites files: `scaffold::write_new` uses `create_new`. Changes to the ideas file only append (`ideas::add_idea`, `ideas::link_idea`). The only code that rewrites notes is the desktop `save_note` command, behind the hash check, using `note::set_properties` (edits just the lines of the properties being set) and `note::replace_body` (keeps the frontmatter as written).
- `project.rs`: a `Project` is a world (project note, world bible in `bible/`, books listed under `## Books`); a `Book` (`books/<slug>/`) owns its status, form, target, deadline, flow and chapters. Measuring and the writing log work per book (`Book::log_key`). `note::set_section` rewrites a single `## section`, which is how story-time edits lists it owns.
- Names and links: files and folders are kebab-case slugs (`text::slugify`), a note's real name is its `title` or first alias (`Note::title`), and `links.rs` resolves and writes wikilinks. Write links through `LinkIndex::link` so Obsidian can follow them.
- Word-counting rules live in `words.rs`, and `compile` reuses `words::prose`, so counts and manuscripts agree. Change both together and update the README's "Word counts" paragraph.
- Built-in templates are `crates/core/templates/*.md`, compiled in with `include_str!`. A vault's `templates/` folder overrides them.

## Boundaries

Never write story content (prose, plots, characters, ideas) into the vault or into tests that use the real vault. Test against temporary vaults.
