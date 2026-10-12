<script lang="ts">
  // File > New… (UI-conventions "Books"): a named book in a chosen folder,
  // with its own backup folder and passphrase (SECU-030), starting empty,
  // with the Standard categories, or as a copy of the open book (UI-085):
  // balances zeroed, carried to a date, or carried with the transactions
  // after it kept (a year-end copy). The carry date's line sits under the
  // Carry choice made.
  // The current book is closed (backed up) only once the new one exists.
  import { call, commands } from "../../api";
  import { completeDate, datePattern, displayDate, parseDate } from "../../format/date";
  import { bookState } from "../../state/book.svelte";
  import { listsState } from "../../state/lists.svelte";
  import type { NewBookContents } from "../../types/bindings";
  import { windowState } from "../../state/windows.svelte";
  import { reloadForBook } from "../../shell/books";
  import Modal from "../Modal.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let name = $state("");
  let folder = $state<string | null>(bookState.status?.folder ?? null);
  let backups = $state<string | null>(null);
  let passphrase = $state("");
  let again = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let start = $state<"empty" | "standard" | "copy">("empty");
  let balances = $state<"zero" | "carry" | "keep">("zero");
  // A carry usually closes a year: the last day of the previous one.
  const yearEnd = `${Number(listsState.today.slice(0, 4)) - 1}-12-31`;
  let carryText = $state(displayDate(yearEnd));
  let keepClosed = $state(true);

  const carryDate = $derived(parseDate(carryText, listsState.today));
  const carryBad = $derived(start === "copy" && balances !== "zero" && carryDate === null);
  const canCopy = $derived(bookState.status?.open ?? false);

  function contents(): NewBookContents {
    if (start !== "copy") return { kind: start };
    return {
      kind: "copy",
      balances:
        balances === "zero" || carryDate === null
          ? { kind: "zero" }
          : { kind: balances === "keep" ? "carry_keep" : "carry", as_of: carryDate },
      closed_accounts: keepClosed,
    };
  }

  /** Tooltips for the Start with and Balances choices. Each ends with
   * when to choose it. */
  const TIPS = {
    empty: "A new book with only the built-in categories. Choose this to import a Quicken file or to start from nothing.",
    standard:
      "A new book with the built-in categories and the Standard list (about 90 everyday categories, with tax lines set). Choose this to start fresh and enter your accounts by hand, with a ready-made category list that you can update.",
    copy:
      "A new book with this book's accounts, categories, payees, tags, securities and prices, schedules, saved reports, insights, and settings. Choose below what happens to balances and transactions. Choose this to start a new book for a new year, keeping this one as the record of past years, or to make a practice book with the same setup.",
    zero: "Every account opens at 0. No transactions, holdings, or lots are copied. Choose this for a practice book, or a fresh start that reuses this book's setup but none of its history.",
    carry:
      `Each account opens with its balance at the end of the carry date, entered reconciled. Investment accounts also get each lot open on that date, with its shares, cost basis, and original acquired date. No other transactions are copied. Choose this at year end to start the new year's book from balances alone, keeping this book as the record of past years; for the carry date, enter the last day of the year being closed, such as ${displayDate(yearEnd)}.`,
    date: "The date whose ending balances and open lots carry to the new book. Usually the last day of the year being closed.",
    keep: `As Carry, and every transaction dated after the carry date is copied too, so balances, holdings, and gains match this book from that date on (a year-end copy). Choose this when the new year has already begun: enter the last day of the previous year, such as ${displayDate(yearEnd)}, as the carry date; balances carry from that day, and everything entered since comes along.`,
    closed:
      "Copy closed accounts too. When off, closed accounts are dropped with the schedules that use them; one with money or shares on the carry date, or that a kept transaction is in, is copied anyway. Turn this off to leave long-closed accounts behind.",
  };

  const mismatch = $derived(again !== "" && passphrase !== again);
  const nameOk = $derived(/^[A-Za-z0-9][A-Za-z0-9_-]{0,39}$/.test(name.trim()));

  async function browse() {
    const picked = await commands.pickFolder(folder);
    if (picked !== null) folder = picked;
  }

  async function browseBackups() {
    const picked = await commands.pickFolder(backups ?? bookState.status?.downloads ?? null);
    if (picked !== null) backups = picked;
  }

  async function create(e: Event) {
    e.preventDefault();
    if (!nameOk || folder === null || mismatch || passphrase.trim() === "" || carryBad) return;
    if (!(await windowState.mayQuit())) return;
    busy = true;
    error = null;
    try {
      await call(commands.bookNew(folder, name.trim(), passphrase, backups, contents()));
      passphrase = again = "";
      reloadForBook();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

{#snippet dateLine()}
  <span class="row sub">
    Carry date:
    <input
      class="date"
      aria-label="Carry date"
      title={TIPS.date}
      bind:value={carryText}
      placeholder={datePattern()}
      onblur={() => (carryText = completeDate(carryText, listsState.today))}
    />
  </span>
{/snippet}

<Modal title="New book" {onclose}>
  <form onsubmit={create}>
    <label>
      <span>Name</span>
      <input bind:value={name} placeholder="barton2026" autocomplete="off" />
    </label>
    <p class="msg note">Letters, digits, - and _. It names the book's files and its backups.</p>
    <span class="lbl">Folder</span>
    <span class="row">
      <span class="path">{folder ?? "—"}</span>
      <button type="button" onclick={browse}>Browse…</button>
    </span>
    <span class="lbl">Start with</span>
    <span class="choices" role="radiogroup" aria-label="Start with">
      <label class="opt" title={TIPS.empty}>
        <input type="radio" bind:group={start} value="empty" /> Empty (built-in categories only, for an import)
      </label>
      <label class="opt" title={TIPS.standard}>
        <input type="radio" bind:group={start} value="standard" /> Standard categories
      </label>
      {#if canCopy}
        <label class="opt" title={TIPS.copy}><input type="radio" bind:group={start} value="copy" /> Copy of this book</label>
      {/if}
    </span>
    {#if start === "copy"}
      <span class="lbl">Balances</span>
      <span class="choices" role="radiogroup" aria-label="Balances">
        <label class="opt" title={TIPS.zero}>
          <input type="radio" bind:group={balances} value="zero" /> Zero: every account opens at 0
        </label>
        <label class="opt" title={TIPS.carry}>
          <input type="radio" bind:group={balances} value="carry" /> Carry each balance
        </label>
        {#if balances === "carry"}{@render dateLine()}{/if}
        <label class="opt" title={TIPS.keep}>
          <input type="radio" bind:group={balances} value="keep" /> Carry, and keep transactions after the carry date
        </label>
        {#if balances === "keep"}{@render dateLine()}{/if}
      </span>
      {#if carryBad}<p class="msg" role="alert"><strong>Enter a valid date.</strong></p>{/if}
      <span class="lbl"></span>
      <label class="opt" title={TIPS.closed}>
        <input type="checkbox" bind:checked={keepClosed} /> Copy closed accounts
      </label>
      <p class="msg note">
        Accounts, categories, payees, tags, securities and prices, schedules, saved reports, insights, and settings are copied.
        {#if balances === "zero"}No transactions are copied.{:else if balances === "carry"}A carried balance is entered
          reconciled; investment accounts carry their cash and each open lot, with its acquired date and basis. No
          transactions are copied.{:else}Balances and lots carry as with Carry; every transaction after the date is
          copied too.{/if}
        {#if !keepClosed}Schedules that use a dropped account are dropped too.{/if}
      </p>
    {/if}
    <span class="lbl">Backup folder</span>
    <span class="row">
      <span class="path">{backups ?? `${bookState.status?.downloads ?? "Downloads"} (default)`}</span>
      <button type="button" onclick={browseBackups}>Browse…</button>
      {#if backups}<button type="button" onclick={() => (backups = null)}>Use Downloads</button>{/if}
    </span>
    <label><span>Passphrase</span><input type="password" bind:value={passphrase} autocomplete="new-password" /></label>
    <label><span>Again</span><input type="password" bind:value={again} autocomplete="new-password" /></label>
    {#if mismatch}<p class="msg" role="alert"><strong>The passphrases do not match.</strong></p>{/if}
    <p class="msg">
      <strong>⚠ This passphrase opens the new book and every backup of it. If it is lost, they cannot be recovered.</strong>
      Record it durably now, for example in a password manager.
    </p>
    <p class="msg note">The open book is closed (and backed up) when the new one is ready.</p>
    {#if error}<p class="msg" role="alert"><strong>{error}</strong></p>{/if}
    <div class="buttons">
      <button
        type="submit"
        disabled={busy || !nameOk || folder === null || passphrase.trim() === "" || passphrase !== again || carryBad}
      >
        {busy ? "Creating…" : "Create book"}
      </button>
      <button type="button" onclick={onclose}>Cancel</button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 0.5rem 0.75rem;
    align-items: center;
  }
  label {
    display: contents;
  }
  label span,
  .lbl {
    text-align: right;
  }
  .row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  label.opt {
    display: flex;
    gap: 0.4rem;
    align-items: center;
  }
  .date {
    width: 8rem;
  }
  .sub {
    padding-left: 1.6rem;
  }
  .path {
    font-family: monospace;
    word-break: break-all;
  }
  .msg,
  .buttons {
    grid-column: 1 / -1;
    margin: 0;
  }
  .note {
    opacity: 0.8;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>
