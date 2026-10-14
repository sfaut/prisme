/**
 * Dialecte de l'éditeur SQL (coloration), construit à partir des
 * mots-clefs du dialecte du serveur (`crates/dialects/keywords/*.yaml`).
 *
 * Les réglages de lecture du texte (guillemets, commentaires `#`, variables
 * `@`...) sont ceux des dialectes MySQL et MariaDB de `@codemirror/lang-sql`,
 * identiques pour les deux ; seules les listes de mots changent. Chaque
 * catégorie se range dans une classe de coloration de CodeMirror : mots
 * réservés et non réservés comme mots-clefs, types comme types, valeurs
 * comme mots intégrés (colorés comme les nombres). `NULL`, `TRUE`, `FALSE`
 * et `UNKNOWN` restent à `lang-sql`, qui les reconnaît lui-même.
 */

import { SQLDialect } from '@codemirror/lang-sql'

import type { DialectKeywords } from '@/api/generated/DialectKeywords'

/**
 * Valeurs que `lang-sql` reconnaît lui-même (nul et booléens).
 */
const NATIVE_VALUES = new Set(['null', 'true', 'false', 'unknown'])

/**
 * Liste de mots telle que l'attend `lang-sql` : en minuscules, séparés par
 * des espaces.
 *
 * @param words Mots, dans n'importe quelle casse.
 */
function wordList(words: string[]): string {
  return words.map((word) => word.toLowerCase()).join(' ')
}

/**
 * Dialecte de l'éditeur pour ces mots-clefs.
 *
 * Avec le mode `ANSI_QUOTES` de la session, les guillemets doubles
 * délimitent des identifiants (et la complétion les emploie pour protéger
 * un nom) ; sinon ce sont des chaînes, et les identifiants se protègent par
 * des accents graves.
 *
 * @param keywords Mots-clefs du dialecte, par catégorie.
 * @param ansiQuotes Le mode `ANSI_QUOTES` est-il actif sur la session ?
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
