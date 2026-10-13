import { describe, expect, it } from 'vitest'

import {
  applyMessage,
  formatDuration,
  initialState,
  progressSummary,
  sessionUrl,
  startBatch,
  tabProgress,
} from '../querySession'

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
    applyMessage(state, {
      type: 'columns',
      query: 1,
      columns: ['id'],
      right_aligned: [true],
      sql: 'x',
      limit: 'added',
    })
    applyMessage(state, {
      type: 'columns',
      query: 0,
      columns: ['id', 'title'],
      right_aligned: [true, false],
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
    state.tabs = [
      { query: 0, sql: 'x', result: null, error: null, done: true, startedAt: 1, endedAt: 2 },
    ]
    startBatch(state)
    expect(state.error).toBeNull()
    expect(state.tabs).toEqual([])
  })
})

describe('tabProgress', () => {
  it('passe d’attente à exécution, puis chargement, puis terminé ou erreur', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [
        { query: 0, sql: 'a', start: 0, end: 1 },
        { query: 1, sql: 'b', start: 2, end: 3 },
      ],
    })
    const [first, second] = state.tabs
    expect(tabProgress(first!)).toBe('waiting')

    applyMessage(state, { type: 'started', query: 0 }, 1000)
    expect(tabProgress(first!)).toBe('executing')
    expect(first?.startedAt).toBe(1000)

    applyMessage(state, {
      type: 'columns',
      query: 0,
      columns: ['x'],
      right_aligned: [false],
      sql: 'a',
      limit: 'added',
    })
    expect(tabProgress(first!)).toBe('loading')

    applyMessage(state, {
      type: 'done',
      query: 0,
      row_count: 0,
      truncated: false,
      affected_rows: 0,
      elapsed_ms: 1,
      database: null,
    })
    expect(tabProgress(first!)).toBe('done')

    applyMessage(state, { type: 'error', query: 1, message: 'refus' })
    expect(tabProgress(second!)).toBe('error')
  })
})

describe('progressSummary', () => {
  it('donne statut, durée qui court et nombre de lignes', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, { type: 'batch', queries: [{ query: 0, sql: 'a', start: 0, end: 1 }] })
    const tab = state.tabs[0]!
    expect(progressSummary(tab, 0)).toEqual(['En attente', '—', '—'])

    applyMessage(state, { type: 'started', query: 0 }, 1000)
    applyMessage(
      state,
      {
        type: 'columns',
        query: 0,
        columns: ['x'],
        right_aligned: [false],
        sql: 'a',
        limit: 'added',
      },
      1100,
    )
    applyMessage(state, { type: 'rows', query: 0, rows: [['1'], ['2']] }, 1200)
    expect(progressSummary(tab, 3500)).toEqual(['Chargement des lignes', '2,5 s', '2 lignes'])

    applyMessage(
      state,
      {
        type: 'done',
        query: 0,
        row_count: 2,
        truncated: true,
        affected_rows: 0,
        elapsed_ms: 420,
        database: null,
      },
      1500,
    )
    expect(progressSummary(tab, 9999)).toEqual(['Terminé', '420 ms', '2 lignes (tronqué)'])
  })

  it('formate les durées', () => {
    expect(formatDuration(42)).toBe('42 ms')
    expect(formatDuration(1000)).toBe('1,0 s')
    expect(formatDuration(12345)).toBe('12,3 s')
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
