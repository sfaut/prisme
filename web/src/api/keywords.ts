/**
 * Keywords of a dialect, for the SQL editor's highlighting.
 *
 * The lists live on the server side, one file per dialect
 * (`crates/dialects/keywords/*.yaml`): the beautifier reads the same ones.
 */

import { request } from './http'
import type { DialectKeywords } from './generated/DialectKeywords'
import type { Flavor } from './generated/Flavor'

/**
 * Reads the keywords of a dialect, limited to those of a server version.
 *
 * @param flavor Flavor of the server.
 * @param version Version announced by the server; when absent, all the words.
 */
export async function fetchKeywords(
  flavor: Flavor,
  version: string | null,
): Promise<DialectKeywords> {
  const query = version === null ? '' : `?version=${encodeURIComponent(version)}`
  return request<DialectKeywords>('GET', `/api/dialects/${flavor}/keywords${query}`, undefined, {
    sync: false,
  })
}
