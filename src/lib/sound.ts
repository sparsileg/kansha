// A short "ding" when a transaction is entered (SET-090): saved in a
// register (new or edited) or entered from a reminder. Made with Web
// Audio, so there is no sound file. Off unless the book setting is on;
// a failure to play is silent and never stops the save.
//
// The first sound starts the audio output, which takes a few seconds on
// Linux; `soundWarmUp` does that when the book opens or the setting is
// turned on, so the first ding is not late.

import { bookSettings } from "./state/booksettings.svelte";

let ctx: AudioContext | null = null;

/** The shared audio context, made on first use; null without Web Audio. */
function context(): AudioContext | null {
  if (typeof AudioContext === "undefined") return null;
  ctx ??= new AudioContext();
  if (ctx.state === "suspended") void ctx.resume().catch(() => {});
  return ctx;
}

/** With the setting on, start the audio output now by playing a moment
 * of silence. */
export function soundWarmUp(): void {
  if (!bookSettings.value.entry_sound) return;
  try {
    const c = context();
    if (!c) return;
    const src = c.createBufferSource();
    src.buffer = c.createBuffer(1, Math.ceil(c.sampleRate / 10), c.sampleRate);
    src.connect(c.destination);
    src.start();
  } catch {
    // No sound device or no Web Audio: say nothing.
  }
}

/** Play the ding if the setting is on. */
export function entryDing(): void {
  if (!bookSettings.value.entry_sound) return;
  try {
    const c = context();
    if (!c) return;
    const t = c.currentTime;
    const out = c.createGain();
    out.gain.setValueAtTime(0.0001, t);
    out.gain.exponentialRampToValueAtTime(0.25, t + 0.005);
    out.gain.exponentialRampToValueAtTime(0.0001, t + 0.45);
    out.connect(c.destination);
    // A bell: the note (C6) and a softer octave above it.
    for (const [freq, level] of [
      [1046.5, 1],
      [2093, 0.3],
    ]) {
      const osc = c.createOscillator();
      const g = c.createGain();
      osc.type = "sine";
      osc.frequency.value = freq;
      g.gain.value = level;
      osc.connect(g);
      g.connect(out);
      osc.start(t);
      osc.stop(t + 0.5);
    }
  } catch {
    // No sound device or no Web Audio: say nothing.
  }
}
