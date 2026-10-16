import { describe, expect, it } from 'vitest'

import { editorDialect } from '../sqlDialect'

describe('editorDialect', () => {
  it('takes the keywords of the dialect, sorted by class', () => {
    const dialect = editorDialect({
      reserved: ['SELECT', 'FROM'],
      keywords: ['COMMIT'],
      types: ['VARCHAR'],
      literals: ['NULL', 'CURRENT_DATE'],
    })
    expect(dialect.spec.keywords).toBe('select from commit')
    expect(dialect.spec.types).toBe('varchar')
    expect(dialect.spec.builtin).toBe('current_date')
    // Same reading rules as the MySQL dialect of lang-sql.
    expect(dialect.spec.hashComments).toBe(true)
    expect(dialect.spec.identifierQuotes).toBe('`')
  })

  it('quotes identifiers with double quotes in ANSI_QUOTES', () => {
    const words = { reserved: [], keywords: [], types: [], literals: [] }
    const ansi = editorDialect(words, true)
    expect(ansi.spec.identifierQuotes).toBe('"`')
    expect(ansi.spec.doubleQuotedStrings).toBe(false)
    expect(editorDialect(words).spec.doubleQuotedStrings).toBe(true)
  })
})
