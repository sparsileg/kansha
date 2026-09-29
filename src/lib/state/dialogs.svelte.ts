// Which modal dialogs are open. Rendered once in App.svelte.

import type { Account, AccountId, IntegrityReport, ScheduleFields, ScheduleId, TxnId } from "../types/bindings";

class DialogState {
  /** `undefined` closed; `null` new account; an account to edit. */
  account = $state<Account | null | undefined>(undefined);
  /** Transaction whose audit history is shown (AUD-020). */
  history = $state<{ txn: TxnId; account: AccountId } | null>(null);
  integrity = $state(false);
  /** The result an automatic check already has, shown without running
   * again; `null` for File > Integrity Check, which runs its own. */
  integrityReport = $state<IntegrityReport | null>(null);
  settings = $state(false);
  /** Help > About Kansha. */
  about = $state(false);
  /** File > Restore… (BAK-070). */
  restore = $state(false);
  /** Settings > Verify backup… (BAK-080). */
  verify = $state(false);
  /** Settings > Change backup passphrase… (SECU-040). */
  passphrase = $state(false);
  /** Settings > Show database key… (SECU-020). */
  dbKey = $state(false);
  /** The Navigation Bar dialog (which buttons, in what order). */
  navbar = $state(false);
  /** Due, overdue, and auto-entered items (REC-130, REC-070). */
  due = $state(false);
  /** Schedule form: `id` set when editing; `fields` seeds a new one
   * ("Schedule this", a calendar day). `undefined` closed. */
  schedule = $state<{ id: ScheduleId | null; fields: ScheduleFields | null; start: string | null } | undefined>(undefined);

  newSchedule(start: string | null = null, fields: ScheduleFields | null = null) {
    this.schedule = { id: null, fields, start };
  }
  editSchedule(id: ScheduleId, fields: ScheduleFields) {
    this.schedule = { id, fields, start: null };
  }

  newAccount() {
    this.account = null;
  }
  editAccount(a: Account) {
    this.account = a;
  }
  closeAccount() {
    this.account = undefined;
  }
}

export const dialogState = new DialogState();
