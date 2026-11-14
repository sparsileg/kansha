// From Reconcile to the register and back (RCN-080, #69): New opens the
// account's entry row with Date empty, saved cleared so it comes back
// checked; a listed transaction opens selected, in its edit. Back
// (UI-025) or the register's "Back to Reconcile" resumes the session,
// its lists read again.

import { openAccount, openReconcile } from "../shell/nav";
import { reconcileState } from "../state/reconcile.svelte";
import { registerState } from "../state/register.svelte";
import type { AccountId, TxnId } from "../types/bindings";

/** New: the register's entry row, Date empty, the entry cleared. */
export async function reconcileNew(): Promise<void> {
  const id = reconcileState.accountId;
  if (id === null) return;
  await openAccount(id);
  registerState.newEntry = { account: id };
}

/** A listed transaction, selected and open for editing. */
export async function reconcileEdit(txn: TxnId): Promise<void> {
  const id = reconcileState.accountId;
  if (id === null) return;
  await openAccount(id);
  await registerState.goToTransaction(id, txn);
  registerState.editSplit = false;
  registerState.editing = txn;
}

/** The register's account has a reconciliation in progress here. */
export function reconciling(account: AccountId): boolean {
  return reconcileState.accountId === account && reconcileState.session !== null;
}

/** Back to Reconcile: the session, its lists read again. */
export async function backToReconcile(): Promise<void> {
  const id = reconcileState.accountId;
  if (id === null) return;
  await openReconcile(id);
}
