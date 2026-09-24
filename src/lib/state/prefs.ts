// Per-computer preferences kept in localStorage. A stand-in until the
// `settings` module stores them with the book (SET-070); the home screen
// in particular will then belong to each book. Every read and write is
// guarded: storage can be missing or blocked.

export function loadPref<T extends string | number | boolean>(
  key: string,
  fallback: T,
  valid?: (v: unknown) => boolean,
): T {
  try {
    const raw = localStorage.getItem(`kansha.${key}`);
    if (raw === null) return fallback;
    const v: unknown = JSON.parse(raw);
    if (typeof v !== typeof fallback) return fallback;
    if (valid && !valid(v)) return fallback;
    return v as T;
  } catch {
    return fallback;
  }
}

export function savePref(key: string, value: string | number | boolean): void {
  try {
    localStorage.setItem(`kansha.${key}`, JSON.stringify(value));
  } catch {
    /* not saved; the setting still applies for this session */
  }
}
