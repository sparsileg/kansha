<script lang="ts">
  // The report view (RPT-010 … RPT-050): the open report's heading, graph,
  // and table, with Customize, Save, CSV export, and Print. Figures open
  // the transactions behind them (RPT-030).
  import CustomizeReportModal from "../lib/components/reports/CustomizeReportModal.svelte";
  import ReportChart from "../lib/components/reports/ReportChart.svelte";
  import ReportTable from "../lib/components/reports/ReportTable.svelte";
  import SavedReportsModal from "../lib/components/reports/SavedReportsModal.svelte";
  import Modal from "../lib/components/Modal.svelte";
  import { commands } from "../lib/api";
  import { displayDate } from "../lib/format/date";
  import { PRESETS, REPORTS, presetLabel } from "../lib/reports/meta";
  import type { Line } from "../lib/reports/rows";
  import { openAccount } from "../lib/shell/nav";
  import { listsState } from "../lib/state/lists.svelte";
  import { registerState } from "../lib/state/register.svelte";
  import { reportState as st } from "../lib/state/reports.svelte";
  import { viewState } from "../lib/state/view.svelte";
  import type { CategoryId, Column, DatePreset, ReportKind, SavedReport } from "../lib/types/bindings";

  let customizing = $state(false);
  let saving = $state<null | "save" | "as">(null);
  let saveName = $state("");
  let saveError = $state<string | null>(null);

  const report = $derived(st.report);
  const heading = $derived.by(() => {
    if (!st.settings) return "";
    const p = st.settings.range.preset;
    return p === "custom" ? st.settings.title : `${st.settings.title} - ${presetLabel(p)}`;
  });
  const dates = $derived.by(() => {
    if (!report) return "";
    if (report.as_of) return `As of ${displayDate(report.to)}`;
    return report.from
      ? `${displayDate(report.from)} through ${displayDate(report.to)}`
      : `Through ${displayDate(report.to)}`;
  });

  async function quickRange(preset: string) {
    if (!st.settings) return;
    await st.apply({ ...$state.snapshot(st.settings), range: { preset: preset as DatePreset, from: null, to: null } });
  }

  function startSave(mode: "save" | "as") {
    saveName = mode === "save" && st.saved ? st.saved.name : (st.settings?.title ?? "");
    saveError = null;
    saving = mode;
  }

  async function doSave(e: Event) {
    e.preventDefault();
    try {
      await st.save(saveName, saving === "as");
      saving = null;
    } catch (err) {
      saveError = err instanceof Error ? err.message : String(err);
    }
  }

  async function exportCsv() {
    try {
      await st.exportCsv();
    } catch (e) {
      st.error = e instanceof Error ? e.message : String(e);
    }
  }

  /** A category and every category under it. */
  function withSubcategories(id: CategoryId): CategoryId[] {
    const out = [id];
    for (let i = 0; i < out.length; i++) {
      for (const c of listsState.categories) if (c.parent === out[i]) out.push(c.id);
    }
    return out;
  }

  async function drill(line: Line, column: Column | null) {
    const d = line.drill;
    if (!d || !report || !st.settings) return;
    switch (d.kind) {
      case "txn":
        viewState.navigate("account");
        await registerState.goToTransaction(d.account, d.txn, d.date);
        break;
      case "account":
        await openAccount(d.account);
        if (column?.to) await registerState.setFilters({ date_from: null, date_to: column.to });
        break;
      case "category": {
        const from = column?.from ?? report.from;
        const to = column?.to ?? report.to;
        const base = $state.snapshot(st.settings);
        const s = await commands.reportDefaults("itemized_categories");
        await st.openWith({
          ...s,
          range: { preset: "custom", from, to },
          accounts: base.accounts,
          payees: base.payees,
          tags: base.tags,
          categories: withSubcategories(d.category),
          title: `Itemized Categories: ${listsState.categoryPath(d.category)}`,
        });
        break;
      }
    }
  }

  async function openSaved(r: SavedReport) {
    st.savedOpen = false;
    await st.openSaved(r);
  }

  const ALL: ReportKind[] = [
    "capital_gains",
    "net_worth",
    "itemized_categories",
    "itemized_payees",
    "income_expense",
    "tax_schedule",
    "tax_summary",
  ];
</script>

