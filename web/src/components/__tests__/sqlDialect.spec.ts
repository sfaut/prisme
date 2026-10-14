import { describe, expect, it } from 'vitest'

import { editorDialect } from '../sqlDialect'

describe('editorDialect', () => {
  it('reprend les mots-clefs du dialecte, rangés par classe', () => {
    const dialect = editorDialect({
      reserved: ['SELECT', 'FROM'],
      keywords: ['COMMIT'],
      types: ['VARCHAR'],
      literals: ['NULL', 'CURRENT_DATE'],
    })
    expect(dialect.spec.keywords).toBe('select from commit')
    expect(dialect.spec.types).toBe('varchar')
    expect(dialect.spec.builtin).toBe('current_date')
    // Mêmes règles de lecture que le dialecte MySQL de lang-sql.
    expect(dialect.spec.hashComments).toBe(true)
    expect(dialect.spec.identifierQuotes).toBe('`')
  })

  it('délimite les identifiants par des guillemets doubles en ANSI_QUOTES', () => {
    const words = { reserved: [], keywords: [], types: [], literals: [] }
    const ansi = editorDialect(words, true)
    expect(ansi.spec.identifierQuotes).toBe('"`')
    expect(ansi.spec.doubleQuotedStrings).toBe(false)
    expect(editorDialect(words).spec.doubleQuotedStrings).toBe(true)
  })
})
