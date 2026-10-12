/**
 * Nœuds de l'arbre de l'explorateur d'objets : bases, tables, colonnes.
 *
 * Les fonctions de ce module construisent tout l'arbre à partir du catalogue
 * renvoyé par l'API ; elles ne dépendent pas de l'affichage.
 */

import type { ColumnInfo } from '@/api/generated/ColumnInfo'
import type { SchemaInfo } from '@/api/generated/SchemaInfo'
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
   * Feuille : rien en dessous (colonne, ou base sans table).
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
   * Enfants : tables d'une base, colonnes d'une table.
   */
  children?: ExplorerNode[]
  /**
   * Nœud de table (et non de base ou de colonne).
   */
  isTable: boolean
}

/**
 * Arbre complet du catalogue : bases, puis tables, puis colonnes.
 *
 * @param schemas Catalogue du serveur.
 */
export function catalogNodes(schemas: SchemaInfo[]): ExplorerNode[] {
  return schemas.map((schema) => ({
    key: `schema:${schema.name}`,
    label: schema.name,
    isLeaf: schema.tables.length === 0,
    isTable: false,
    schema: schema.name,
    children: schema.tables.map((table) => tableNode(schema.name, table)),
  }))
}

/**
 * Nœud d'une table, avec ses colonnes. Les vues et tables système sont signalées dans le libellé.
 *
 * @param schema Nom de la base.
 * @param table Table, avec ses colonnes.
 */
function tableNode(schema: string, table: TableInfo): ExplorerNode {
  return {
    key: `table:${schema}.${table.name}`,
    label: table.kind === 'table' ? table.name : `${table.name} (${kindLabel(table.kind)})`,
    isLeaf: table.columns.length === 0,
    isTable: true,
    schema,
    table: table.name,
    children: table.columns.map((column) => columnNode(schema, table.name, column)),
  }
}

/**
 * Nœud d'une colonne : nom, type, rôle dans les index, nullabilité.
 *
 * @param schema Nom de la base.
 * @param table Nom de la table.
 * @param column Colonne.
 */
function columnNode(schema: string, table: string, column: ColumnInfo): ExplorerNode {
  const details = [column.column_type, column.key, column.nullable ? 'NULL' : ''].filter(Boolean)
  return {
    key: `column:${schema}.${table}.${column.name}`,
    label: `${column.name} · ${details.join(' · ')}`,
    isLeaf: true,
    isTable: false,
    schema,
    table,
  }
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
