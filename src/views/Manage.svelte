<script lang="ts">
  import CategoryManager from "../lib/components/CategoryManager.svelte";
  import PayeeManager from "../lib/components/PayeeManager.svelte";
  import TagManager from "../lib/components/TagManager.svelte";
  import SecurityManager from "../lib/components/invest/SecurityManager.svelte";
  import { viewState } from "../lib/state/view.svelte";

  const tab = $derived(viewState.params.tab ?? "payees");
</script>

<section>
  <div class="tabs" role="tablist">
    {#each ["payees", "categories", "tags", "securities"] as const as t (t)}
      <button type="button" role="tab" aria-selected={tab === t} class:on={tab === t} onclick={() => viewState.navigate("manage", { tab: t })}>
        {t[0].toUpperCase() + t.slice(1)}
      </button>
    {/each}
  </div>
  {#if tab === "payees"}<PayeeManager />{:else if tab === "categories"}<CategoryManager />{:else if tab === "securities"}<SecurityManager />{:else}<TagManager />{/if}
</section>

<style>
  .tabs {
    display: flex;
    gap: 0.25rem;
    margin-bottom: 0.75rem;
  }
  .on {
    font-weight: 700;
    text-decoration: underline;
  }
</style>
