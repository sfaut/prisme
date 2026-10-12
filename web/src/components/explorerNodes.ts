/**
 * Nœuds de l'arbre de l'explorateur d'objets : bases, tables, colonnes.
 *
 * Chaque niveau se charge à la demande, quand l'utilisateur ouvre un nœud.
 * Les fonctions de ce module construisent les nœuds à partir des réponses de
 * l'API ; elles ne dépendent pas de l'affichage.
 */

import type { ColumnInfo } from '@/api/generated/ColumnInfo'
import type { TableInfo } from '@/api/generated/TableInfo'

/**
 * Nœud de l'arbre, au format attendu par `NTree` de Naive UI.
 */
export interface ExplorerNode {
  /**
   * Clef unique dans l'arbre.
   */
  key: string
  /**
   * Texte affiché.
   */
  label: string
  /**
   * Feuille (colonne) : rien à charger en dessous.
   */
  isLeaf: boolean
  /**
   * Base à laquelle appartient le nœud.
   */
  schema: string
  /**
   * Table à laquelle appartient le nœud, absente pour une base.
   */
  table?: string
  /**
   * Enfants, absents tant qu'ils ne sont pas chargés.
   */
  children?: ExplorerNode[]
}

/**
 * Nœuds des bases du serveur.
 *
 * @param schemas Noms des bases.
 */
export function schemaNodes(schemas: string[]): ExplorerNode[] {
  return schemas.map((schema) => ({
    key: `schema:${schema}`,
    label: schema,
    isLeaf: false,
    schema,
  }))
}

/**
 * Nœuds des tables d'une base. Les vues et tables système sont signalées dans le libellé.
 *
 * @param schema Nom de la base.
 * @param tables Tables de la base.
 */
export function tableNodes(schema: string, tables: TableInfo[]): ExplorerNode[] {
  return tables.map((table) => ({
    key: `table:${schema}.${table.name}`,
    label: table.kind === 'table' ? table.name : `${table.name} (${kindLabel(table.kind)})`,
    isLeaf: false,
    schema,
    table: table.name,
  }))
}

/**
 * Nœuds des colonnes d'une table : nom, type, rôle dans les index, nullabilité.
 *
 * @param schema Nom de la base.
 * @param table Nom de la table.
 * @param columns Colonnes de la table.
 */
export function columnNodes(schema: string, table: string, columns: ColumnInfo[]): ExplorerNode[] {
  return columns.map((column) => {
    const details = [column.column_type, column.key, column.nullable ? 'NULL' : ''].filter(Boolean)
    return {
      key: `column:${schema}.${table}.${column.name}`,
      label: `${column.name} · ${details.join(' · ')}`,
      isLeaf: true,
      schema,
      table,
    }
  })
}

/**
 * Libellé de la nature d'une table, quand ce n'est pas une table ordinaire.
 *
 * @param kind Nature de la table.
 */
function kindLabel(kind: TableInfo['kind']): string {
  return kind === 'view' ? 'vue' : 'système'
}

/**
 * Requête qui affiche le contenu d'une table, avec des noms protégés par des accents graves.
 *
 * @param schema Nom de la base.
 * @param table Nom de la table.
 */
export function selectAllQuery(schema: string, table: string): string {
  return `SELECT * FROM ${quoteIdentifier(schema)}.${quoteIdentifier(table)}`
}

/**
 * Protège un identifiant MySQL : accents graves autour, accent grave interne doublé.
 *
 * @param name Identifiant à protéger.
 */
function quoteIdentifier(name: string): string {
  return `\`${name.replace(/`/g, '``')}\``
}
