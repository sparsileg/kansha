<script lang="ts">
  import "./manager.css";
  import { call, commands } from "../api";
  import { formatMoney } from "../format/money";
  import { hiddenLast } from "../manage/order";
  import { confirmState } from "../state/confirm.svelte";
  import { listsState } from "../state/lists.svelte";
  import { registerState } from "../state/register.svelte";
  import type { Payee } from "../types/bindings";

  let selected = $state<Payee | null>(null);
  let name = $state("");
  let hidden = $state(false);
  let mergeInto = $state("");
  let showAll = $state(false);
  let error = $state<string | null>(null);

  /** A memorized payee has defaults; they show its last use (PAY-020). */
  const memorized = (p: Payee) =>
    p.default_category !== null ||
    p.default_tag !== null ||
    p.default_memo !== "" ||
    p.default_amount !== null;

  const shown = $derived(
    hiddenLast(
      listsState.payees.filter((p) => showAll || memorized(p) || p.id === selected?.id),
      (p) => p.name,
    ),
  );

  function pick(p: Payee | null) {
    selected = p;
    name = p?.name ?? "";
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
    if (!selected) {
      error = "Payees are created by entering a transaction; select one to edit.";
      return;
    }
    // The defaults travel unchanged; they come from the last use, not from here.
    const fields = {
      name,
      default_category: selected.default_category,
      default_tag: selected.default_tag,
      default_memo: selected.default_memo,
      default_amount: selected.default_amount,
      hidden,
    };
    try {
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
    <label class="check"><input type="checkbox" bind:checked={showAll} /> Show all payees</label>
    <div class="list">
      <table>
        <thead><tr><th>Payee</th><th>Category</th><th>Memo</th><th class="num">Amount</th></tr></thead>
        <tbody>
          {#each shown as p (p.id)}
            <tr class:sel={selected?.id === p.id} class:dim={p.hidden} onclick={() => pick(p)}>
              <td>{p.name}</td>
              <td>{p.default_category != null ? listsState.categoryPath(p.default_category) : ""}</td>
              <td>{p.default_memo}</td>
              <td class="num">{p.default_amount != null ? formatMoney(p.default_amount) : ""}</td>
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
      <p class="note">The list shows payees you have memorized, with the category, memo, and amount of their last use. Payees are created when you enter a transaction.</p>
    {/if}
  </form>
</div>
