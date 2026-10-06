<script lang="ts">
  // Book settings (SET-030 … SET-070), stored in the book. Theme and font
  // size are on the menu bar and stored per computer.
  //
  // One category at a time in one card; edits are held until OK.
  import { onMount } from "svelte";
  import { call, commands } from "../api";
  import { LOT_METHODS } from "../invest/form";
  import { startupChoices } from "../shell/nav";
  import { bookSettings } from "../state/booksettings.svelte";
  import { dialogState } from "../state/dialogs.svelte";
  import { listsState } from "../state/lists.svelte";
  import { DATE_FORMATS } from "../state/dateformat.svelte";
  import type { BackupInfo, Settings } from "../types/bindings";
  import Modal from "./Modal.svelte";

  // Reads listsState.accounts, so a new account shows up at once.
  const choices = $derived(startupChoices());

  const CATEGORIES = [
    ["interface", "Interface"],
    ["data", "Data"],
    ["investments", "Investments"],
    ["register", "Register"],
    ["notifications", "Notifications"],
    ["backups", "Backups"],
  ] as const;
  type Category = (typeof CATEGORIES)[number][0];
  let category = $state<Category>("interface");

  // Edits are kept here until OK; Cancel drops them.
  let s = $state<Settings>({ ...bookSettings.value });

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

  const NUMBERS: [keyof Settings, string][] = [
    ["stale_price_days", "Price is stale after"],
    ["upcoming_days", "Due soon items within"],
    ["spending_rows", "Rows shown on spending cards"],
    ["backup_keep_last", "Backups to keep"],
    ["backup_keep_months", "Months of monthly backups"],
    ["backup_timeout_minutes", "Minutes before a timed backup"],
    ["purge_payees_months", "Months before a payee is removed"],
  ];

  /** Store every edit; the dialog closes only when that worked. */
  async function ok() {
    const bad = NUMBERS.find(([k]) => !Number.isInteger(s[k]));
    if (bad) {
      error = `${bad[1]} must be a whole number.`;
      return;
    }
    const centsChanged = s.account_bar_cents !== bookSettings.value.account_bar_cents;
    error = await bookSettings.update(s);
    if (error) {
      await bookSettings.load();
      return;
    }
    // Rust rounds the account bar's figures; fetch them again.
    if (centsChanged) void listsState.loadBalances();
    dialogState.settings = false;
  }

  async function browse() {
    const picked = await commands.pickFolder(s.backup_folder ?? info?.folder ?? null);
    if (picked !== null) s.backup_folder = picked;
  }

  type Tool = "verify" | "passphrase" | "dbKey";
  let tool = $state<Tool>("verify");

  /** Close Settings and open the chosen backup tool's dialog. */
  function applyTool() {
    dialogState.settings = false;
    dialogState[tool] = true;
  }
</script>

