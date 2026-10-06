<script lang="ts">
  import { formatMoney, formatMoneyWhole } from "../format/money";

  // `whole`: the amount is already rounded to dollars (by Rust); drop ".00".
  let { amount, whole = false }: { amount: string | undefined; whole?: boolean } = $props();
</script>

{#if amount !== undefined}
  <span class="bal" class:neg={amount.startsWith("-")}
    >{whole ? formatMoneyWhole(amount) : formatMoney(amount)}</span
  >
{/if}

<style>
  .bal {
    font-variant-numeric: tabular-nums;
  }
  .neg {
    color: var(--bad);
  }
</style>
