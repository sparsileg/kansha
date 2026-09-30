<script lang="ts">
  // File > Rename Book…: renames the book's two files; backups made from
  // now on carry the new name. Earlier backups keep the old one and are no
  // longer pruned. The book is closed and opened again, hence the
  // passphrase.
  import { call, commands } from "../../api";
  import { bookState } from "../../state/book.svelte";
  import { windowState } from "../../state/windows.svelte";
  import { reloadForBook } from "../../shell/books";
  import Modal from "../Modal.svelte";

  let { onclose }: { onclose: () => void } = $props();

  const current = bookState.status?.name ?? "";
  let name = $state(current);
  let passphrase = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  const nameOk = $derived(/^[A-Za-z0-9][A-Za-z0-9_-]{0,39}$/.test(name.trim()) && name.trim() !== current);

  async function rename(e: Event) {
    e.preventDefault();
    if (!nameOk || passphrase === "") return;
    if (!(await windowState.mayQuit())) return;
    busy = true;
    error = null;
    try {
      await call(commands.bookRename(name.trim(), passphrase));
      reloadForBook();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      passphrase = "";
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Rename book" {onclose}>
  <form onsubmit={rename}>
    <label><span>New name</span><input bind:value={name} autocomplete="off" /></label>
    <p class="msg note">
      Letters, digits, - and _. Renames {current}.db and {current}.key. New backups are named with it; backups
      already made keep the name {current} and are no longer deleted automatically.
    </p>
    <label><span>Passphrase</span><input type="password" bind:value={passphrase} autocomplete="off" /></label>
    {#if error}<p class="msg" role="alert"><strong>{error}</strong></p>{/if}
    <div class="buttons">
      <button type="submit" disabled={busy || !nameOk || passphrase === ""}>{busy ? "Renaming…" : "Rename"}</button>
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
  label span {
    text-align: right;
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
