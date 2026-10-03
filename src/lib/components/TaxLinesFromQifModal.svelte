<script lang="ts">
  // Set tax lines from a QIF (CAT-050, MIG-020): read Quicken's tax codes
  // from a QIF's category list and give the book's categories with no tax
  // line the matching one. A line already set is never changed.
  import Modal from "./Modal.svelte";
  import { call, commands } from "../api";
  import { listsState } from "../state/lists.svelte";
  import type { TaxLinePlan, TaxLinePlanStatus } from "../types/bindings";

  let { onclose }: { onclose: () => void } = $props();

  let path = $state<string | null>(null);
  let plan = $state<TaxLinePlan | null>(null);
  let error = $state<string | null>(null);
  let done = $state<string | null>(null);
  let busy = $state(false);

  const ORDER: TaxLinePlanStatus[] = ["set", "kept", "missing", "unmapped", "system"];
  const STATUS: Record<TaxLinePlanStatus, string> = {
    set: "Set",
    kept: "Kept (has a line)",
    missing: "Not in the book",
    unmapped: "No Kansha line",
    system: "Built-in (left alone)",
  };

  const items = $derived(
    plan ? [...plan.items].sort((a, b) => ORDER.indexOf(a.status) - ORDER.indexOf(b.status)) : [],
  );
  const toSet = $derived(items.filter((i) => i.status === "set").length);

  async function load(p: string) {
    plan = await call(commands.taxLinesFromQifPreview(p));
  }

  async function choose() {
    const p = await commands.pickImportFile(null);
    if (p === null) return;
    error = null;
    done = null;
    plan = null;
    path = p;
    try {
      await load(p);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function apply() {
    if (path === null) return;
    error = null;
    busy = true;
    try {
      const n = await call(commands.taxLinesFromQifApply(path));
      done = `Tax lines set on ${n} ${n === 1 ? "category" : "categories"}.`;
      await listsState.loadAll();
      await load(path);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Set tax lines from QIF" {onclose} wide>
  <div class="tl">
    <p class="note">
      Reads Quicken's tax codes from a QIF file's category list. Categories with no tax line get the matching one and
      become tax-related; a tax line already set is kept. The book is backed up first.
    </p>
    <div class="row">
      <button type="button" onclick={choose}>Choose QIF file…</button>
      {#if path}<span class="file">{path}</span>{/if}
    </div>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    {#if done}<p class="ok" role="status">✓ {done}</p>{/if}

    {#if plan}
      {#if items.length === 0}
        <p>The file's category list has no tax codes.</p>
      {:else}
        <p>{toSet} of {items.length} coded categories get a tax line.</p>
        <div class="list">
          <table>
            <thead><tr><th>Result</th><th>Category</th><th>Code</th><th>Tax line</th></tr></thead>
            <tbody>
              {#each items as i, n (n)}
                <tr class:dim={i.status !== "set"}>
                  <td>{STATUS[i.status]}</td>
                  <td>{i.category !== null ? listsState.categoryPath(i.category) : i.qif_name}</td>
                  <td class="num">{i.code}</td>
                  <td>{i.tax_line !== null ? listsState.taxLineLabel(i.tax_line) : ""}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    {/if}
    <div class="row">
      <button type="button" onclick={apply} disabled={busy || toSet === 0}>Apply</button>
      <button type="button" onclick={onclose}>Close</button>
    </div>
  </div>
</Modal>

<style>
  .tl {
    display: grid;
    gap: 0.5rem;
  }
  .row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .file {
    opacity: 0.8;
    overflow-wrap: anywhere;
  }
  .list {
    max-height: 50vh;
    overflow: auto;
  }
  table {
    border-collapse: collapse;
    font-size: var(--fs-register);
  }
  th,
  td {
    text-align: left;
    padding: 0.1rem 0.5rem;
    white-space: nowrap;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .dim td {
    opacity: 0.7;
  }
  .err {
    color: var(--bad);
  }
  .ok {
    color: var(--good);
  }
  .note {
    opacity: 0.8;
    margin: 0;
  }
</style>
