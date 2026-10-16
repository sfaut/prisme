/**
 * Analysis of the editor's text by the server: queries of the text and
 * syntax errors.
 */

import { request } from './http'
import type { Analysis } from './generated/Analysis'
import type { Flavor } from './generated/Flavor'

/**
 * Analyses the editor's text.
 *
 * @param sql Whole text of the editor.
 * @param flavor Flavor of the server, which sets the dialect of the analysis.
 */
export function analyzeSql(sql: string, flavor: Flavor): Promise<Analysis> {
  return request('POST', '/api/analyze', { sql, flavor })
}
