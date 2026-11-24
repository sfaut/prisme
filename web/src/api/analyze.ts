/**
 * Analysis of the editor's text by the server: queries of the text and
 * syntax errors.
 */

import { request } from './http'
import type { Analysis } from './generated/Analysis'
import type { AnalyzeRequest } from './generated/AnalyzeRequest'
import type { DialectOptions } from './generated/DialectOptions'
import type { Flavor } from './generated/Flavor'

/**
 * Analyses the editor's text.
 *
 * @param sql Whole text of the editor.
 * @param flavor Flavor of the server, which sets the dialect of the analysis.
 * @param options
 *     Lexical settings of the session (`ANSI_QUOTES`): double quotes then
 *     delimit names.
 */
export function analyzeSql(
  sql: string,
  flavor: Flavor,
  options: DialectOptions,
): Promise<Analysis> {
  const body: AnalyzeRequest = { sql, flavor, ...options }
  return request('POST', '/api/analyze', body, { sync: false })
}
