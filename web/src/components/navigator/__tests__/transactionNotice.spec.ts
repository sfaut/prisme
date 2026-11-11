import { describe, expect, it } from 'vitest'

import { openTransactionsNotice } from '../transactionNotice'

describe('openTransactionsNotice', () => {
  it('tells the transactions an action rolls back, in the singular or the plural', () => {
    expect(openTransactionsNotice(1, 'enregistrer')).toBe(
      'Une transaction est ouverte dans un éditeur de cette connexion : enregistrer ferme sa session, ce qui l’annule (ROLLBACK).',
    )
    expect(openTransactionsNotice(2, 'supprimer la connexion')).toBe(
      'Des transactions sont ouvertes dans 2 éditeurs de cette connexion : supprimer la connexion ferme leurs sessions, ce qui les annule (ROLLBACK).',
    )
  })
})
