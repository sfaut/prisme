import { describe, expect, it } from 'vitest'

import type { SchemaInfo } from '@/api/generated/SchemaInfo'

import { catalogNodes, selectAllQuery } from '../explorerNodes'

const catalog: SchemaInfo[] = [
  {
    name: 'test',
    tables: [
      {
        name: 't',
        kind: 'table',
        estimated_rows: 10,
        columns: [
          { name: 'id', column_type: 'int', nullable: false, key: 'PRI' },
          { name: 'label', column_type: 'varchar(20)', nullable: true, key: '' },
        ],
      },
      { name: 'v', kind: 'view', estimated_rows: null, columns: [] },
      { name: 'TABLES', kind: 'system', estimated_rows: null, columns: [] },
    ],
  },
  { name: 'vide', tables: [] },
]

describe('catalogNodes', () => {
  it('construit tout l’arbre', () => {
    const [test, empty] = catalogNodes(catalog)
    expect(test?.label).toBe('test')
    expect(test?.children?.map((node) => node.label)).toEqual(['t', 'v (vue)', 'TABLES (système)'])
    expect(empty?.isLeaf).toBe(true)
  })

  it('décrit les colonnes', () => {
    const table = catalogNodes(catalog)[0]?.children?.[0]
    expect(table?.isTable).toBe(true)
    expect(table?.children?.map((node) => node.label)).toEqual([
      'id · int · PRI',
      'label · varchar(20) · NULL',
    ])
    expect(table?.children?.[0]?.isLeaf).toBe(true)
  })

  it('protège les noms dans la requête de contenu', () => {
    expect(selectAllQuery('ma base', 'drôle`nom')).toBe('SELECT * FROM `ma base`.`drôle``nom`')
  })
})
