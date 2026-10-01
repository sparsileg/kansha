<script lang="ts">
  // The investments view (POS-010, LOT-150): every chosen investment
  // account with its equities, cash, and lots on one date. Views pick the
  // columns, accounts, and equities. Every figure comes from Rust.
  import { untrack } from "svelte";
  import ContextMenu from "../lib/components/ContextMenu.svelte";
  import GearButton from "../lib/components/GearButton.svelte";
  import CustomizeViewModal from "../lib/components/invest/CustomizeViewModal.svelte";
  import SecurityDetailsModal from "../lib/components/invest/SecurityDetailsModal.svelte";
  import DatePicker from "../lib/components/invest/DatePicker.svelte";
  import { buildRows } from "../lib/invest/rows";
  import { columnLabel } from "../lib/invest/views";
  import { downloadPrices } from "../lib/shell/actions";
  import { openAccount } from "../lib/shell/nav";
  import { investViewState as st } from "../lib/state/investview.svelte";
  import { listsState } from "../lib/state/lists.svelte";

  let customizing = $state(false);
  let downloading = $state(false);
  /** The security whose details are open. */
  let details = $state<number | null>(null);
  /** Where the gear's menu opens, while it is open. */
  let menu = $state<{ x: number; y: number } | null>(null);

  function openMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = menu ? null : { x: r.right, y: r.bottom };
  }

  /** Prices for the As of date (PRC-040): the latest when it is today,
   * else that day's close. Then the figures follow. */
  async function download() {
    downloading = true;
    try {
      await downloadPrices(st.asOf || listsState.today);
      await st.load();
    } finally {
      downloading = false;
    }
  }

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

<section class="inv">
  <div class="bar">
    <select aria-label="View" value={String(st.selected)} onchange={(e) => st.select(Number(e.currentTarget.value))}>
      {#each st.views as v, i (i)}<option value={String(i)}>{v.name}</option>{/each}
    </select>
    <DatePicker value={st.asOf || listsState.today} today={listsState.today} label="As of" onchange={(iso) => (st.asOf = iso)} />
    <button type="button" disabled={downloading} onclick={() => void download()}>Download Prices</button>
    <span class="grow"></span>
    <GearButton label="Investments options" onclick={openMenu} />
  </div>
  {#if menu}
    <ContextMenu
      x={menu.x}
      y={menu.y}
      onclose={() => (menu = null)}
      items={[
        { label: "Customize…", action: () => (customizing = true) },
        { label: `${st.showClosed ? "✓ " : ""}Show closed lots`, action: () => st.setShowClosed(!st.showClosed) },
      ]}
    />
  {/if}
  {#if st.error}<p class="err" role="alert">{st.error}</p>{/if}
  {#if st.available.length === 0}
    <p>No investment accounts. Add one with Tools &gt; Accounts.</p>
  {:else}
    <div class="scroll sheet">
      <table>
        <thead>
          <tr>
            <th class="name">Name</th>
            {#each columns as c (c)}<th class:num={NUMERIC.has(c)}>{columnLabel(c)}</th>{/each}
          </tr>
        </thead>
        <tbody>
          {#each rows as row, i (row.key)}
            <tr class={row.kind} class:alt={i % 2 === 1 && row.kind !== "total"}>
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
                {:else if row.security !== undefined}
                  <button type="button" class="link" title="Security details" onclick={() => (details = row.security!)}>{row.name}</button>
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

{#if details !== null}
  <SecurityDetailsModal security={details} onclose={() => (details = null)} />
{/if}

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
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    align-items: center;
    margin-bottom: 0.6rem;
  }
  .grow {
    flex: 1;
  }
  .inv {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  /* A sheet (base.css) filling the window, like a report's page. */
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  /* Full width; Name takes the room the figures do not need. */
  table {
    border-collapse: collapse;
    width: 100%;
  }
  th.name {
    width: 100%;
  }
  th,
  td {
    text-align: left;
    padding: 0.15rem 0.7rem;
    white-space: nowrap;
  }
  thead th {
    position: sticky;
    top: 0;
    background: var(--head-bg);
    color: var(--head-fg);
    border-bottom: 1px solid var(--line);
  }
  /* Stripes by position, as in the registers. */
  tbody tr.alt {
    background: var(--row-alt);
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
  .sale .name {
    padding-left: 3.2rem;
  }
  .sale td {
    font-style: italic;
    opacity: 0.85;
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
