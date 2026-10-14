import { describe, expect, it } from 'vitest'

import { type CatalogSchema, applyCatalogEvent, emptyCatalog } from '@/stores/catalog'

import {
  catalogNodes,
  displayedType,
  explorerTree,
  foreignKeyLabel,
  indexLabel,
  indexNote,
  selectAllQuery,
  sourceNodes,
  triggerLabel,
  upperType,
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
            { name: 'id', column_type: 'int', nullable: false, key: 'PRI', auto_increment: true },
            {
              name: 'note',
              column_type: 'varchar(20)',
              nullable: true,
              key: '',
              auto_increment: false,
            },
            {
              name: 'client_id',
              column_type: 'int(10)',
              nullable: false,
              key: 'MUL',
              auto_increment: false,
            },
          ],
          indexes: [
            { name: 'PRIMARY', columns: ['id'], unique: true },
            { name: 'client_id', columns: ['client_id'], unique: false },
          ],
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

describe('sourceNodes', () => {
  it('met une racine par source, ses bases en dessous', () => {
    const [first, second, never] = sourceNodes([
      {
        connectionId: 1,
        name: 'MariaDB Origen 2 (II)',
        catalog: { schemas: catalog, flavor: 'mariadb', done: true, errors: [] },
        currentDatabase: 'shop',
        flavor: 'mariadb',
        showSystemSchemas: true,
      },
      {
        connectionId: 2,
        name: 'Origen 2',
        catalog: { schemas: catalog, flavor: 'mariadb', done: true, errors: [] },
        currentDatabase: null,
        flavor: 'mysql',
        showSystemSchemas: true,
      },
      {
        connectionId: 3,
        name: 'Jamais lue',
        catalog: undefined,
        currentDatabase: null,
        flavor: null,
        showSystemSchemas: true,
      },
    ])
    expect([first?.label, second?.label, never?.label]).toEqual([
      'MariaDB Origen 2 (II)',
      'Origen 2',
      'Jamais lue',
    ])
    expect(labels(first)).toEqual(['shop', 'pas_encore'])
    expect(first?.children?.[0]?.current).toBe(true)
    expect(second?.children?.[0]?.current).toBe(false)
    expect(labels(never)).toEqual(['chargement…'])
    // Badge de la variante du serveur, absent tant qu'elle est inconnue.
    expect([first?.badge, second?.badge, never?.badge]).toEqual(['mariadb', 'mysql', undefined])
    // Mêmes bases dans deux sources : des clefs distinctes, et chaque nœud connaît sa connexion.
    expect(first?.children?.[0]?.key).not.toBe(second?.children?.[0]?.key)
    expect(second?.children?.[0]?.children?.[0]?.children?.[0]?.connectionId).toBe(2)
  })
})

describe('bases système', () => {
  it('cache les bases système si la connexion le demande', () => {
    const schemas = ['information_schema', 'mysql', 'shop', 'sys'].map((name) => ({
      name,
      tables: [],
      routines: [],
    }))
    const source = (showSystemSchemas: boolean) => ({
      connectionId: 1,
      name: 'Origen 2',
      catalog: { schemas, flavor: null, done: true, errors: [] },
      currentDatabase: null,
      flavor: null,
      showSystemSchemas,
    })
    expect(labels(sourceNodes([source(false)])[0])).toEqual(['shop'])
    expect(labels(sourceNodes([source(true)])[0])).toEqual([
      'information_schema',
      'mysql',
      'shop',
      'sys',
    ])
  })
})

describe('explorerTree', () => {
  it('range les sources sous « Sources de données », puis « Notebooks »', () => {
    const source = {
      connectionId: 1,
      name: 'Origen 2',
      catalog: undefined,
      currentDatabase: null,
      flavor: null,
      showSystemSchemas: true,
    }
    const [root, notebooks] = explorerTree([source], true)
    expect(notebooks?.label).toBe('Notebooks')
    expect(root?.label).toBe('Sources de données')
    expect(root?.note).toBe('chargement…')
    expect(labels(root)).toEqual(['Origen 2'])
    expect(explorerTree([], false)[0]?.isLeaf).toBe(true)
  })
})