<Modal title="Settings" wide onclose={() => (dialogState.settings = false)}>
  <select class="category" aria-label="Settings category" bind:value={category}>
    {#each CATEGORIES as [value, label] (value)}<option {value}>{label}</option>{/each}
  </select>
  <article class="card sheet" aria-labelledby="set-head">
    <header><h2 id="set-head">{CATEGORIES.find(([v]) => v === category)?.[1]}</h2></header>
    <div class="body form">
      {#if category === "interface"}
        <label>
          <span>Date format</span>
          <select bind:value={s.date_format}>
            {#each DATE_FORMATS as f (f.value)}<option value={f.value}>{f.label}</option>{/each}
          </select>
        </label>
        <label>
          <span>First day of week</span>
          <select bind:value={s.week_start}>
            <option value="sunday">Sunday</option>
            <option value="monday">Monday</option>
          </select>
        </label>
        <label>
          <span>On startup open to:</span>
          <select bind:value={s.startup}>
            {#each choices as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
          </select>
        </label>
        <label>
          <span>Account list side</span>
          <select bind:value={s.account_panel_side}>
            <option value="left">Left</option>
            <option value="right">Right</option>
          </select>
        </label>
        <label>
          <span>Show cents in Account Bar balances</span>
          <input type="checkbox" bind:checked={s.account_bar_cents} />
        </label>
        <label>
          <span>Rows shown on spending cards (more rows scroll)</span>
          <input type="number" min="3" max="50" bind:value={s.spending_rows} />
        </label>
      {:else if category === "data"}
        <label>
          <span>Integrity check at startup</span>
          <input type="checkbox" bind:checked={s.integrity_at_startup} />
        </label>
        <label>
          <span>Due soon shows items due within (days)</span>
          <input type="number" min="1" max="366" bind:value={s.upcoming_days} />
        </label>
      {:else if category === "investments"}
        <label>
          <span>Price is stale after (days)</span>
          <input type="number" min="1" max="365" bind:value={s.stale_price_days} />
        </label>
        <label>
          <span>Lot method for new investment accounts</span>
          <select bind:value={s.default_lot_method}>
            {#each LOT_METHODS as [m, label] (m)}<option value={m}>{label}</option>{/each}
          </select>
        </label>
        <label>
          <span>Allow price download (internet)</span>
          <input type="checkbox" bind:checked={s.price_download} />
        </label>
      {:else if category === "register"}
        <label>
          <span>Gray reconciled transactions</span>
          <input type="checkbox" bind:checked={s.gray_reconciled} />
        </label>
        <label>
          <span>Recall memorized payees</span>
          <input type="checkbox" bind:checked={s.recall_payees} />
        </label>
        <label>
          <span>Capitalize payees and categories</span>
          <input type="checkbox" bind:checked={s.capitalize_names} />
        </label>
        <label>
          <span>Automatically memorize new payees</span>
          <input type="checkbox" bind:checked={s.auto_memorize_payees} />
        </label>
        <label>
          <span>Remove memorized payees not used in last (months, 0 = never)</span>
          <input type="number" min="0" max="120" bind:value={s.purge_payees_months} />
        </label>
      {:else if category === "notifications"}
        <label>
          <span>When entering out-of-date transactions (past, or over a year ahead)</span>
          <input type="checkbox" bind:checked={s.warn_out_of_date} />
        </label>
        <label>
          <span>A check number is reused</span>
          <input type="checkbox" bind:checked={s.warn_check_reuse} />
        </label>
        <label>
          <span>Ask before saving changes to an existing transaction</span>
          <input type="checkbox" bind:checked={s.confirm_save_change} />
        </label>
        <label>
          <span>When entering uncategorized transactions (split lines included)</span>
          <input type="checkbox" bind:checked={s.warn_uncategorized} />
        </label>
      {:else}
        <div class="folder">
          <span class="head">Backup folder</span>
          <span class="path">{s.backup_folder ?? "Downloads (default)"}</span>
          <div class="buttons">
            <button type="button" onclick={browse}>Browse…</button>
            {#if s.backup_folder}<button type="button" onclick={() => (s.backup_folder = null)}>Use Downloads</button>{/if}
          </div>
          {#if info?.folder_missing_now}
            <p role="alert"><strong>⚠ This folder is missing; backups go to Downloads until another is chosen.</strong></p>
          {/if}
        </div>
        <label>
          <span>Keep the newest automatic backups</span>
          <input type="number" min="1" max="1000" bind:value={s.backup_keep_last} />
        </label>
        <label>
          <span>… plus one per month for (months)</span>
          <input type="number" min="0" max="120" bind:value={s.backup_keep_months} />
        </label>
        <label>
          <span>Back up after a change (minutes, 0 = off)</span>
          <input type="number" min="0" max="1440" bind:value={s.backup_timeout_minutes} />
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
      {/if}
    </div>
  </article>
  {#if error}<p class="note err" role="alert"><strong>{error}</strong></p>{/if}
  <div class="row">
    <button type="button" onclick={ok}>OK</button>
    <button type="button" onclick={() => (dialogState.settings = false)}>Cancel</button>
  </div>
</Modal>

<style>
  .category {
    margin-bottom: 0.5rem;
  }
  /* One line per setting: the label right-aligned on the left, its
     control on the right. Each label's text and control take the two
     columns (`display: contents`), so every control lines up. Every
     category's card is as tall as the fullest one (Backups), so the
     dialog does not jump when the category changes. */
  .form {
    display: grid;
    grid-template-columns: fit-content(60%) minmax(0, 1fr);
    gap: 0.5rem 0.75rem;
    align-items: center;
    align-content: start;
    min-height: 19rem;
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
  .note.err {
    width: auto;
    margin-top: 0.5rem;
    opacity: 1;
    color: var(--bad);
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.75rem;
  }
</style>
