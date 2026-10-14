/**
 * Mots-clefs d'un dialecte, pour la coloration de l'éditeur SQL.
 *
 * Les listes vivent côté serveur, un fichier par dialecte
 * (`crates/dialects/keywords/*.yaml`) : l'enjoliveur lit les mêmes.
 */

import { request } from './http'
import type { DialectKeywords } from './generated/DialectKeywords'
import type { Flavor } from './generated/Flavor'

/**
 * Lit les mots-clefs d'un dialecte, limités à ceux d'une version du serveur.
 *
 * @param flavor Variante du serveur.
 * @param version Version annoncée par le serveur ; absente, tous les mots.
 */
export async function fetchKeywords(
  flavor: Flavor,
  version: string | null,
): Promise<DialectKeywords> {
  const query = version === null ? '' : `?version=${encodeURIComponent(version)}`
  return request<DialectKeywords>('GET', `/api/dialects/${flavor}/keywords${query}`)
}
