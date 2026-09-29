// The status bar's message (beside the Accounts button): a note that
// clears itself. Any component may post one. An alert flashes and stays
// longer than an ordinary note.

export type StatusKind = "info" | "alert";

export const INFO_MS = 30_000;
export const ALERT_MS = 60_000;

class StatusState {
  /** What the bar shows; `null` when it is empty. */
  message = $state<{ text: string; kind: StatusKind } | null>(null);
  private timer: ReturnType<typeof setTimeout> | undefined;

  /** Show `text`, replacing what is there; the clock starts over. */
  show(text: string, kind: StatusKind = "info"): void {
    this.clear();
    this.message = { text, kind };
    this.timer = setTimeout(() => this.clear(), kind === "alert" ? ALERT_MS : INFO_MS);
  }

  clear(): void {
    clearTimeout(this.timer);
    this.timer = undefined;
    this.message = null;
  }
}

export const statusState = new StatusState();
