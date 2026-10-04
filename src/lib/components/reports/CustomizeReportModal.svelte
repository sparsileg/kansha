<script lang="ts">
  // Customize a report (RPT-020): Display settings, then one tab per
  // filter, each with Select All and Clear All. Nothing changes until OK.
  import { onMount } from "svelte";
  import { call, commands } from "../../api";
  import { displayDate } from "../../format/date";
  import {
    INTERVALS,
    PERIOD_PRESETS,
    REPORTS,
    SORTS,
    SUBTOTALS,
    TAX_GROUPS,
    TAB_LABELS,
    presetGroups,
    toggleFilter,
    type FilterTab,
  } from "../../reports/meta";
  import { investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import type { Column, PeriodChoice, ReportSettings, ResolvedRange } from "../../types/bindings";
  import DatePicker from "../invest/DatePicker.svelte";
  import Modal from "../Modal.svelte";

  let {
    settings,
    totalsOnHeading = false,
    onsave,
    onclose,
  }: {
    settings: ReportSettings;
    /** Where the report shown now puts its totals, for a setting left
     * at the report's default. */
    totalsOnHeading?: boolean;
    onsave: (s: ReportSettings) => void;
    onclose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let draft = $state<ReportSettings>(structuredClone($state.snapshot(settings)));
  let tab = $state<"display" | FilterTab>("display");
  let columns = $state<Column[]>([]);
  let resolved = $state<ResolvedRange | null>(null);
  /** Monthly, Quarterly, Yearly: the periods to pick from. */
  let periods = $state<PeriodChoice[]>([]);
  let showHidden = $state(false);
  let contains = $state("");
  let error = $state<string | null>(null);

  // svelte-ignore state_referenced_locally
  const meta = REPORTS[settings.kind];
  const today = $derived(listsState.today);

  onMount(async () => {
    columns = await commands.reportColumns(draft.kind);
    if (meta.tabs.includes("securities") && investState.securities.length === 0) {
      await investState.loadSecurities();
    }
  });

  // The preset's dates, shown beside it.
  $effect(() => {
    const range = $state.snapshot(draft.range);
    call(commands.reportRange(range)).then(
      (r) => ((resolved = r), (error = null)),
      (e) => (error = e instanceof Error ? e.message : String(e)),
    );
  });

  $effect(() => {
    const range = $state.snapshot(draft.range);
    if (!PERIOD_PRESETS.includes(range.preset)) {
      periods = [];
      return;
    }
    call(commands.reportPeriodChoices(range)).then(
      (c) => (periods = c),
      (e) => (error = e instanceof Error ? e.message : String(e)),
    );
  });

  async function setPreset(value: string) {
    const preset = value as ReportSettings["range"]["preset"];
    if (PERIOD_PRESETS.includes(preset)) {
      // The current period first; the second list picks another.
      try {
        const [first] = await call(commands.reportPeriodChoices({ preset, from: null, to: null }));
        draft.range = { preset, from: first.from, to: null };
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
      }
      return;
    }
    if (PERIOD_PRESETS.includes(draft.range.preset)) draft.range = { preset, from: null, to: null };
    draft.range.preset = preset;
    if (value === "custom" && resolved) {
      draft.range.from = draft.range.from ?? resolved.from;
      draft.range.to = draft.range.to ?? resolved.to;
    }
  }

  // --- Filter lists -----------------------------------------------------

  interface Item {
    id: number;
    label: string;
    heading?: string;
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

  const INVESTMENT = new Set(["brokerage", "traditional_ira", "roth_ira", "hsa", "retirement_401k", "donor_advised_fund"]);

  function items(t: FilterTab): Item[] {
    switch (t) {
      case "accounts":
        return listsState.accounts
          .filter((a) => draft.kind !== "capital_gains" || INVESTMENT.has(a.account_type))
          .map((a) => ({
            id: a.id,
            label: a.name,
            heading: GROUP_LABEL[a.group] ?? a.group,
            hidden: a.status === "closed" || !a.show_in_list,
          }));
      case "categories":
        return listsState.categories
          .filter((c) => c.kind !== "equity")
          .map((c) => ({
            id: c.id,
            label: listsState.categoryPath(c.id),
            heading: c.kind === "income" ? "Income" : "Expense",
            hidden: c.hidden,
          }));
      case "payees":
        return [...listsState.payees]
          .sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()))
          .map((p) => ({ id: p.id, label: p.name, hidden: p.hidden }));
      case "securities":
        return [...investState.securities]
          .sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()))
          .map((s) => ({ id: s.id, label: s.ticker ? `${s.name} (${s.ticker})` : s.name, hidden: s.hidden }));
      case "tags":
        return listsState.tags.map((t) => ({ id: t.id, label: t.name, hidden: t.hidden }));
    }
  }

  const field = (t: FilterTab): "accounts" | "categories" | "payees" | "securities" | "tags" => t;

  /** What "no filter" includes: taxable investment accounts for Capital
   * Gains, everything otherwise. */
  function base(t: FilterTab): number[] {
    if (t === "accounts" && draft.kind === "capital_gains") {
      return listsState.accounts
        .filter((a) => INVESTMENT.has(a.account_type) && a.tax_treatment === "taxable")
        .map((a) => a.id);
    }
    return items(t).map((i) => i.id);
  }

  const shown = $derived.by(() => {
    if (tab === "display") return [];
    const text = contains.trim().toLowerCase();
    return items(tab).filter((i) => (showHidden || !i.hidden) && (!text || i.label.toLowerCase().includes(text)));
  });

  function checked(t: FilterTab, id: number): boolean {
    const f = draft[field(t)] ?? null;
    return (f ?? base(t)).includes(id);
  }

  function set(t: FilterTab, id: number, on: boolean) {
    const all = items(t).map((i) => i.id);
    draft[field(t)] = toggleFilter(draft[field(t)] ?? null, base(t), all, id, on);
  }

  function selectAll(t: FilterTab) {
    const all = items(t).map((i) => i.id);
    draft[field(t)] = base(t).length === all.length ? null : all;
  }

  function clearAll(t: FilterTab) {
    draft[field(t)] = [];
  }

  // --- Columns ----------------------------------------------------------

  function setColumn(id: string, on: boolean) {
    const hidden = draft.hidden_columns ?? [];
    draft.hidden_columns = on ? hidden.filter((c) => c !== id) : [...hidden, id];
  }

  function save(e: Event) {
    e.preventDefault();
    if (!draft.title.trim()) draft.title = meta.name;
    onsave($state.snapshot(draft));
  }
