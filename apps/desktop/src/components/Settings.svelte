<script lang="ts">
  import { onMount } from "svelte";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api } from "../lib/api";
  import {
    app,
    chooseBookshelfPlace,
    chooseVault,
    saveSettings,
    setTheme,
    setTypewriter,
    toastError,
    type Theme,
    type Typewriter,
  } from "../lib/store.svelte";
  import type { SettingsInfo } from "../lib/types";

  let settings = $state<SettingsInfo | null>(null);
  let dailyGoal = $state(0);
  let author = $state("");
  let contact = $state("");
  let paper = $state("");
  let spelling = $state("");

  onMount(async () => {
    try {
      settings = await api.settings();
      dailyGoal = settings.dailyGoal;
      author = settings.author;
      contact = settings.contact.join("\n");
      paper = settings.paper;
      spelling = settings.spelling.join(", ");
    } catch (error) {
      toastError(error);
    }
  });

  function saveWriting() {
    void saveSettings({
      dailyGoal: Math.max(0, Math.round(dailyGoal || 0)),
      author,
      contact: contact.split("\n").map((line) => line.trim()).filter(Boolean),
      paper,
      spelling: spelling.split(",").map((code) => code.trim()).filter(Boolean),
    });
  }

  async function setBackup(on: boolean, folder: string | null) {
    try {
      settings = await api.setBackup(on, folder);
    } catch (error) {
      toastError(error);
    }
  }

  async function chooseBackupFolder() {
    if (!settings) return;
    const picked = await chooseBookshelfPlace(settings.backupFolder);
    if (picked) await setBackup(settings.backup, picked);
  }

  const reveal = (path: string) => revealItemInDir(path).catch(toastError);
  /** The date a copy was made, from its name. */
  const made = (path: string) => path.split(/[\\/]/).pop()?.replace(/\.zip$/, "") ?? path;

  const THEMES: { key: Theme; label: string }[] = [
    { key: "system", label: "Same as the system" },
    { key: "light", label: "Light" },
    { key: "dark", label: "Dark" },
  ];

  const TYPEWRITER: { key: Typewriter; label: string }[] = [
    { key: "focus", label: "In focus mode" },
    { key: "always", label: "Always" },
    { key: "off", label: "Off" },
  ];
</script>

<div class="view">
  <header>
    <h1>Settings</h1>
    <p class="muted">Writing settings belong to this bookshelf. Backups and appearance belong to this computer.</p>
  </header>

  {#if settings}
    <section>
      <h2>Writing</h2>
      <div class="fields">
        <label class="field">
          <span>Daily goal, in words</span>
          <input type="number" min="0" step="50" bind:value={dailyGoal} onchange={saveWriting} />
        </label>
        <label class="field">
          <span>Author</span>
          <input bind:value={author} placeholder="Your name" onchange={saveWriting} />
        </label>
        <p class="hint">Printed on compiled manuscripts.</p>
        <label class="field">
          <span>Spelling</span>
          <input bind:value={spelling} placeholder="The system's language" onchange={saveWriting} />
        </label>
        <p class="hint">Languages to check spelling in, as codes such as en_AU or en_GB, separated by commas.</p>
      </div>
    </section>

    <section>
      <h2>Manuscripts</h2>
      <p class="muted">
        A book exported as a Word document is in standard manuscript format, ready to send to editors and agents: your
        name and these details top left of the first page, the word count top right, and your surname, a word from the
        title and the page number at the top of every other page.
      </p>
      <div class="fields">
        <label class="field">
          <span>Contact details</span>
          <textarea rows="3" bind:value={contact} placeholder="Address, phone, email: a line each" onchange={saveWriting}></textarea>
        </label>
        <label class="field">
          <span>Paper</span>
          <select bind:value={paper} onchange={saveWriting}>
            <option value="">Follow the language (Letter in the US and Canada, else A4)</option>
            <option value="a4">A4</option>
            <option value="letter">US Letter</option>
          </select>
        </label>
      </div>
    </section>

    <section>
      <h2>Bookshelf</h2>
      <p><strong>{app.vault?.name}</strong> <code class="path">{app.vault?.path}</code></p>
      <div class="actions">
        <button class="btn" onclick={() => app.vault && reveal(app.vault.path)}>Show in file manager</button>
        <button class="btn" onclick={chooseVault}>Open another bookshelf</button>
        <button class="btn" onclick={() => (app.starting = true)}>Start a new bookshelf</button>
      </div>
    </section>

    <section>
      <h2>Backups</h2>
      <label class="check">
        <input type="checkbox" checked={settings.backup} onchange={(e) => setBackup(e.currentTarget.checked, settings?.chosenBackupFolder ? settings.backupFolder : null)} />
        <span>Back up automatically: once a day while Story Time is open, and when it closes</span>
      </label>
      <p class="muted">
        Each copy is a zip of the whole bookshelf. A later copy replaces that day's earlier one, and the 30 most recent
        copies are kept. Put them on another drive or in a cloud folder, so one lost disk doesn't take both.
      </p>
      <div class="place">
        <code class="path" title={settings.backupFolder}>{settings.backupFolder}</code>
        <button class="btn" onclick={chooseBackupFolder}>Change…</button>
        {#if settings.chosenBackupFolder}
          <button class="btn" onclick={() => settings && setBackup(settings.backup, null)}>Use the default</button>
        {/if}
      </div>
      <p class="muted">
        {#if settings.lastBackup}
          {@const last = settings.lastBackup}
          Last copy: {made(last)}.
          <button class="link" onclick={() => reveal(last)}>Show it</button>
        {:else}
          No copies yet.
        {/if}
      </p>
    </section>

    <section>
      <h2>Appearance</h2>
      <label class="field narrow">
        <span>Colours</span>
        <select value={app.theme} onchange={(e) => setTheme(e.currentTarget.value as Theme)}>
          {#each THEMES as theme (theme.key)}
            <option value={theme.key}>{theme.label}</option>
          {/each}
        </select>
      </label>
      <label class="field narrow">
        <span>Typewriter scrolling</span>
        <select value={app.typewriter} onchange={(e) => setTypewriter(e.currentTarget.value as Typewriter)}>
          {#each TYPEWRITER as option (option.key)}
            <option value={option.key}>{option.label}</option>
          {/each}
        </select>
      </label>
      <p class="hint">Keeps the line you're writing in the middle of the window, so your eyes stay in one place.</p>
    </section>
  {/if}
</div>

<style>
  .view {
    height: 100%;
    overflow-y: auto;
    padding: 2rem 2.5rem 4rem;
    display: grid;
    gap: 1.8rem;
    align-content: start;
  }

  .view > * {
    max-width: 640px;
  }

  header,
  section {
    display: grid;
    gap: 0.6rem;
    justify-items: start;
  }

  h1 {
    font-size: 26px;
  }

  h2 {
    font-size: 16px;
  }

  p {
    margin: 0;
    line-height: 1.5;
  }

  .fields {
    display: grid;
    gap: 0.6rem;
    width: 100%;
  }

  .field.narrow {
    width: min(20rem, 100%);
  }

  .hint {
    margin-top: -0.3rem;
    font-size: 12px;
    color: var(--muted);
  }

  .actions,
  .place {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }

  .place {
    width: 100%;
  }

  .path {
    overflow-wrap: anywhere;
  }

  .place .path {
    flex: 1;
    min-width: 0;
  }

  .check {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
  }

  .link {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    cursor: pointer;
  }
</style>
