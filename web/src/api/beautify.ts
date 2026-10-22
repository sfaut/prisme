/**
 * Beautifier: rewrites SQL as "beautiful SQL" (keywords in capitals...).
 *
 * The rules are applied by the server (`crates/dialects/src/beautify.rs`).
 */

import { request } from './http'
import type { BeautifyCatalog } from './generated/BeautifyCatalog'
import type { BeautifyRequest } from './generated/BeautifyRequest'
import type { Flavor } from './generated/Flavor'
import type { SqlText } from './generated/SqlText'

/**
 * Context of beautifying: what is known about the server.
 */
export interface BeautifyContext {
  /**
   * Names of the catalog, as defined: the names of the query take their spelling.
   */
  catalog?: BeautifyCatalog
  /**
   * Flavor of the server, for its keywords; when absent, MySQL.
   */
  flavor?: Flavor
  /**
   * Version announced by the server; when absent, all the dialect's keywords.
   */
  version?: string
  /**
   * Width of an indentation level, in spaces, that of the SQL editor; when absent, 4.
   */
  indent_size?: number
}

/**
 * Beautifies a SQL text, one or more queries: the query is analysed, then its
 * keywords go to capitals and its names take the catalog's spelling.
 *
 * @param sql Text to beautify.
 * @param context Catalog, flavor and version of the server, indentation.
 * @returns The beautified text.
 */
export async function beautify(sql: string, context: BeautifyContext = {}): Promise<string> {
  const body: BeautifyRequest = { sql, ...context }
  const result = await request<SqlText>('POST', '/api/beautify', body, { sync: false })
  return result.sql
}
