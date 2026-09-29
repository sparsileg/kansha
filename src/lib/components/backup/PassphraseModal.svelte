<script lang="ts">
  // Change backup passphrase (SECU-040). Old backups keep the passphrase
  // they were made with.
  import { call, commands } from "../../api";
  import Modal from "../Modal.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let old = $state("");
  let next = $state("");
  let again = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let done = $state(false);

  const mismatch = $derived(again !== "" && next !== again);

  async function change(e: Event) {
    e.preventDefault();
    if (mismatch || next.trim() === "") return;
    busy = true;
    error = null;
    try {
      await call(commands.passphraseChange(old, next));
      done = true;
      old = next = again = "";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Change backup passphrase" {onclose}>
  {#if done}
    <p role="status">
      <strong>✓ Passphrase changed.</strong> Backups made from now on use the new one; older backups still need the
      passphrase they were made with. Record the new passphrase somewhere safe.
    </p>
    <div class="buttons"><button type="button" onclick={onclose}>Close</button></div>
  {:else}
    <form onsubmit={change}>
      <label><span>Current passphrase</span><input type="password" bind:value={old} autocomplete="off" /></label>
      <label><span>New passphrase</span><input type="password" bind:value={next} autocomplete="new-password" /></label>
      <label><span>New passphrase again</span><input type="password" bind:value={again} autocomplete="new-password" /></label>
      {#if mismatch}<p class="msg" role="alert"><strong>The new passphrases do not match.</strong></p>{/if}
      <p class="msg">
        A lost passphrase cannot be recovered: the book and every backup made with it are then unreadable.
        Record it durably, for example in a password manager.
      </p>
      {#if error}<p class="msg" role="alert"><strong>{error}</strong></p>{/if}
      <div class="buttons">
        <button type="submit" disabled={busy || old === "" || next.trim() === "" || next !== again}>Change</button>
        <button type="button" onclick={onclose}>Cancel</button>
      </div>
    </form>
  {/if}
</Modal>

<style>
  form {
    display: grid;
    grid-template-columns: max-content auto;
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
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>
