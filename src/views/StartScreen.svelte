<script lang="ts">
  // Shown until a book is open: the backup passphrase (SECU-020), first-run
  // setup (SECU-080) including conversion of the unencrypted prototype
  // database, or, with the key file gone, restore only (SECU-090).
  import { call, commands } from "../lib/api";
  import RestoreModal from "../lib/components/backup/RestoreModal.svelte";
  import ThemePicker from "../lib/components/shell/ThemePicker.svelte";
  import { bookState } from "../lib/state/book.svelte";
  import { openBookFile, switchBook } from "../lib/shell/books";
  import type { RecentBook } from "../lib/types/bindings";

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

  const message = (e: unknown) => (e instanceof Error ? e.message : String(e));
  const mismatch = $derived(again !== "" && passphrase !== again);

  async function unlock(e: Event) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      await call(commands.bookUnlock(passphrase));
      passphrase = "";
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
  <main>
    <h1>Kansha</h1>
    {#if bookState.error}<p role="alert"><strong>{bookState.error}</strong></p>{/if}
    {#if status?.state === "locked" && !creating}
      <p class="book">Book: <strong>{status.name}</strong> <span class="path note">{status.db_path}</span></p>
      <form class="unlock" onsubmit={unlock}>
        <label>
          Backup passphrase
          <!-- svelte-ignore a11y_autofocus -->
          <input type="password" bind:value={passphrase} autocomplete="off" autofocus />
        </label>
        <button type="submit" disabled={busy || passphrase === ""}>{busy ? "Opening…" : "Open"}</button>
      </form>
      {#if error}<p role="alert"><strong>{error}</strong></p>{/if}
      <p><button type="button" class="link" onclick={() => (restoring = true)}>Restore from a backup…</button></p>
      <p>
        <button type="button" class="link" onclick={() => (creating = true)}>Create a new book…</button>
        · <button type="button" class="link" onclick={() => void other(null)}>Open another book…</button>
        {#each recent as b (b.path)}
          · <button type="button" class="link" title={b.path} onclick={() => void other(b.path)}>{b.name}</button>
        {/each}
      </p>
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
    margin: 2rem auto;
  }
  h1 {
    margin-top: 0;
  }
  .unlock {
    display: flex;
    gap: 0.5rem;
    align-items: end;
  }
  .unlock label {
    display: flex;
    flex-direction: column;
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
