import { describe, expect, it } from 'vitest'

import {
  SOURCE,
  SOURCE_LABEL,
  choiceItems,
  choiceLabel,
  effectiveDialect,
  parseChoice,
} from '../dialectChoice'

describe('dialectChoice', () => {
  it('takes the dialect of the editor, then the one of the profile', () => {
    expect(effectiveDialect('mysql 8.0.19', 'mariadb 10.6.3')).toBe('mysql 8.0.19')
    expect(effectiveDialect('', 'mariadb 10.6.3')).toBe('mariadb 10.6.3')
    expect(effectiveDialect('', undefined)).toBe(SOURCE)
  })

  it('reads a choice and names it', () => {
    expect(parseChoice('mysql 8.0.19')).toEqual({ id: 'mysql', version: '8.0.19' })
    expect(parseChoice(SOURCE)).toBeNull()
    expect(parseChoice('mysql')).toBeNull()
    expect(choiceLabel('mariadb 11.3.2')).toBe('MariaDB 11.3.2')
    expect(choiceLabel(SOURCE)).toBe(SOURCE_LABEL)
  })

  it('offers the source, then each version of each dialect', () => {
    const items = choiceItems([
      { id: 'mysql', label: 'MySQL', versions: ['8.0.11', '8.0.19'] },
      { id: 'mariadb', label: 'MariaDB', versions: ['10.6.3'] },
    ])
    expect(items.map((item) => item.value)).toEqual([
      SOURCE,
      'mysql 8.0.11',
      'mysql 8.0.19',
      'mariadb 10.6.3',
    ])
    expect(items[2]!.label).toBe('MySQL 8.0.19')
  })
})
