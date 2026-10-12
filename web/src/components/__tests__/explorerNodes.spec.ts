import { describe, expect, it } from 'vitest'

import { columnNodes, schemaNodes, selectAllQuery, tableNodes } from '../explorerNodes'

describe('explorerNodes', () => {
  it('construit les nœuds des bases', () => {
    expect(schemaNodes(['test'])).toEqual([
      { key: 'schema:test', label: 'test', isLeaf: false, schema: 'test' },
    ])
  })

  it('signale les vues et les tables système', () => {
    const labels = tableNodes('test', [
      { name: 't', kind: 'table', estimated_rows: 10 },
      { name: 'v', kind: 'view', estimated_rows: null },
      { name: 'TABLES', kind: 'system', estimated_rows: null },
    ]).map((node) => node.label)
    expect(labels).toEqual(['t', 'v (vue)', 'TABLES (système)'])
  })

  it('décrit les colonnes', () => {
    const [id, label] = columnNodes('test', 't', [
      { name: 'id', column_type: 'int', nullable: false, key: 'PRI' },
      { name: 'label', column_type: 'varchar(20)', nullable: true, key: '' },
    ])
    expect(id?.label).toBe('id · int · PRI')
    expect(label?.label).toBe('label · varchar(20) · NULL')
    expect(label?.isLeaf).toBe(true)
  })

  it('protège les noms dans la requête de contenu', () => {
    expect(selectAllQuery('ma base', 'drôle`nom')).toBe('SELECT * FROM `ma base`.`drôle``nom`')
  })
})
