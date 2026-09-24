<script lang="ts">
  import { untrack } from "svelte";
  import { call, commands, withConfirmation } from "../api";
  import { parseDate } from "../format/date";
  import { GROUP_LABEL, GROUP_ORDER } from "../state/groups";
  import { confirmState } from "../state/confirm.svelte";
  import { dialogState } from "../state/dialogs.svelte";
  import { listsState } from "../state/lists.svelte";
  import { registerState } from "../state/register.svelte";
  import { viewState } from "../state/view.svelte";
  import type {
    Account,
    AccountFields,
    AccountType,
    AssetSubtype,
    CashMode,
    LotMethod,
    MmfMode,
    TaxTreatment,
  } from "../types/bindings";
  import { selectOnFocus } from "../ui/selectOnFocus";
  import Modal from "./Modal.svelte";

  let { account }: { account: Account | null } = $props();

  const TYPES: [AccountType, string][] = [
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
  const TAX: [TaxTreatment, string][] = [
    ["taxable", "Taxable"],
    ["tax_deferred", "Tax-deferred"],
    ["tax_exempt", "Tax-exempt"],
  ];
  const LOT: LotMethod[] = ["fifo", "specific", "average", "hifo", "min_tax"];

  // Fields are seeded once from the prop; the modal is remounted per open.
  const initial = untrack(() => account);
  let f = $state<AccountFields | null>(
    initial ? structuredClone($state.snapshot(initial)) : null,
  );
  let openingDate = $state(initial?.opening_date ?? "");
  let interest = $state(initial?.interest_rate ?? "");
  let limit = $state(initial?.credit_limit ?? "");
  let revealed = $state(false);
  let masked = $state("");
  let error = $state<string | null>(null);
  let busy = $state(false);

  const isNew = initial === null;
  const today = $derived(listsState.today);

  $effect(() => {
    if (isNew && f === null) void pickType("checking");
  });

  $effect(() => {
    const n = f?.account_number ?? "";
    void commands.accountNumberMasked(n).then((m) => (masked = m));
  });

  async function pickType(t: AccountType) {
    const d = await commands.accountDefaults(f?.name ?? "", t);
    // Keep what the user already typed; take the type's structure.
    f = f ? { ...d, ...keepTyped(f) } : d;
  }

  function keepTyped(x: AccountFields) {
    return {
      name: x.name,
      description: x.description,
      institution: x.institution,
      account_number: x.account_number,
      contact_phone: x.contact_phone,
      home_url: x.home_url,
      notes: x.notes,
      sort_order: x.sort_order,
    };
  }

  const rateType = $derived(
    f?.account_type === "checking" ||
      f?.account_type === "savings" ||
      f?.account_type === "money_market",
  );

  async function save(e: Event) {
    e.preventDefault();
    if (!f) return;
    error = null;
    let od: string | null = null;
    if (openingDate.trim()) {
      od = parseDate(openingDate, today);
      if (od === null) {
        error = "Opening date is not a valid date.";
        return;
      }
    }
    const fields: AccountFields = {
      ...$state.snapshot(f),
      opening_date: od,
      interest_rate: rateType && interest.trim() ? interest.trim() : null,
      credit_limit:
        f.account_type === "credit_card" && limit.trim() ? limit.trim() : null,
    };
    busy = true;
    try {
      const saved = account
        ? await call(commands.accountUpdate(account.id, fields))
        : await call(commands.accountCreate(fields));
      await listsState.loadAll();
      dialogState.closeAccount();
      if (isNew) {
        viewState.navigate("account");
        await registerState.open(saved.id);
      } else if (registerState.accountId === saved.id) {
        await registerState.refresh();
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function run(action: () => Promise<unknown>) {
    error = null;
    try {
      await action();
      await listsState.loadAll();
      dialogState.closeAccount();
      if (registerState.accountId !== null) await registerState.refresh();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  const closeAccount = () =>
    run(() =>
      withConfirmation(
        (c) => commands.accountClose(account!.id, today, c),
        confirmState.ask,
      ),
    );
  const reopen = () => run(() => call(commands.accountReopen(account!.id)));
  async function remove() {
    if (!(await confirmState.ask(`Delete account "${account!.name}"?`))) return;
    await run(async () => {
      await call(commands.accountDelete(account!.id));
      if (registerState.accountId === account!.id) {
        registerState.close();
        viewState.navigate("dashboard");
      }
    });
  }
</script>

<Modal
  title={isNew ? "New account" : `Edit account: ${account?.name}`}
  onclose={() => dialogState.closeAccount()}
>
  {#if f}
    <form onsubmit={save}>
      <label>Name <input bind:value={f.name} required /></label>
      <label>
        Type
        <select
          value={f.account_type}
          disabled={!isNew}
          onchange={(e) => pickType(e.currentTarget.value as AccountType)}
        >
          {#each TYPES as [v, label] (v)}<option value={v}>{label}</option>{/each}
        </select>
      </label>
      <label>
        Group
        <select bind:value={f.group}>
          {#each GROUP_ORDER as g (g)}<option value={g}>{GROUP_LABEL[g]}</option>{/each}
        </select>
      </label>
      <label>
        Tax treatment
        <select bind:value={f.tax_treatment}>
          {#each TAX as [v, label] (v)}<option value={v}>{label}</option>{/each}
        </select>
      </label>
      <label>Opening date <input bind:value={openingDate} placeholder="MM/DD/YYYY" /></label>
      <label>Institution <input bind:value={f.institution} /></label>
      <label>
        Account number
        <span class="inline">
          {#if revealed}
            <input bind:value={f.account_number} />
          {:else}
            <input value={masked} readonly aria-label="Masked account number" />
          {/if}
          <button type="button" onclick={() => (revealed = !revealed)}>
            {revealed ? "Hide" : "Reveal"}
          </button>
        </span>
      </label>
      <label>Phone <input bind:value={f.contact_phone} /></label>
      <label>Web address <input bind:value={f.home_url} /></label>
      <label>Description <input bind:value={f.description} /></label>

      {#if rateType}
        <label>Interest rate (%) <input bind:value={interest} inputmode="decimal" use:selectOnFocus /></label>
      {/if}
      {#if f.account_type === "credit_card"}
        <label>Credit limit <input bind:value={limit} inputmode="decimal" use:selectOnFocus /></label>
      {/if}

      {#if f.investment}
        <fieldset>
          <legend>Investment settings</legend>
          <label>Subtype <input value={f.investment.subtype ?? ""} oninput={(e) => (f!.investment!.subtype = e.currentTarget.value || null)} /></label>
          <label>
            Cash
            <select bind:value={f.investment.cash_mode}>
              {#each ["internal", "linked"] as CashMode[] as m (m)}<option value={m}>{m}</option>{/each}
            </select>
          </label>
          {#if f.investment.cash_mode === "linked"}
            <label>
              Linked cash account
              <select
                value={f.investment.linked_cash_account ?? ""}
                onchange={(e) => (f!.investment!.linked_cash_account = e.currentTarget.value ? Number(e.currentTarget.value) : null)}
              >
                <option value="">—</option>
                {#each listsState.accounts.filter((a) => a.group === "banking" && a.id !== account?.id) as a (a.id)}
                  <option value={a.id}>{a.name}</option>
                {/each}
              </select>
            </label>
          {/if}
          <label>
            Money market funds
            <select bind:value={f.investment.mmf_mode}>
              {#each ["cash", "security"] as MmfMode[] as m (m)}<option value={m}>{m}</option>{/each}
            </select>
          </label>
          <label>
            Default lot method
            <select bind:value={f.investment.default_lot_method}>
              {#each LOT as m (m)}<option value={m}>{m}</option>{/each}
            </select>
          </label>
        </fieldset>
      {/if}

      {#if f.other_asset}
        <fieldset>
          <legend>Other asset</legend>
          <label>
            Kind
            <select bind:value={f.other_asset.subtype}>
              {#each ["house", "vehicle", "other"] as AssetSubtype[] as m (m)}<option value={m}>{m}</option>{/each}
            </select>
          </label>
          <label>
            Linked liability
            <select
              value={f.other_asset.linked_liability ?? ""}
              onchange={(e) => (f!.other_asset!.linked_liability = e.currentTarget.value ? Number(e.currentTarget.value) : null)}
            >
              <option value="">—</option>
              {#each listsState.accounts.filter((a) => a.group === "liabilities" || a.group === "credit") as a (a.id)}
                <option value={a.id}>{a.name}</option>
              {/each}
            </select>
          </label>
        </fieldset>
      {/if}

      <label>Sort order <input type="number" bind:value={f.sort_order} /></label>
      <label class="check"><input type="checkbox" bind:checked={f.show_in_list} /> Show in account list</label>
      <label class="check"><input type="checkbox" bind:checked={f.show_in_bar} /> Show in icon bar</label>
      <label>Notes <textarea rows="2" bind:value={f.notes}></textarea></label>

      {#if error}<p class="err" role="alert">{error}</p>{/if}

      <div class="row">
        {#if account}
          {#if account.status === "open"}
            <button type="button" onclick={closeAccount}>Close account</button>
          {:else}
            <button type="button" onclick={reopen}>Reopen</button>
          {/if}
          <button type="button" onclick={remove}>Delete</button>
        {/if}
        <span class="spacer"></span>
        <button type="submit" disabled={busy}>Save</button>
        <button type="button" onclick={() => dialogState.closeAccount()}>Cancel</button>
      </div>
    </form>
  {/if}
</Modal>

<style>
  form {
    display: grid;
    gap: 0.5rem;
  }
  label {
    display: grid;
    grid-template-columns: 9rem 1fr;
    align-items: center;
    gap: 0.5rem;
  }
  label.check {
    display: block;
  }
  .inline {
    display: flex;
    gap: 0.25rem;
  }
  .inline input {
    flex: 1;
  }
  fieldset {
    display: grid;
    gap: 0.5rem;
  }
  .row {
    display: flex;
    gap: 0.5rem;
  }
  .spacer {
    flex: 1;
  }
  .err {
    color: var(--bad, #a83200);
    margin: 0;
  }
</style>
