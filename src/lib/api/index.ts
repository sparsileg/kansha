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

  constructor(error: IpcError | string) {
    // Tauri rejects with a bare string when it cannot decode the arguments
    // or find the command; never leave the message empty.
    const e: IpcError =
      typeof error === "string" || !error?.message
        ? {
            kind: "internal",
            message: (typeof error === "string" ? error : "") || "The command failed.",
          }
        : error;
    super(e.message);
    this.name = "ApiError";
    this.kind = e.kind;
  }

  get needsConfirmation(): boolean {
    return this.kind === "confirmation_required";
  }
}

type Result<T> =
  | { status: "ok"; data: T }
  | { status: "error"; error: IpcError | string };

/** Await a command result; return its data or throw `ApiError`. */
export async function call<T>(result: Promise<Result<T>>): Promise<T> {
  const r = await result;
  if (r.status === "ok") return r.data;
  throw new ApiError(r.error);
}

/** Returned by `withConfirmation` when the user says no. A command that
 * succeeds with no data returns `null`, so `null` cannot mean "declined". */
export const DECLINED = Symbol("declined");

/**
 * Run a command that takes a trailing `confirmed` flag. On
 * `confirmation_required`, ask the user with the error's message; if they
 * agree, repeat with `confirmed = true`. Returns `DECLINED` when the user
 * declines, otherwise the command's data.
 */
export async function withConfirmation<T>(
  run: (confirmed: boolean) => Promise<Result<T>>,
  ask: (message: string) => Promise<boolean>,
): Promise<T | typeof DECLINED> {
  try {
    return await call(run(false));
  } catch (e) {
    if (!(e instanceof ApiError) || !e.needsConfirmation) throw e;
    if (!(await ask(e.message))) return DECLINED;
    return await call(run(true));
  }
}
