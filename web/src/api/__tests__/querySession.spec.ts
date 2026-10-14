import { describe, expect, it } from 'vitest'

import {
  applyMessage,
  executionDuration,
  formatBytes,
  formatDuration,
  initialState,
  removeTab,
  savedTab,
  sessionUrl,
  startBatch,
  tabFraction,
  tabProgress,
  transferDuration,
  utf8Length,
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
        { query: 0, tab: 0, sql: 'select * from video', start: 0, end: 20 },
        { query: 1, tab: 0, sql: 'select * from person', start: 21, end: 42 },
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
      expected_rows: null,
      execution_us: 0,
    })
    applyMessage(state, {
      type: 'columns',
      query: 0,
      columns: ['id', 'title'],
      right_aligned: [true, false],
      sql: 'y',
      limit: 'added',
      expected_rows: null,
      execution_us: 0,
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
      queries: [{ query: 0, tab: 0, sql: 'USE shop', start: 0, end: 8 }],
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

  it('suit la base choisie par une demande use', () => {
    const state = readyState()
    applyMessage(state, { type: 'database', database: 'shop' })
    expect(state.database).toBe('shop')
    expect(state.status).toBe('ready')
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

  it('efface les onglets et l’erreur précédents au début d’un lot, sauf les onglets épinglés', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [
        { query: 0, tab: 0, sql: 'a', start: 0, end: 1 },
        { query: 1, tab: 0, sql: 'b', start: 2, end: 3 },
      ],
    })
    state.tabs[1]!.pinned = true
    state.error = 'ancienne erreur'

    startBatch(state)
    expect(state.error).toBeNull()
    expect(state.tabs.map((tab) => tab.sql)).toEqual(['b'])

    // Le nouveau lot suit l'onglet épinglé ; ses messages ne touchent pas ce dernier,
    // qui portait pourtant le même numéro de requête dans l'ancien lot.
    applyMessage(state, {
      type: 'batch',
      queries: [{ query: 1, tab: 0, sql: 'c', start: 0, end: 1 }],
    })
    applyMessage(state, { type: 'started', query: 1 }, 1000)
    expect(state.tabs.map((tab) => [tab.sql, tab.startedAt])).toEqual([
      ['b', null],
      ['c', 1000],
    ])
    expect(new Set(state.tabs.map((tab) => tab.id)).size).toBe(2)
  })

  it('retire un onglet fermé', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [{ query: 0, tab: 0, sql: 'a', start: 0, end: 1 }],
    })
    const id = state.tabs[0]!.id
    expect(removeTab(state, id)?.sql).toBe('a')
    expect(state.tabs).toEqual([])
    expect(removeTab(state, id)).toBeUndefined()
  })
})

