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
      },
      { name: 'pas_encore', tables: null, routines: null },
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
