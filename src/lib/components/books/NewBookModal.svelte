<script lang="ts">
  // File > New… (UI-conventions "Books"): a named book in a chosen folder,
  // with its own backup folder and passphrase (SECU-030). The current book
  // is closed (backed up) only once the new one exists.
  import { call, commands } from "../../api";
  import { bookState } from "../../state/book.svelte";
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
    if (!nameOk || folder === null || mismatch || passphrase.trim() === "") return;
    if (!(await windowState.mayQuit())) return;
    busy = true;
    error = null;
    try {
      await call(commands.bookNew(folder, name.trim(), passphrase, backups));
      passphrase = again = "";
      reloadForBook();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

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
        disabled={busy || !nameOk || folder === null || passphrase.trim() === "" || passphrase !== again}
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
