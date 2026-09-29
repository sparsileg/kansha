<script lang="ts">
  // Theme, font, and base font size (SET-010, SET-025, SET-020), at the
  // right end of the menu bar: the size against the edge.
  import { FONT_SIZES, FONTS, THEMES, themeState, type Font, type Theme } from "../../state/theme.svelte";
</script>

<div class="pickers">
  <select
    aria-label="Theme"
    title="Theme"
    value={themeState.theme}
    onchange={(e) => themeState.setTheme(e.currentTarget.value as Theme)}
  >
    {#each THEMES as t (t.value)}<option value={t.value}>{t.label}</option>{/each}
  </select>
  <select
    aria-label="Font"
    title="Font"
    value={themeState.font}
    onchange={(e) => themeState.setFont(e.currentTarget.value as Font)}
  >
    {#each FONTS as f (f.value)}<option value={f.value}>{f.label}</option>{/each}
  </select>
  <select
    aria-label="Font size"
    title="Font size"
    value={String(themeState.fontSize)}
    onchange={(e) => themeState.setFontSize(Number(e.currentTarget.value))}
  >
    {#each FONT_SIZES as px (px)}<option value={String(px)}>{px} px</option>{/each}
  </select>
</div>

<style>
  .pickers {
    margin-left: auto;
    display: flex;
    gap: 0.5rem;
    align-items: center;
    padding: 0.15rem 0;
  }
</style>
