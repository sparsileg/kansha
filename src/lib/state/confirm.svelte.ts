// A promise-based confirmation dialog, rendered once in App.svelte. Feeds
// `withConfirmation` (api) and the UI's own "are you sure?" prompts.

class ConfirmState {
  message = $state<string | null>(null);
  #resolve: ((ok: boolean) => void) | null = null;

  ask = (message: string): Promise<boolean> => {
    this.#resolve?.(false);
    this.message = message;
    return new Promise((resolve) => {
      this.#resolve = resolve;
    });
  };

  answer(ok: boolean): void {
    const r = this.#resolve;
    this.#resolve = null;
    this.message = null;
    r?.(ok);
  }
}

export const confirmState = new ConfirmState();
