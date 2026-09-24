<script lang="ts">
  import { formatMoney } from "../format/money";

  let { amount }: { amount: string | undefined } = $props();
</script>

{#if amount !== undefined}
  <span class="bal" class:neg={amount.startsWith("-")}>{formatMoney(amount)}</span>
{/if}

<style>
  .bal {
    font-variant-numeric: tabular-nums;
  }
  .neg {
    color: #c0392b;
  }
</style>
