<script lang="ts">
  // Shown until a book is open: the backup passphrase (SECU-020), first-run
  // setup (SECU-080) including conversion of the unencrypted prototype
  // database, or, with the key file gone, restore only (SECU-090).
  import { call, commands } from "../lib/api";
  import RestoreModal from "../lib/components/backup/RestoreModal.svelte";
  import ThemePicker from "../lib/components/shell/ThemePicker.svelte";
  import { verifyLastBackup } from "../lib/state/attention.svelte";
  import { bookState } from "../lib/state/book.svelte";
  import { openBookFile, switchBook } from "../lib/shell/books";
  import type { RecentBook } from "../lib/types/bindings";
  import { compactWindow, fullWindow } from "../lib/shell/windowsize";
  import mark from "../assets/kansha-mark.png";

  const status = $derived(bookState.status);

  // Other books on this computer, to switch to instead.
  let recent = $state<RecentBook[]>([]);
  $effect(() => {
    void commands.bookRecent().then((r) => (recent = r.filter((b) => b.exists && !b.current)));
  });
  let bookName = $state<string | null>(null);
  let bookFolder = $state<string | null>(null);
  /** "Create a new book…" from the passphrase or missing-key screen: the
   * first-run form, for a book beside the one that is there. */
  let creating = $state(false);
  /** The setup form makes a new book (first run, or `creating`). */
  const fresh = $derived(status?.state === "new" || creating);
  // A new book beside an existing one starts with no name, so it cannot
  // take the existing book's.
  const name = $derived(bookName ?? (creating ? "" : (status?.name ?? "kansha")));
  const nameOk = $derived(/^[A-Za-z0-9][A-Za-z0-9_-]{0,39}$/.test(name.trim()));

  async function browseBook() {
    const picked = await commands.pickFolder(bookFolder ?? status?.folder ?? null);
    if (picked !== null) bookFolder = picked;
  }

  async function other(path: string | null) {
    error = null;
    try {
      if (path === null) await openBookFile(status?.folder ?? null);
      else await switchBook(path);
    } catch (err) {
      error = message(err);
    }
  }

  let passphrase = $state("");
  let again = $state("");
  let folder = $state<string | null>(null);
  let choice = $state<"create" | "restore">("create");
  let restoring = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  /** Only the passphrase to ask: a small window, the mark centred above. */
  const compact = $derived(status?.state === "locked" && !creating && !restoring);
  $effect(() => {
    if (!status) return;
    void (compact ? compactWindow() : fullWindow());
  });

  const message = (e: unknown) => (e instanceof Error ? e.message : String(e));
  const mismatch = $derived(again !== "" && passphrase !== again);

  async function unlock(e: Event) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      const typed = passphrase;
      await call(commands.bookUnlock(typed));
      passphrase = "";
      // In the background; the book is usable meanwhile (BAK-080).
      void verifyLastBackup(typed);
      await bookState.refresh();
    } catch (err) {
      error = message(err);
      passphrase = "";
    } finally {
      busy = false;
    }
  }

  async function browse() {
    const picked = await commands.pickFolder(folder ?? status?.downloads ?? null);
    if (picked !== null) folder = picked;
  }

  async function setup(e: Event) {
    e.preventDefault();
    if (mismatch || passphrase.trim() === "") return;
    busy = true;
    error = null;
    try {
      await call(commands.bookSetup(passphrase, folder, name.trim(), bookFolder));
      passphrase = again = "";
      creating = false;
      await bookState.refresh();
    } catch (err) {
      error = message(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="start">
  <div class="top"><ThemePicker /></div>
  <main class:compact>
    <header class="brand">
      <span class="mark" style:--mark="url({mark})" role="img" aria-label="感謝, kansha, in brush calligraphy"></span>
      <div>
        <h1>Kansha</h1>
        <p class="meaning">Gratitude</p>
      </div>
    </header>
    {#if bookState.error}<p role="alert"><strong>{bookState.error}</strong></p>{/if}
    {#if status?.state === "locked" && !creating}
      <p class="book"><strong>{status.name}</strong><span class="path note" title={status.db_path}>{status.db_path}</span></p>
      <form class="unlock" onsubmit={unlock}>
        <label>
          Backup passphrase
          <!-- svelte-ignore a11y_autofocus -->
          <input type="password" bind:value={passphrase} autocomplete="off" autofocus />
        </label>
        <button type="submit" disabled={busy || passphrase === ""}>{busy ? "Opening…" : "Open"}</button>
      </form>
      {#if error}<p role="alert"><strong>{error}</strong></p>{/if}
      <nav class="others">
        {#if recent.length > 0}
          <p>
            Open:
            {#each recent as b, i (b.path)}
              {i > 0 ? " · " : ""}<button type="button" class="link" title={b.path} onclick={() => void other(b.path)}>{b.name}</button>
            {/each}
          </p>
        {/if}
        <p>
          <button type="button" class="link" onclick={() => void other(null)}>Open another book…</button>
          · <button type="button" class="link" onclick={() => (creating = true)}>Create a new book…</button>
        </p>
        <p><button type="button" class="link" onclick={() => (restoring = true)}>Restore from a backup…</button></p>
      </nav>
    {:else if status?.state === "key_missing" && !creating}
      <p role="alert">
        <strong>The key file is missing or damaged, so this book cannot be opened.</strong> It cannot be repaired:
        restore the most recent backup. Changes made since that backup are lost.
      </p>
      <p><button type="button" onclick={() => (restoring = true)}>Restore from a backup…</button></p>
      <p>
        <button type="button" class="link" onclick={() => (creating = true)}>Create a new book…</button>
        · <button type="button" class="link" onclick={() => void other(null)}>Open another book…</button>
      </p>
      <p class="note">Book: <span class="path">{status.db_path}</span></p>
    {:else if status}
      <form class="setup" onsubmit={setup}>
        <h2>Set up</h2>
        <fieldset>
          <legend>1. Book</legend>
          <label class="opt">
            <input type="radio" bind:group={choice} value="create" />
            {status.state === "unencrypted" && !creating
              ? "Keep the existing book; it will be encrypted"
              : "Create a new, empty book"}
          </label>
          <label class="opt">
            <input type="radio" bind:group={choice} value="restore" />
            Restore from a backup
          </label>
          {#if fresh}
            <p class="opt">
              <button type="button" class="link" onclick={() => void other(null)}>Open an existing book…</button>
            </p>
          {/if}
        </fieldset>
        {#if choice === "restore"}
          <p><button type="button" onclick={() => (restoring = true)}>Choose backup…</button></p>
        {:else}
          {#if fresh}
            <div class="grid">
              <label>
                <span>Name</span>
                <input value={name} oninput={(e) => (bookName = e.currentTarget.value)} autocomplete="off" />
              </label>
              <span class="lbl">Folder</span>
              <span class="row">
                <span class="path">{bookFolder ?? status.folder}</span>
                <button type="button" onclick={browseBook}>Browse…</button>
              </span>
            </div>
            <p class="note">Letters, digits, - and _. The name is the book's file name and names its backups.</p>
            {#if recent.length > 0}
              <p>
                Or open:
                {#each recent as b, i (b.path)}
                  {i > 0 ? " · " : ""}<button type="button" class="link" title={b.path} onclick={() => void other(b.path)}>{b.name}</button>
                {/each}
              </p>
            {/if}
          {:else}
            <p class="note">Book: <strong>{status.name}</strong> <span class="path">{status.db_path}</span></p>
          {/if}
          <fieldset>
            <legend>2. Backup folder</legend>
            <p class="row">
              <span class="path">{folder ?? `${status.downloads ?? "Downloads"} (default)`}</span>
              <button type="button" onclick={browse}>Browse…</button>
              {#if folder}<button type="button" onclick={() => (folder = null)}>Use Downloads</button>{/if}
            </p>
            <p class="note">
              Backups are made on closing, a few minutes after a change, and when you ask. A folder on this computer, Downloads included, is lost
              with the computer; a cloud-synced folder, network drive, or USB drive is not.
            </p>
          </fieldset>
          <fieldset>
            <legend>3. Backup passphrase</legend>
            <div class="grid">
              <label>
                <span>Passphrase</span>
                <input type="password" bind:value={passphrase} autocomplete="new-password" />
              </label>
              <label>
                <span>Again</span>
                <input type="password" bind:value={again} autocomplete="new-password" />
              </label>
            </div>
            {#if mismatch}<p role="alert"><strong>The passphrases do not match.</strong></p>{/if}
            <p class="warning">
              <strong>⚠ This passphrase opens the book and every backup. If it is lost, they cannot be recovered.</strong>
              Record it durably now, for example in a password manager. It is typed each time Kansha starts.
            </p>
          </fieldset>
          {#if error}<p role="alert"><strong>{error}</strong></p>{/if}
          <button type="submit" disabled={busy || !nameOk || passphrase.trim() === "" || passphrase !== again}>
            {busy ? "Setting up…" : status.state === "unencrypted" && !creating ? "Encrypt book" : "Create book"}
          </button>
          {#if creating}
            <button type="button" onclick={() => { creating = false; error = null; }}>Cancel</button>
          {/if}
        {/if}
      </form>
    {/if}
  </main>
</div>

{#if restoring}
  <RestoreModal
    onclose={() => (restoring = false)}
    ondone={() => {
      restoring = false;
      void bookState.refresh();
    }}
  />
{/if}

<style>
  .start {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
  .top {
    display: flex;
    justify-content: flex-end;
    padding: 0.25rem 0.5rem;
  }
  main {
    width: min(40rem, 94vw);
    margin: 1rem auto 2rem;
  }
  /* The mark is drawn in the text colour (a mask over the ink), so it
     reads on every theme. */
  .mark {
    display: block;
    flex: none;
    width: 4.7rem;
    aspect-ratio: 390 / 866;
    /* The image, set on the element. */
    --mark: none;
    background: var(--fg);
    -webkit-mask: var(--mark) center / contain no-repeat;
    mask: var(--mark) center / contain no-repeat;
  }
  .brand {
    display: flex;
    gap: 1rem;
    align-items: center;
    margin-bottom: 1.5rem;
  }
  h1 {
    margin: 0;
    font-size: var(--fs-title);
    letter-spacing: 0.04em;
  }
  .meaning {
    margin: 0.25rem 0 0;
    opacity: 0.8;
  }
  .compact {
    width: min(24rem, 92vw);
    /* About two lines below the theme and font buttons. */
    margin-top: 2.5rem;
    text-align: center;
  }
  .compact .brand {
    flex-direction: column;
    gap: 0.75rem;
  }
  .compact .mark {
    width: 10.2rem;
  }
  .compact .brand > div {
    padding-bottom: 1rem;
    border-bottom: 1px solid var(--line-soft);
    width: 100%;
  }
  .book {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .book .path {
    font-size: var(--fs-small);
  }
  .compact .book .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    word-break: normal;
  }
  .unlock {
    display: flex;
    gap: 0.5rem;
    align-items: end;
  }
  .unlock label {
    display: flex;
    flex-direction: column;
    flex: 1;
    text-align: left;
  }
  .others {
    margin-top: 1.5rem;
    font-size: var(--fs-small);
    opacity: 0.9;
  }
  .others p {
    margin: 0.35rem 0;
  }
  fieldset {
    margin: 0 0 1rem;
    border: 1px solid var(--line-soft);
    border-radius: 4px;
  }
  legend {
    font-weight: bold;
  }
  .opt {
    display: block;
  }
  .row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .path {
    font-family: monospace;
    word-break: break-all;
  }
  .grid {
    display: grid;
    grid-template-columns: max-content auto;
    gap: 0.5rem 0.75rem;
    align-items: center;
  }
  .grid label {
    display: contents;
  }
  .grid span,
  .grid .lbl {
    text-align: right;
  }
  .grid .row {
    text-align: left;
  }
  .note {
    opacity: 0.8;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    text-decoration: underline;
    cursor: pointer;
    color: inherit;
  }
</style>
