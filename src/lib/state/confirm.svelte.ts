// A promise-based confirmation dialog, rendered once in App.svelte. Feeds
// `withConfirmation` (api) and the UI's own "are you sure?" prompts.

class ConfirmState {
  message = $state<string | null>(null);
  /** Buttons for `choose`; empty means OK and Cancel. */
  choices = $state<string[]>([]);
  #resolve: ((choice: string | null) => void) | null = null;

  ask = async (message: string): Promise<boolean> => (await this.#open(message, [])) === "OK";

  /** One of `choices`, or null for Cancel (Esc, or closing the dialog). */
  choose = (message: string, choices: string[]): Promise<string | null> => this.#open(message, choices);

  #open(message: string, choices: string[]): Promise<string | null> {
    this.#resolve?.(null);
    this.message = message;
    this.choices = choices;
    return new Promise((resolve) => {
      this.#resolve = resolve;
    });
  }

  /** `true` is OK, `false` Cancel; a string is one of the choices. */
  answer(choice: boolean | string): void {
    const r = this.#resolve;
    this.#resolve = null;
    this.message = null;
    this.choices = [];
    r?.(choice === true ? "OK" : choice === false ? null : choice);
  }
}

export const confirmState = new ConfirmState();
