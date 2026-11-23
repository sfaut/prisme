import { describe, expect, it } from 'vitest'

import { describe as describeError, locationMessage } from '../messages'

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

  it('refuses the XA statements with their own message', () => {
    expect(describeError({ code: 'xa_transaction_unsupported' })).toContain(
      'Requêtes XA non prises en charge par Prisme',
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

  it('names the engine and the driver of a failure', () => {
    expect(
      describeError({
        code: 'query_failed',
        flavor: 'sqlite',
        cause: { source: 'engine', code: '1', name: 'SQLITE_ERROR', message: 'no such table: t' },
      }),
    ).toBe('SQLite a répondu (SQLITE_ERROR, code 1) :\n\nno such table: t')
    expect(
      describeError({
        code: 'query_failed',
        flavor: 'sqlite',
        cause: { source: 'driver', message: 'Invalid column type' },
      }),
    ).toBe('Le pilote de connexion (rusqlite) a signalé :\n\nInvalid column type')
  })

  it('tells what is wrong with the databases of a connection held in files', () => {
    expect(describeError({ code: 'main_database_first' })).toBe(
      'La première base est la base principale, de schéma « main ».',
    )
    expect(describeError({ code: 'too_many_databases', max: 10 })).toBe(
      '10 bases attachées au plus, en plus de la base principale.',
    )
    expect(describeError({ code: 'duplicate_schema', schema: 'Ventes' })).toBe(
      'Le schéma « Ventes » est donné deux fois (sans tenir compte de la casse).',
    )
    expect(describeError({ code: 'reserved_schema', schema: 'temp' })).toBe(
      'Le schéma « temp » est réservé par le moteur : choisir un autre nom.',
    )
    expect(
      describeError({
        code: 'location',
        schema: 'ventes',
        error: { code: 'already_exists', address: '/srv/v.db' },
      }),
    ).toBe(
      `Schéma « ventes » : ${locationMessage({ code: 'already_exists', address: '/srv/v.db' })}`,
    )
    expect(
      describeError({ code: 'field_length', field: 'schema_name', min: 1, max: 64, length: 0 }),
    ).toBe('Schéma : entre 1 et 64 caractères, 0 reçus.')
  })

  it('tells what is wrong with the file of a database', () => {
    expect(
      describeError({
        code: 'connect',
        failure: { reason: 'file_not_found', location: 'D:\\data\\v.db' },
      }),
    ).toBe('Le fichier D:\\data\\v.db n’existe pas.')
    expect(
      describeError({
        code: 'connect',
        failure: {
          reason: 'not_a_database',
          location: '/srv/v.db',
          cause: {
            source: 'engine',
            code: '26',
            name: 'SQLITE_NOTADB',
            message: 'file is not a database',
          },
        },
      }),
    ).toBe(
      '/srv/v.db n’est pas une base de données.\n\n' +
        'Le moteur a répondu (SQLITE_NOTADB, code 26) :\n\nfile is not a database',
    )
    expect(
      describeError({
        code: 'connect',
        failure: {
          reason: 'location',
          error: {
            code: 'unreachable',
            address: 'https://example.com/v.db',
            message: '404 Not Found',
          },
        },
      }),
    ).toBe('https://example.com/v.db est injoignable :\n\n404 Not Found')
  })
})
