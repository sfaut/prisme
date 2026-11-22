/**
 * Dialect of the SQL editor (highlighting), built from the syntax of the
 * server's dialect: its keywords (`crates/dialects/keywords/*.yaml`) and the
 * lexical rules of its texts, which the splitter of the server follows too.
 *
 * The lexical rules give the comments (`#`, `--` with or without a space),
 * the quotes of the strings and of the names, and the backslash escapes. The
 * operators, the `@` variables, the `_utf8'…'` casts and the `b'…'` bit
 * values stay those of the MySQL and MariaDB dialects of `@codemirror/lang-sql`.
 * Each category of words goes into a CodeMirror highlighting class: reserved
 * and non-reserved words as keywords, types as types, values as builtins
 * (colored like numbers). `NULL`, `TRUE`, `FALSE` and `UNKNOWN` are left to
 * `lang-sql`, which recognizes them itself.
 */

import { SQLDialect } from '@codemirror/lang-sql'

import type { DialectSyntax } from '@/api/generated/DialectSyntax'

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
 * Editor dialect for this syntax.
 *
 * With the session's `ANSI_QUOTES` mode, double quotes delimit identifiers
 * (and completion uses them to quote a name), as in a dialect whose double
 * quotes are never those of a string.
 *
 * @param syntax Keywords of the dialect, by category, and lexical rules.
 * @param ansiQuotes Is the `ANSI_QUOTES` mode active on the session?
 */
export function editorDialect(syntax: DialectSyntax, ansiQuotes = false): SQLDialect {
  const { lexical } = syntax
  const doubleQuotedStrings = lexical.double_quoted_strings && !ansiQuotes
  const identifierQuotes =
    doubleQuotedStrings || lexical.identifier_quotes.includes('"')
      ? lexical.identifier_quotes
      : `"${lexical.identifier_quotes}`
  return SQLDialect.define({
    operatorChars: '*+-%<>!=&|^',
    charSetCasts: true,
    doubleQuotedStrings,
    unquotedBitLiterals: true,
    hashComments: lexical.hash_comments,
    spaceAfterDashes: lexical.space_after_dashes,
    backslashEscapes: lexical.backslash_escapes,
    specialVar: '@?',
    identifierQuotes,
    keywords: wordList([...syntax.reserved, ...syntax.keywords]),
    types: wordList(syntax.types),
    builtin: wordList(syntax.literals.filter((word) => !NATIVE_VALUES.has(word.toLowerCase()))),
  })
}
