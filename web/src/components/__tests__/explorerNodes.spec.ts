import { describe, expect, it } from 'vitest'

import { type CatalogSchema, applyCatalogEvent, emptyCatalog } from '@/stores/catalog'

import {
  catalogNodes,
  displayedType,
  foreignKeyLabel,
  indexLabel,
  selectAllQuery,
  triggerLabel,
} from '../explorerNodes'

const catalog: CatalogSchema[] = [
  {
    name: 'shop',
    tables: [
      {
        name: 'orders',
        kind: 'table',
        details: {
          table: 'orders',
          columns: [
            { name: 'id', column_type: 'int', nullable: false, key: 'PRI' },
            { name: 'note', column_type: 'varchar(20)', nullable: true, key: '' },
            { name: 'client_id', column_type: 'int(10)', nullable: false, key: 'MUL' },
          ],
          indexes: [{ name: 'PRIMARY', columns: ['id'], unique: true }],
          foreign_keys: [
            {
              name: 'fk_client',
              columns: ['client_id'],
              referenced_schema: 'shop',
              referenced_table: 'clients',
              referenced_columns: ['id'],
            },
          ],
          triggers: [],
        },
      },
      { name: 'pending', kind: 'table', details: null },
      { name: 'v_orders', kind: 'view', details: null },
    ],
    routines: [
      { name: 'purge', kind: 'procedure' },
      { name: 'total', kind: 'function' },
    ],
  },
  { name: 'pas_encore', tables: null, routines: null },
]

/**
 * Libellés des enfants d'un nœud.
 */
function labels(node: { children?: { label: string }[] } | undefined): string[] {
  return node?.children?.map((child) => child.label) ?? []
}

describe('catalogNodes', () => {
  it("range les objets d'une base par dossier", () => {
    const [shop, pending] = catalogNodes(catalog)
    expect(labels(shop)).toEqual(['Tables (2)', 'Vues (1)', 'Procédures (1)', 'Fonctions (1)'])
    expect(labels(shop?.children?.[2])).toEqual(['purge'])
    expect(labels(pending)).toEqual(['chargement…'])
  })

  it('détaille une table, et montre ce qui reste à recevoir', () => {
    const tables = catalogNodes(catalog)[0]?.children?.[0]
    const orders = tables?.children?.[0]
    expect(orders?.table).toBe('orders')
    expect(labels(orders)).toEqual([
      'Colonnes (3)',
      'Index (1)',
      'Clefs étrangères (1)',
      'Déclencheurs (0)',
    ])
    expect(labels(orders?.children?.[0])).toEqual([
      'id · int',
      'note · varchar(20) · NULL',
      '#client_id · int',
    ])
    expect(orders?.children?.[3]?.isLeaf).toBe(true)
    const columns = orders?.children?.[0]?.children ?? []
    expect(columns.map((node) => node.column?.primaryKey)).toEqual([true, false, false])
    expect(columns.map((node) => node.column?.foreignKey)).toEqual([false, false, true])
    expect(labels(tables?.children?.[1])).toEqual(['chargement…'])
  })

  it('retire la largeur d’affichage des entiers, et seulement elle', () => {
    expect(displayedType('int(10) unsigned')).toBe('int unsigned')
    expect(displayedType('bigint(20)')).toBe('bigint')
    expect(displayedType('tinyint(1)')).toBe('tinyint')
    expect(displayedType('decimal(10,2)')).toBe('decimal(10,2)')
    expect(displayedType('varchar(50)')).toBe('varchar(50)')
    expect(displayedType('bit(1)')).toBe('bit(1)')
  })

  it('libelle index, clefs étrangères et déclencheurs', () => {
    expect(indexLabel({ name: 'PRIMARY', columns: ['id'], unique: true })).toBe('PRIMARY (id)')
    expect(indexLabel({ name: 'uq', columns: ['a', 'b'], unique: true })).toBe('uq (a, b) · unique')
    const key = {
      name: 'fk_customer',
      columns: ['customer_id'],
      referenced_schema: 'crm',
      referenced_table: 'customers',
      referenced_columns: ['id'],
    }
    expect(foreignKeyLabel('shop', key)).toBe('fk_customer (customer_id) → crm.customers (id)')
    expect(foreignKeyLabel('crm', key)).toBe('fk_customer (customer_id) → customers (id)')
    expect(triggerLabel({ name: 'orders_bi', timing: 'BEFORE', event: 'INSERT' })).toBe(
      'orders_bi · BEFORE INSERT',
    )
  })

  it('protège les noms dans la requête de contenu', () => {
    expect(selectAllQuery('ma base', 'drôle`nom')).toBe('SELECT * FROM `ma base`.`drôle``nom`')
  })
})

describe('applyCatalogEvent', () => {
  it('remplit le catalogue message par message', () => {
    const state = emptyCatalog()
    applyCatalogEvent(state, { type: 'schemas', schemas: ['a'] })
    expect(state.schemas).toEqual([{ name: 'a', tables: null, routines: null }])

    applyCatalogEvent(state, {
      type: 'objects',
      schema: 'a',
      objects: {
        tables: [
          { name: 't', kind: 'table' },
          { name: 'u', kind: 'table' },
        ],
        routines: [{ name: 'p', kind: 'procedure' }],
      },
    })
    applyCatalogEvent(state, {
      type: 'details',
      schema: 'a',
      tables: [
        {
          table: 't',
          columns: [{ name: 'id', column_type: 'int', nullable: false, key: '' }],
          indexes: [],
          foreign_keys: [],
          triggers: [],
        },
      ],
    })
    const [t, u] = state.schemas[0]?.tables ?? []
    expect(t?.details?.columns.map((column) => column.name)).toEqual(['id'])
    expect(u?.details?.columns).toEqual([])
    expect(state.schemas[0]?.routines).toEqual([{ name: 'p', kind: 'procedure' }])

    applyCatalogEvent(state, { type: 'error', message: 'refus' })
    applyCatalogEvent(state, { type: 'done' })
    expect(state.errors).toEqual(['refus'])
    expect(state.done).toBe(true)
  })
})
