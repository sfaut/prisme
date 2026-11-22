import { describe, expect, it } from 'vitest'

import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'

import {
  connectedText,
  connectionOf,
  copyForm,
  formOf,
  isChanged,
  sinceText,
} from '../connectionForm'

/**
 * A saved MariaDB connection, last connected at `connected_at`.
 *
 * @param connected_at Date of the last connection, `null` if never.
 */
function saved(connected_at: string | null = null): ConnectionSummary {
  return {
    id: 1,
    name: 'prod',
    engine: 'mysql',
    settings: {
      family: 'server',
      host: '10.0.4.12',
      port: 3306,
      username: 'lecteur',
      database_name: 'ventes',
    },
    init_sql: '',
    session_timeout_minutes: 5,
    show_system_schemas: false,
    probe:
      connected_at === null ? null : { flavor: 'mariadb', version: '11.4.4-MariaDB', connected_at },
    created_at: '2026-11-01T10:00:00.000Z',
  }
}

describe('connectionForm', () => {
  it('starts from the saved settings, password empty, and sees any change', () => {
    const form = formOf(saved())
    expect(form.password).toBe('')
    expect(isChanged(form, saved())).toBe(false)
    expect(isChanged({ ...form, port: 3307 }, saved())).toBe(true)
    // A typed password is a change, though the saved one is unknown.
    expect(isChanged({ ...form, password: 'secret' }, saved())).toBe(true)
  })

  it('sends the settings of the server under their family', () => {
    expect(connectionOf({ ...formOf(saved()), password: 'typed' })).toEqual({
      name: 'prod',
      engine: 'mysql',
      settings: {
        family: 'server',
        host: '10.0.4.12',
        port: 3306,
        username: 'lecteur',
        password: 'typed',
        database_name: 'ventes',
      },
      init_sql: '',
      session_timeout_minutes: 5,
      show_system_schemas: false,
    })
  })

  it('names a copy after its connection, with the typed password', () => {
    const copy = copyForm({ ...formOf(saved()), password: 'typed' })
    expect([copy.name, copy.host, copy.password]).toEqual(['prod-copie', '10.0.4.12', 'typed'])
    expect(copyForm({ ...formOf(saved()), name: 'x'.repeat(100) }).name).toHaveLength(100)
  })

  it('dates the last connection relative to now', () => {
    const now = Date.parse('2026-11-22T12:00:00.000Z')
    expect(sinceText('2026-11-22T11:59:50.000Z', now)).toBe('à l’instant')
    expect(sinceText('2026-11-22T11:55:00.000Z', now)).toBe('il y a 5 minutes')
    expect(sinceText('2026-11-21T12:00:00.000Z', now)).toBe('hier')
    expect(connectedText(saved('2026-11-22T10:00:00.000Z'), now)).toBe('Connectée il y a 2 heures')
    expect(connectedText(saved(), now)).toBe('Jamais connectée')
  })
})
