import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ALERT_MS, INFO_MS, statusState } from "./status.svelte";

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  statusState.clear();
  vi.useRealTimers();
});

describe("statusState", () => {
  it("shows a note for 30 seconds, then clears it", () => {
    statusState.show("All good");
    expect(statusState.message).toEqual({ text: "All good", kind: "info" });
    vi.advanceTimersByTime(INFO_MS - 1);
    expect(statusState.message).not.toBeNull();
    vi.advanceTimersByTime(1);
    expect(statusState.message).toBeNull();
  });

  it("keeps an alert for 60 seconds", () => {
    statusState.show("Trouble", "alert");
    vi.advanceTimersByTime(INFO_MS);
    expect(statusState.message?.kind).toBe("alert");
    vi.advanceTimersByTime(ALERT_MS - INFO_MS);
    expect(statusState.message).toBeNull();
  });

  it("a new message replaces the old one and restarts the clock", () => {
    statusState.show("First");
    vi.advanceTimersByTime(INFO_MS - 1000);
    statusState.show("Second");
    vi.advanceTimersByTime(INFO_MS - 1000);
    expect(statusState.message?.text).toBe("Second");
    vi.advanceTimersByTime(1000);
    expect(statusState.message).toBeNull();
  });

  it("an old note's timer does not clear a newer alert", () => {
    statusState.show("Note");
    statusState.show("Alert", "alert");
    vi.advanceTimersByTime(INFO_MS);
    expect(statusState.message?.text).toBe("Alert");
  });
});
