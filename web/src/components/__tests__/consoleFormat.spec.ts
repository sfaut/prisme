import { describe, expect, it } from 'vitest'

import type { QueryEntry } from '@/api/generated/QueryEntry'

import { messageOf, outcomeOf, transferOf } from '../consoleFormat'

/**
 * Query of the history, with `changes` over a successful SELECT.
 */
function entry(changes: Partial<QueryEntry> = {}): QueryEntry {
  return {
    id: 1,
    started_at: '2026-10-20T12:40:00.123Z',
    action: 'run',
    connection_name: 'Local',
    sql: 'SELECT *\n  FROM video',
    duration_us: 1200,
    transfer_us: 0,
    returns_rows: true,
    row_count: 120,
    affected_rows: 0,
    truncated: false,
    error: '',
    ...changes,
  }
}

describe('console formats', () => {
  it('writes the SQL on one line, then the outcome', () => {
    expect(messageOf(entry())).toBe('SELECT * FROM video — 120 lignes')
    expect(outcomeOf(entry({ row_count: 1 }))).toBe('1 ligne')
    expect(outcomeOf(entry({ row_count: 1200, truncated: true }))).toBe(
      '1 200 lignes (résultat tronqué)',
    )
    expect(outcomeOf(entry({ returns_rows: false, affected_rows: 3 }))).toBe('3 lignes modifiées')
    expect(outcomeOf(entry({ returns_rows: false }))).toBe('OK')
    expect(outcomeOf(entry({ error: 'Unknown table' }))).toBe('erreur : Unknown table')
  })

  it('shows a dash for a transfer not measured', () => {
    expect(transferOf(entry())).toBe('—')
    expect(transferOf(entry({ transfer_us: 2500 }))).toBe('3 ms')
  })
})
