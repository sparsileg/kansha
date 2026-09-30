<script lang="ts">
  /** A small gear button that opens a screen's options. The click stays
   * here, so a menu it opens is not closed by the same click. */
  let { label, onclick }: { label: string; onclick: (e: MouseEvent) => void } = $props();
</script>

<button
  type="button"
  class="gear"
  aria-label={label}
  title={label}
  onclick={(e) => {
    e.stopPropagation();
    onclick(e);
  }}
>
  <!-- Eight square-cut teeth round a hub; the hole is cut out (even-odd).
       A thin stroke in the same color rounds the corners. -->
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      fill-rule="evenodd"
      d="M10.38 4.17L10.68 1.58L13.32 1.58L13.62 4.17L16.39 5.31L18.44 3.7L20.3 5.56L18.69 7.61L19.83 10.38L22.42 10.68L22.42 13.32L19.83 13.62L18.69 16.39L20.3 18.44L18.44 20.3L16.39 18.69L13.62 19.83L13.32 22.42L10.68 22.42L10.38 19.83L7.61 18.69L5.56 20.3L3.7 18.44L5.31 16.39L4.17 13.62L1.58 13.32L1.58 10.68L4.17 10.38L5.31 7.61L3.7 5.56L5.56 3.7L7.61 5.31Z M15.4 12a3.4 3.4 0 1 0-6.8 0a3.4 3.4 0 1 0 6.8 0Z"
    />
  </svg>
</button>

<style>
  .gear {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.15rem;
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    border-radius: var(--btn-radius);
  }
  .gear:hover {
    background: var(--hover-bg);
  }
  svg {
    width: 1.1rem;
    height: 1.1rem;
    fill: currentColor;
    stroke: currentColor;
    stroke-width: 1;
    stroke-linejoin: round;
  }
</style>
