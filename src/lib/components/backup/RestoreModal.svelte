<script lang="ts">
  // Restore from a backup (BAK-070, BAK-075): pick the .zip, see its
  // manifest, type the passphrase it was made with, then compare it with
  // the current book before anything is replaced.
  import { call, commands } from "../../api";
  import { formatMoney } from "../../format/money";
  import type { ComparisonRow, Manifest, RestorePreview } from "../../types/bindings";
  import Modal from "../Modal.svelte";

  let { onclose, ondone, start = null }: { onclose: () => void; ondone: () => void; start?: string | null } =
    $props();

  let path = $state("");
  let manifest = $state<Manifest | null>(null);
  let passphrase = $state("");
  let preview = $state<RestorePreview | null>(null);
  let onlyDiffering = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

  async function choose() {
    error = null;
    const picked = await commands.pickBackupFile(start);
    if (picked === null) return;
    path = picked;
    manifest = null;
    preview = null;
    try {
      manifest = await call(commands.backupManifest(path));
    } catch (e) {
      error = message(e);
    }
  }

  async function open(e: Event) {
    e.preventDefault();
    if (!manifest || busy) return;
    busy = true;
    error = null;
    try {
      preview = await call(commands.restoreOpen(path, passphrase));
      passphrase = "";
    } catch (err) {
      error = message(err);
    } finally {
      busy = false;
    }
  }

  async function restore() {
    busy = true;
    error = null;
    try {
      await call(commands.restoreApply());
      ondone();
    } catch (e) {
      error = message(e);
      busy = false;
    }
  }

  function cancel() {
    if (preview) void commands.restoreCancel();
    onclose();
  }

  const both = $derived(preview?.comparison.rows.filter((r) => r.backup && r.current) ?? []);
  const backupOnly = $derived(preview?.comparison.rows.filter((r) => r.backup && !r.current) ?? []);
  const currentOnly = $derived(preview?.comparison.rows.filter((r) => !r.backup && r.current) ?? []);
  const shown = $derived(onlyDiffering ? both.filter((r) => r.differs) : both);
  const differing = $derived(preview?.comparison.rows.filter((r) => r.differs).length ?? 0);

  /** Mark a differing figure with a symbol and bold, never by color alone. */
  const differs = (r: ComparisonRow, f: (s: NonNullable<ComparisonRow["backup"]>) => string | number) =>
    r.backup && r.current && f(r.backup) !== f(r.current);
</script>

