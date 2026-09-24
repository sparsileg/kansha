<script lang="ts">
  import type { Component } from "svelte";
  import { themeState } from "./lib/state/theme.svelte";
  import { viewState } from "./lib/state/view.svelte";
  import Dashboard from "./views/Dashboard.svelte";

  // Placeholder until the other views exist (Phase 3+); Dashboard is the
  // only real one in Phase 0.
  const views: Record<string, Component> = {
    dashboard: Dashboard,
  };
</script>

<div
  class="app"
  data-theme={themeState.theme}
  style="font-size: {themeState.fontSize}px"
>
  <nav>
    <button onclick={() => viewState.navigate("dashboard")}>Dashboard</button>
    <button onclick={() => themeState.toggle()}>
      Toggle theme ({themeState.theme})
    </button>
  </nav>
  <main>
    {#if views[viewState.current]}
      {@const View = views[viewState.current]}
      <View />
    {/if}
  </main>
</div>

<style>
  :global(body) {
    margin: 0;
  }
  .app {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .app[data-theme="dark"] {
    background: #1e1e1e;
    color: #eee;
  }
  .app[data-theme="light"] {
    background: #fff;
    color: #111;
  }
  nav {
    padding: 0.5rem;
    display: flex;
    gap: 0.5rem;
    border-bottom: 1px solid rgba(128, 128, 128, 0.3);
  }
  main {
    padding: 1rem;
    flex: 1;
  }
</style>
