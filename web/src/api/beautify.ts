/**
 * Enjoliveur : réécrit du SQL en « beau SQL » (mots-clefs en capitales...).
 *
 * Les règles sont appliquées par le serveur (`crates/dialects/src/beautify.rs`).
 */

import { request } from './http'
import type { SqlText } from './generated/SqlText'

/**
 * Enjolive un texte SQL, une ou plusieurs requêtes.
 *
 * @param sql Texte à enjoliver.
 * @returns Le texte enjolivé.
 */
export async function beautify(sql: string): Promise<string> {
  const result = await request<SqlText>('POST', '/api/beautify', { sql })
  return result.sql
}
