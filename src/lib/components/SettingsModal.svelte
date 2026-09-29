<script lang="ts">
  // Book settings (SET-030 … SET-070), stored in the book. Theme and font
  // size are on the menu bar and stored per computer.
  import { onMount } from "svelte";
  import { call, commands } from "../api";
  import { startupChoices } from "../shell/nav";
  import { bookSettings } from "../state/booksettings.svelte";
  import { dialogState } from "../state/dialogs.svelte";
  import { DATE_FORMATS, dateFormatState, type DateFormat } from "../state/dateformat.svelte";
  import { settingsState, type PanelSide } from "../state/settings.svelte";
  import type { BackupInfo, Settings } from "../types/bindings";
  import Modal from "./Modal.svelte";

  // Reads listsState.accounts, so a new account shows up at once.
  const choices = $derived(startupChoices());
  const s = $derived(bookSettings.value);

  let info = $state<BackupInfo | null>(null);
  let error = $state<string | null>(null);

  async function loadInfo() {
    try {
      info = await call(commands.backupInfo());
    } catch {
      info = null;
    }
  }
  onMount(loadInfo);

  async function save(patch: Partial<Settings>) {
    error = await bookSettings.update(patch);
    if (error) await bookSettings.load();
    if ("backup_folder" in patch) await loadInfo();
  }

  /** A whole number typed into a field, or no change. */
  function saveNumber(key: keyof Settings, e: Event) {
    const v = Number((e.currentTarget as HTMLInputElement).value);
    if (Number.isInteger(v)) void save({ [key]: v } as Partial<Settings>);
  }

  async function browse() {
    const picked = await commands.pickFolder(s.backup_folder ?? info?.folder ?? null);
    if (picked !== null) await save({ backup_folder: picked });
  }

  type Tool = "verify" | "passphrase" | "dbKey";
  let tool = $state<Tool>("verify");

  /** Close Settings and open the chosen backup tool's dialog. */
  function applyTool() {
    dialogState.settings = false;
    dialogState[tool] = true;
  }
</script>

<Modal title="Settings" onclose={() => (dialogState.settings = false)}>
  <div class="form">
    <label>
      <span>Date format</span>
      <select value={dateFormatState.value} onchange={(e) => dateFormatState.set(e.currentTarget.value as DateFormat)}>
        {#each DATE_FORMATS as f (f.value)}<option value={f.value}>{f.label}</option>{/each}
      </select>
    </label>
    <label>
      <span>First day of week</span>
      <select
        value={s.week_start}
        onchange={(e) => save({ week_start: e.currentTarget.value as Settings["week_start"] })}
      >
        <option value="sunday">Sunday</option>
        <option value="monday">Monday</option>
      </select>
    </label>
    <label>
      <span>On startup open to:</span>
      <select value={settingsState.startup} onchange={(e) => settingsState.setStartup(e.currentTarget.value)}>
        {#each choices as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
      </select>
    </label>
    <label>
      <span>Integrity check at startup</span>
      <input
        type="checkbox"
        checked={s.integrity_at_startup}
        onchange={(e) => save({ integrity_at_startup: e.currentTarget.checked })}
      />
    </label>
    <label>
      <span>Account list side</span>
      <select value={settingsState.accountPanelSide} onchange={(e) => settingsState.setAccountPanelSide(e.currentTarget.value as PanelSide)}>
        <option value="left">Left</option>
        <option value="right">Right</option>
      </select>
    </label>
    <label>
      <span>Price is stale after (days)</span>
      <input type="number" min="1" max="365" value={s.stale_price_days} onchange={(e) => saveNumber("stale_price_days", e)} />
    </label>
    <label>
      <span>Dashboard shows items due within (days)</span>
      <input type="number" min="1" max="366" value={s.upcoming_days} onchange={(e) => saveNumber("upcoming_days", e)} />
    </label>

    <h3>Backups</h3>
    <div class="folder">
      <span class="head">Backup folder</span>
      <span class="path">{s.backup_folder ?? "Downloads (default)"}</span>
      <div class="buttons">
        <button type="button" onclick={browse}>Browse…</button>
        {#if s.backup_folder}<button type="button" onclick={() => save({ backup_folder: null })}>Use Downloads</button>{/if}
      </div>
      {#if info?.folder_missing_now}
        <p role="alert"><strong>⚠ This folder is missing; backups go to Downloads until another is chosen.</strong></p>
      {/if}
    </div>
    <label>
      <span>Keep the newest automatic backups</span>
      <input type="number" min="1" max="1000" value={s.backup_keep_last} onchange={(e) => saveNumber("backup_keep_last", e)} />
    </label>
    <label>
      <span>… plus one per month for (months)</span>
      <input type="number" min="0" max="120" value={s.backup_keep_months} onchange={(e) => saveNumber("backup_keep_months", e)} />
    </label>
    <label>
      <span>Back up after a change (minutes, 0 = off)</span>
      <input type="number" min="0" max="1440" value={s.backup_timeout_minutes} onchange={(e) => saveNumber("backup_timeout_minutes", e)} />
    </label>
    {#if info}
      <p class="note">
        Last backup: {info.status.last_at ?? "none yet"}. Last full verification: {info.status.last_verified_at ?? "never"}.
        Manual backups are never deleted. A timed backup is temporary: only the newest is kept, until another kind of backup is made.
      </p>
    {/if}
    <span class="head">Backup tools</span>
    <div class="inline">
      <select aria-label="Backup tools" bind:value={tool}>
        <option value="verify">Verify backup…</option>
        <option value="passphrase">Change backup passphrase…</option>
        <option value="dbKey">Show database key…</option>
      </select>
      <button type="button" onclick={applyTool}>Apply</button>
    </div>
    {#if error}<p class="note" role="alert"><strong>{error}</strong></p>{/if}
    <div class="row">
      <button type="button" onclick={() => (dialogState.settings = false)}>Close</button>
    </div>
  </div>
</Modal>

<style>
  /* One line per setting: the label right-aligned on the left, its
     control on the right. Each label's text and control take the two
     columns (`display: contents`), so every control lines up. */
  .form {
    display: grid;
    grid-template-columns: fit-content(60%) minmax(0, 1fr);
    gap: 0.5rem 0.75rem;
    align-items: center;
  }
  label {
    display: contents;
  }
  label > span,
  .head {
    text-align: right;
  }
  /* A long choice (the startup list names every account) is cut to the
     dialog, never widens it. */
  label select,
  label input {
    justify-self: start;
    max-width: 100%;
  }
  .inline {
    display: flex;
    gap: 0.5rem;
    min-width: 0;
  }
  .inline select {
    min-width: 0;
    max-width: 100%;
  }
  /* The folder gets the full width: label, then the path, then buttons. */
  .folder {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    min-width: 0;
  }
  .folder .head {
    text-align: left;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .folder p {
    margin: 0;
  }
  input[type="number"] {
    width: 6em;
  }
  h3 {
    grid-column: 1 / -1;
    margin: 0.5rem 0 0;
    font-size: var(--fs-register);
  }
  .path {
    font-family: monospace;
    word-break: break-all;
  }
  /* Wraps to the width the settings take: `width: 0` keeps the note from
     widening the grid, `min-width: 100%` then fills it. */
  .note {
    grid-column: 1 / -1;
    width: 0;
    min-width: 100%;
    margin: 0;
    opacity: 0.8;
    font-size: var(--fs-register);
  }
  .row {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
</style>
