/**
 * Complétion de l'éditeur SQL à partir du catalogue de la connexion.
 *
 * Le catalogue chargé pour l'explorateur devient l'espace de noms de
 * `@codemirror/lang-sql` : bases, puis tables et vues, puis colonnes. La
 * complétion propose alors les tables de la base en cours sans préfixe, et
 * les colonnes après `table.` ou `alias.`. Chaque proposition affiche sa
 * nature (base, table, vue) ou, pour une colonne, son type.
 */

import type { SQLNamespace } from '@codemirror/lang-sql'

import type { CatalogSchema, CatalogTable } from '@/stores/catalog'

import { displayedType, upperType } from './explorerNodes'

/**
 * Espace de noms de la complétion, construit à partir du catalogue, même
 * incomplet : une partie pas encore reçue n'est simplement pas proposée.
 *
 * @param schemas Bases du catalogue.
 */
export function completionSchema(schemas: CatalogSchema[]): SQLNamespace {
  const namespace: Record<string, SQLNamespace> = {}
  for (const schema of schemas) {
    const tables: Record<string, SQLNamespace> = {}
    for (const table of schema.tables ?? []) {
      tables[table.name] = tableNamespace(table)
    }
    namespace[schema.name] = {
      self: { label: schema.name, type: 'namespace', detail: 'base' },
      children: tables,
    }
  }
  return namespace
}

/**
 * Espace de noms d'une table ou d'une vue : la table elle-même, puis ses colonnes avec leur type.
 *
 * @param table Table ou vue du catalogue.
 */
function tableNamespace(table: CatalogTable): SQLNamespace {
  return {
    self: { label: table.name, type: 'type', detail: table.kind === 'table' ? 'table' : 'vue' },
    children: (table.details?.columns ?? []).map((column) => ({
      label: column.name,
      type: 'property',
      detail: upperType(displayedType(column.column_type)),
    })),
  }
}
