import { describe, expect, it } from 'vitest'

import { MYSQL_CAPABILITIES } from '@/api/__tests__/capabilities'
import { probeMessage, unavailableWarning, versionWarning } from '../connections'
import type { DatabaseState } from '../generated/DatabaseState'
import type { SourceInfo } from '../generated/SourceInfo'

/**
 * Identity of a server, supported unless `oldest_supported` is given.
 */
function server(overrides: Partial<SourceInfo>): SourceInfo {
  return {
    flavor: 'mysql',
    version: '8.4.11',
    ansi_quotes: false,
    no_backslash_escapes: false,
    oldest_supported: null,
    capabilities: MYSQL_CAPABILITIES,
    ...overrides,
  }
}

describe('versions', () => {
  it('warns only on a server older than the oldest supported version', () => {
    expect(versionWarning(server({}))).toBeNull()
    expect(
      versionWarning(server({ flavor: 'mariadb', version: '10.5.29', oldest_supported: '10.6.3' })),
    ).toBe(
      'Version antérieure à MariaDB 10.6.3, la plus ancienne prise en charge par Prisme : les requêtes s’exécutent, sans garantie.',
    )
  })

  it('turns the message of a connection test into a warning on an older server', () => {
    expect(probeMessage(server({}))).toEqual({ tone: 'success', title: 'MySQL 8.4.11' })
    const old = probeMessage(server({ version: '5.7.44', oldest_supported: '8.0.11' }))
    expect(old.tone).toBe('warning')
    expect(old.title).toBe(
      'MySQL 5.7.44\nVersion antérieure à MySQL 8.0.11, la plus ancienne prise en charge par Prisme : les requêtes s’exécutent, sans garantie.',
    )
  })
})

describe('databases not opened', () => {
  /**
   * Database `schema` of the file at `address`, missing.
   */
  function missing(schema: string, address: string): DatabaseState {
    return {
      schema,
      location: { scheme: 'file', address },
      status: 'unavailable',
      failure: { reason: 'file_not_found', location: address },
    }
  }

  /**
   * Database `schema`, open.
   */
  function open(schema: string): DatabaseState {
    return {
      schema,
      location: { scheme: 'memory', address: '' },
      status: 'open',
      bytes: 0,
      journal_mode: 'memory',
      read_only: false,
    }
  }

  const sqlite = { flavor: 'sqlite' as const, version: '3.54.1' }

  it('tells each database not opened, and why', () => {
    expect(unavailableWarning(server({ ...sqlite, databases: [open('main')] }))).toBeNull()
    expect(
      unavailableWarning(
        server({ ...sqlite, databases: [open('main'), missing('ref', 'D:/a.db')] }),
      ),
    ).toBe(
      'La base « ref » ne s’ouvre pas : les sessions s’ouvrent sans elle.\nLe fichier D:/a.db n’existe pas.',
    )
    expect(
      unavailableWarning(
        server({ ...sqlite, databases: [missing('ref', 'D:/a.db'), missing('old', 'D:/b.db')] }),
      ),
    ).toBe(
      [
        '2 bases ne s’ouvrent pas : les sessions s’ouvrent sans elles.',
        '« ref » : Le fichier D:/a.db n’existe pas.',
        '« old » : Le fichier D:/b.db n’existe pas.',
      ].join('\n'),
    )
  })

  it('turns the message of a test into a warning, unless the states show beside it', () => {
    const info = server({ ...sqlite, databases: [open('main'), missing('ref', 'D:/a.db')] })
    expect(probeMessage(info).tone).toBe('warning')
    expect(probeMessage(info).title.split('\n')[0]).toBe('SQLite 3.54.1')
    expect(probeMessage(info, false)).toEqual({ tone: 'success', title: 'SQLite 3.54.1' })
  })
})