describe('tabProgress', () => {
  it('passe d’attente à exécution, puis chargement, puis terminé ou erreur', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [
        { query: 0, tab: 0, sql: 'a', start: 0, end: 1 },
        { query: 1, tab: 0, sql: 'b', start: 2, end: 3 },
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
      expected_rows: null,
      execution_us: 0,
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

describe('durées', () => {
  it("sépare l'exécution du transfert des lignes", () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [
        { query: 0, tab: 0, sql: 'a', start: 0, end: 1 },
        { query: 1, tab: 0, sql: 'b', start: 2, end: 3 },
      ],
    })
    const tab = state.tabs[0]!
    expect(executionDuration(tab, 0)).toBeNull()

    applyMessage(state, { type: 'started', query: 0 }, 1000)
    expect(executionDuration(tab, 1300)).toBe(300)
    expect(transferDuration(tab, 1300)).toBeNull()

    applyMessage(
      state,
      {
        type: 'columns',
        query: 0,
        columns: ['x'],
        right_aligned: [false],
        sql: 'a',
        limit: 'added',
        expected_rows: null,
        execution_us: 0,
      },
      1400,
    )
    // Colonnes reçues : le temps mesuré par le serveur (ici 0 µs dans le message).
    tab.result!.executionUs = 380_500
    expect(executionDuration(tab, 2000)).toBe(380.5)
    expect(transferDuration(tab, 2000)).toBe(600)

    applyMessage(
      state,
      {
        type: 'done',
        query: 0,
        row_count: 0,
        truncated: false,
        affected_rows: 0,
        elapsed_ms: 500,
        database: null,
      },
      2500,
    )
    expect(executionDuration(tab, 9999)).toBe(380.5)
    expect(transferDuration(tab, 9999)).toBe(1100)
  })

  it("mesure la part accomplie d'une requête, quand elle est connue", () => {
    const state = readyState()
    startBatch(state)
    applyMessage(state, {
      type: 'batch',
      queries: [{ query: 0, tab: 0, sql: 'a', start: 0, end: 1 }],
    })
    const tab = state.tabs[0]!
    expect(tabFraction(tab)).toBe(0)
    applyMessage(state, { type: 'started', query: 0 })
    expect(tabFraction(tab)).toBeNull()
    applyMessage(state, {
      type: 'columns',
      query: 0,
      columns: ['x'],
      right_aligned: [false],
      sql: 'a LIMIT 101',
      limit: 'added',
      expected_rows: 100,
      execution_us: 0,
    })
    applyMessage(state, { type: 'rows', query: 0, rows: [['1'], ['2'], ['3'], ['4']] })
    expect(tabFraction(tab)).toBe(0.04)
    tab.result!.expectedRows = null
    expect(tabFraction(tab)).toBeNull()
    applyMessage(state, { type: 'error', query: 0, message: 'perdu' })
    expect(tabFraction(tab)).toBe(1)
  })

  it('compte le volume reçu par requête', () => {
    const state = readyState()
    startBatch(state)
    applyMessage(
      state,
      { type: 'batch', queries: [{ query: 0, tab: 0, sql: 'a', start: 0, end: 1 }] },
      0,
      50,
    )
    applyMessage(
      state,
      {
        type: 'columns',
        query: 0,
        columns: ['x'],
        right_aligned: [false],
        sql: 'a',
        limit: 'added',
        expected_rows: null,
        execution_us: 0,
      },
      0,
      120,
    )
    applyMessage(state, { type: 'rows', query: 0, rows: [['1']] }, 0, 30)
    expect(state.tabs[0]!.bytes).toBe(150)
  })

  it('mesure en octets un texte UTF-8', () => {
    expect(utf8Length('abc')).toBe(3)
    expect(utf8Length('é')).toBe(2)
    expect(utf8Length('€')).toBe(3)
    expect(utf8Length('😀')).toBe(4)
  })

  it('formate les volumes', () => {
    expect(formatBytes(0)).toBe('0 o')
    expect(formatBytes(999)).toBe('999 o')
    expect(formatBytes(1000)).toBe('1,0 ko')
    expect(formatBytes(1_234_567)).toBe('1,2 Mo')
    expect(formatBytes(5_600_000_000)).toBe('5,6 Go')
  })

  it('formate les durées', () => {
    expect(formatDuration(0.25)).toBe('250 µs')
    expect(formatDuration(0.9994)).toBe('999 µs')
    expect(formatDuration(42)).toBe('42 ms')
    expect(formatDuration(999)).toBe('999 ms')
    expect(formatDuration(1000)).toBe('1,0 s')
    expect(formatDuration(12345)).toBe('12,3 s')
    expect(formatDuration(59_999)).toBe('59,9 s')
    expect(formatDuration(60_000)).toBe('1 min 00 s')
    expect(formatDuration(187_400)).toBe('3 min 07 s')
    expect(formatDuration(3_600_000)).toBe('1 h 00 min')
    expect(formatDuration(3_725_000)).toBe('1 h 02 min')
  })
})

describe('savedTab', () => {
  it('reprend un onglet enregistré, hors des lots de la session', () => {
    const tab = savedTab({
      id: 42,
      query: 1,
      sql: 'SELECT x FROM t',
      name: 'Gardé',
      pinned: true,
      result: {
        started: true,
        done: true,
        error: null,
        columns: {
          columns: ['x'],
          right_aligned: [true],
          sql: 'SELECT x FROM t LIMIT 1001',
          limit: 'added',
          expected_rows: 1000,
          execution_us: 1500,
        },
        row_count: 2,
        truncated: false,
        affected_rows: 0,
        transfer_us: 4000,
        bytes: 120,
      },
      rows: [['1'], ['2']],
    })
    expect([tab.id, tab.batch, tab.pinned, tab.name]).toEqual([42, 0, true, 'Gardé'])
    expect(tab.result?.rows).toEqual([['1'], ['2']])
    expect(executionDuration(tab, 0)).toBe(1.5)
    expect(transferDuration(tab, 0)).toBe(4)
    expect(tabProgress(tab)).toBe('done')
  })
})

describe('sessionUrl', () => {
  it('suit le protocole de la page', () => {
    expect(sessionUrl(3, { protocol: 'http:', host: 'localhost:5173' } as Location)).toBe(
      'ws://localhost:5173/api/editors/3/session',
    )
    expect(sessionUrl(3, { protocol: 'https:', host: 'prisme.example' } as Location)).toBe(
      'wss://prisme.example/api/editors/3/session',
    )
  })
})
