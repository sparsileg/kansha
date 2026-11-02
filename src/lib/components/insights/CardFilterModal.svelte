<script lang="ts">
  // Customize a spending card (CARD-060, CARD-070): its name, then
  // Accounts, Categories (spending categories only), and Tags tabs, as a
  // report's Customize has, each with Select All and Clear All, and a
  // Columns tab, where at least one must stay checked. Nothing changes
  // until OK; a refused name keeps the dialog open with the reason.
  import { listsState } from "../../state/lists.svelte";
  import type { AccountId, CategoryId, SpendingColumn, TagId } from "../../types/bindings";
  import Modal from "../Modal.svelte";

  type Tab = "accounts" | "categories" | "tags";

  const COLUMNS: [SpendingColumn, string][] = [
    ["last_year", "Last Year"],
    ["year", "Year"],
    ["last_month", "Last Month"],
    ["month", "Month"],
    ["monthly_avg", "Monthly Avg"],
    ["three_year_avg", "3 Yr Avg"],
  ];
  const TABS: [Tab | "columns", string][] = [
    ["accounts", "Accounts"],
    ["categories", "Categories"],
    ["tags", "Tags"],
    ["columns", "Columns"],
  ];

  let {
    title,
    name: startName,
    accounts,
    categories,
    tags,
    columns,
    onsave,
    onclose,
  }: {
    title: string;
    name: string;
    /** The accounts chosen; `null` (never chosen) checks every open one. */
    accounts: AccountId[] | null;
    categories: CategoryId[];
    tags: TagId[];
    columns: SpendingColumn[];
    /** Rejects with the reason when Rust refuses (a name in use). */
    onsave: (name: string, accounts: AccountId[], categories: CategoryId[], tags: TagId[], columns: SpendingColumn[]) => Promise<void>;
    onclose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let name = $state(startName);
  let error = $state<string | null>(null);
  let saving = $state(false);

  // svelte-ignore state_referenced_locally
  let draft = $state<Record<Tab, number[]>>({
    accounts: accounts ?? listsState.accounts.filter((a) => a.status !== "closed").map((a) => a.id),
    categories: [...categories],
    tags: [...tags],
  });
  // svelte-ignore state_referenced_locally
  let shownColumns = $state<SpendingColumn[]>([...columns]);
  let tab = $state<Tab | "columns">("accounts");
  let showHidden = $state(false);
  let contains = $state("");

  interface Item {
    id: number;
    label: string;
    heading: string;
    hidden: boolean;
  }

  const GROUP_LABEL: Record<string, string> = {
    banking: "Banking",
    credit: "Credit",
    investments: "Investment",
    retirement: "Retirement",
    assets: "Assets",
    liabilities: "Liabilities",
    other: "Other",
  };

  function items(t: Tab): Item[] {
    if (t === "tags") {
      return listsState.tags.map((x) => ({ id: x.id, label: x.name, heading: "", hidden: x.hidden }));
    }
    if (t === "accounts") {
      return listsState.accounts.map((a) => ({
        id: a.id,
        label: a.name,
        heading: GROUP_LABEL[a.group] ?? a.group,
        hidden: a.status === "closed" || !a.show_in_list,
      }));
    }
    return listsState.categories
      .filter((c) => c.kind === "expense")
      .map((c) => ({
        id: c.id,
        label: listsState.categoryPath(c.id),
        heading: "",
        hidden: c.hidden,
      }));
  }

  const shown = $derived.by(() => {
    if (tab === "columns") return [];
    const text = contains.trim().toLowerCase();
    return items(tab).filter((i) => (showHidden || !i.hidden) && (!text || i.label.toLowerCase().includes(text)));
  });

  /** Ticking or unticking a category does the same to its subcategories. */
  function set(t: Tab, id: number, on: boolean) {
    const cur = draft[t];
    const ids = t === "categories" ? listsState.withSubcategories(id) : [id];
    draft[t] = on ? [...cur, ...ids.filter((x) => !cur.includes(x))] : cur.filter((x) => !ids.includes(x));
  }

  function setColumn(c: SpendingColumn, on: boolean) {
    shownColumns = on ? [...shownColumns, c] : shownColumns.filter((x) => x !== c);
  }

  async function save(e: Event) {
    e.preventDefault();
    if (!name.trim() || shownColumns.length === 0 || saving) return;
    saving = true;
    error = null;
    try {
      await onsave(name.trim(), [...draft.accounts], [...draft.categories], [...draft.tags], [...shownColumns]);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }
</script>

<Modal {title} {onclose} wide>
  <form onsubmit={save}>
    <label class="name">
      <span>Name</span>
      <input type="text" bind:value={name} maxlength="60" />
    </label>
    <div class="tabs" role="tablist">
      {#each TABS as [t, label] (t)}
        <button type="button" role="tab" aria-selected={tab === t} class:on={tab === t} onclick={() => ((tab = t), (contains = ""))}>{label}</button>
      {/each}
    </div>
    {#if tab === "columns"}
      <ul class="list" aria-label="Columns">
        {#each COLUMNS as [c, label] (c)}
          <li>
            <label class="check">
              <input type="checkbox" checked={shownColumns.includes(c)} onchange={(e) => setColumn(c, e.currentTarget.checked)} />
              {label}
            </label>
          </li>
        {/each}
      </ul>
      {#if shownColumns.length === 0}<p class="err" role="alert">Check at least one column.</p>{/if}
    {:else}
      {@const list = tab}
      <div class="filter">
        <div>
          <ul class="list" aria-label={TABS.find(([t]) => t === list)?.[1]}>
            {#each shown as item, i (item.id)}
              {#if item.heading && item.heading !== shown[i - 1]?.heading}<li class="head">{item.heading}</li>{/if}
              <li>
                <label class="check" class:dim={item.hidden}>
                  <input type="checkbox" checked={draft[list].includes(item.id)} onchange={(e) => set(list, item.id, e.currentTarget.checked)} />
                  {item.label}
                </label>
              </li>
            {:else}
              <li class="none">None.</li>
            {/each}
          </ul>
          <div class="under">
            <label class="check"><input type="checkbox" bind:checked={showHidden} /> Show hidden and closed</label>
            {#if list === "categories"}<label>Contains <input bind:value={contains} size="14" /></label>{/if}
          </div>
          {#if list === "tags"}<p class="hint">None checked counts every line. Tags with no category checked list every spending category that has them.</p>{/if}
        </div>
        <div class="side">
          <button type="button" onclick={() => (draft[list] = items(list).map((i) => i.id))}>Select all</button>
          <button type="button" onclick={() => (draft[list] = [])}>Clear all</button>
        </div>
      </div>
    {/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <div class="buttons">
      <span class="grow"></span>
      <button type="button" onclick={onclose}>Cancel</button>
      <button type="submit" disabled={!name.trim() || shownColumns.length === 0 || saving}>OK</button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: grid;
    gap: 0.6rem;
  }
  .name {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .name input {
    flex: 1;
  }
  .err {
    color: var(--bad);
    margin: 0;
  }
  .tabs {
    display: flex;
    gap: 0.25rem;
    border-bottom: 1px solid var(--line);
  }
  .on {
    font-weight: 700;
    text-decoration: underline;
  }
  .check {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0.25rem;
    border: 1px solid var(--line);
    border-radius: 4px;
    height: 16rem;
    min-width: min(20rem, 90vw);
    overflow: auto;
  }
  .head {
    font-weight: 700;
    margin-top: 0.3rem;
  }
  .dim {
    font-style: italic;
    opacity: 0.75;
  }
  .filter {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 1rem;
    align-items: start;
  }
  .side {
    display: grid;
    gap: 0.4rem;
    max-width: 12rem;
  }
  .under {
    display: flex;
    gap: 1rem;
    align-items: center;
    margin-top: 0.3rem;
  }
  .under label {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
  }
  .hint {
    margin: 0.3rem 0 0;
    font-size: var(--fs-register);
    opacity: 0.75;
  }
  .none {
    opacity: 0.75;
    margin: 0;
    font-size: var(--fs-register);
  }
  .buttons {
    display: flex;
    gap: 0.5rem;
  }
  .grow {
    flex: 1;
  }
</style>
