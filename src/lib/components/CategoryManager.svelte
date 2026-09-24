<script lang="ts">
  import "./manager.css";
  import { call, commands } from "../api";
  import { confirmState } from "../state/confirm.svelte";
  import { listsState } from "../state/lists.svelte";
  import { registerState } from "../state/register.svelte";
  import type { Category, CategoryFields } from "../types/bindings";

  const blank = (): CategoryFields => ({
    parent: null,
    kind: "expense",
    name: "",
    tax_related: false,
    tithable: false,
    giving: false,
    hidden: false,
  });

  let selected = $state<Category | null>(null);
  let f = $state<CategoryFields>(blank());
  let mergeInto = $state("");
  let error = $state<string | null>(null);

  const locked = $derived(selected?.system != null);

  function pick(c: Category | null) {
    selected = c;
    f = c
      ? {
          parent: c.parent,
          kind: c.kind,
          name: c.name,
          tax_related: c.tax_related,
          tithable: c.tithable,
          giving: c.giving,
          hidden: c.hidden,
        }
      : blank();
    mergeInto = "";
    error = null;
  }

  async function run(fn: () => Promise<Category | void>) {
    error = null;
    try {
      const r = await fn();
      await listsState.loadAll();
      if (registerState.accountId !== null) await registerState.refresh();
      pick(r ? (listsState.category(r.id) ?? null) : null);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  const save = (e: Event) => {
    e.preventDefault();
    const fields = $state.snapshot(f);
    return run(() =>
      selected
        ? call(commands.categoryUpdate(selected.id, fields))
        : call(commands.categoryCreate(fields)),
    );
  };

  async function remove() {
    if (!selected || !(await confirmState.ask(`Delete category "${listsState.categoryPath(selected.id)}"?`))) return;
    const id = selected.id;
    await run(async () => {
      await call(commands.categoryDelete(id));
    });
  }

  async function merge() {
    if (!selected || !mergeInto) return;
    const target = Number(mergeInto);
    const from = listsState.categoryPath(selected.id);
    const to = listsState.categoryPath(target);
    if (!(await confirmState.ask(`Merge "${from}" into "${to}"? "${from}" is removed.`))) return;
    const id = selected.id;
    await run(async () => {
      await call(commands.categoryMerge(id, target));
    });
  }

  const parents = $derived(
    listsState.categories.filter((c) => c.kind === f.kind && c.id !== selected?.id),
  );
  const mergeTargets = $derived(
    listsState.categories.filter((c) => selected && c.kind === selected.kind && c.id !== selected.id),
  );
</script>

<div class="mgr">
  <div class="list">
    <table>
      <thead><tr><th>Category</th><th>Kind</th><th>Flags</th></tr></thead>
      <tbody>
        {#each listsState.categories as c (c.id)}
          <tr class:sel={selected?.id === c.id} class:dim={c.hidden} onclick={() => pick(c)}>
            <td>{listsState.categoryPath(c.id)}{c.system ? " (built-in)" : ""}</td>
            <td>{c.kind}</td>
            <td>{[c.tax_related && "tax", c.tithable && "tithable", c.giving && "giving", c.hidden && "hidden"].filter(Boolean).join(", ")}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <form onsubmit={save}>
    <h3>{selected ? "Edit category" : "New category"}</h3>
    {#if locked}<p class="note">Built-in category: it can't be changed.</p>{/if}
    <label>Name <input bind:value={f.name} required disabled={locked} /></label>
    <label>
      Kind
      <select bind:value={f.kind} disabled={locked || selected !== null}>
        <option value="income">Income</option>
        <option value="expense">Expense</option>
      </select>
    </label>
    <label>
      Parent
      <select
        value={f.parent ?? ""}
        disabled={locked}
        onchange={(e) => (f.parent = e.currentTarget.value ? Number(e.currentTarget.value) : null)}
      >
        <option value="">(top level)</option>
        {#each parents as p (p.id)}<option value={p.id}>{listsState.categoryPath(p.id)}</option>{/each}
      </select>
    </label>
    <label class="check"><input type="checkbox" bind:checked={f.tax_related} disabled={locked} /> Tax-related</label>
    <label class="check"><input type="checkbox" bind:checked={f.tithable} disabled={locked} /> Tithable</label>
    <label class="check"><input type="checkbox" bind:checked={f.giving} disabled={locked} /> Giving</label>
    <label class="check"><input type="checkbox" bind:checked={f.hidden} disabled={locked} /> Hidden</label>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <div class="row">
      <button type="submit" disabled={locked}>Save</button>
      <button type="button" onclick={() => pick(null)}>New</button>
      {#if selected && !locked}<button type="button" onclick={remove}>Delete</button>{/if}
    </div>
    {#if selected && !locked}
      <label>
        Merge into
        <select bind:value={mergeInto}>
          <option value="">—</option>
          {#each mergeTargets as c (c.id)}<option value={String(c.id)}>{listsState.categoryPath(c.id)}</option>{/each}
        </select>
      </label>
      <div class="row"><button type="button" disabled={!mergeInto} onclick={merge}>Merge</button></div>
      <p class="note">Delete works only for an unused category; hide it otherwise.</p>
    {/if}
  </form>
</div>
