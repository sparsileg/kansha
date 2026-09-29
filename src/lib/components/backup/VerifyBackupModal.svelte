<script lang="ts">
  // Verify backup… (BAK-080): decrypt a backup with its passphrase and run
  // the integrity check on its database. Nothing is changed; a clean
  // result is recorded as the last full verification.
  import { call, commands } from "../../api";
  import type { VerifyResult } from "../../types/bindings";
  import Modal from "../Modal.svelte";

  let { onclose, start = null }: { onclose: () => void; start?: string | null } = $props();

  let path = $state("");
  let passphrase = $state("");
  let result = $state<VerifyResult | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function choose() {
    const picked = await commands.pickBackupFile(start);
    if (picked !== null) {
      path = picked;
      result = null;
      error = null;
    }
  }

  async function verify(e: Event) {
    e.preventDefault();
    busy = true;
    error = null;
    result = null;
    try {
      result = await call(commands.backupVerify(path, passphrase));
      passphrase = "";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Verify backup" {onclose}>
  <p class="row">
    <button type="button" onclick={choose}>Choose backup file…</button>
    {#if path}<span class="path">{path}</span>{/if}
  </p>
  {#if path}
    <form onsubmit={verify}>
      <label>
        Passphrase this backup was made with
        <input type="password" bind:value={passphrase} autocomplete="off" />
      </label>
      <button type="submit" disabled={busy || passphrase === ""}>{busy ? "Checking…" : "Verify"}</button>
    </form>
  {/if}
  {#if result}
    {#if result.integrity.issues.length === 0}
      <p role="status"><strong>✓ The backup opens and passes the integrity check.</strong></p>
    {:else}
      <p role="alert">
        <strong>⚠ The backup opens, but its integrity check found {result.integrity.issues.length} problem(s).</strong>
      </p>
    {/if}
    <p>Made {result.manifest.created_at} by Kansha {result.manifest.app_version}.</p>
  {/if}
  {#if error}<p role="alert"><strong>{error}</strong></p>{/if}
  <div class="buttons"><button type="button" onclick={onclose}>Close</button></div>
</Modal>

<style>
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
  form {
    display: flex;
    gap: 0.5rem;
    align-items: end;
    flex-wrap: wrap;
  }
  form label {
    display: flex;
    flex-direction: column;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
  }
</style>
