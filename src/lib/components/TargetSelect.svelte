<script lang="ts">
  import { SPLIT, type TargetValue } from "../register/draft";
  import { listsState } from "../state/lists.svelte";
  import type { AccountId } from "../types/bindings";

  let {
    value = $bindable(),
    excludeAccount,
    allowSplit = false,
    onchange,
    id,
    label = "Category",
  }: {
    value: TargetValue;
    excludeAccount?: AccountId;
    allowSplit?: boolean;
    onchange?: () => void;
    id?: string;
    label?: string;
  } = $props();

  const cats = $derived(
    listsState.categories.filter(
      (c) =>
        c.kind !== "equity" && (!c.hidden || value === `c:${c.id}`),
    ),
  );
  const accounts = $derived(
    listsState.accounts.filter(
      (a) =>
        a.id !== excludeAccount &&
        a.investment === null &&
        (a.status === "open" || value === `a:${a.id}`),
    ),
  );
</script>

<select {id} aria-label={label} bind:value {onchange}>
  <option value="">—</option>
  {#if allowSplit}<option value={SPLIT}>--Split--</option>{/if}
  {#each ["income", "expense"] as kind (kind)}
    <optgroup label={kind === "income" ? "Income" : "Expense"}>
      {#each cats.filter((c) => c.kind === kind) as c (c.id)}
        <option value={`c:${c.id}`}>{listsState.categoryPath(c.id)}</option>
      {/each}
    </optgroup>
  {/each}
  <optgroup label="Transfer">
    {#each accounts as a (a.id)}
      <option value={`a:${a.id}`}>[{a.name}]</option>
    {/each}
  </optgroup>
</select>
