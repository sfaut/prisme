import { describe, expect, it } from 'vitest'

import type { CatalogSchema } from '@/stores/catalog'

import { completionSchema } from '../sqlCompletion'

describe('completionSchema', () => {
  it('quotes the names that are not plain identifiers, according to ANSI_QUOTES', () => {
    const schemas: CatalogSchema[] = [
      {
        name: 'ma base',
        tables: [{ name: 'clients', kind: 'table', details: null }],
        routines: [],
        events: [],
      },
    ]
    const backticks = completionSchema(schemas) as Record<string, { self: { apply?: string } }>
    expect(backticks['ma base']!.self.apply).toBe('`ma base`')
    const ansi = completionSchema(schemas, true) as Record<
      string,
      { self: { apply?: string }; children: Record<string, { self: { apply?: string } }> }
    >
    expect(ansi['ma base']!.self.apply).toBe('"ma base"')
    expect(ansi['ma base']!.children.clients!.self.apply).toBeUndefined()
  })

  it('quotes the names starting with a digit, which the server could read as numbers', () => {
    const schemas: CatalogSchema[] = ['123', '1e5', '2fa', 'été_2026', '$total', 't1'].map(
      (name) => ({ name, tables: [], routines: [], events: [] }),
    )
    const names = completionSchema(schemas) as Record<string, { self: { apply?: string } }>
    expect(names['123']!.self.apply).toBe('`123`')
    expect(names['1e5']!.self.apply).toBe('`1e5`')
    expect(names['2fa']!.self.apply).toBe('`2fa`')
    expect(names['été_2026']!.self.apply).toBeUndefined()
    expect(names['$total']!.self.apply).toBeUndefined()
    expect(names['t1']!.self.apply).toBeUndefined()
  })

  it('quotes the names that are reserved words of the server, whatever their case', () => {
    const schemas: CatalogSchema[] = [
      {
        name: 'shop',
        tables: [
          {
            name: 'order',
            kind: 'table',
            details: {
              table: 'order',
              columns: ['Select', 'total'].map((name) => ({
                name,
                column_type: 'int',
                nullable: false,
                key: '',
                auto_increment: false,
              })),
              indexes: [],
              foreign_keys: [],
              triggers: [],
            },
          },
        ],
        routines: [],
        events: [],
      },
    ]
    const namespace = completionSchema(schemas, false, new Set(['ORDER', 'SELECT']))
    expect(namespace).toMatchObject({
      shop: {
        children: {
          order: {
            self: { label: 'order', apply: '`order`' },
            children: [{ label: 'Select', apply: '`Select`' }, { label: 'total' }],
          },
        },
      },
    })
    expect(namespace).not.toHaveProperty(['shop', 'children', 'order', 'children', 1, 'apply'])
  })

  it('offers databases, tables, columns and routines with their nature, even with an incomplete catalog', () => {
    const schemas: CatalogSchema[] = [
      {
        name: 'shop',
        tables: [
          {
            name: 'orders',
            kind: 'table',
            details: {
              table: 'orders',
              columns: [
                {
                  name: 'id',
                  column_type: 'int(10) unsigned',
                  nullable: false,
                  key: 'PRI',
                  auto_increment: true,
                },
              ],
              indexes: [],
              foreign_keys: [],
              triggers: [],
            },
          },
          { name: 'v_orders', kind: 'view', details: null },
        ],
        routines: [
          { name: 'total_ttc', kind: 'function' },
          { name: 'purge', kind: 'procedure' },
        ],
        events: [],
      },
      { name: 'pas_encore', tables: null, routines: null, events: null },
    ]
    expect(completionSchema(schemas)).toEqual({
      shop: {
        self: { label: 'shop', type: 'namespace', detail: 'base' },
        children: {
          orders: {
            self: { label: 'orders', type: 'type', detail: 'table' },
            children: [{ label: 'id', type: 'property', detail: 'INT UNSIGNED' }],
          },
          v_orders: {
            self: { label: 'v_orders', type: 'type', detail: 'vue' },
            children: [],
          },
          total_ttc: {
            self: { label: 'total_ttc', type: 'function', detail: 'fonction' },
            children: [],
          },
          purge: {
            self: { label: 'purge', type: 'function', detail: 'procédure' },
            children: [],
          },
        },
      },
      pas_encore: {
        self: { label: 'pas_encore', type: 'namespace', detail: 'base' },
        children: {},
      },
    })
  })
})
