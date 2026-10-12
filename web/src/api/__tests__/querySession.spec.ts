import { describe, expect, it } from 'vitest'

import { applyMessage, initialState, sessionUrl, startBatch } from '../querySession'

/**
 * État d'une session prête, sur la base `xlist`.
 */
function readyState() {
  const state = initialState()
  applyMessage(state, {
    type: 'ready',
    server: { flavor: 'mariadb', version: '11.3.2' },
    database: 'xlist',
  })
  return state
}

describe('applyMessage', () => {
  it('suit un lot de deux requêtes, chacune dans son onglet', () => {
    const state = readyState()
    expect(state.status).toBe('ready')
    expect(state.database).toBe('xlist')

    startBatch(state)
    expect(state.status).toBe('running')
    applyMessage(state, {
      type: 'batch',
      queries: [
        { query: 0, sql: 'select * from video', start: 0, end: 20 },
        { query: 1, sql: 'select * from person', start: 21, end: 42 },
      ],
    })
    expect(state.tabs.map((tab) => tab.sql)).toEqual([
      'select * from video',
      'select * from person',
    ])

    // Les messages des deux requêtes s'entremêlent.
    applyMessage(state, { type: 'columns', query: 1, columns: ['id'], sql: 'x', limit: 'added' })
    applyMessage(state, {
      type: 'columns',
      query: 0,
      columns: ['id', 'title'],
      sql: 'y',
      limit: 'added',
    })
    applyMessage(state, { type: 'rows', query: 0, rows: [['1', 'a']] })
    applyMessage(state, { type: 'rows', query: 1, rows: [['7'], ['8']] })
    applyMessage(state, {
      type: 'done',
      query: 1,
      row_count: 2,
      truncated: false,
      affected_rows: 0,
      elapsed_ms: 5,
      database: 'xlist',
    })
    applyMessage(state, { type: 'error', query: 0, message: 'interrompue' })
    applyMessage(state, { type: 'batch_done' })

    const [video, person] = state.tabs
    expect(video?.result?.rows).toEqual([['1', 'a']])
    expect(video?.error).toBe('interrompue')
    expect(video?.done).toBe(true)
    expect(person?.result?.rows).toEqual([['7'], ['8']])
    expect(person?.result?.elapsedMs).toBe(5)
    expect(person?.done).toBe(true)
    expect(state.status).toBe('ready')
  })

  it('suit la base en cours à la fin de chaque requête', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [{ query: 0, sql: 'USE shop', start: 0, end: 8 }],
    })
    applyMessage(state, {
      type: 'done',
      query: 0,
      row_count: 0,
      truncated: false,
      affected_rows: 0,
      elapsed_ms: 1,
      database: 'shop',
    })
    expect(state.database).toBe('shop')
  })

  it('garde la session ouverte après une erreur de session', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, { type: 'error', query: null, message: 'aucune requête à exécuter' })
    expect(state.status).toBe('ready')
    expect(state.error).toBe('aucune requête à exécuter')
  })

  it('ferme la session si la connexion au serveur cible échoue', () => {
    const state = initialState()
    applyMessage(state, { type: 'error', query: null, message: 'Access denied' })
    expect(state.status).toBe('closed')
  })

  it('efface les onglets et l’erreur précédents au début d’un lot', () => {
    const state = readyState()
    state.error = 'ancienne erreur'
    state.tabs = [{ query: 0, sql: 'x', result: null, error: null, done: true }]
    startBatch(state)
    expect(state.error).toBeNull()
    expect(state.tabs).toEqual([])
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
