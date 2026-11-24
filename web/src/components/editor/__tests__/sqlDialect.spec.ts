import { describe, expect, it } from 'vitest'

import type { LexicalRules } from '@/api/generated/LexicalRules'

import { editorDialect } from '../sqlDialect'

/**
 * Lexical rules of MySQL, as the server gives them.
 */
const MYSQL: LexicalRules = {
  hash_comments: true,
  space_after_dashes: true,
  backslash_escapes: true,
  double_quoted_strings: true,
  identifier_quotes: '`',
  bracket_identifiers: false,
  delimiter_command: true,
  trigger_bodies: false,
}

/**
 * Lexical rules of SQLite, as the server gives them.
 */
const SQLITE: LexicalRules = {
  hash_comments: false,
  space_after_dashes: false,
  backslash_escapes: false,
  double_quoted_strings: false,
  identifier_quotes: '"`',
  bracket_identifiers: true,
  delimiter_command: false,
  trigger_bodies: true,
}

/**
 * Syntax without keywords, with the lexical rules given.
 *
 * @param lexical Lexical rules of the dialect.
 */
function syntaxOf(lexical: LexicalRules) {
  return { reserved: [], keywords: [], types: [], literals: [], lexical }
}

describe('editorDialect', () => {
  it('takes the keywords of the dialect, sorted by class', () => {
    const dialect = editorDialect(
      {
        reserved: ['SELECT', 'FROM'],
        keywords: ['COMMIT'],
        types: ['VARCHAR'],
        literals: ['NULL', 'CURRENT_DATE'],
        lexical: MYSQL,
      },
      'mysql',
    )
    expect(dialect.spec.keywords).toBe('select from commit')
    expect(dialect.spec.types).toBe('varchar')
    expect(dialect.spec.builtin).toBe('current_date')
  })

  it('reads the texts by the lexical rules of the dialect', () => {
    const mysql = editorDialect(syntaxOf(MYSQL), 'mysql').spec
    expect([mysql.hashComments, mysql.spaceAfterDashes, mysql.backslashEscapes]).toEqual([
      true,
      true,
      true,
    ])
    expect([mysql.doubleQuotedStrings, mysql.identifierQuotes]).toEqual([true, '`'])
    const sqlite = editorDialect(syntaxOf(SQLITE), 'sqlite').spec
    expect([sqlite.hashComments, sqlite.spaceAfterDashes, sqlite.backslashEscapes]).toEqual([
      false,
      false,
      false,
    ])
    expect([sqlite.doubleQuotedStrings, sqlite.identifierQuotes]).toEqual([false, '"`['])
  })

  it('reads the tokens of the flavor', () => {
    const mysql = editorDialect(syntaxOf(MYSQL), 'mysql').spec
    expect([mysql.charSetCasts, mysql.unquotedBitLiterals, mysql.specialVar]).toEqual([
      true,
      true,
      '@?',
    ])
    const sqlite = editorDialect(syntaxOf(SQLITE), 'sqlite').spec
    expect([sqlite.charSetCasts, sqlite.unquotedBitLiterals, sqlite.specialVar]).toEqual([
      false,
      false,
      '@:?$',
    ])
    expect(sqlite.operatorChars).toContain('~')
  })

  it('quotes identifiers with double quotes in ANSI_QUOTES', () => {
    const options = { ansi_quotes: true, no_backslash_escapes: false }
    const ansi = editorDialect(syntaxOf(MYSQL), 'mysql', options)
    expect(ansi.spec.identifierQuotes).toBe('"`')
    expect(ansi.spec.doubleQuotedStrings).toBe(false)
    expect(editorDialect(syntaxOf(SQLITE), 'sqlite', options).spec.identifierQuotes).toBe('"`[')
  })

  it('reads the backslash as a character in NO_BACKSLASH_ESCAPES', () => {
    expect(editorDialect(syntaxOf(MYSQL), 'mysql').spec.backslashEscapes).toBe(true)
    const options = { ansi_quotes: false, no_backslash_escapes: true }
    expect(editorDialect(syntaxOf(MYSQL), 'mysql', options).spec.backslashEscapes).toBe(false)
  })
})
