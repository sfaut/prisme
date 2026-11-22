import { describe, expect, it } from 'vitest'

import { MYSQL_CAPABILITIES, MYSQL_SOURCE } from '@/api/__tests__/capabilities'
import type { ForeignKeyInfo } from '@/api/generated/ForeignKeyInfo'
import { event, routine, schema, table, view } from '@/stores/__tests__/catalogFixtures'
import {
  type CatalogObject,
  type CatalogSchema,
  applyCatalogEvent,
  emptyCatalog,
} from '@/stores/catalog'

import {
  catalogNodes,
  displayedType,
  foreignKeyLabel,
  indexLabel,
  indexNote,
  quoteIfNeeded,
  referenceLabel,
  selectAllQuery,
  sourceNodes,
  triggerLabel,
  upperType,
} from '../navigatorNodes'

const catalog: CatalogSchema[] = [
  schema('shop', [
    table('orders', 1300, {
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
    }),
    table('pending', 0),
    view('v_orders'),
    routine('purge', 'procedure'),
    routine('total', 'function'),
    event('nettoyage', true),
    event('archive', false),
  ]),
  schema('pas_encore', null),
]

/**
 * Labels of the children of a node.
 */
function labels(node: { children?: { label: string }[] } | undefined): string[] {
  return node?.children?.map((child) => child.label) ?? []
}

describe('sourceNodes', () => {
  it('puts one root per source, its databases below', () => {
    const [first, second, never] = sourceNodes([
      {
        connectionId: 1,
        name: 'MariaDB Origen 2 (II)',
        catalog: {
          schemas: catalog,
          source: null,
          done: true,
          errors: [],
        },
        receiving: false,
        chosen: true,
        currentDatabase: 'shop',
        flavor: 'mariadb',
        version: '10.11.11-MariaDB',
        showSystemSchemas: true,
      },
      {
        connectionId: 2,
        name: 'Origen 2',
        catalog: { schemas: catalog, source: null, done: true, errors: [] },
        receiving: false,
        chosen: false,
        currentDatabase: null,
        flavor: 'mysql',
        version: null,
        showSystemSchemas: true,
      },
      {
        connectionId: 3,
        name: 'Jamais lue',
        catalog: undefined,
        receiving: false,
        chosen: false,
        currentDatabase: null,
        flavor: null,
        version: null,
        showSystemSchemas: true,
      },
    ])
    expect([first?.label, second?.label, never?.label]).toEqual([
      'MariaDB Origen 2 (II)',
      'Origen 2',
      'Jamais lue',
    ])
    expect(labels(first)).toEqual(['shop', 'pas_encore'])
    // The source of the editor.
    expect([first?.chosen, second?.chosen, never?.chosen]).toEqual([true, false, false])
    expect(first?.children?.[0]?.current).toBe(true)
    expect(second?.children?.[0]?.current).toBe(false)
    expect(labels(never)).toEqual(['chargement…'])
    // Badge of the server's flavor, absent as long as it is unknown.
    expect([first?.badge, second?.badge, never?.badge]).toEqual(['mariadb', 'mysql', undefined])
    // Its tooltip names the server, with its version once known.
    expect([first?.badgeVersion, second?.badgeVersion, never?.badgeVersion]).toEqual([
      '10.11.11-MariaDB',
      null,
      undefined,
    ])
    // Same databases in two sources: distinct keys, and each node knows its connection.
    expect(first?.children?.[0]?.key).not.toBe(second?.children?.[0]?.key)
    expect(second?.children?.[0]?.children?.[0]?.children?.[0]?.connectionId).toBe(2)
  })
})

