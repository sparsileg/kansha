<script lang="ts">
  // A report window (RPT-010 … RPT-050): toolbar, then the report as a
  // printed page (white, whatever the theme): heading, graph, and table.
  // Figures open the transactions behind them (RPT-030); a transaction
  // opens its register and the report waits in the dock. Showing the
  // window again rebuilds the report, so it is never stale.
  import { onMount, tick } from "svelte";
  import CustomizeReportModal from "./CustomizeReportModal.svelte";
  import ReportChart from "./ReportChart.svelte";
  import ReportTable from "./ReportTable.svelte";
  import Modal from "../Modal.svelte";
  import DatePicker from "../invest/DatePicker.svelte";
  import { commands } from "../../api";
  import { displayDate } from "../../format/date";
  import { INTERVALS, PRESETS, SORTABLE, SORTS, SUBTOTALS } from "../../reports/meta";
  import type { Line } from "../../reports/rows";
  import { openAccount } from "../../shell/nav";
  import { investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import { registerState } from "../../state/register.svelte";
  import { reportState, type ReportInstance } from "../../state/reports.svelte";
  import { viewState } from "../../state/view.svelte";
  import { windowState } from "../../state/windows.svelte";
  import type {
    CategoryId,
    Column,
    DatePreset,
    DetailSort,
    Interval,
    ReportSettings,
    Subtotal,
  } from "../../types/bindings";

  let { inst }: { inst: ReportInstance } = $props();

  let customizing = $state(false);
  let table = $state<ReturnType<typeof ReportTable>>();
  /** The Save PDF dialog is open. */
  let pdfOpen = $state(false);
  let saving = $state<null | "save" | "as">(null);
  let saveName = $state("");
  let saveError = $state<string | null>(null);
  /** The Custom dates dialog's fields (ISO; empty From is the first
   * transaction). */
  let custom = $state<null | { from: string; to: string }>(null);
  let customError = $state<string | null>(null);

  onMount(() => void inst.run());

  // Closing asked to save a report that has no name yet.
  $effect(() => {
    if (inst.saveOnClose && !saving) startSave("save");
  });

  const st = $derived(inst.settings);
  const report = $derived(inst.report);
  /** Sortable column headings: itemized reports only. */
  const tableSort = $derived(
    SORTABLE.includes(st.kind) ? { by: st.sort ?? "date", desc: st.sort_desc ?? false } : null,
  );
  const dates = $derived.by(() => {
    if (!report) return "";
    if (report.as_of) return `As of ${displayDate(report.to)}`;
    return report.from
      ? `${displayDate(report.from)} through ${displayDate(report.to)}`
      : `Through ${displayDate(report.to)}`;
  });

  /** Apply one toolbar change to the report. */
  async function change(patch: Partial<ReportSettings>) {
    await inst.apply({ ...$state.snapshot(inst.settings), ...patch });
  }

  async function quickRange(select: HTMLSelectElement) {
    if (select.value === "custom") {
      // The select shows the current range until the dialog is confirmed.
      select.value = st.range.preset;
      openCustom();
      return;
    }
    await change({ range: { preset: select.value as DatePreset, from: null, to: null } });
  }

  function openCustom() {
    customError = null;
    custom = { from: report?.from ?? "", to: report?.to ?? listsState.today };
  }

  async function applyCustom(e: Event) {
    e.preventDefault();
    if (!custom) return;
    if (!custom.to) {
      customError = "Enter the To date.";
      return;
    }
    if (custom.from && custom.from > custom.to) {
      customError = "From must be on or before To.";
      return;
    }
    const range = { preset: "custom" as DatePreset, from: custom.from || null, to: custom.to };
    custom = null;
    await change({ range });
  }

  /** A sortable heading was clicked: sort on it, or reverse its order. */
  async function sortBy(by: DetailSort) {
    await change(st.sort === by ? { sort_desc: !st.sort_desc } : { sort: by, sort_desc: false });
  }

  function startSave(mode: "save" | "as") {
    saveName = mode === "save" && inst.saved ? inst.saved.name : st.title;
    saveError = null;
    saving = mode;
  }

  function cancelSave() {
    saving = null;
    inst.saveOnClose = false;
  }

  async function doSave(e: Event) {
    e.preventDefault();
    try {
      await inst.save(saveName, saving === "as");
      saving = null;
      if (inst.saveOnClose) {
        inst.saveOnClose = false;
        await windowState.close(inst.id);
      }
    } catch (err) {
      saveError = err instanceof Error ? err.message : String(err);
    }
  }

  async function exportCsv() {
    try {
      await inst.exportCsv();
    } catch (e) {
      inst.error = e instanceof Error ? e.message : String(e);
    }
  }

  /** Close the Save PDF dialog first so it is not on the page, split a
   * compact report into pages, then save. */
  async function savePdf() {
    pdfOpen = false;
    await tick();
    try {
      await table?.preparePrint();
      await inst.savePdf();
    } catch (e) {
      inst.error = e instanceof Error ? e.message : String(e);
    } finally {
      table?.endPrint();
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
    if (!d || !report) return;
    const from = column?.from ?? report.from;
    const to = column?.to ?? report.to;
    const base = $state.snapshot(inst.settings);
    switch (d.kind) {
      case "txn":
        viewState.navigate("account");
        await registerState.goToTransaction(d.account, d.txn);
        break;
      case "account":
        await openAccount(d.account);
        if (column?.to) await registerState.setFilters({ date_from: null, date_to: column.to });
        break;
      case "category": {
        const s = await commands.reportDefaults("itemized_categories");
        await reportState.openWith({
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
      case "asset_class": {
        // The Holdings report, limited to that class's securities.
        if (investState.securities.length === 0) await investState.loadSecurities();
        const s = await commands.reportDefaults("holdings");
        await reportState.openWith({
          ...s,
          range: { preset: "custom", from: null, to: report.to },
          accounts: base.accounts,
          securities: investState.securities.filter((x) => x.asset_class === d.asset_class).map((x) => x.id),
          title: `Holdings: ${line.label}`,
        });
        break;
      }
      case "payee": {
        const s = await commands.reportDefaults("itemized_payees");
        await reportState.openWith({
          ...s,
          range: { preset: "custom", from, to },
          accounts: base.accounts,
          categories: base.categories,
          tags: base.tags,
          payees: d.payee === null ? base.payees : [d.payee],
          transfers: false,
          title: `Itemized Payees: ${line.label}`,
        });
        break;
      }
    }
  }

  // Fold icons (16x16): chevron up to hide, down to open.
  const UP = "M4 10l4-4 4 4";
  const DOWN = "M4 6l4 4 4-4";
</script>

<section class="reports">
  <div class="bar no-print">
    <label>
      Date range
      <select value={st.range.preset} onchange={(e) => quickRange(e.currentTarget)}>
        {#each PRESETS as [v, label] (v)}<option value={v}>{v === "custom" ? `${label}…` : label}</option>{/each}
      </select>
    </label>
    {#if st.range.preset === "custom"}<button type="button" onclick={openCustom}>Change Dates…</button>{/if}
    {#if SORTABLE.includes(st.kind)}
      <label>
        Sort by:
        <select value={st.sort} onchange={(e) => change({ sort: e.currentTarget.value as DetailSort, sort_desc: false })}>
          {#each SORTS as [v, label] (v)}<option value={v}>{label}</option>{/each}
          {#if st.sort === "num"}<option value="num">Num</option>{/if}
        </select>
      </label>
    {/if}
    {#if st.kind === "capital_gains"}
      <label>
        Subtotal by:
        <select value={st.subtotal} onchange={(e) => change({ subtotal: e.currentTarget.value as Subtotal })}>
          {#each SUBTOTALS as [v, label] (v)}<option value={v}>{label}</option>{/each}
        </select>
      </label>
    {/if}
    {#if st.kind === "income_expense" || st.kind === "income_expense_payee"}
      <label>
        Interval:
        <select value={st.interval} onchange={(e) => change({ interval: e.currentTarget.value as Interval })}>
          {#each INTERVALS as [v, label] (v)}<option value={v}>{label}</option>{/each}
        </select>
      </label>
    {/if}
    <button type="button" onclick={() => (customizing = true)}>Customize…</button>
    <button type="button" onclick={() => startSave("save")}>{inst.saved ? "Save" : "Save…"}</button>
    {#if inst.saved}<button type="button" onclick={() => startSave("as")}>Save As…</button>{/if}
    <span class="sep"></span>
    <button type="button" onclick={() => inst.expandAll()}>Expand All</button>
    <button type="button" onclick={() => inst.collapseAll()}>Collapse All</button>
    <span class="sep"></span>
    <button type="button" onclick={exportCsv} disabled={!report}>Export CSV</button>
    <button type="button" onclick={() => (pdfOpen = true)} disabled={!report}>Save PDF…</button>
  </div>
  {#if inst.error}<p class="err" role="alert">{inst.error}</p>{/if}

  <div class="page" data-theme="light">
    <header class="title">
      <h1>{inst.heading}</h1>
      {#if report}
        {#if report.note}<p>{report.note}</p>{/if}
        <p>{dates}</p>
      {/if}
      <p class="today">{displayDate(listsState.today)}</p>
    </header>
    {#if inst.loading && !report}<p class="note">Building the report…</p>{/if}
    {#if report}
      {#if report.chart}
        <!-- The graph is for the screen; paper gets the table only. -->
        <div class="part no-print">
          <div class="fold no-print">
            <button type="button" aria-expanded={!inst.hideGraph} onclick={() => (inst.hideGraph = !inst.hideGraph)}>
              <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d={inst.hideGraph ? DOWN : UP} /></svg>
              {inst.hideGraph ? "Open Graph" : "Hide Graph"}
            </button>
          </div>
          {#if !inst.hideGraph}<ReportChart chart={report.chart} />{/if}
        </div>
        <hr class="split no-print" />
        <div class="fold no-print">
          <button type="button" aria-expanded={!inst.hideTable} onclick={() => (inst.hideTable = !inst.hideTable)}>
            <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d={inst.hideTable ? DOWN : UP} /></svg>
            {inst.hideTable ? "Open Report" : "Hide Report"}
          </button>
        </div>
      {/if}
      {#if !(report.chart && inst.hideTable)}
        <ReportTable bind:this={table} {report} {inst} ondrill={drill} sort={tableSort} onsort={sortBy} orientation={inst.orientation} />
      {/if}
    {/if}
  </div>
</section>

{#if customizing}
  <CustomizeReportModal
    settings={inst.settings}
    totalsOnHeading={report?.totals_on_heading ?? false}
    onclose={() => (customizing = false)}
    onsave={(s) => {
      customizing = false;
      void inst.apply(s);
    }}
  />
{/if}
{#if custom}
  <Modal title="Custom dates" onclose={() => (custom = null)}>
    <form class="save custom" onsubmit={applyCustom}>
      <div class="dates">
        <span>From:</span>
        <DatePicker label="From" value={custom.from} today={listsState.today} onchange={(d) => custom && (custom.from = d)} />
        <span>To:</span>
        <DatePicker label="To" value={custom.to} today={listsState.today} onchange={(d) => custom && (custom.to = d)} />
      </div>
      {#if customError}<p class="err" role="alert">{customError}</p>{/if}
      <div class="buttons">
        <span class="grow"></span>
        <button type="button" onclick={() => (custom = null)}>Cancel</button>
        <button type="submit">OK</button>
      </div>
    </form>
  </Modal>
{/if}
{#if pdfOpen}
  <Modal title="Save PDF" onclose={() => (pdfOpen = false)}>
    <div class="save">
      <fieldset class="orient">
        <legend>Orientation</legend>
        <label><input type="radio" bind:group={inst.orientation} value="portrait" /> Portrait</label>
        <label><input type="radio" bind:group={inst.orientation} value="landscape" /> Landscape</label>
      </fieldset>
      <div class="buttons">
        <span class="grow"></span>
        <button type="button" onclick={() => (pdfOpen = false)}>Cancel</button>
        <button type="button" onclick={savePdf}>Save PDF</button>
      </div>
    </div>
  </Modal>
{/if}
{#if saving}
  <Modal title={saving === "as" ? "Save report as" : "Save report"} onclose={cancelSave}>
    <form class="save" onsubmit={doSave}>
      <label>Name <input bind:value={saveName} maxlength="80" required /></label>
      {#if saveError}<p class="err" role="alert">{saveError}</p>{/if}
      <div class="buttons">
        <span class="grow"></span>
        <button type="button" onclick={cancelSave}>Cancel</button>
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
  /* The report looks like the printed page in every theme: the page
     takes the light theme (data-theme="light"), for white paper, dark
     ink, and its graph colors. The page scrolls, so the table's heading
     row can stay at its top. */
  .page {
    flex: 1;
    min-height: 0;
    overflow: auto;
    /* No top padding: the sticky heading row meets the page's top edge. */
    padding: 0 1rem 0.75rem;
    background: var(--bg);
    color: var(--fg);
    color-scheme: var(--color-scheme);
    border: 1px solid var(--line);
  }
  .title {
    text-align: center;
    position: relative;
    padding-top: 0.75rem;
    margin-bottom: 0.5rem;
  }
  .title h1 {
    font-size: var(--fs-heading);
    margin: 0;
  }
  .title p {
    margin: 0.1rem 0;
  }
  .today {
    position: absolute;
    left: 0;
    top: 0.75rem;
    font-size: var(--fs-register);
    opacity: 0.8;
  }
  .part {
    position: relative;
  }
  .fold {
    display: flex;
    justify-content: flex-end;
  }
  .fold button {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }
  .fold svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
  }
  .split {
    border: 0;
    border-top: 1px solid var(--line);
    margin: 0.5rem 0;
  }
  .note {
    opacity: 0.8;
    margin: 0.2rem 0;
  }
  .err {
    color: var(--bad);
  }
  .save {
    display: grid;
    gap: 0.5rem;
  }
  .save label {
    display: grid;
    gap: 0.15rem;
  }
  .orient {
    display: flex;
    gap: 1rem;
    border: 0;
    padding: 0;
    margin: 0;
  }
  .orient label {
    display: inline-flex;
    gap: 0.3rem;
    align-items: center;
  }
  /* Room below the To field for its calendar, which the dialog would
     otherwise cut off; the buttons sit at the bottom. */
  .custom {
    min-height: 20rem;
    align-content: space-between;
  }
  .dates {
    display: grid;
    grid-template-columns: auto auto;
    gap: 0.4rem 0.5rem;
    align-items: center;
    justify-content: start;
  }
  .buttons {
    display: flex;
    gap: 0.5rem;
  }
  .grow {
    flex: 1;
  }
  /* Paper: a smaller fixed size, whatever the screen setting, so more
     columns fit on a page. */
  @media print {
    /* Page breaks only work outside flex layout. */
    .reports {
      display: block;
    }
    .page {
      overflow: visible;
      border: 0;
      padding: 0;
      font-size: 9pt;
    }
    .title {
      padding-top: 0;
    }
    .title h1 {
      font-size: 12pt;
    }
    .today {
      top: 0;
    }
  }
</style>
