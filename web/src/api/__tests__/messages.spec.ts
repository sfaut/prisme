import { describe, expect, it } from 'vitest'

import { describe as describeError } from '../messages'

describe('messages of the errors', () => {
  it('writes the messages of Prisme in French, with their parameters', () => {
    expect(
      describeError({ code: 'field_length', field: 'host', min: 1, max: 253, length: 300 }),
    ).toBe('Hôte : entre 1 et 253 caractères, 300 reçus.')
    expect(
      describeError({ code: 'field_length', field: 'name', min: 0, max: 100, length: 120 }),
    ).toBe('Nom : 100 caractères au plus, 120 reçus.')
    expect(
      describeError({ code: 'field_range', field: 'history_retention_days', min: 1, max: 3650 }),
    ).toBe('Rétention historique : valeur attendue entre 1 et 3 650 jours.')
    expect(describeError({ code: 'query_interrupted' })).toBe(
      'Requête interrompue par l’utilisateur.',
    )
    expect(describeError({ code: 'query_skipped' })).toBe(
      'Requête non exécutée : une requête précédente du lot a échoué.',
    )
  })

  it('names the switches that a refused write needs', () => {
    expect(describeError({ code: 'write_not_allowed', kinds: ['create_table'] })).toBe(
      'Écriture refusée : CREATE TABLE n’est pas autorisé dans les Paramètres.',
    )
    expect(describeError({ code: 'write_not_allowed', kinds: ['insert', 'delete'] })).toBe(
      'Écriture refusée : INSERT, DELETE ne sont pas autorisés dans les Paramètres.',
    )
  })

  it('tells the transactions still possible in read-only sessions', () => {
    expect(describeError({ code: 'read_write_transaction_read_only' })).toContain(
      'START TRANSACTION et BEGIN restent possibles',
    )
  })

  it('introduces the answer of the target server, quoted as it is', () => {
    expect(
      describeError({
        code: 'query_failed',
        flavor: 'mariadb',
        cause: {
          source: 'server',
          number: 1146,
          sqlstate: '42S02',
          message: "Table 'nope.nope' doesn't exist",
        },
      }),
    ).toBe(
      "Le serveur MariaDB a répondu (erreur 1146, SQLSTATE 42S02) :\n\nTable 'nope.nope' doesn't exist",
    )
  })

  it('follows a message of Prisme with the text that explains it', () => {
    expect(
      describeError({
        code: 'connect',
        failure: {
          reason: 'access_denied',
          username: 'root',
          cause: {
            source: 'server',
            number: 1045,
            sqlstate: '28000',
            message: "Access denied for user 'root'@'172.17.0.1'",
          },
        },
      }),
    ).toBe(
      'Accès refusé au compte « root » : mot de passe erroné, ou compte non autorisé depuis cette machine.\n\n' +
        "Le serveur a répondu (erreur 1045, SQLSTATE 28000) :\n\nAccess denied for user 'root'@'172.17.0.1'",
    )
  })
})