<section class="reports">
  {#if st.settings}
    <div class="bar no-print">
      <label>
        Date range
        <select value={st.settings.range.preset} onchange={(e) => quickRange(e.currentTarget.value)}>
          {#each PRESETS as [v, label] (v)}{#if v !== "custom" || st.settings.range.preset === "custom"}<option value={v}>{label}</option>{/if}{/each}
        </select>
      </label>
      <button type="button" onclick={() => (customizing = true)}>Customize…</button>
      <button type="button" onclick={() => startSave("save")}>{st.saved ? "Save" : "Save…"}</button>
      {#if st.saved}<button type="button" onclick={() => startSave("as")}>Save As…</button>{/if}
      <span class="sep"></span>
      <button type="button" onclick={() => st.expandAll()}>Expand All</button>
      <button type="button" onclick={() => st.collapseAll()}>Collapse All</button>
      <span class="sep"></span>
      <button type="button" onclick={exportCsv} disabled={!report}>Export CSV</button>
      <button type="button" onclick={() => window.print()} disabled={!report}>Print…</button>
    </div>
    {#if st.exported}<p class="note no-print">Saved to {st.exported}</p>{/if}
    {#if st.error}<p class="err" role="alert">{st.error}</p>{/if}

    <div class="page">
      <header class="title">
        <h1>{heading}</h1>
        {#if report}
          {#if report.note}<p>{report.note}</p>{/if}
          <p>{dates}</p>
        {/if}
        <p class="today">{displayDate(listsState.today)}</p>
      </header>
      {#if st.loading && !report}<p class="note">Building the report…</p>{/if}
      {#if report}
        {#if report.chart}<ReportChart chart={report.chart} />{/if}
        <div class="table">
          <ReportTable {report} ondrill={drill} />
        </div>
      {/if}
    </div>
  {:else}
    <h1>Reports</h1>
    <p>Choose a report:</p>
    <ul class="pick">
      {#each ALL as k (k)}<li><button type="button" onclick={() => st.open(k)}>{REPORTS[k].name}</button></li>{/each}
      <li><button type="button" onclick={() => (st.savedOpen = true)}>Saved Reports…</button></li>
    </ul>
  {/if}
</section>

{#if customizing && st.settings}
  <CustomizeReportModal
    settings={st.settings}
    onclose={() => (customizing = false)}
    onsave={(s) => {
      customizing = false;
      void st.apply(s);
    }}
  />
{/if}
{#if st.savedOpen}
  <SavedReportsModal onopen={openSaved} onclose={() => (st.savedOpen = false)} />
{/if}
{#if saving}
  <Modal title={saving === "as" ? "Save report as" : "Save report"} onclose={() => (saving = null)}>
    <form class="save" onsubmit={doSave}>
      <label>Name <input bind:value={saveName} maxlength="80" required /></label>
      {#if saveError}<p class="err" role="alert">{saveError}</p>{/if}
      <div class="buttons">
        <span class="grow"></span>
        <button type="button" onclick={() => (saving = null)}>Cancel</button>
        <button type="submit">Save</button>
      </div>
    </form>
  </Modal>
{/if}

<style>
  .reports {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
    margin-bottom: 0.5rem;
  }
  .bar label {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
  }
  .sep {
    width: 0.75rem;
  }
  .page {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .title {
    text-align: center;
    position: relative;
    margin-bottom: 0.5rem;
  }
  .title h1 {
    font-size: 1.25em;
    margin: 0;
  }
  .title p {
    margin: 0.1rem 0;
  }
  .today {
    position: absolute;
    left: 0;
    top: 0;
    font-size: 0.85em;
    opacity: 0.8;
  }
  .table {
    overflow-x: auto;
  }
  .note {
    opacity: 0.8;
    margin: 0.2rem 0;
  }
  .err {
    color: var(--bad);
  }
  .pick {
    list-style: none;
    padding: 0;
    display: grid;
    gap: 0.35rem;
    max-width: 20rem;
  }
  .pick button {
    width: 100%;
    text-align: left;
  }
  .save {
    display: grid;
    gap: 0.5rem;
  }
  .save label {
    display: grid;
    gap: 0.15rem;
  }
  .buttons {
    display: flex;
    gap: 0.5rem;
  }
  .grow {
    flex: 1;
  }
  @media print {
    .page {
      overflow: visible;
    }
    .table {
      overflow: visible;
    }
  }
</style>