</script>

<Modal title="Customize {meta.name}" {onclose} wide>
  <form onsubmit={save}>
    <div class="range">
      <label>
        Date range
        <select value={draft.range.preset} onchange={(e) => setPreset(e.currentTarget.value)}>
          {#each presetGroups(draft.kind, draft.range.preset) as group, i (i)}
            {#if i > 0}<hr />{/if}
            {#each group as [v, label] (v)}<option value={v}>{label}</option>{/each}
          {/each}
        </select>
      </label>
      {#if PERIOD_PRESETS.includes(draft.range.preset) && periods.length}
        <select aria-label="Period" value={draft.range.from ?? periods[0].from} onchange={(e) => (draft.range.from = e.currentTarget.value)}>
          {#each periods as c (c.from)}<option value={c.from}>{c.label}</option>{/each}
        </select>
      {/if}
      {#if draft.range.preset === "custom"}
        <span class="dates">
          From <DatePicker label="From" value={draft.range.from ?? ""} {today} onchange={(d) => (draft.range.from = d)} />
          To <DatePicker label="To" value={draft.range.to ?? today} {today} onchange={(d) => (draft.range.to = d)} />
        </span>
      {:else if resolved}
        <span class="dates">
          {resolved.from ? `From ${displayDate(resolved.from)}` : "From the first transaction"} &nbsp; To {displayDate(resolved.to)}
        </span>
      {/if}
    </div>
    {#if error}<p class="err" role="alert">{error}</p>{/if}

    <div class="tabs" role="tablist">
      <button type="button" role="tab" aria-selected={tab === "display"} class:on={tab === "display"} onclick={() => (tab = "display")}>Display</button>
      {#each meta.tabs as t (t)}
        <button type="button" role="tab" aria-selected={tab === t} class:on={tab === t} onclick={() => ((tab = t), (contains = ""))}>{TAB_LABELS[t]}</button>
      {/each}
    </div>

    {#if tab === "display"}
      <div class="display">
        <fieldset>
          <legend>Report layout</legend>
          <label>Title <input bind:value={draft.title} maxlength="80" /></label>
          {#if meta.subtotal}
            <label>
              Subtotal by
              <select bind:value={draft.subtotal}>
                {#each SUBTOTALS as [v, label] (v)}<option value={v}>{label}</option>{/each}
              </select>
            </label>
          {/if}
          {#if meta.taxGroup}
            <label>
              Subtotal by
              <select bind:value={draft.tax_group}>
                {#each TAX_GROUPS as [v, label] (v)}<option value={v}>{label}</option>{/each}
              </select>
            </label>
          {/if}
          {#if meta.interval}
            <label>
              {draft.kind === "net_worth" ? "Interval" : "Columns by"}
              <select bind:value={draft.interval}>
                {#each INTERVALS as [v, label] (v)}<option value={v}>{label}</option>{/each}
              </select>
            </label>
          {/if}
          {#if meta.sort}
            <label>
              Sort by
              <select bind:value={draft.sort}>
                {#each SORTS as [v, label] (v)}<option value={v}>{label}</option>{/each}
              </select>
            </label>
          {/if}
          <fieldset class="show">
            <legend>Show</legend>
            <label class="check"><input type="checkbox" bind:checked={draft.cents} /> Cents (no rounding)</label>
            {#if meta.totalsOnly}<label class="check"><input type="checkbox" bind:checked={draft.totals_only} /> Totals only</label>{/if}
            <label class="check"
              ><input
                type="checkbox"
                checked={draft.totals_on_heading ?? totalsOnHeading}
                onchange={(e) => (draft.totals_on_heading = e.currentTarget.checked)}
              /> Totals on group heading</label
            >
            {#if meta.transfers}<label class="check"><input type="checkbox" bind:checked={draft.transfers} /> Transfers</label>{/if}
            {#if meta.showZero}<label class="check"><input type="checkbox" bind:checked={draft.show_zero} /> Accounts with zero balances</label>{/if}
          </fieldset>
        </fieldset>
        {#if columns.length}
          <fieldset>
            <legend>Show columns</legend>
            <ul class="list cols">
              {#each columns as c (c.id)}
                <li><label class="check"><input type="checkbox" checked={!(draft.hidden_columns ?? []).includes(c.id)} onchange={(e) => setColumn(c.id, e.currentTarget.checked)} /> {c.label}</label></li>
              {/each}
            </ul>
            <button type="button" onclick={() => (draft.hidden_columns = [])}>Reset Columns</button>
          </fieldset>
        {/if}
      </div>
    {:else}
      <div class="filter">
        <div>
          <ul class="list" aria-label={TAB_LABELS[tab]}>
            {#each shown as item, i (item.id)}
              {#if item.heading && item.heading !== shown[i - 1]?.heading}<li class="head">{item.heading}</li>{/if}
              <li>
                <label class="check" class:dim={item.hidden}>
                  <input type="checkbox" checked={checked(tab, item.id)} onchange={(e) => set(tab as FilterTab, item.id, e.currentTarget.checked)} />
                  {item.label}
                </label>
              </li>
            {:else}
              <li class="none">None.</li>
            {/each}
          </ul>
          <div class="under">
            <label class="check"><input type="checkbox" bind:checked={showHidden} /> Show hidden and closed</label>
            {#if tab === "payees" || tab === "securities" || tab === "categories"}
              <label>Contains <input bind:value={contains} size="14" /></label>
            {/if}
          </div>
        </div>
        <div class="side">
          <button type="button" onclick={() => selectAll(tab as FilterTab)}>Select All</button>
          <button type="button" onclick={() => clearAll(tab as FilterTab)}>Clear All</button>
          {#if tab === "accounts" && draft.kind === "capital_gains"}
            <p class="hint">With no change here, taxable investment accounts are included.</p>
          {/if}
        </div>
      </div>
    {/if}

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
  .range {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
    align-items: center;
  }
  .range label,
  .dates {
    display: inline-flex;
    gap: 0.4rem;
    align-items: center;
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
  .display {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 1rem;
    align-items: start;
  }
  fieldset {
    display: grid;
    gap: 0.45rem;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  fieldset label:not(.check) {
    display: grid;
    grid-template-columns: 7rem 1fr;
    align-items: center;
    gap: 0.4rem;
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
  .list.cols {
    height: auto;
    min-width: 12rem;
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
  .hint,
  .none {
    opacity: 0.75;
    margin: 0;
    font-size: var(--fs-register);
  }
  .err {
    color: var(--bad);
    margin: 0;
  }
  .buttons {
    display: flex;
    gap: 0.5rem;
  }
  .grow {
    flex: 1;
  }
</style>
