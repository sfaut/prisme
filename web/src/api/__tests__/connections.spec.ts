import { describe, expect, it } from 'vitest'

import { probeMessage, versionWarning } from '../connections'
import type { ServerInfo } from '../generated/ServerInfo'

/**
 * Identity of a server, supported unless `oldest_supported` is given.
 */
function server(overrides: Partial<ServerInfo>): ServerInfo {
  return {
    flavor: 'mysql',
    version: '8.4.11',
    ansi_quotes: false,
    oldest_supported: null,
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
