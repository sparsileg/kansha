// Which modal dialogs are open. Rendered once in App.svelte.

import type { Account, AccountId, TxnId } from "../types/bindings";

class DialogState {
  /** `undefined` closed; `null` new account; an account to edit. */
  account = $state<Account | null | undefined>(undefined);
  /** Transaction whose audit history is shown (AUD-020). */
  history = $state<{ txn: TxnId; account: AccountId } | null>(null);
  integrity = $state(false);

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
