<script lang="ts">
  import { onMount } from "svelte";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api } from "../lib/api";
  import {
    app,
    chooseBookshelfPlace,
    chooseVault,
    saveSettings,
    connectSync,
    disconnectSync,
    setKeepHistory,
    syncNow,
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

  let turning = $state(false);
  let address = $state("");
  let connecting = $state(false);

  async function connect(event: SubmitEvent) {
    event.preventDefault();
    connecting = true;
    if (await connectSync(address)) {
      address = "";
      settings = await api.settings().catch(() => settings);
    }
    connecting = false;
  }

  async function disconnect() {
    settings = (await disconnectSync()) ?? settings;
  }

  async function keepHistory(on: boolean) {
    turning = true;
    settings = (await setKeepHistory(on)) ?? settings;
    turning = false;
  }

  /** When a change was recorded, from its local time: "today at 9:41 pm" or "Sat 14 Nov at 9:41 pm". */
  function when(at: string) {
    const date = new Date(at);
    const time = date.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
    const today = new Date().toDateString() === date.toDateString();
    const day = date.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
    return `${today ? "today" : day} at ${time}`;
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
      <h2>History</h2>
      <label class="check">
        <input
          type="checkbox"
          checked={settings.keepHistory}
          disabled={turning}
          onchange={(e) => keepHistory(e.currentTarget.checked)}
        />
        <span>Keep a history: record every change a few minutes after you stop writing, and when Story Time closes</span>
      </label>
      <p class="muted">
        Earlier versions of your notes are kept with the bookshelf, inside its folder. Turning this off keeps what was
        recorded.
      </p>
      {#if settings.syncedFolder}
        <p class="warning">
          This bookshelf is in {settings.syncedFolder}. {settings.syncedFolder} can damage a history while it copies the
          folder, so keep a history only for a bookshelf outside {settings.syncedFolder}.
        </p>
      {/if}
      {#if settings.keepHistory || settings.lastRecorded}
        <p class="muted">
          {#if settings.lastRecorded}
            Last recorded {when(settings.lastRecorded.at)}: {settings.lastRecorded.message}.
          {:else}
            Nothing recorded yet.
          {/if}
        </p>
      {/if}
      {#if settings.keepHistory}
        <h3>Sync with your other computers</h3>
        {#if !settings.canSync}
          <p class="muted">This bookshelf is part of a larger git repository, so Story Time leaves syncing it to you.</p>
        {:else if settings.syncAddress}
          <p>Syncing with <code class="path">{settings.syncAddress}</code></p>
          <p class="muted">
            Story Time syncs when the bookshelf opens, a few minutes after you stop writing, and when you come back to
            it after a while; and shares your writing as it closes.
            {#if app.sync.state === "synced" && app.sync.at}
              Last synced {when(app.sync.at)}.
            {:else if app.sync.state === "failed"}
              <span class="problem">Not synced: {app.sync.error}.</span>
            {/if}
          </p>
          <div class="actions">
            <button class="btn" disabled={app.sync.state === "syncing"} onclick={() => syncNow()}>
              {app.sync.state === "syncing" ? "Syncing…" : "Sync now"}
            </button>
            <button class="btn" onclick={disconnect}>Stop syncing</button>
          </div>
        {:else}
          <p class="muted">
            To have this bookshelf on another computer, create an empty private repository with a service such as
            GitHub, GitLab or Codeberg, and paste its address here. Then, on the other computer, choose
            <strong>Open a bookshelf from another computer</strong> when Story Time starts.
          </p>
          <form class="place" onsubmit={connect}>
            <input
              class="address"
              bind:value={address}
              placeholder="https://github.com/you/stories.git"
              aria-label="The repository's address"
            />
            <button class="btn" disabled={connecting || !address.trim()}>{connecting ? "Connecting…" : "Connect"}</button>
          </form>
        {/if}
      {/if}
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

  h3 {
    margin: 1rem 0 0;
    font-size: 14px;
  }

  .address {
    flex: 1;
    min-width: 0;
  }

  .problem {
    color: var(--danger);
  }

  .warning {
    padding: 0.5rem 0.75rem;
    border-radius: 6px;
    background: var(--danger-soft);
    color: var(--danger);
  }

  .link {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    cursor: pointer;
  }
</style>
