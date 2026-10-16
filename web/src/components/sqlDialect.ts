/**
 * Dialect of the SQL editor (highlighting), built from the keywords of the
 * server's dialect (`crates/dialects/keywords/*.yaml`).
 *
 * The text-reading settings (quotes, `#` comments, `@` variables...) are
 * those of the MySQL and MariaDB dialects of `@codemirror/lang-sql`, identical
 * for both; only the word lists change. Each category goes into a CodeMirror
 * highlighting class: reserved and non-reserved words as keywords, types as
 * types, values as builtins (colored like numbers). `NULL`, `TRUE`, `FALSE`
 * and `UNKNOWN` are left to `lang-sql`, which recognizes them itself.
 */

import { SQLDialect } from '@codemirror/lang-sql'

import type { DialectKeywords } from '@/api/generated/DialectKeywords'

/**
 * Values that `lang-sql` recognizes itself (null and booleans).
 */
const NATIVE_VALUES = new Set(['null', 'true', 'false', 'unknown'])

/**
 * Word list as `lang-sql` expects it: in lower case, separated by spaces.
 *
 * @param words Words, in any case.
 */
function wordList(words: string[]): string {
  return words.map((word) => word.toLowerCase()).join(' ')
}

/**
 * Editor dialect for these keywords.
 *
 * With the session's `ANSI_QUOTES` mode, double quotes delimit identifiers
 * (and completion uses them to quote a name); otherwise they are strings,
 * and identifiers are quoted with backticks.
 *
 * @param keywords Keywords of the dialect, by category.
 * @param ansiQuotes Is the `ANSI_QUOTES` mode active on the session?
 */
export function editorDialect(keywords: DialectKeywords, ansiQuotes = false): SQLDialect {
  return SQLDialect.define({
    operatorChars: '*+-%<>!=&|^',
    charSetCasts: true,
    doubleQuotedStrings: !ansiQuotes,
    unquotedBitLiterals: true,
    hashComments: true,
    spaceAfterDashes: true,
    specialVar: '@?',
    identifierQuotes: ansiQuotes ? '"`' : '`',
    keywords: wordList([...keywords.reserved, ...keywords.keywords]),
    types: wordList(keywords.types),
    builtin: wordList(keywords.literals.filter((word) => !NATIVE_VALUES.has(word.toLowerCase()))),
  })
}