describe('catalogNodes', () => {
  it("range les objets d'une base par dossier", () => {
    const [shop, pending] = catalogNodes(1, catalog)
    expect(labels(shop)).toEqual(['Tables (2)', 'Vues (1)', 'Procédures (1)', 'Fonctions (1)'])
    expect(labels(shop?.children?.[2])).toEqual(['purge'])
    expect(labels(pending)).toEqual(['chargement…'])
  })

  it('signale la base en cours', () => {
    const [shop, pending] = catalogNodes(1, catalog, 'shop')
    expect(shop?.current).toBe(true)
    expect(pending?.current).toBe(false)
  })

  it('détaille une table, et montre ce qui reste à recevoir', () => {
    const tables = catalogNodes(1, catalog)[0]?.children?.[0]
    const orders = tables?.children?.[0]
    expect(orders?.table).toBe('orders')
    expect(labels(orders)).toEqual([
      'Colonnes (3)',
      'Index (2)',
      'Clefs étrangères (1)',
      'Déclencheurs (0)',
    ])
    expect(labels(orders?.children?.[0])).toEqual([
      'id INT AUTO_INCREMENT',
      'note VARCHAR(20) NULL',
      'client_id INT',
    ])
    expect(orders?.children?.[3]?.isLeaf).toBe(true)
    const columns = orders?.children?.[0]?.children ?? []
    expect(columns.map((node) => node.column?.primaryKey)).toEqual([true, false, false])
    expect(columns.map((node) => node.column?.foreignKey)).toEqual([false, false, true])
    expect(columns.map((node) => node.column?.indexed)).toEqual([true, false, true])
    // Alignement : le nom le plus long (client_id) et au plus un symbole par colonne.
    expect(columns.map((node) => node.column?.nameWidth)).toEqual([9, 9, 9])
    expect(columns.map((node) => node.column?.markSlots)).toEqual([1, 1, 1])
    expect(labels(tables?.children?.[1])).toEqual(['chargement…'])
  })

  it('met les types en capitales, sauf les valeurs d’un enum', () => {
    expect(upperType('int unsigned')).toBe('INT UNSIGNED')
    expect(upperType("enum('a','B','it''s')")).toBe("ENUM('a','B','it''s')")
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
    expect(indexLabel({ name: 'uq', columns: ['a', 'b'], unique: true })).toBe('uq (a, b)')
    expect(indexNote({ name: 'PRIMARY', columns: ['id'], unique: true })).toBe('PRIMARY KEY')
    expect(indexNote({ name: 'uq', columns: ['a', 'b'], unique: true })).toBe('UNIQUE')
    expect(indexNote({ name: 'idx', columns: ['a'], unique: false })).toBeUndefined()
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
    expect(selectAllQuery('ma base', 'drôle`nom')).toBe('SELECT ALL * FROM `ma base`.`drôle``nom`')
    // ANSI_QUOTES : guillemets doubles, doublés dans le nom.
    expect(selectAllQuery('ma base', 'dit "x"', true)).toBe(
      'SELECT ALL * FROM "ma base"."dit ""x"""',
    )
  })
})

describe('applyCatalogEvent', () => {
  it('remplit le catalogue message par message', () => {
    const state = emptyCatalog()
    applyCatalogEvent(state, { type: 'schemas', schemas: ['a'], flavor: 'mysql' })
    expect(state.schemas).toEqual([{ name: 'a', tables: null, routines: null }])
    expect(state.flavor).toBe('mysql')

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
          columns: [
            { name: 'id', column_type: 'int', nullable: false, key: '', auto_increment: false },
          ],
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
