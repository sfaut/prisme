import { describe, expect, it } from 'vitest'

import { flavorsOfWriteKind } from '../engines'
import type { EngineInfo } from '../generated/EngineInfo'

import { MYSQL_CAPABILITIES } from './capabilities'

/**
 * Engines of Prisme: MySQL without the files' switches, SQLite without
 * TRUNCATE.
 */
const ENGINES: EngineInfo[] = [
  {
    engine: 'mysql',
    flavors: ['mysql', 'mariadb'],
    capabilities: { ...MYSQL_CAPABILITIES, write_kinds: ['insert', 'truncate_table'] },
  },
  {
    engine: 'sqlite',
    flavors: ['sqlite'],
    capabilities: { ...MYSQL_CAPABILITIES, write_kinds: ['insert', 'attach'] },
  },
]

describe('engines', () => {
  it('gives the flavors whose engine knows a kind of write', () => {
    expect(flavorsOfWriteKind(ENGINES, 'insert')).toEqual(['mysql', 'mariadb', 'sqlite'])
    expect(flavorsOfWriteKind(ENGINES, 'truncate_table')).toEqual(['mysql', 'mariadb'])
    expect(flavorsOfWriteKind(ENGINES, 'attach')).toEqual(['sqlite'])
    expect(flavorsOfWriteKind(ENGINES, 'drop_event')).toEqual([])
  })
})
