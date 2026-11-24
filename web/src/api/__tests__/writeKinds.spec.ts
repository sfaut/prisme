import { describe, expect, it } from 'vitest'

import { WRITE_KIND_GROUPS, WRITE_KIND_STATEMENTS, writeKindTooltip } from '../writeKinds'

describe('kinds of write', () => {
  it('groups every kind once, in the order of the server', () => {
    expect(WRITE_KIND_GROUPS.flatMap((group) => group.kinds.map(({ kind }) => kind))).toEqual(
      Object.keys(WRITE_KIND_STATEMENTS),
    )
  })

  it('tells in the tooltip what else a kind allows', () => {
    expect(writeKindTooltip('alter_table')).toEqual({
      title: 'ALTER TABLE',
      description: 'Aussi CREATE INDEX et DROP INDEX',
    })
    expect(writeKindTooltip('drop_view')).toEqual({ title: 'DROP VIEW', description: undefined })
    expect(writeKindTooltip('rename_temporary_table').title).toBe(
      'RENAME TABLE sur des tables temporaires',
    )
  })

  it('names in the tooltip the engines a kind concerns', () => {
    expect(writeKindTooltip('drop_view', ['mysql', 'mariadb', 'sqlite']).description).toBe(
      'Pour MySQL, MariaDB et SQLite',
    )
    expect(writeKindTooltip('alter_table', ['mysql', 'mariadb']).description).toBe(
      'Aussi CREATE INDEX et DROP INDEX\nPour MySQL et MariaDB',
    )
    expect(writeKindTooltip('attach', ['sqlite']).description?.split('\n')[1]).toBe('Pour SQLite')
  })
})
