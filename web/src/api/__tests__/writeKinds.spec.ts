import { describe, expect, it } from 'vitest'

import { WRITE_KIND_GROUPS, WRITE_KIND_STATEMENTS, writeKindTooltip } from '../writeKinds'

describe('kinds of write', () => {
  it('groups every kind once, in the order of the server', () => {
    expect(WRITE_KIND_GROUPS.flatMap((group) => group.kinds.map(({ kind }) => kind))).toEqual(
      Object.keys(WRITE_KIND_STATEMENTS),
    )
  })

  it('tells in the tooltip what else a kind allows', () => {
    expect(writeKindTooltip('alter_table')).toBe('ALTER TABLE. Aussi CREATE INDEX et DROP INDEX.')
    expect(writeKindTooltip('drop_view')).toBe('DROP VIEW')
    expect(writeKindTooltip('rename_temporary_table')).toBe(
      'RENAME TABLE sur des tables temporaires',
    )
  })
})
