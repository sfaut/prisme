/**
 * Warning of a change that closes the sessions of a connection while
 * transactions are open on them: saving a changed connection, or deleting
 * it, rolls these transactions back. The windows that make the change show
 * it, and their button says that the transactions will be rolled back.
 */

/**
 * Warning of the open transactions that an action on a connection rolls back.
 *
 * @param count Editors of the connection with a transaction open (1 or more).
 * @param action What closes their sessions, as a verb ("enregistrer"...).
 */
export function openTransactionsNotice(count: number, action: string): string {
  return count === 1
    ? `Une transaction est ouverte dans un éditeur de cette connexion : ${action} ferme sa session, ce qui l’annule (ROLLBACK).`
    : `Des transactions sont ouvertes dans ${count} éditeurs de cette connexion : ${action} ferme leurs sessions, ce qui les annule (ROLLBACK).`
}
