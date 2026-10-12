import { describe, expect, it } from 'vitest'

import { type CatalogSchema, applyCatalogEvent, emptyCatalog } from '@/stores/catalog'

import { catalogNodes, selectAllQuery } from '../explorerNodes'

const catalog: CatalogSchema[] = [
  {
    name: 'test',
    tables: [
      {
        name: 't',
        kind: 'table',
        columns: [
          { name: 'id', column_type: 'int', nullable: false, key: 'PRI' },
          { name: 'label', column_type: 'varchar(20)', nullable: true, key: '' },
        ],
      },
      { name: 'v', kind: 'view', columns: null },
      { name: 'TABLES', kind: 'system', columns: [] },
    ],
  },
  { name: 'vide', tables: [] },
  { name: 'pas_encore', tables: null },
]

describe('catalogNodes', () => {
  it('construit l’arbre, même incomplet', () => {
    const [test, empty, pending] = catalogNodes(catalog)
    expect(test?.children?.map((node) => node.label)).toEqual(['t', 'v (vue)', 'TABLES (système)'])
    expect(empty?.isLeaf).toBe(true)
    expect(pending?.children?.map((node) => node.label)).toEqual(['chargement…'])
    expect(test?.children?.[1]?.children?.[0]?.disabled).toBe(true)
  })

  it('décrit les colonnes', () => {
    const table = catalogNodes(catalog)[0]?.children?.[0]
    expect(table?.isTable).toBe(true)
    expect(table?.children?.map((node) => node.label)).toEqual([
      'id · int · PRI',
      'label · varchar(20) · NULL',
    ])
  })

  it('protège les noms dans la requête de contenu', () => {
    expect(selectAllQuery('ma base', 'drôle`nom')).toBe('SELECT * FROM `ma base`.`drôle``nom`')
  })
})

describe('applyCatalogEvent', () => {
  it('remplit le catalogue message par message', () => {
    const state = emptyCatalog()
    applyCatalogEvent(state, { type: 'schemas', schemas: ['a', 'b'] })
    expect(state.schemas).toEqual([
      { name: 'a', tables: null },
      { name: 'b', tables: null },
    ])

    applyCatalogEvent(state, {
      type: 'tables',
      schema: 'a',
      tables: [
        { name: 't', kind: 'table' },
        { name: 'u', kind: 'table' },
      ],
    })
    applyCatalogEvent(state, {
      type: 'columns',
      schema: 'a',
      tables: [
        { table: 't', columns: [{ name: 'id', column_type: 'int', nullable: false, key: '' }] },
      ],
    })
    const [t, u] = state.schemas[0]?.tables ?? []
    expect(t?.columns?.map((column) => column.name)).toEqual(['id'])
    expect(u?.columns).toEqual([])

    applyCatalogEvent(state, { type: 'error', message: 'refus' })
    applyCatalogEvent(state, { type: 'done' })
    expect(state.errors).toEqual(['refus'])
    expect(state.done).toBe(true)
  })
})
