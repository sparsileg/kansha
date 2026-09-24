// The only place `invoke` is called (spec §17.3). `commands` is the raw
// generated surface; `call` unwraps its `{status, data|error}` result into
// a value or a thrown `ApiError`, and `withConfirmation` drives the
// `confirmation_required` retry flow.

import { commands } from "../types/bindings";
import type { ErrorKind, IpcError } from "../types/bindings";

export { commands };
export type { ErrorKind, IpcError };

/** A failed IPC command. `kind` says how the UI should react. */
export class ApiError extends Error {
  readonly kind: ErrorKind;

  constructor(error: IpcError) {
    super(error.message);
    this.name = "ApiError";
    this.kind = error.kind;
  }

  get needsConfirmation(): boolean {
    return this.kind === "confirmation_required";
  }
}

type Result<T> =
  | { status: "ok"; data: T }
  | { status: "error"; error: IpcError };

/** Await a command result; return its data or throw `ApiError`. */
export async function call<T>(result: Promise<Result<T>>): Promise<T> {
  const r = await result;
  if (r.status === "ok") return r.data;
  throw new ApiError(r.error);
}

/**
 * Run a command that takes a trailing `confirmed` flag. On
 * `confirmation_required`, ask the user with the error's message; if they
 * agree, repeat with `confirmed = true`. Returns `null` when the user
 * declines, otherwise the command's data.
 */
export async function withConfirmation<T>(
  run: (confirmed: boolean) => Promise<Result<T>>,
  ask: (message: string) => Promise<boolean>,
): Promise<T | null> {
  try {
    return await call(run(false));
  } catch (e) {
    if (!(e instanceof ApiError) || !e.needsConfirmation) throw e;
    if (!(await ask(e.message))) return null;
    return await call(run(true));
  }
}
