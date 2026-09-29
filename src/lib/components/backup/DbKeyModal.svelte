<script lang="ts">
  // Show database key (SECU-020): after the passphrase, the SQLCipher key,
  // which opens the book in DB Browser for SQLite (SQLCipher build) as a
  // raw key. Read-only browsing only (SECU-050).
  import { call, commands } from "../../api";
  import Modal from "../Modal.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let passphrase = $state("");
  let key = $state<string | null>(null);
  let error = $state<string | null>(null);

  async function show(e: Event) {
    e.preventDefault();
    error = null;
    try {
      key = await call(commands.databaseKeyShow(passphrase));
      passphrase = "";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<Modal title="Database key" {onclose}>
  {#if key}
    <p>In DB Browser for SQLite (SQLCipher), choose <em>Raw key</em> and enter:</p>
    <p><input class="key" readonly value={key} aria-label="Database key" /></p>
    <p class="note">
      Look, do not change: edits made outside Kansha are not supported, and the integrity check reports what they
      break.
    </p>
  {:else}
    <form onsubmit={show}>
      <label>
        Backup passphrase
        <input type="password" bind:value={passphrase} autocomplete="off" />
      </label>
      <button type="submit" disabled={passphrase === ""}>Show key</button>
    </form>
    {#if error}<p role="alert"><strong>{error}</strong></p>{/if}
  {/if}
  <div class="buttons"><button type="button" onclick={onclose}>Close</button></div>
</Modal>

<style>
  .key {
    font-family: monospace;
    width: 100%;
  }
  form {
    display: flex;
    gap: 0.5rem;
    align-items: end;
  }
  form label {
    display: flex;
    flex-direction: column;
  }
  .note {
    opacity: 0.8;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
  }
</style>
