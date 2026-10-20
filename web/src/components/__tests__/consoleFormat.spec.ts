import { describe, expect, it } from 'vitest'

import type { QueryEntry } from '@/api/generated/QueryEntry'

import { messageOf, rowsOf, rowsTitleOf, transferOf } from '../consoleFormat'

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
  it('writes the SQL on one line, then the failure if any', () => {
    expect(messageOf(entry())).toBe('SELECT * FROM video')
    expect(messageOf(entry({ error: 'Unknown table' }))).toBe(
      'SELECT * FROM video — erreur : Unknown table',
    )
  })

  it('counts the rows read or changed', () => {
    expect(rowsOf(entry())).toBe('120')
    expect(rowsOf(entry({ row_count: 1000, truncated: true }))).toBe('+1 000')
    expect(rowsTitleOf(entry({ truncated: true }))).toBe('Lignes lues, résultat tronqué')
    expect(rowsOf(entry({ returns_rows: false, affected_rows: 3 }))).toBe('3')
    expect(rowsTitleOf(entry({ returns_rows: false, affected_rows: 3 }))).toBe('Lignes modifiées')
    expect(rowsOf(entry({ returns_rows: false }))).toBe('—')
  })

  it('shows a dash for a transfer not measured', () => {
    expect(transferOf(entry())).toBe('—')
    expect(transferOf(entry({ transfer_us: 2500 }))).toBe('3 ms')
  })
})
