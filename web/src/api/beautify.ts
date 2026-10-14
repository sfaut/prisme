/**
 * Enjoliveur : réécrit du SQL en « beau SQL » (mots-clefs en capitales...).
 *
 * Les règles sont appliquées par le serveur (`crates/dialects/src/beautify.rs`).
 */

import { request } from './http'
import type { BeautifyCatalog } from './generated/BeautifyCatalog'
import type { BeautifyRequest } from './generated/BeautifyRequest'
import type { Flavor } from './generated/Flavor'
import type { SqlText } from './generated/SqlText'

/**
 * Contexte de l'enjolivement : ce que l'on sait du serveur.
 */
export interface BeautifyContext {
  /**
   * Noms du catalogue, tels que définis : les noms de la requête prennent leur écriture.
   */
  catalog?: BeautifyCatalog
  /**
   * Variante du serveur, pour ses mots-clefs ; absente, MySQL.
   */
  flavor?: Flavor
  /**
   * Version annoncée par le serveur ; absente, tous les mots-clefs du dialecte.
   */
  version?: string
}

/**
 * Enjolive un texte SQL, une ou plusieurs requêtes : la requête est
 * analysée, puis ses mots-clefs passent en capitales et ses noms prennent
 * l'écriture du catalogue.
 *
 * @param sql Texte à enjoliver.
 * @param context Catalogue, variante et version du serveur.
 * @returns Le texte enjolivé.
 */
export async function beautify(sql: string, context: BeautifyContext = {}): Promise<string> {
  const body: BeautifyRequest = { sql, ...context }
  const result = await request<SqlText>('POST', '/api/beautify', body)
  return result.sql
}
