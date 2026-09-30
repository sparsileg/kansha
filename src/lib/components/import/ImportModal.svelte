<script lang="ts">
  // File > Import… (MIG-040 … MIG-100): choose a Quicken QIF file, see
  // what it holds, map accounts, categories, and securities, try the
  // import, then import it as one batch. Past imports can be rolled back.
  // Every figure comes from Rust; this only shows it and collects the
  // mapping.
  import { SECURITY_TYPES } from "../../invest/securityTypes";
  import Modal from "../Modal.svelte";
  import { call, commands } from "../../api";
  import { displayDate } from "../../format/date";
  import { formatMoney } from "../../format/money";
  import { confirmState } from "../../state/confirm.svelte";
  import { investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import { scheduleState } from "../../state/schedule.svelte";
  import type {
    AccountChoice,
    AccountPreview,
    AccountType,
    CategoryChoice,
    CategoryKind,
    CategoryPreview,
    DateOrder,
    ImportBatch,
    ImportOptions,
    ImportPreview,
    ImportResult,
    SecurityChoice,
    SecurityPreview,
    SecurityType,
  } from "../../types/bindings";

  let { onclose }: { onclose: () => void } = $props();

  const ACCOUNT_TYPES: [AccountType, string][] = [
    ["checking", "Checking"],
    ["savings", "Savings"],
    ["money_market", "Money market"],
    ["cash", "Cash"],
    ["credit_card", "Credit card"],
    ["brokerage", "Brokerage"],
    ["traditional_ira", "Traditional IRA"],
    ["roth_ira", "Roth IRA"],
    ["hsa", "HSA"],
    ["retirement_401k", "401(k)/403(b)"],
    ["other_asset", "Other asset"],
    ["other_liability", "Other liability"],
    ["loan", "Loan"],
  ];

  let options = $state<ImportOptions>({
    date_order: null,
    accounts: {},
    categories: {},
    securities: {},
    keep_categories: [],
    keep_tags: [],
    keep_securities: [],
    show_securities: [],
    prices: true,
    skip_errors: false,
  });
  let preview = $state<ImportPreview | null>(null);
  let result = $state<ImportResult | null>(null);
  let batches = $state<ImportBatch[] | null>(null);
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let showUnused = $state(false);

  const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

  async function work<T>(label: string, f: () => Promise<T>): Promise<T | undefined> {
    busy = label;
    error = null;
    try {
      return await f();
    } catch (e) {
      error = message(e);
      return undefined;
    } finally {
      busy = null;
    }
  }

  async function choose() {
    const path = await commands.pickImportFile(null);
    if (path === null) return;
    result = null;
    await work("Reading…", async () => {
      preview = await call(commands.importOpen(path));
    });
  }

  async function refresh() {
    result = null;
    await work("Checking…", async () => {
      preview = await call(commands.importPreview(options));
    });
  }

  async function run(dryRun: boolean) {
    const r = await work(dryRun ? "Testing…" : "Importing…", () => call(commands.importRun(options, dryRun)));
    if (!r) return;
    result = r;
    if (r.committed) {
      preview = null;
      await Promise.all([listsState.loadAll(), investState.loadSecurities(), scheduleState.changed()]);
    }
  }

  function cancel() {
    if (preview) void commands.importCancel();
    onclose();
  }

  // --- Mapping -------------------------------------------------------

  function setAccount(a: AccountPreview, choice: AccountChoice) {
    options.accounts[a.name] = choice;
    void refresh();
  }

  function accountModeChanged(a: AccountPreview, mode: string) {
    if (mode === "skip") setAccount(a, { kind: "skip" });
    else if (mode === "create") setAccount(a, { kind: "create", name: a.name, account_type: a.default_type });
    else {
      const first = listsState.accounts.find((x) => x.status === "open");
      if (first) setAccount(a, { kind: "existing", id: first.id });
    }
  }

  function setCategory(c: CategoryPreview, choice: CategoryChoice) {
    options.categories[c.name] = choice;
    void refresh();
  }

  function categoryModeChanged(c: CategoryPreview, mode: string) {
    if (mode === "create") {
      setCategory(c, { kind: "create", path: c.name || "Uncategorized", category_kind: c.kind });
    } else {
      const first = listsState.categories.find((x) => x.kind === c.kind);
      if (first) setCategory(c, { kind: "existing", id: first.id });
    }
  }

  function setSecurity(s: SecurityPreview, choice: SecurityChoice) {
    options.securities[s.name] = choice;
    void refresh();
  }

  function securityModeChanged(s: SecurityPreview, mode: string) {
    if (mode === "create") {
      setSecurity(s, { kind: "create", name: s.name, ticker: s.symbol, security_type: "other" });
    } else {
      const first = investState.securities[0];
      if (first) setSecurity(s, { kind: "existing", id: first.id });
    }
  }

  function toggleKeep(list: "keep_categories" | "keep_tags" | "keep_securities", name: string, on: boolean) {
    options[list] = on ? [...options[list], name] : options[list].filter((n) => n !== name);
    void refresh();
  }

  /** A new security no account holds afterwards is created hidden; this
   * keeps it shown. Nothing in the preview depends on it. */
  function toggleShown(name: string, on: boolean) {
    options.show_securities = on
      ? [...options.show_securities, name]
      : options.show_securities.filter((n) => n !== name);
  }

  function setOrder(v: string) {
    options.date_order = v === "" ? null : (v as DateOrder);
    void refresh();
  }

  const shownCategories = $derived(preview?.categories.filter((c) => showUnused || c.imported) ?? []);
  const shownTags = $derived(preview?.tags.filter((t) => showUnused || t.imported) ?? []);
  const shownSecurities = $derived(preview?.securities.filter((s) => showUnused || s.imported) ?? []);
  const unused = $derived(
    (preview?.categories.filter((c) => !c.imported).length ?? 0) +
      (preview?.tags.filter((t) => !t.imported).length ?? 0) +
      (preview?.securities.filter((s) => !s.imported).length ?? 0),
  );
  // A mapping problem (no line) always blocks; bad records unless left out.
  const blocked = $derived(
    (preview?.errors.some((e) => e.line === null) ?? false) ||
      ((preview?.errors.length ?? 0) > 0 && !options.skip_errors),
  );
  const kindLabel = (k: CategoryKind) => (k === "income" ? "Income" : k === "expense" ? "Expense" : "Equity");
  const range = (a: string | null, b: string | null) => (a && b ? `${displayDate(a)} – ${displayDate(b)}` : "");

  // --- Past imports (MIG-080) ------------------------------------------

  async function showBatches() {
    result = null;
    await work("Loading…", async () => {
      batches = await call(commands.importBatches());
    });
  }

  async function rollBack(b: ImportBatch) {
    const ok = await confirmState.ask(
      `Roll back the import of ${b.source_file}? Its ${b.txns} transactions are deleted, with the accounts, categories, payees, tags, and securities it created that nothing else uses. The book is backed up first.`,
    );
    if (!ok) return;
    const r = await work("Rolling back…", () => call(commands.importRollback(b.id)));
    if (!r) return;
    error = null;
    batches = await call(commands.importBatches());
    rolledBack = `Rolled back: ${r.transactions} transactions deleted; ${r.removed} created records removed, ${r.kept} kept (in use).`;
    await Promise.all([listsState.loadAll(), investState.loadSecurities(), scheduleState.changed()]);
  }
  let rolledBack = $state<string | null>(null);
</script>

<Modal title="Import from Quicken" fit onclose={cancel}>
  {#if batches}
    <h3>Past imports</h3>
    {#if batches.length === 0}<p>None yet.</p>{/if}
    <div class="scroll">
      <table>
        <thead><tr><th>#</th><th>File</th><th>Status</th><th>Imported</th><th class="num">Transactions</th><th></th></tr></thead>
        <tbody>
          {#each batches as b (b.id)}
            <tr>
              <td>{b.id}</td><td>{b.source_file}</td><td>{b.status.replace("_", " ")}</td>
              <td>{b.committed_at ?? b.created_at}</td><td class="num">{b.txns}</td>
              <td>
                {#if b.status === "committed"}
                  <button type="button" onclick={() => rollBack(b)} disabled={busy !== null}>Roll back…</button>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if rolledBack}<p role="status">✓ {rolledBack}</p>{/if}
    <div class="buttons"><button type="button" onclick={() => (batches = null)}>Back</button></div>
  {:else if !preview}
    <p>
      Choose a QIF file exported from Quicken (File &gt; Export &gt; QIF File, all accounts). Nothing is imported until you
      check what the file holds and confirm.
    </p>
    <p class="row">
      <button type="button" onclick={choose} disabled={busy !== null}>Choose QIF file…</button>
      <button type="button" onclick={showBatches} disabled={busy !== null}>Past imports…</button>
    </p>
  {:else}
    <table class="facts">
      <tbody>
        <tr><th>File</th><td>{preview.file_name}</td></tr>
        <tr><th>Dates</th><td>{range(preview.first_date, preview.last_date)}</td></tr>
        <tr>
          <th>Date order</th>
          <td>
            <select aria-label="Date order" value={options.date_order ?? ""} onchange={(e) => setOrder(e.currentTarget.value)}>
              <option value="">From the file ({preview.date_order === "mdy" ? "month first" : "day first"})</option>
              <option value="mdy">Month first (1/31/26)</option>
              <option value="dmy">Day first (31/1/26)</option>
            </select>
            {#if preview.date_ambiguous}<strong> ⚠ No date shows the order; month first assumed.</strong>{/if}
          </td>
        </tr>
        <tr><th>Transactions</th><td>{preview.transactions} ({preview.transfers_matched} transfers found on both sides, imported once)</td></tr>
        <tr><th>New payees</th><td>{preview.new_payees}</td></tr>
        <tr>
          <th>Prices</th>
          <td><label><input type="checkbox" bind:checked={options.prices} onchange={refresh} /> Import {preview.prices} prices of the securities kept</label></td>
        </tr>
        <tr><th>Memorized</th><td>{preview.memorized_skipped} memorized transactions, never imported</td></tr>
      </tbody>
    </table>
    {#if preview.imported_before}
      <p role="alert"><strong>⚠ This file was imported before ({preview.imported_before}).</strong></p>
    {/if}

    <h3>Accounts</h3>
    <div class="scroll">
      <table>
        <thead>
          <tr><th>QIF account</th><th>Type</th><th class="num">Records</th><th>Dates</th><th class="num">Adds</th><th>Import as</th></tr>
        </thead>
        <tbody>
          {#each preview.accounts as a (a.name)}
            <tr class:off={a.choice.kind === "skip"}>
              <td>{a.name}{#if !a.defined} <small>(only in transfers)</small>{/if}</td>
              <td>{a.qif_type}</td>
              <td class="num">{a.records}</td>
              <td>{range(a.first_date, a.last_date)}</td>
              <td class="num">{formatMoney(a.total)}</td>
              <td class="choice">
                <select aria-label={`Import ${a.name} as`} value={a.choice.kind} onchange={(e) => accountModeChanged(a, e.currentTarget.value)}>
                  <option value="skip">Skip</option>
                  <option value="create">New account</option>
                  <option value="existing">Existing account</option>
                </select>
                {#if a.choice.kind === "create"}
                  {@const c = a.choice}
                  <input aria-label={`Name for ${a.name}`} value={c.name} onchange={(e) => setAccount(a, { ...c, name: e.currentTarget.value })} />
                  <select aria-label={`Type for ${a.name}`} value={c.account_type} onchange={(e) => setAccount(a, { ...c, account_type: e.currentTarget.value as AccountType })}>
                    {#each ACCOUNT_TYPES as [t, label] (t)}<option value={t}>{label}</option>{/each}
                  </select>
                {:else if a.choice.kind === "existing"}
                  <select aria-label={`Account for ${a.name}`} value={a.choice.id} onchange={(e) => setAccount(a, { kind: "existing", id: Number(e.currentTarget.value) })}>
                    {#each listsState.accounts as x (x.id)}<option value={x.id}>{x.name}</option>{/each}
                  </select>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <h3>Categories, tags, securities</h3>
    <label class="inline">
      <input type="checkbox" bind:checked={showUnused} /> Show the {unused} the imported transactions do not use (tick to keep)
    </label>
    <div class="scroll">
      <table>
        <thead><tr><th></th><th>QIF category</th><th>Kind</th><th class="num">Lines</th><th class="num">Total</th><th>Import as</th></tr></thead>
        <tbody>
          {#each shownCategories as c (c.name)}
            <tr class:off={!c.imported}>
              <td>
                {#if c.used === 0}
                  <input type="checkbox" aria-label={`Keep ${c.name}`} checked={c.imported} onchange={(e) => toggleKeep("keep_categories", c.name, e.currentTarget.checked)} />
                {/if}
              </td>
              <td>{c.name || "(no category)"}</td>
              <td>{kindLabel(c.kind)}</td>
              <td class="num">{c.used}</td>
              <td class="num">{formatMoney(c.total)}</td>
              <td class="choice">
                <select aria-label={`Import category ${c.name} as`} value={c.choice.kind} onchange={(e) => categoryModeChanged(c, e.currentTarget.value)}>
                  <option value="create">New or same path</option>
                  <option value="existing">Existing category</option>
                </select>
                {#if c.choice.kind === "create"}
                  {@const ch = c.choice}
                  <input aria-label={`Path for ${c.name}`} value={ch.path} onchange={(e) => setCategory(c, { ...ch, path: e.currentTarget.value })} />
                {:else}
                  <select aria-label={`Category for ${c.name}`} value={c.choice.id} onchange={(e) => setCategory(c, { kind: "existing", id: Number(e.currentTarget.value) })}>
                    {#each listsState.categories as x (x.id)}<option value={x.id}>{listsState.categoryPath(x.id)}</option>{/each}
                  </select>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if shownTags.length}
        <table>
          <thead><tr><th></th><th>Tag</th><th class="num">Uses</th><th></th></tr></thead>
          <tbody>
            {#each shownTags as t (t.name)}
              <tr class:off={!t.imported}>
                <td>
                  {#if t.used === 0}
                    <input type="checkbox" aria-label={`Keep ${t.name}`} checked={t.imported} onchange={(e) => toggleKeep("keep_tags", t.name, e.currentTarget.checked)} />
                  {/if}
                </td>
                <td>{t.name}</td><td class="num">{t.used}</td><td>{t.existing ? "already in the book" : "new"}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
      {#if shownSecurities.length}
        <table>
          <thead><tr><th></th><th>Security</th><th>Symbol</th><th class="num">Uses</th><th class="num">Prices</th><th>Import as</th></tr></thead>
          <tbody>
            {#each shownSecurities as s (s.name)}
              <tr class:off={!s.imported}>
                <td>
                  {#if s.used === 0}
                    <input type="checkbox" aria-label={`Keep ${s.name}`} checked={s.imported} onchange={(e) => toggleKeep("keep_securities", s.name, e.currentTarget.checked)} />
                  {/if}
                </td>
                <td>{s.name}</td><td>{s.symbol ?? ""}</td><td class="num">{s.used}</td><td class="num">{s.prices}</td>
                <td class="choice">
                  <select aria-label={`Import security ${s.name} as`} value={s.choice.kind} onchange={(e) => securityModeChanged(s, e.currentTarget.value)}>
                    <option value="create">New security</option>
                    <option value="existing">Existing security</option>
                  </select>
                  {#if s.choice.kind === "create"}
                    {@const ch = s.choice}
                    <input aria-label={`Ticker for ${s.name}`} value={ch.ticker ?? ""} onchange={(e) => setSecurity(s, { ...ch, ticker: e.currentTarget.value.trim() || null })} />
                    <select aria-label={`Security type for ${s.name}`} value={ch.security_type} onchange={(e) => setSecurity(s, { ...ch, security_type: e.currentTarget.value as SecurityType })}>
                      {#each SECURITY_TYPES as [t, label] (t)}<option value={t}>{label}</option>{/each}
                    </select>
                    <label class="inline" title="A new security that no account holds afterwards is created hidden; tick to keep it shown.">
                      <input type="checkbox" aria-label={`Keep ${s.name} shown`} checked={options.show_securities.includes(s.name)} onchange={(e) => toggleShown(s.name, e.currentTarget.checked)} />
                      Keep shown
                    </label>
                  {:else}
                    <select aria-label={`Security for ${s.name}`} value={s.choice.id} onchange={(e) => setSecurity(s, { kind: "existing", id: Number(e.currentTarget.value) })}>
                      {#each investState.securities as x (x.id)}<option value={x.id}>{investState.label(x.id)}</option>{/each}
                    </select>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>

    {#if preview.errors.length}
      <h3>⚠ Cannot be imported ({preview.errors.length})</h3>
      <ul class="notes bad">
        {#each preview.errors as n, i (i)}
          <li>{n.account}{n.line !== null ? `, line ${n.line}` : ""}: {n.message}</li>
        {/each}
      </ul>
      <label class="inline">
        <input type="checkbox" bind:checked={options.skip_errors} /> Import the rest and leave these out
      </label>
    {/if}
    {#if preview.warnings.length}
      <h3>Notes ({preview.warnings.length})</h3>
      <ul class="notes">
        {#each preview.warnings as n, i (i)}
          <li>{n.account}{n.line !== null ? `${n.account ? ", " : ""}line ${n.line}` : ""}{n.account || n.line !== null ? ": " : ""}{n.message}</li>
        {/each}
      </ul>
    {/if}
  {/if}

  {#if result}
    <h3>
      {result.committed
        ? `✓ Imported ${result.transactions} transactions (import #${result.batch}).`
        : result.dry_run && (result.errors.length === 0 || options.skip_errors)
          ? `Test import: ${result.transactions} transactions would be imported. Nothing was written.`
          : `Not imported: ${result.errors.length} record(s) cannot be imported. Nothing was written.`}
    </h3>
    {#if result.transactions > 0}
      <p>
        New: {result.accounts_created} accounts, {result.categories_created} categories, {result.payees_created} payees,
        {result.tags_created} tags, {result.securities_created} securities, {result.prices} prices.
        {#if result.securities_hidden > 0}
          {result.securities_hidden} of the new securities are no longer held, so they are hidden and prices are not
          downloaded for them (Tools &gt; Securities shows them).
        {/if}
      </p>
      <table>
        <thead>
          <tr><th></th><th>Account</th><th class="num">File adds</th><th class="num">Before</th><th class="num">After</th></tr>
        </thead>
        <tbody>
          {#each result.accounts as a (a.account)}
            <tr>
              <td class="mark">{a.differs ? "≠" : ""}</td>
              <td class:b={a.differs}>{a.qif_name}</td>
              <td class="num">{formatMoney(a.expected)}</td>
              <td class="num">{formatMoney(a.before)}</td>
              <td class="num" class:b={a.differs}>{formatMoney(a.after)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="note">Investment accounts show cash only. Compare with Quicken's reports (MIG-100).</p>
    {/if}
    {#if result.errors.length}
      <ul class="notes bad">
        {#each result.errors as n, i (i)}
          <li>{n.account}{n.line !== null ? `, line ${n.line}` : ""}: {n.message}</li>
        {/each}
      </ul>
    {/if}
  {/if}

  {#if error}<p class="err" role="alert">{error}</p>{/if}
  {#if busy}<p role="status">{busy}</p>{/if}
  {#if !batches}
    <div class="buttons">
      {#if preview}
        <button type="button" onclick={() => run(true)} disabled={busy !== null}>Test import</button>
        <button type="button" onclick={() => run(false)} disabled={busy !== null || blocked}>Import</button>
      {/if}
      <button type="button" onclick={cancel} disabled={busy !== null}>{result?.committed ? "Close" : "Cancel"}</button>
    </div>
  {/if}
</Modal>

<style>
  .row {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .facts th {
    text-align: right;
    font-weight: normal;
    padding-right: 0.75rem;
    opacity: 0.8;
  }
  .scroll {
    max-height: 30vh;
    overflow: auto;
  }
  table {
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    padding: 0.1rem 0.5rem;
    vertical-align: top;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  /* The mapping controls stay on one line; the dialog widens to them. */
  .choice {
    display: flex;
    gap: 0.25rem;
    flex-wrap: nowrap;
  }
  /* Prose wraps to the width the tables set, never widens the dialog. */
  p,
  h3,
  .notes {
    contain: inline-size;
  }
  .off td {
    opacity: 0.6;
  }
  h3 {
    font-size: var(--fs-register);
    margin: 0.75rem 0 0.25rem;
  }
  .notes {
    max-height: 20vh;
    overflow: auto;
    margin: 0;
    font-size: var(--fs-small);
  }
  .bad,
  .err {
    color: var(--bad);
  }
  .mark {
    width: 1.5rem;
    font-weight: bold;
  }
  .b {
    font-weight: bold;
  }
  .inline {
    display: inline-flex;
    gap: 0.25rem;
    align-items: center;
  }
  .note {
    opacity: 0.8;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.75rem;
    flex-wrap: wrap;
  }
</style>