describe('system databases', () => {
  it('hides the system databases if the connection asks for it', () => {
    const schemas = ['information_schema', 'mysql', 'shop', 'sys'].map((name) =>
      schema(name, [], name !== 'shop'),
    )
    const source = (showSystemSchemas: boolean) => ({
      connectionId: 1,
      name: 'Origen 2',
      catalog: { schemas, source: null, done: true, errors: [] },
      receiving: false,
      chosen: false,
      currentDatabase: null,
      flavor: null,
      version: null,
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

describe('receiving', () => {
  it('marks a source whose catalog is being received, without a note', () => {
    const source = (receiving: boolean) => ({
      connectionId: 1,
      name: 'Origen 2',
      catalog: undefined,
      receiving,
      chosen: false,
      currentDatabase: null,
      flavor: null,
      version: null,
      showSystemSchemas: true,
    })
    expect(sourceNodes([source(true)])[0]?.receiving).toBe(true)
    expect(sourceNodes([source(true)])[0]?.note).toBeUndefined()
    expect(sourceNodes([source(false)])[0]?.receiving).toBe(false)
  })
})

describe('catalogNodes', () => {
  it('puts the objects of a database by folder', () => {
    const [shop, pending] = catalogNodes(1, catalog)
    expect(labels(shop)).toEqual([
      'Tables (2)',
      'Vues (1)',
      'Procédures (1)',
      'Fonctions (1)',
      'Événements (2)',
    ])
    expect(labels(shop?.children?.[2])).toEqual(['purge'])
    expect(labels(shop?.children?.[4])).toEqual(['nettoyage', 'archive · désactivé'])
    expect(labels(pending)).toEqual(['chargement…'])
  })

  it('follows the capabilities of the source: folders of its kinds, USE if it has one', () => {
    const [shop] = catalogNodes(1, catalog, null, new Set(), {
      ...MYSQL_CAPABILITIES,
      switch_namespace: false,
      object_kinds: ['table', 'view'],
    })
    expect(labels(shop)).toEqual(['Tables (2)', 'Vues (1)'])
    expect(shop?.usable).toBe(false)
    expect(catalogNodes(1, catalog, null, new Set(), MYSQL_CAPABILITIES)[0]?.usable).toBe(true)
  })

  it('notes the estimated row count of a table, not of a view', () => {
    const [tables, views] = catalogNodes(1, catalog)[0]?.children ?? []
    expect(tables?.children?.map((node) => node.note)).toEqual(['1,3 k', '0'])
    expect(views?.children?.[0]?.note).toBeUndefined()
  })

  it('sorts the tables of a folder by name, or by row count when chosen', () => {
    const schemas: CatalogSchema[] = [
      schema('shop', [table('a', 10), table('b', null), table('c', 500), table('d', 10)]),
    ]
    const byName = catalogNodes(1, schemas)[0]?.children?.[0]
    expect(byName?.tableSort).toBe('name')
    expect(byName?.children?.map((node) => node.table)).toEqual(['a', 'b', 'c', 'd'])

    const byRows = catalogNodes(1, schemas, null, new Set([byName!.key]))[0]?.children?.[0]
    expect(byRows?.tableSort).toBe('rows')
    // The largest first, ties by name, without an estimate last.
    expect(byRows?.children?.map((node) => node.table)).toEqual(['c', 'a', 'd', 'b'])
    expect(byRows?.key).toBe('folder:tables:1/shop')
  })

  it('gives no order to a folder of less than two tables', () => {
    const schemas: CatalogSchema[] = [schema('shop', [table('a', 10)])]
    expect(catalogNodes(1, schemas)[0]?.children?.[0]?.tableSort).toBeUndefined()
  })

  it('marks the current database', () => {
    const [shop, pending] = catalogNodes(1, catalog, 'shop')
    expect(shop?.current).toBe(true)
    expect(pending?.current).toBe(false)
  })

  it('details a table, and shows what remains to be received', () => {
    const tables = catalogNodes(1, catalog)[0]?.children?.[0]
    const orders = tables?.children?.[0]
    expect(orders?.table).toBe('orders')
    expect(labels(orders)).toEqual([
      'Colonnes (3)',
      'Index (2)',
      'Clefs étrangères (1)',
      'Références (0)',
      'Déclencheurs (0)',
    ])
    expect(labels(orders?.children?.[0])).toEqual([
      'id INT AUTO_INCREMENT',
      'note VARCHAR(20) NULL',
      'client_id INT',
    ])
    expect(orders?.children?.[4]?.isLeaf).toBe(true)
    const columns = orders?.children?.[0]?.children ?? []
    expect(columns.map((node) => node.column?.primaryKey)).toEqual([true, false, false])
    expect(columns.map((node) => node.column?.foreignKey)).toEqual([false, false, true])
    expect(columns.map((node) => node.column?.indexed)).toEqual([true, false, true])
    // Alignment: the longest name (client_id).
    expect(columns.map((node) => node.column?.nameWidth)).toEqual([9, 9, 9])
    // An index designates its table and its name, not a table to show.
    const indexes = orders?.children?.[1]?.children ?? []
    expect(indexes.map((node) => node.index)).toEqual([
      { table: 'orders', name: 'PRIMARY' },
      { table: 'orders', name: 'client_id' },
    ])
    expect(indexes.map((node) => node.table)).toEqual([undefined, undefined])
    expect(labels(tables?.children?.[1])).toEqual(['chargement…'])
  })

  it('lists the foreign keys of every database that point to a table', () => {
    const key = (
      name: string,
      columns: string[],
      table: string,
      referenced: string[],
    ): ForeignKeyInfo => ({
      name,
      columns,
      referenced_schema: 'shop',
      referenced_table: table,
      referenced_columns: referenced,
    })
    const withKeys = (name: string, keys: ForeignKeyInfo[]): CatalogObject =>
      table(name, null, { table: name, columns: [], indexes: [], foreign_keys: keys, triggers: [] })
    const schemas: CatalogSchema[] = [
      schema('shop', [
        withKeys('address', [
          key('address_place', ['country', 'region_code'], 'customer', ['country', 'code']),
        ]),
        withKeys('customer', [key('customer_referrer', ['referrer_id'], 'customer', ['id'])]),
        withKeys('order', [key('order_customer', ['customer_id'], 'customer', ['id'])]),
      ]),
      schema('crm', [
        withKeys('contact', [key('contact_customer', ['customer_id'], 'customer', ['id'])]),
      ]),
      schema('pas_encore', null),
    ]
    const tables = catalogNodes(1, schemas)[0]?.children?.[0]?.children ?? []
    const folders = (name: string) => tables.find((node) => node.table === name)?.children ?? []
    expect(folders('customer')[3]?.label).toBe('Références (4)')
    expect(labels(folders('customer')[3])).toEqual([
      '(country, code) ← address (country, region_code)',
      'id ← crm.contact.customer_id',
      'id ← customer.referrer_id',
      'id ← order.customer_id',
    ])
    expect(folders('order')[3]?.label).toBe('Références (0)')
    expect(folders('order')[3]?.isLeaf).toBe(true)
  })

  it('puts the types in capitals, except the values of an enum', () => {
    expect(upperType('int unsigned')).toBe('INT UNSIGNED')
    expect(upperType("enum('a','B','it''s')")).toBe("ENUM('a','B','it''s')")
  })

  it('removes the display width of integers, and only it', () => {
    expect(displayedType('int(10) unsigned')).toBe('int unsigned')
    expect(displayedType('bigint(20)')).toBe('bigint')
    expect(displayedType('tinyint(1)')).toBe('tinyint')
    expect(displayedType('decimal(10,2)')).toBe('decimal(10,2)')
    expect(displayedType('varchar(50)')).toBe('varchar(50)')
    expect(displayedType('bit(1)')).toBe('bit(1)')
  })

  it('labels indexes, foreign keys and triggers', () => {
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

  it('labels a reference, a composite key and a reference of another database', () => {
    const reference = {
      schema: 'shop',
      table: 'address',
      key: {
        name: 'fk_address_place',
        columns: ['country', 'region_code'],
        referenced_schema: 'shop',
        referenced_table: 'customer',
        referenced_columns: ['country', 'code'],
      },
    }
    expect(referenceLabel('shop', reference)).toBe(
      '(country, code) ← address (country, region_code)',
    )
    expect(referenceLabel('crm', reference)).toBe(
      '(country, code) ← shop.address (country, region_code)',
    )
    const itself = {
      schema: 'shop',
      table: 'customer',
      key: {
        name: 'fk_referrer',
        columns: ['referrer_id'],
        referenced_schema: 'shop',
        referenced_table: 'customer',
        referenced_columns: ['id'],
      },
    }
    expect(referenceLabel('shop', itself)).toBe('id ← customer.referrer_id')
  })

  it('quotes the names in the content query', () => {
    expect(selectAllQuery('ma base', 'drôle`nom')).toBe('SELECT ALL * FROM `ma base`.`drôle``nom`')
    // ANSI_QUOTES: double quotes, doubled in the name.
    expect(selectAllQuery('ma base', 'dit "x"', true)).toBe(
      'SELECT ALL * FROM "ma base"."dit ""x"""',
    )
  })

  it('quotes a name only when it is not a plain identifier or is a reserved word', () => {
    const reserved = new Set(['SELECT'])
    expect(quoteIfNeeded('created_at', false, reserved)).toBe('created_at')
    expect(quoteIfNeeded('créé', false, reserved)).toBe('créé')
    expect(quoteIfNeeded('a b', false, reserved)).toBe('`a b`')
    expect(quoteIfNeeded('1e5', false, reserved)).toBe('`1e5`')
    expect(quoteIfNeeded('select', false, reserved)).toBe('`select`')
    expect(quoteIfNeeded('select', true, reserved)).toBe('"select"')
  })
})

describe('applyCatalogEvent', () => {
  it('fills the catalog message by message', () => {
    const state = emptyCatalog()
    applyCatalogEvent(state, {
      type: 'namespaces',
      source: MYSQL_SOURCE,
      namespaces: [{ namespace: ['a'], system: false }],
    })
    expect(state.schemas).toEqual([schema('a', null)])
    expect(state.source).toEqual(MYSQL_SOURCE)

    applyCatalogEvent(state, {
      type: 'objects',
      namespace: ['a'],
      objects: [
        { name: 't', kind: 'table', rows: 0, enabled: null },
        { name: 'u', kind: 'table', rows: 0, enabled: null },
        // A procedure named as a table: the details of the table are not its own.
        { name: 't', kind: 'procedure', rows: null, enabled: null },
      ],
    })
    applyCatalogEvent(state, {
      type: 'details',
      namespace: ['a'],
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
    const [t, u, p] = state.schemas[0]?.objects ?? []
    expect(t?.details?.columns.map((column) => column.name)).toEqual(['id'])
    expect(u?.details?.columns).toEqual([])
    expect(p?.details).toBeNull()

    applyCatalogEvent(state, {
      type: 'error',
      message: '',
      detail: {
        code: 'catalog_unreadable',
        schema: 'shop',
        flavor: 'mysql',
        cause: {
          source: 'server',
          number: 1142,
          sqlstate: '42000',
          message: 'SELECT command denied',
        },
      },
    })
    applyCatalogEvent(state, { type: 'done' })
    expect(state.errors).toEqual([
      'Lecture du catalogue impossible (base « shop »).\n\n' +
        'Le serveur MySQL a répondu (erreur 1142, SQLSTATE 42000) :\n\nSELECT command denied',
    ])
    expect(state.done).toBe(true)
  })
})
