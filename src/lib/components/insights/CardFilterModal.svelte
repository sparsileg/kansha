<script lang="ts">
  // Customize the Auto Expenses card (CARD-060): an Accounts tab and a
  // Categories tab, as a report's Customize has, each with Select All
  // and Clear All. Nothing changes until OK.
  import { listsState } from "../../state/lists.svelte";
  import type { AccountId, CategoryId } from "../../types/bindings";
  import Modal from "../Modal.svelte";

  type Tab = "accounts" | "categories";

  let {
    title,
    accounts,
    categories,
    onsave,
    onclose,
  }: {
    title: string;
    /** The accounts chosen; `null` (never chosen) checks every open one. */
    accounts: AccountId[] | null;
    categories: CategoryId[];
    onsave: (accounts: AccountId[], categories: CategoryId[]) => void;
    onclose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let draft = $state<Record<Tab, number[]>>({
    accounts: accounts ?? listsState.accounts.filter((a) => a.status !== "closed").map((a) => a.id),
    categories: [...categories],
  });
  let tab = $state<Tab>("accounts");
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
    if (t === "accounts") {
      return listsState.accounts.map((a) => ({
        id: a.id,
        label: a.name,
        heading: GROUP_LABEL[a.group] ?? a.group,
        hidden: a.status === "closed" || !a.show_in_list,
      }));
    }
    return listsState.categories
      .filter((c) => c.kind !== "equity")
      .map((c) => ({
        id: c.id,
        label: listsState.categoryPath(c.id),
        heading: c.kind === "income" ? "Income" : "Expense",
        hidden: c.hidden,
      }));
  }

  const shown = $derived.by(() => {
    const text = contains.trim().toLowerCase();
    return items(tab).filter((i) => (showHidden || !i.hidden) && (!text || i.label.toLowerCase().includes(text)));
  });

  function set(id: number, on: boolean) {
    const cur = draft[tab];
    draft[tab] = on ? (cur.includes(id) ? cur : [...cur, id]) : cur.filter((x) => x !== id);
  }

  function save(e: Event) {
    e.preventDefault();
    onsave([...draft.accounts], [...draft.categories]);
  }
</script>

<Modal {title} {onclose} wide>
  <form onsubmit={save}>
    <div class="tabs" role="tablist">
      {#each [["accounts", "Accounts"], ["categories", "Categories"]] as [t, label] (t)}
        <button type="button" role="tab" aria-selected={tab === t} class:on={tab === t} onclick={() => ((tab = t as Tab), (contains = ""))}>{label}</button>
      {/each}
    </div>
    <div class="filter">
      <div>
        <ul class="list" aria-label={tab === "accounts" ? "Accounts" : "Categories"}>
          {#each shown as item, i (item.id)}
            {#if item.heading !== shown[i - 1]?.heading}<li class="head">{item.heading}</li>{/if}
            <li>
              <label class="check" class:dim={item.hidden}>
                <input type="checkbox" checked={draft[tab].includes(item.id)} onchange={(e) => set(item.id, e.currentTarget.checked)} />
                {item.label}
              </label>
            </li>
          {:else}
            <li class="none">None.</li>
          {/each}
        </ul>
        <div class="under">
          <label class="check"><input type="checkbox" bind:checked={showHidden} /> Show hidden and closed</label>
          {#if tab === "categories"}<label>Contains <input bind:value={contains} size="14" /></label>{/if}
        </div>
      </div>
      <div class="side">
        <button type="button" onclick={() => (draft[tab] = items(tab).map((i) => i.id))}>Select All</button>
        <button type="button" onclick={() => (draft[tab] = [])}>Clear All</button>
      </div>
    </div>
    <div class="buttons">
      <span class="grow"></span>
      <button type="button" onclick={onclose}>Cancel</button>
      <button type="submit">OK</button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: grid;
    gap: 0.6rem;
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
