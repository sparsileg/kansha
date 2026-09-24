<script lang="ts">
  import "./manager.css";
  import { call, commands } from "../api";
  import { confirmState } from "../state/confirm.svelte";
  import { listsState } from "../state/lists.svelte";
  import type { Tag, TagFields } from "../types/bindings";

  let selected = $state<Tag | null>(null);
  let f = $state<TagFields>({ name: "", hidden: false });
  let mergeInto = $state("");
  let error = $state<string | null>(null);

  function pick(t: Tag | null) {
    selected = t;
    f = t ? { name: t.name, hidden: t.hidden } : { name: "", hidden: false };
    mergeInto = "";
    error = null;
  }

  async function run(fn: () => Promise<Tag | void>) {
    error = null;
    try {
      const r = await fn();
      await listsState.loadAll();
      pick(r ? (listsState.tag(r.id) ?? null) : null);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  const save = (e: Event) => {
    e.preventDefault();
    return run(() =>
      selected
        ? call(commands.tagUpdate(selected.id, $state.snapshot(f)))
        : call(commands.tagCreate($state.snapshot(f))),
    );
  };
  async function remove() {
    if (!selected || !(await confirmState.ask(`Delete tag "${selected.name}"?`))) return;
    const id = selected.id;
    await run(async () => {
      await call(commands.tagDelete(id));
    });
  }
  async function merge() {
    if (!selected || !mergeInto) return;
    const target = listsState.tag(Number(mergeInto));
    if (!target || !(await confirmState.ask(`Merge "${selected.name}" into "${target.name}"? "${selected.name}" is removed.`))) return;
    const id = selected.id;
    await run(async () => {
      await call(commands.tagMerge(id, target.id));
    });
  }
</script>

<div class="mgr">
  <div class="list">
    <table>
      <thead><tr><th>Tag</th><th>Hidden</th></tr></thead>
      <tbody>
        {#each listsState.tags as t (t.id)}
          <tr class:sel={selected?.id === t.id} class:dim={t.hidden} onclick={() => pick(t)}>
            <td>{t.name}</td><td>{t.hidden ? "yes" : ""}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <form onsubmit={save}>
    <h3>{selected ? "Edit tag" : "New tag"}</h3>
    <label>Name <input bind:value={f.name} required /></label>
    <label class="check"><input type="checkbox" bind:checked={f.hidden} /> Hidden</label>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <div class="row">
      <button type="submit">Save</button>
      <button type="button" onclick={() => pick(null)}>New</button>
      {#if selected}<button type="button" onclick={remove}>Delete</button>{/if}
    </div>
    {#if selected}
      <label>
        Merge into
        <select bind:value={mergeInto}>
          <option value="">—</option>
          {#each listsState.tags.filter((t) => t.id !== selected?.id) as t (t.id)}<option value={String(t.id)}>{t.name}</option>{/each}
        </select>
      </label>
      <div class="row"><button type="button" disabled={!mergeInto} onclick={merge}>Merge</button></div>
      <p class="note">Delete works only for an unused tag; hide it otherwise.</p>
    {/if}
  </form>
</div>
