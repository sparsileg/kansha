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

  function open(which: "verify" | "passphrase" | "dbKey") {
    dialogState.settings = false;
    dialogState[which] = true;
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
    <div class="field">
      <span>Backup folder</span>
      <div>
        <span class="path">{s.backup_folder ?? "Downloads (default)"}</span>
        <button type="button" onclick={browse}>Browse…</button>
        {#if s.backup_folder}<button type="button" onclick={() => save({ backup_folder: null })}>Use Downloads</button>{/if}
        {#if info?.folder_missing_now}
          <p role="alert"><strong>⚠ This folder is missing; backups go to Downloads until another is chosen.</strong></p>
        {/if}
      </div>
    </div>
    <p class="note">
      A folder on this computer, Downloads included, is lost with the computer; a cloud-synced folder, network
      drive, or USB drive is not.
    </p>
    <label>
      <span>Keep the newest automatic backups</span>
      <input type="number" min="1" max="1000" value={s.backup_keep_last} onchange={(e) => saveNumber("backup_keep_last", e)} />
    </label>
    <label>
      <span>… plus one per month for (months)</span>
      <input type="number" min="0" max="120" value={s.backup_keep_months} onchange={(e) => saveNumber("backup_keep_months", e)} />
    </label>
    {#if info}
      <p class="note">
        Last backup: {info.status.last_at ?? "none yet"}. Last full verification: {info.status.last_verified_at ?? "never"}.
        Manual backups are never deleted.
      </p>
    {/if}
    <div class="row left">
      <button type="button" onclick={() => open("verify")}>Verify backup…</button>
      <button type="button" onclick={() => open("passphrase")}>Change backup passphrase…</button>
      <button type="button" onclick={() => open("dbKey")}>Show database key…</button>
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
    grid-template-columns: max-content auto;
    gap: 0.5rem 0.75rem;
    align-items: center;
  }
  label,
  .field {
    display: contents;
  }
  label > span,
  .field > span {
    text-align: right;
  }
  label select,
  label input {
    justify-self: start;
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
    margin-right: 0.5rem;
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
  .row.left {
    justify-content: flex-start;
  }
</style>
