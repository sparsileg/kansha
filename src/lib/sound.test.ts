import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { entryDing, soundWarmUp } from "./sound";
import { bookSettings, DEFAULT_SETTINGS } from "./state/booksettings.svelte";

const started = vi.fn();

class FakeParam {
  value = 0;
  setValueAtTime() {}
  exponentialRampToValueAtTime() {}
}
class FakeNode {
  gain = new FakeParam();
  frequency = new FakeParam();
  type = "";
  connect() {}
  start() {
    started();
  }
  stop() {}
}
class FakeAudioContext {
  state = "running";
  currentTime = 0;
  destination = {};
  createGain() {
    return new FakeNode();
  }
  createOscillator() {
    return new FakeNode();
  }
  sampleRate = 48000;
  createBuffer() {
    return {};
  }
  createBufferSource() {
    return new FakeNode();
  }
  resume() {
    return Promise.resolve();
  }
}

describe("entryDing (SET-090)", () => {
  beforeEach(() => {
    started.mockClear();
    vi.stubGlobal("AudioContext", FakeAudioContext);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    bookSettings.value = { ...DEFAULT_SETTINGS };
  });

  it("is silent while the setting is off", () => {
    bookSettings.value = { ...DEFAULT_SETTINGS, entry_sound: false };
    entryDing();
    expect(started).not.toHaveBeenCalled();
  });

  it("plays when the setting is on", () => {
    bookSettings.value = { ...DEFAULT_SETTINGS, entry_sound: true };
    entryDing();
    expect(started).toHaveBeenCalled();
  });

  it("warms up only while the setting is on", () => {
    bookSettings.value = { ...DEFAULT_SETTINGS, entry_sound: false };
    soundWarmUp();
    expect(started).not.toHaveBeenCalled();
    bookSettings.value = { ...DEFAULT_SETTINGS, entry_sound: true };
    soundWarmUp();
    expect(started).toHaveBeenCalledTimes(1);
  });

  it("never throws without Web Audio", () => {
    vi.stubGlobal("AudioContext", undefined);
    bookSettings.value = { ...DEFAULT_SETTINGS, entry_sound: true };
    expect(() => entryDing()).not.toThrow();
  });
});
