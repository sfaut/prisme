/**
 * Dialect of the SQL editor (highlighting), built from the syntax of the
 * server's dialect: its keywords (`crates/dialects/keywords/*.yaml`) and the
 * lexical rules of its texts, which the splitter of the server follows too.
 *
 * The lexical rules give the comments (`#`, `--` with or without a space),
 * the quotes of the strings and of the names (square brackets included), and
 * the backslash escapes. The operators, the variables and parameters (`@`,
 * `?`, `:`, `$`), the `_utf8'…'` casts and the `0b101` bit values are those
 * of the flavor (`FLAVORS`, `flavors.ts`).
 * Each category of words goes into a CodeMirror highlighting class: reserved
 * and non-reserved words as keywords, types as types, values as builtins
 * (colored like numbers). `NULL`, `TRUE`, `FALSE` and `UNKNOWN` are left to
 * `lang-sql`, which recognizes them itself.
 */

import { SQLDialect } from '@codemirror/lang-sql'

import { FLAVORS } from '@/api/flavors'
import type { DialectOptions } from '@/api/generated/DialectOptions'
import type { DialectSyntax } from '@/api/generated/DialectSyntax'
import type { Flavor } from '@/api/generated/Flavor'

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
 * quotes are never those of a string. With its `NO_BACKSLASH_ESCAPES` mode,
 * the backslash is an ordinary character of a string.
 *
 * @param syntax Keywords of the dialect, by category, and lexical rules.
 * @param flavor Flavor of the source, which gives the other tokens.
 * @param options Lexical settings of the session; none by default.
 */
export function editorDialect(
  syntax: DialectSyntax,
  flavor: Flavor,
  options: DialectOptions = { ansi_quotes: false, no_backslash_escapes: false },
): SQLDialect {
  const { lexical } = syntax
  const tokens = FLAVORS[flavor].editor
  const doubleQuotedStrings = lexical.double_quoted_strings && !options.ansi_quotes
  const quotes =
    doubleQuotedStrings || lexical.identifier_quotes.includes('"')
      ? lexical.identifier_quotes
      : `"${lexical.identifier_quotes}`
  return SQLDialect.define({
    operatorChars: tokens.operatorChars,
    charSetCasts: tokens.charSetCasts,
    doubleQuotedStrings,
    unquotedBitLiterals: tokens.unquotedBitLiterals,
    hashComments: lexical.hash_comments,
    spaceAfterDashes: lexical.space_after_dashes,
    backslashEscapes: lexical.backslash_escapes && !options.no_backslash_escapes,
    specialVar: tokens.specialVar,
    identifierQuotes: lexical.bracket_identifiers ? `${quotes}[` : quotes,
    keywords: wordList([...syntax.reserved, ...syntax.keywords]),
    types: wordList(syntax.types),
    builtin: wordList(syntax.literals.filter((word) => !NATIVE_VALUES.has(word.toLowerCase()))),
  })
}
