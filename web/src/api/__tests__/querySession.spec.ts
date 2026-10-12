import { describe, expect, it } from 'vitest'

import { applyMessage, initialState, sessionUrl, startQuery } from '../querySession'

describe('applyMessage', () => {
  it('suit une requête du début à la fin', () => {
    const state = initialState()
    applyMessage(state, {
      type: 'ready',
      server: { flavor: 'mariadb', version: '11.3.2' },
      database: 'xlist',
    })
    expect(state.status).toBe('ready')
    expect(state.database).toBe('xlist')

    startQuery(state)
    expect(state.status).toBe('running')

    applyMessage(state, {
      type: 'columns',
      columns: ['id', 'label'],
      sql: 'SELECT id, label FROM t LIMIT 1001',
      limit: 'added',
    })
    applyMessage(state, { type: 'rows', rows: [['1', 'a']] })
    applyMessage(state, { type: 'rows', rows: [['2', null]] })
    applyMessage(state, {
      type: 'done',
      row_count: 2,
      truncated: false,
      affected_rows: 0,
      elapsed_ms: 12,
      database: 'shop',
    })
    expect(state.database).toBe('shop')

    expect(state.status).toBe('ready')
    expect(state.result).toMatchObject({
      columns: ['id', 'label'],
      rows: [
        ['1', 'a'],
        ['2', null],
      ],
      done: true,
      truncated: false,
      elapsedMs: 12,
    })
  })

  it('garde la session ouverte après une erreur de requête', () => {
    const state = initialState()
    applyMessage(state, {
      type: 'ready',
      server: { flavor: 'mysql', version: '8.1.0' },
      database: null,
    })
    startQuery(state)
    applyMessage(state, { type: 'error', message: "Table 'test.missing' doesn't exist" })

    expect(state.status).toBe('ready')
    expect(state.error).toBe("Table 'test.missing' doesn't exist")
  })

  it('ferme la session si la connexion au serveur cible échoue', () => {
    const state = initialState()
    applyMessage(state, { type: 'error', message: 'Access denied' })
    expect(state.status).toBe('closed')
  })

  it('efface le résultat et l’erreur précédents au début d’une requête', () => {
    const state = initialState()
    state.error = 'ancienne erreur'
    startQuery(state)
    expect(state.error).toBeNull()
    expect(state.result).toBeNull()
  })
})

describe('sessionUrl', () => {
  it('suit le protocole de la page', () => {
    expect(sessionUrl(3, { protocol: 'http:', host: 'localhost:5173' } as Location)).toBe(
      'ws://localhost:5173/api/connections/3/query',
    )
    expect(sessionUrl(3, { protocol: 'https:', host: 'prisme.example' } as Location)).toBe(
      'wss://prisme.example/api/connections/3/query',
    )
  })
})
