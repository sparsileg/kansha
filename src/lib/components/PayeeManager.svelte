<script lang="ts">
  import "./manager.css";
  import { call, commands } from "../api";
  import { formatMoney, parseMoney } from "../format/money";
  import { confirmState } from "../state/confirm.svelte";
  import { listsState } from "../state/lists.svelte";
  import { registerState } from "../state/register.svelte";
  import { selectOnFocus } from "../ui/selectOnFocus";
  import type { Payee } from "../types/bindings";

  let selected = $state<Payee | null>(null);
  let name = $state("");
  let category = $state("");
  let tag = $state("");
  let memo = $state("");
  let amount = $state("");
  let hidden = $state(false);
  let mergeInto = $state("");
  let filter = $state("");
  let error = $state<string | null>(null);

  const shown = $derived(
    listsState.payees.filter((p) => p.name.toLowerCase().includes(filter.trim().toLowerCase())),
  );

  function pick(p: Payee | null) {
    selected = p;
    name = p?.name ?? "";
    category = p?.default_category != null ? String(p.default_category) : "";
    tag = p?.default_tag != null ? String(p.default_tag) : "";
    memo = p?.default_memo ?? "";
    amount = p?.default_amount != null ? formatMoney(p.default_amount) : "";
    hidden = p?.hidden ?? false;
    mergeInto = "";
    error = null;
  }

  async function reload() {
    await listsState.loadPayees();
    if (registerState.accountId !== null) await registerState.refresh();
  }

  async function save(e: Event) {
    e.preventDefault();
    error = null;
    let amt: string | null = null;
    if (amount.trim()) {
      amt = parseMoney(amount);
      if (amt === null) {
        error = "Default amount is not a valid amount.";
        return;
      }
    }
    const fields = {
      name,
      default_category: category ? Number(category) : null,
      default_tag: tag ? Number(tag) : null,
      default_memo: memo,
      default_amount: amt,
      hidden,
    };
    try {
      if (!selected) {
        error = "Payees are created by entering a transaction; select one to edit.";
        return;
      }
      const p = await call(commands.payeeUpdate(selected.id, fields));
      await reload();
      pick(listsState.payee(p.id) ?? null);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function remove() {
    if (!selected || !(await confirmState.ask(`Delete payee "${selected.name}"?`))) return;
    try {
      await call(commands.payeeDelete(selected.id));
      await reload();
      pick(null);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function merge() {
    if (!selected || !mergeInto) return;
    const target = listsState.payee(Number(mergeInto));
    if (!target || !(await confirmState.ask(`Merge "${selected.name}" into "${target.name}"? "${selected.name}" is removed.`))) return;
    try {
      await call(commands.payeeMerge(selected.id, target.id));
      await reload();
      pick(null);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<div class="mgr">
  <div>
    <label>Find <input type="search" bind:value={filter} /></label>
    <div class="list">
      <table>
        <thead><tr><th>Payee</th><th>Category</th><th>Amount</th></tr></thead>
        <tbody>
          {#each shown as p (p.id)}
            <tr class:sel={selected?.id === p.id} class:dim={p.hidden} onclick={() => pick(p)}>
              <td>{p.name}</td>
              <td>{p.default_category != null ? listsState.categoryPath(p.default_category) : ""}</td>
              <td>{p.default_amount != null ? formatMoney(p.default_amount) : ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
  <form onsubmit={save}>
    <h3>{selected ? "Edit payee" : "Select a payee"}</h3>
    {#if selected}
      <label>Name <input bind:value={name} required /></label>
      <label>
        Default category
        <select bind:value={category}>
          <option value="">—</option>
          {#each listsState.categories.filter((c) => c.kind !== "equity") as c (c.id)}<option value={String(c.id)}>{listsState.categoryPath(c.id)}</option>{/each}
        </select>
      </label>
      <label>
        Default tag
        <select bind:value={tag}>
          <option value="">—</option>
          {#each listsState.tags as t (t.id)}<option value={String(t.id)}>{t.name}</option>{/each}
        </select>
      </label>
      <label>Default memo <input bind:value={memo} /></label>
      <label>Default amount (negative = payment) <input bind:value={amount} inputmode="decimal" use:selectOnFocus /></label>
      <label class="check"><input type="checkbox" bind:checked={hidden} /> Hidden</label>
      {#if error}<p class="err" role="alert">{error}</p>{/if}
      <div class="row">
        <button type="submit">Save</button>
        <button type="button" onclick={remove}>Delete</button>
      </div>
      <label>
        Merge into
        <select bind:value={mergeInto}>
          <option value="">—</option>
          {#each listsState.payees.filter((p) => p.id !== selected?.id) as p (p.id)}<option value={String(p.id)}>{p.name}</option>{/each}
        </select>
      </label>
      <div class="row"><button type="button" disabled={!mergeInto} onclick={merge}>Merge</button></div>
      <p class="note">Delete works only for an unused payee; hide it otherwise.</p>
    {:else}
      <p class="note">Payees are created when you enter a transaction.</p>
    {/if}
  </form>
</div>
