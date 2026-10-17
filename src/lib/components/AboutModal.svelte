<script lang="ts">
  // Help > About Kansha: what it is, its versions, where to write.
  import { onMount } from "svelte";
  import { loadVersions } from "../versions";
  import { dialogState } from "../state/dialogs.svelte";
  import Modal from "./Modal.svelte";
  // Made small ahead of time: the browser shrinks a mask roughly, so
  // the full-size image looked jagged at this size.
  import mark from "../../assets/kansha-mark-small.png";
  // Bundled, so the full text shows without a network.
  import licenseText from "../../../LICENSE?raw";

  let version = $state("");
  let schema = $state("");
  let showLicense = $state(false);

  onMount(async () => {
    ({ app: version, schema } = await loadVersions());
  });

  const close = () => (dialogState.about = false);
</script>

<Modal title="About Kansha" onclose={close}>
  <div class="about">
    <header>
      <span class="mark" style:--mark="url({mark})" role="img" aria-label="感謝, kansha, in brush calligraphy"></span>
      <div>
        <p class="name">Kansha</p>
        <p class="meaning"><span lang="ja">感謝</span> <i>kansha</i> — Japanese for gratitude.</p>
        <dl>
          <dt>Version</dt>
          <dd>{version}</dd>
          <dt>Schema</dt>
          <dd>{schema}</dd>
        </dl>
      </div>
    </header>
    <p>
      Kansha is a personal finance app for one person, running on your own computer. It keeps bank, credit card, loan, and
      investment accounts, with lots and cost basis, scheduled transactions, reports, and tax summaries. Your book is a single
      encrypted file that you control: no cloud, no subscription, no telemetry.
    </p>
    <p class="license">
      © 2026 sparsile. MIT License.
      <button type="button" class="link" aria-expanded={showLicense} onclick={() => (showLicense = !showLicense)}>License</button>
    </p>
    {#if showLicense}<pre class="license-text">{licenseText}</pre>{/if}
    <p class="contact">Suggestions or bug reports: <span class="email">feedback@astryx.tools</span></p>
    <p class="actions"><button type="button" onclick={close}>Close</button></p>
  </div>
</Modal>

<style>
  .about {
    line-height: 1.45;
  }
  header {
    display: flex;
    gap: 1.25rem;
    align-items: center;
    padding-bottom: 1rem;
    border-bottom: 1px solid var(--line-soft);
  }
  /* Drawn in the text colour, so it reads on every theme. */
  .mark {
    flex: none;
    width: 2.95rem;
    aspect-ratio: 390 / 866;
    /* The image, set on the element. */
    --mark: none;
    background: var(--fg);
    -webkit-mask: var(--mark) center / contain no-repeat;
    mask: var(--mark) center / contain no-repeat;
  }
  .name {
    margin: 0;
    font-size: var(--fs-title);
    font-weight: bold;
    letter-spacing: 0.04em;
  }
  .meaning {
    margin: 0.15rem 0 0.6rem;
    opacity: 0.8;
  }
  dl {
    display: grid;
    grid-template-columns: max-content auto;
    gap: 0.1rem 0.75rem;
    margin: 0;
  }
  dt {
    opacity: 0.8;
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  .license {
    margin-bottom: 0.5rem;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    text-decoration: underline;
    cursor: pointer;
    color: inherit;
  }
  .license-text {
    max-height: 14rem;
    overflow: auto;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--line-soft);
    border-radius: 4px;
    font-size: var(--fs-small);
    white-space: pre-wrap;
  }
  .contact {
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--line-soft);
    border-radius: 4px;
  }
  .email {
    font-family: monospace;
    user-select: all;
  }
  .actions {
    margin-bottom: 0;
    text-align: right;
  }
</style>
