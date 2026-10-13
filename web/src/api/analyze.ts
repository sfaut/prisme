/**
 * Analyse du texte de l'éditeur par le serveur : requêtes du texte et
 * erreurs de syntaxe.
 */

import { request } from './http'
import type { Analysis } from './generated/Analysis'
import type { Flavor } from './generated/Flavor'

/**
 * Analyse le texte de l'éditeur.
 *
 * @param sql Texte complet de l'éditeur.
 * @param flavor Variante du serveur, qui fixe le dialecte d'analyse.
 */
export function analyzeSql(sql: string, flavor: Flavor): Promise<Analysis> {
  return request('POST', '/api/analyze', { sql, flavor })
}
