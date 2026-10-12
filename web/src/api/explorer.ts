/**
 * Explorateur d'objets : catalogue complet d'une connexion (bases, tables, colonnes).
 */

import { request } from './http'
import type { SchemaInfo } from './generated/SchemaInfo'

/**
 * Lit tout le catalogue du serveur, en un seul appel.
 *
 * @param connectionId Identifiant de la connexion.
 */
export function loadCatalog(connectionId: number): Promise<SchemaInfo[]> {
  return request('GET', `/api/connections/${connectionId}/catalog`)
}
