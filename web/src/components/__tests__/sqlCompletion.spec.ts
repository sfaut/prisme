import { describe, expect, it } from 'vitest'

import type { CatalogSchema } from '@/stores/catalog'

import { completionSchema } from '../sqlCompletion'

describe('completionSchema', () => {
  it('propose bases, tables et colonnes avec leur type, même avec un catalogue incomplet', () => {
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
        routines: [],
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
        },
      },
      pas_encore: {
        self: { label: 'pas_encore', type: 'namespace', detail: 'base' },
        children: {},
      },
    })
  })
})
