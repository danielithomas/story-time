# How writers organise their work, and where story-time fits

Tools like Scrivener, and the habits of working novelists and screenwriters, keep returning to a handful of ideas. This note summarises them, maps each one onto the bookshelf and story-time, and lists what could come next. The craft is the writer's; the tooling only removes friction.

## 1. Keep the book apart from everything about the book

Scrivener's binder separates the *Draft* (the manuscript) from *Research* and notes. Television writers keep the scripts apart from the *series bible*. The point is that the manuscript can be measured, read and exported on its own, while notes can grow freely without polluting it.

**On the bookshelf:** a book's `scenes/`, in the order its contents list gives them, are the book; `bible/` (characters, locations, world notes and the timeline) is the world bible, shared by every book set in the world; the flow note is the plan. Research and scratchpads sit beside them and are never compiled. Inside a scene, notes-to-self go in `%% comments %%` or callouts (`> [!todo] check the physics`). Obsidian hides comments in Reading view, and story-time leaves both out of word counts and compiled manuscripts. You can leave a note mid-sentence without breaking the flow of drafting.

## 2. The world bible

A bible holds the facts the story must stay consistent with. Keep it lean: it serves the book, it is not the book.

- **Characters.** The useful split is *want* (the external goal they chase) versus *need* (the internal truth they must accept), plus what they fear. Voice, meaning how someone talks, is what readers notice when it slips.
- **Places.** Sensory detail (sight, sound, smell, touch, taste) makes a place feel real. Many writers treat an important place as almost a character.
- **World.** Brandon Sanderson's *Second Law*: limitations are more interesting than powers. For science fiction this is the core of a world note. What does the technology or the magic *cost*, and what can it *not* do?
- **Style sheet.** Copy editors keep a sheet of spellings, capitalisation and invented terms: is it *jump drive* or *Jump Drive*, *Mara* or *Marra*? Science fiction invents a lot of vocabulary, so this pays off quickly. It isn't in the scaffold yet; a `bible/world/style-sheet.md` per project would be a natural home.
- **Chronology.** The order events *happen* in the world often differs from the order they are *told*. A book's contents and flow hold the telling order; the world's timeline note, and each scene's `when`, hold the order things happened in.

**story-time:** templates for each kind of note prompt for these (how characters look, speak and what drives them; how a place feels; how the world works). `aliases` record nicknames. `check` reports both directions of drift: names linked in the story with no bible note yet, and bible notes that never reach the page.

## 3. Planning: from plotter to discovery writer

Writers sit on a spectrum. *Plotters* outline first; *discovery writers* ("pantsers") find the story by writing it; most work somewhere between. No position is better. What matters is knowing where you sit and making the tooling fit.

- **Grow the story outward.** Randy Ingermanson's *Snowflake Method* starts from one sentence and expands it: a paragraph, character summaries, a page of synopsis, then a scene list. The project note is built for this: logline, then premise, then the flow note, then scenes.
- **Beat sheets** describe where turning points tend to land. They are most useful as a *diagnostic* during revision ("my midpoint arrives at 70%"), less so as a template to fill. `new --structure` lays out three-act, Save the Cat (Jessica Brody's novel version), the Hero's Journey (Vogler), Dan Harmon's Story Circle and kishōtenketsu, each beat with the word count it lands at for your target.
- **Scenes.** Dwight Swain's *scene and sequel*: a scene is goal → conflict → disaster (the character ends worse off), and a sequel is reaction → dilemma → decision (which sets the next goal). One line per scene in the flow note (`POV · location · goal → conflict → outcome`) is enough to see whether each scene turns.
- **Corkboard.** A one- or two-sentence `synopsis` per scene lets you see the whole story at once. That is Scrivener's corkboard: the app's Board, and what `outline` prints.

## 4. Drafting

- **Draft forward.** Editing while drafting stalls first drafts. Mark problems with a `%% fix %%` and keep going. A scene's `status` is a reminder of which pass it is on.
- **A modest daily goal, kept.** Consistency beats bursts. Graham Greene famously stopped at 500 words a day. The NaNoWriMo pace of 50,000 words in a month is about 1,667 a day, a sprint rather than a habit. A goal you hit most days builds the habit; a streak makes it visible.
- **Count honestly.** Net words is the honest measure: a day spent cutting 2,000 words of a bad subplot is a good day, even if the count goes down.

**story-time:** `today`, `history` and the writing log; `daily_goal` in `.story-time/config.toml`; `snapshot` for a nightly cron job so no day is missed.

## 5. Revising

Revise in passes, from large to small, so you don't polish a scene you later cut:

1. **Structure.** Does the story work, are the turning points where they need to be, does every scene turn?
2. **Scenes and lines.** Does each scene earn its place; is the prose clear and in voice?
3. **Copy edit.** Consistency of names, terms and facts (the style sheet, `check`).
4. **Proofread.**

Reading the book in a different form exposes problems the editor hides. Compile to EPUB and read it on a phone or e-reader (`compile --format epub`), or print a Word copy.

## 6. Length

The Science Fiction and Fantasy Writers Association (Nebula Awards) categories, also used by the Hugos:

| Form | Words |
|---|---|
| Flash fiction | under 1,000 (common usage, not an SFWA category) |
| Short story | under 7,500 |
| Novelette | 7,500–17,500 |
| Novella | 17,500–40,000 |
| Novel | 40,000 and over |

Science fiction and fantasy novels from new authors typically run 90,000–120,000 words. `new --form` sets a default target (novel 90,000, novella 30,000, ...), `stats` shows which category a draft currently falls into, and a `deadline` turns the remaining words into a daily figure.

## 7. Backups and versions

The 3-2-1 rule: three copies, on two kinds of media, one off-site. The desktop app backs a bookshelf up by itself, a zip a day for the last 30 days that have one, outside the bookshelf; pointing `backup_folder` at a drive or cloud folder of its own makes those copies a second medium or off-site. A bookshelf in git with a remote such as GitHub meets it: the working copy, the local history and the remote. Git commits also give you what Scrivener calls *snapshots*: every earlier version of every scene.

- The obsidian-git plugin commits and syncs on a timer (`autoSaveInterval`, in minutes), and only while Obsidian is open. Work done between syncs, or with Obsidian closed, exists only locally until the next sync. A shorter interval narrows that window.
- **OneDrive** works fine for a plain folder, and story-time doesn't care where the bookshelf lives. Don't put a *git repository* inside a synced folder, though: two sync engines working on `.git/` can corrupt it. Choose git with GitHub, or OneDrive, for a given folder. On Linux there is no official OneDrive client; the usual choices are the open-source `onedrive` client (abraunegg) or `rclone`.

## What could come next

Ordered roughly by value for effort:

1. **Desktop app, next steps.** Delete notes, and test the Windows and macOS installers.
2. **Style-sheet check.** Flag near-miss spellings of bible names and terms (*Marra* for *Mara*) using edit distance.
3. **History from git.** Backfill the writing log from obsidian-git's commits, so days when story-time wasn't run still count.
4. **Standard manuscript format.** A pandoc reference document so `compile --format docx` produces William Shunn's submission format (12-point serif, double-spaced, header with surname / title / page).
5. **Chronology.** Read each scene's `when` and the timeline note, and show story time against telling order (#21).
6. **`--json` output** for integrations: Obsidian's Shell Commands plugin, n8n, or a web dashboard.
