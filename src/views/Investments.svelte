<script lang="ts">
  // The investments view (POS-010, LOT-150): every chosen investment
  // account with its equities, cash, and lots on one date. Views pick the
  // columns, accounts, and equities. Every figure comes from Rust.
  import { untrack } from "svelte";
  import CustomizeViewModal from "../lib/components/invest/CustomizeViewModal.svelte";
  import DatePicker from "../lib/components/invest/DatePicker.svelte";
  import { buildRows } from "../lib/invest/rows";
  import { columnLabel } from "../lib/invest/views";
  import { openAccount } from "../lib/shell/nav";
  import { investViewState as st } from "../lib/state/investview.svelte";
  import { listsState } from "../lib/state/lists.svelte";

  let customizing = $state(false);

  // Reload when the date, the view, or the account list changes; `load`
  // reads and writes state, so keep it out of this effect's dependencies.
  $effect(() => {
    void st.available;
    void st.view;
    void st.asOf;
    untrack(() => void st.load());
  });

  const rows = $derived(
    st.portfolio ? buildRows(st.portfolio, st.expanded, (id) => listsState.account(id)?.name ?? `#${id}`) : [],
  );
  const columns = $derived(st.view.columns);
  const NUMERIC = new Set(["price", "shares", "cost_basis", "market_value", "gain", "day_gain", "day_percent"]);
</script>

<section>
  <h1>Investments</h1>
  <div class="bar">
    <label>
      View:
      <select value={String(st.selected)} onchange={(e) => st.select(Number(e.currentTarget.value))}>
        {#each st.views as v, i (i)}<option value={String(i)}>{v.name}</option>{/each}
      </select>
    </label>
    <DatePicker value={st.asOf || listsState.today} today={listsState.today} label="As of" onchange={(iso) => (st.asOf = iso)} />
    <button type="button" onclick={() => (customizing = true)}>Customize</button>
  </div>
  {#if st.error}<p class="err" role="alert">{st.error}</p>{/if}
  {#if st.available.length === 0}
    <p>No investment accounts. Add one with Tools &gt; Accounts.</p>
  {:else}
    <div class="scroll">
      <table>
        <thead>
          <tr>
            <th>Name</th>
            {#each columns as c (c)}<th class:num={NUMERIC.has(c)}>{columnLabel(c)}</th>{/each}
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.key)}
            <tr class={row.kind}>
              <td class="name">
                {#if row.toggleKey !== undefined}
                  <button
                    type="button"
                    class="tog"
                    aria-expanded={row.expanded}
                    aria-label={`${row.expanded ? "Collapse" : "Expand"} ${row.name}`}
                    onclick={() => st.toggle(row.toggleKey!)}
                  >{row.kind === "account" ? (row.expanded ? "▾" : "▸") : row.expanded ? "−" : "+"}</button>
                {/if}
                {#if row.kind === "account"}
                  <button type="button" class="link" onclick={() => void openAccount(row.account!)}>{row.name}</button>
                {:else}
                  {row.name}
                {/if}
              </td>
              {#each columns as c (c)}
                <td class:num={NUMERIC.has(c)}>
                  {row.cells[c] ?? ""}{#if c === "market_value" && row.warn}<span class="flag" title={row.warn}> ⚠</span>{/if}{#if c === "price" && row.priceWarn}<span class="flag" title={row.priceWarn}> ⚠</span>{/if}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

{#if customizing}
  <CustomizeViewModal
    view={st.view}
    onsave={(v) => {
      customizing = false;
      st.update(v);
    }}
    onclose={() => (customizing = false)}
  />
{/if}

<style>
  h1 {
    font-size: var(--fs-title);
    margin: 0 0 0.5rem;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    align-items: center;
    margin-bottom: 0.6rem;
  }
  .bar label {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
  }
  .scroll {
    overflow: auto;
  }
  table {
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    padding: 0.15rem 0.7rem;
    white-space: nowrap;
  }
  thead th {
    border-bottom: 1px solid var(--line);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .account td {
    font-weight: 700;
  }
  .cash .name,
  .position .name {
    padding-left: 1.6rem;
  }
  .lot .name {
    padding-left: 3.2rem;
  }
  .lot td {
    opacity: 0.9;
  }
  .total td {
    border-top: 1px solid var(--line);
    font-weight: 700;
  }
  .tog {
    width: 1.4rem;
    padding: 0;
    margin-right: 0.2rem;
    background: none;
    border: none;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    color: inherit;
    text-decoration: underline;
    cursor: pointer;
    font: inherit;
  }
  .flag,
  .err {
    color: var(--bad);
  }
</style>