<Modal title="Restore from backup" wide onclose={cancel}>
  {#if !preview}
    <p>
      Choose a Kansha backup (.zip). The current book is backed up first and nothing is replaced until you
      compare and confirm.
    </p>
    <p class="row">
      <button type="button" onclick={choose}>Choose backup file…</button>
      {#if path}<span class="path">{path}</span>{/if}
    </p>
    {#if manifest}
      <table class="facts">
        <tbody>
          <tr><th>Made</th><td>{manifest.created_at}</td></tr>
          <tr><th>Kind</th><td>{manifest.kind}</td></tr>
          <tr><th>Kansha version</th><td>{manifest.app_version}</td></tr>
          <tr><th>Schema version</th><td>{manifest.schema_version}</td></tr>
        </tbody>
      </table>
      <form onsubmit={open}>
        <label>
          Passphrase this backup was made with
          <!-- svelte-ignore a11y_autofocus -->
          <input type="password" bind:value={passphrase} autocomplete="off" autofocus />
        </label>
        <button type="submit" disabled={busy || passphrase === ""}>{busy ? "Opening…" : "Open backup"}</button>
      </form>
    {/if}
  {:else}
    <table class="facts">
      <tbody>
        <tr><th>Backup made</th><td>{preview.comparison.backup_created_at}</td></tr>
        <tr><th>Last change in backup</th><td>{preview.comparison.backup_last_change ?? "none"}</td></tr>
        <tr><th>Last change in current book</th><td>{preview.comparison.current_last_change ?? "none"}</td></tr>
      </tbody>
    </table>
    {#if preview.integrity.issues.length > 0}
      <p class="warn" role="alert">
        ⚠ The backup's integrity check found {preview.integrity.issues.length} problem(s).
      </p>
    {/if}
    <p>
      {differing === 0 ? "No differences." : `${differing} account(s) differ, marked ≠.`}
      <label class="inline"><input type="checkbox" bind:checked={onlyDiffering} /> Show only rows that differ</label>
    </p>
    <div class="scroll">
      <table class="cmp">
        <thead>
          <tr>
            <th></th><th>Account</th>
            <th class="num">Transactions (backup)</th><th class="num">Transactions (current)</th>
            <th class="num">Balance (backup)</th><th class="num">Balance (current)</th>
          </tr>
        </thead>
        <tbody>
          {#each shown as r (r.account)}
            <tr class:diff={r.differs}>
              <td class="mark">{r.differs ? "≠" : ""}</td>
              <td class:b={differs(r, (s) => s.name)}>
                {r.backup?.name}{#if differs(r, (s) => s.name)} (now {r.current?.name}){/if}
              </td>
              <td class="num" class:b={differs(r, (s) => s.txns)}>{r.backup?.txns}</td>
              <td class="num" class:b={differs(r, (s) => s.txns)}>{r.current?.txns}</td>
              <td class="num" class:b={differs(r, (s) => s.value)}>{formatMoney(r.backup?.value ?? "0")}</td>
              <td class="num" class:b={differs(r, (s) => s.value)}>{formatMoney(r.current?.value ?? "0")}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if backupOnly.length}
        <h3>Only in the backup</h3>
        <table class="cmp">
          <tbody>
            {#each backupOnly as r (r.account)}
              <tr class="diff">
                <td class="mark">≠</td><td class="b">{r.backup?.name}</td>
                <td class="num">{r.backup?.txns} transactions</td>
                <td class="num">{formatMoney(r.backup?.value ?? "0")}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
      {#if currentOnly.length}
        <h3>Only in the current book (lost by restoring)</h3>
        <table class="cmp">
          <tbody>
            {#each currentOnly as r (r.account)}
              <tr class="diff">
                <td class="mark">≠</td><td class="b">{r.current?.name}</td>
                <td class="num">{r.current?.txns} transactions</td>
                <td class="num">{formatMoney(r.current?.value ?? "0")}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
    <p class="note">
      After restoring, the passphrase this backup was made with opens the book.
    </p>
  {/if}
  {#if error}<p class="err" role="alert">{error}</p>{/if}
  <div class="buttons">
    {#if preview}
      <button type="button" onclick={restore} disabled={busy}>{busy ? "Restoring…" : "Restore"}</button>
    {/if}
    <button type="button" onclick={cancel} disabled={busy}>Cancel</button>
  </div>
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
  .facts th {
    text-align: right;
    font-weight: normal;
    padding-right: 0.75rem;
    opacity: 0.8;
  }
  form {
    display: flex;
    gap: 0.5rem;
    align-items: end;
    flex-wrap: wrap;
    margin-top: 0.5rem;
  }
  form label {
    display: flex;
    flex-direction: column;
  }
  .inline {
    margin-left: 1rem;
  }
  .scroll {
    max-height: 50vh;
    overflow: auto;
  }
  .cmp {
    border-collapse: collapse;
    width: 100%;
  }
  .cmp th,
  .cmp td {
    padding: 0.15rem 0.5rem;
    border-bottom: 1px solid var(--line-soft);
    text-align: left;
  }
  .num {
    text-align: right !important;
  }
  .mark {
    width: 1.5rem;
    font-weight: bold;
  }
  .b {
    font-weight: bold;
  }
  h3 {
    font-size: var(--fs-register);
    margin: 0.75rem 0 0.25rem;
  }
  .warn,
  .err {
    font-weight: bold;
  }
  .note {
    opacity: 0.8;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.75rem;
  }
</style>
