/**
 * Enjoliveur : réécrit du SQL en « beau SQL » (mots-clefs en capitales...).
 *
 * Les règles sont appliquées par le serveur (`crates/dialects/src/beautify.rs`).
 */

import { request } from './http'
import type { BeautifyRequest } from './generated/BeautifyRequest'
import type { Flavor } from './generated/Flavor'
import type { SqlText } from './generated/SqlText'

/**
 * Enjolive un texte SQL, une ou plusieurs requêtes.
 *
 * @param sql Texte à enjoliver.
 * @param routines
 *     Noms des fonctions et procédures de l'utilisateur, tels que définis :
 *     ils gardent leur casse, les autres fonctions passent en capitales.
 * @param flavor Variante du serveur, pour ses mots-clefs ; absente, MySQL.
 * @returns Le texte enjolivé.
 */
export async function beautify(
  sql: string,
  routines: string[] = [],
  flavor?: Flavor,
): Promise<string> {
  const body: BeautifyRequest = { sql, routines, flavor }
  const result = await request<SqlText>('POST', '/api/beautify', body)
  return result.sql
}
