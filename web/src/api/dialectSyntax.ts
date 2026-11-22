/**
 * Syntax of a dialect, for the SQL editor's highlighting: its keywords and
 * the lexical rules of its texts.
 *
 * Both live on the server side: the keywords in one file per dialect
 * (`crates/dialects/keywords/*.yaml`), which the beautifier reads too, the
 * lexical rules with the dialect, which the splitter follows too.
 */

import { request } from './http'
import type { DialectSyntax } from './generated/DialectSyntax'
import type { Flavor } from './generated/Flavor'

/**
 * Reads the syntax of a dialect, its keywords limited to those of a server
 * version.
 *
 * @param flavor Flavor of the server.
 * @param version Version announced by the server; when absent, all the words.
 */
export async function fetchDialectSyntax(
  flavor: Flavor,
  version: string | null,
): Promise<DialectSyntax> {
  const query = version === null ? '' : `?version=${encodeURIComponent(version)}`
  return request<DialectSyntax>('GET', `/api/dialects/${flavor}/syntax${query}`, undefined, {
    sync: false,
  })
}
