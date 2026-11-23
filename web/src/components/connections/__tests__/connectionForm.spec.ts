import { describe, expect, it } from 'vitest'

import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'

import {
  connectedText,
  connectionOf,
  copyForm,
  formOf,
  isChanged,
  placeOf,
  placeOfForm,
  schemeOf,
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

/**
 * A saved SQLite connection: a main database in a file, a remote copy and a
 * database in memory attached to it.
 */
function savedFiles(): ConnectionSummary {
  return {
    id: 2,
    name: 'ventes',
    engine: 'sqlite',
    settings: {
      family: 'files',
      databases: [
        { schema: 'main', location: { scheme: 'file', address: 'D:\\data\\ventes.db' } },
        { schema: 'ref', location: { scheme: 'https', address: 'https://example.org/ref.db' } },
        { schema: 'brouillon', location: { scheme: 'memory', address: '' } },
      ],
    },
    init_sql: 'PRAGMA foreign_keys = ON;',
    session_timeout_minutes: 5,
    show_system_schemas: true,
    probe: null,
    created_at: '2026-11-23T10:00:00.000Z',
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

  it('starts a source held in files from its databases, and sees any change', () => {
    const form = formOf(savedFiles())
    expect(form.databases).toEqual([
      {
        schema: 'main',
        origin: 'existing',
        location: 'D:\\data\\ventes.db',
        saved: true,
        removed: false,
      },
      {
        schema: 'ref',
        origin: 'existing',
        location: 'https://example.org/ref.db',
        saved: true,
        removed: false,
      },
      { schema: 'brouillon', origin: 'memory', location: '', saved: true, removed: false },
    ])
    expect(isChanged(form, savedFiles())).toBe(false)
    form.databases[1]!.schema = 'refs'
    expect(isChanged(form, savedFiles())).toBe(true)
    form.databases[1]!.schema = 'ref'
    form.databases[2]!.location = 'D:\\unused.db'
    expect(isChanged(form, savedFiles())).toBe(false)
    const shorter = formOf(savedFiles())
    shorter.databases.pop()
    expect(isChanged(shorter, savedFiles())).toBe(true)
    form.databases[1]!.removed = true
    expect(isChanged(form, savedFiles())).toBe(true)
    form.databases[1]!.removed = false
    expect(isChanged(form, savedFiles())).toBe(false)
  })

  it('sends the databases held in files under their family', () => {
    const form = formOf(savedFiles())
    form.databases.push({
      schema: 'stats',
      origin: 'new',
      location: 'D:\\data\\stats.db',
      saved: false,
      removed: false,
    })
    form.databases[2]!.removed = true
    expect(connectionOf(form)).toEqual({
      name: 'ventes',
      engine: 'sqlite',
      settings: {
        family: 'files',
        databases: [
          { schema: 'main', location: 'D:\\data\\ventes.db', create: false },
          { schema: 'ref', location: 'https://example.org/ref.db', create: false },
          { schema: 'stats', location: 'D:\\data\\stats.db', create: true },
        ],
      },
      init_sql: 'PRAGMA foreign_keys = ON;',
      session_timeout_minutes: 5,
      show_system_schemas: true,
    })
  })

  it('copies the databases, not their input', () => {
    const form = formOf(savedFiles())
    const copy = copyForm(form)
    copy.databases[0]!.location = 'D:\\data\\autre.db'
    expect(form.databases[0]!.location).toBe('D:\\data\\ventes.db')
    form.databases[1]!.removed = true
    expect(copyForm(form).databases.map(({ schema, saved }) => [schema, saved])).toEqual([
      ['main', false],
      ['brouillon', false],
    ])
  })

  it('tells where a connection points', () => {
    expect(placeOf(saved().settings)).toBe('10.0.4.12:3306')
    expect(placeOf(savedFiles().settings)).toBe('D:\\data\\ventes.db')
    const form = formOf(savedFiles())
    expect(placeOfForm(form)).toBe('D:\\data\\ventes.db')
    form.databases[0]!.origin = 'memory'
    expect(placeOfForm(form)).toBe(':memory:')
  })

  it('reads the protocol of a location as typed', () => {
    expect(schemeOf('D:\\data\\v.db')).toBe('file')
    expect(schemeOf('D:/data/v.db')).toBe('file')
    expect(schemeOf('/srv/v.db')).toBe('file')
    expect(schemeOf('\\\\server\\share\\v.db')).toBe('file')
    expect(schemeOf('file:///D:/data/v.db')).toBe('file')
    expect(schemeOf(' HTTPS://example.org/v.db')).toBe('https')
    expect(schemeOf('s3://bucket/v.db')).toBe('s3')
    expect(schemeOf(':memory:')).toBe('memory')
    expect(schemeOf('v.db')).toBeNull()
    expect(schemeOf('')).toBeNull()
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
