/**
 * Analyse du texte de l'éditeur par le serveur : requêtes du texte.
 */

import { request } from './http'
import type { Analysis } from './generated/Analysis'

/**
 * Analyse le texte de l'éditeur.
 *
 * @param sql Texte complet de l'éditeur.
 */
export function analyzeSql(sql: string): Promise<Analysis> {
  return request('POST', '/api/analyze', { sql })
}
