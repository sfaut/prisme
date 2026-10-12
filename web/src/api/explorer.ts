/**
 * Explorateur d'objets : bases, tables et colonnes, chargées niveau par niveau.
 */

import { request } from './http'
import type { ColumnInfo } from './generated/ColumnInfo'
import type { TableInfo } from './generated/TableInfo'

/**
 * Liste les bases du serveur.
 *
 * @param connectionId Identifiant de la connexion.
 */
export function listSchemas(connectionId: number): Promise<string[]> {
  return request('GET', `/api/connections/${connectionId}/schemas`)
}

/**
 * Liste les tables et vues d'une base.
 *
 * @param connectionId Identifiant de la connexion.
 * @param schema Nom de la base.
 */
export function listTables(connectionId: number, schema: string): Promise<TableInfo[]> {
  return request(
    'GET',
    `/api/connections/${connectionId}/schemas/${encodeURIComponent(schema)}/tables`,
  )
}

/**
 * Liste les colonnes d'une table, dans l'ordre de déclaration.
 *
 * @param connectionId Identifiant de la connexion.
 * @param schema Nom de la base.
 * @param table Nom de la table.
 */
export function listColumns(
  connectionId: number,
  schema: string,
  table: string,
): Promise<ColumnInfo[]> {
  const path = `${encodeURIComponent(schema)}/tables/${encodeURIComponent(table)}`
  return request('GET', `/api/connections/${connectionId}/schemas/${path}/columns`)
}
