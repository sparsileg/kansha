<script lang="ts">
  import CategoryManager from "../lib/components/CategoryManager.svelte";
  import PayeeManager from "../lib/components/PayeeManager.svelte";
  import TagManager from "../lib/components/TagManager.svelte";
  import SecurityManager from "../lib/components/invest/SecurityManager.svelte";
  import { viewState } from "../lib/state/view.svelte";

  const tab = $derived(viewState.params.tab ?? "payees");
</script>

<section class="manage">
  <div class="tabs" role="tablist">
    {#each ["payees", "categories", "tags", "securities"] as const as t (t)}
      <button type="button" role="tab" aria-selected={tab === t} class:on={tab === t} onclick={() => viewState.navigate("manage", { tab: t })}>
        {t === "payees" ? "Memorized Payees" : t[0].toUpperCase() + t.slice(1)}
      </button>
    {/each}
  </div>
  <!-- The manager on a sheet (base.css), the window's color around it. -->
  <div class="pane sheet">
    {#if tab === "payees"}<PayeeManager />{:else if tab === "categories"}<CategoryManager />{:else if tab === "securities"}<SecurityManager />{:else}<TagManager />{/if}
  </div>
</section>

<style>
  .manage {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .pane {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0.75rem;
  }
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
