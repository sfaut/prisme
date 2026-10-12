/**
 * Nœuds de l'arbre de l'explorateur d'objets.
 *
 * Structure de l'arbre :
 *
 * ```text
 * base
 * ├─ Tables (n)        ── table ─┬─ Colonnes (n)
 * │                              ├─ Index (n)
 * │                              ├─ Clefs étrangères (n)
 * │                              └─ Déclencheurs (n)
 * ├─ Vues (n)          ── vue ─── Colonnes (n)
 * ├─ Procédures (n)
 * └─ Fonctions (n)
 * ```
 *
 * L'arbre se construit à partir du catalogue, même incomplet : une partie
 * pas encore reçue s'affiche « chargement… ». Les fonctions de ce module ne
 * dépendent pas de l'affichage.
 */

import type { ColumnInfo } from '@/api/generated/ColumnInfo'
import type { ForeignKeyInfo } from '@/api/generated/ForeignKeyInfo'
import type { IndexInfo } from '@/api/generated/IndexInfo'
import type { RoutineInfo } from '@/api/generated/RoutineInfo'
import type { TableDetails } from '@/api/generated/TableDetails'
import type { TableKind } from '@/api/generated/TableKind'
import type { TriggerInfo } from '@/api/generated/TriggerInfo'
import type { CatalogSchema, CatalogTable } from '@/stores/catalog'

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
   * Feuille : rien en dessous.
   */
  isLeaf: boolean
  /**
   * Base à laquelle appartient le nœud.
   */
  schema: string
  /**
   * Table ou vue désignée par le nœud, pour l'ouvrir d'un double clic.
   */
  table?: string
  /**
   * Enfants.
   */
  children?: ExplorerNode[]
  /**
   * Nœud non cliquable (« chargement… »).
   */
  disabled?: boolean
}

/**
 * Arbre du catalogue, une branche par base.
 *
 * @param schemas Catalogue du serveur, éventuellement incomplet.
 */
export function catalogNodes(schemas: CatalogSchema[]): ExplorerNode[] {
  return schemas.map((schema) => {
    const key = `schema:${schema.name}`
    return {
      key,
      label: schema.name,
      isLeaf: false,
      schema: schema.name,
      children:
        schema.tables === null || schema.routines === null
          ? [loadingNode(key, schema.name)]
          : schemaFolders(schema.name, schema.tables, schema.routines),
    }
  })
}

/**
 * Dossiers d'une base : tables, vues, procédures, fonctions.
 *
 * @param schema Nom de la base.
 * @param tables Tables et vues de la base.
 * @param routines Procédures et fonctions de la base.
 */
function schemaFolders(
  schema: string,
  tables: CatalogTable[],
  routines: RoutineInfo[],
): ExplorerNode[] {
  const routineNode = (routine: RoutineInfo): ExplorerNode => ({
    key: `routine:${schema}.${routine.name}:${routine.kind}`,
    label: routine.name,
    isLeaf: true,
    schema,
  })
  return [
    folder(
      `tables:${schema}`,
      'Tables',
      schema,
      tables.filter((table) => table.kind === 'table').map((table) => tableNode(schema, table)),
    ),
    folder(
      `views:${schema}`,
      'Vues',
      schema,
      tables.filter((table) => table.kind !== 'table').map((table) => tableNode(schema, table)),
    ),
    folder(
      `procedures:${schema}`,
      'Procédures',
      schema,
      routines.filter((routine) => routine.kind === 'procedure').map(routineNode),
    ),
    folder(
      `functions:${schema}`,
      'Fonctions',
      schema,
      routines.filter((routine) => routine.kind === 'function').map(routineNode),
    ),
  ]
}

/**
 * Nœud d'une table ou d'une vue, avec ses dossiers de détail.
 *
 * @param schema Nom de la base.
 * @param table Table ou vue.
 */
function tableNode(schema: string, table: CatalogTable): ExplorerNode {
  const key = `table:${schema}.${table.name}`
  return {
    key,
    label: table.kind === 'system' ? `${table.name} (système)` : table.name,
    isLeaf: false,
    schema,
    table: table.name,
    children:
      table.details === null
        ? [loadingNode(key, schema)]
        : tableFolders(schema, table.kind, table.details),
  }
}

/**
 * Dossiers de détail d'une table : colonnes, index, clefs étrangères,
 * déclencheurs. Une vue n'a que ses colonnes.
 *
 * @param schema Nom de la base.
 * @param kind Nature de la table.
 * @param details Détail reçu du serveur.
 */
function tableFolders(schema: string, kind: TableKind, details: TableDetails): ExplorerNode[] {
  const base = `${schema}.${details.table}`
  const item = (prefix: string, name: string, label: string): ExplorerNode => ({
    key: `${prefix}:${base}.${name}`,
    label,
    isLeaf: true,
    schema,
  })
  const columns = folder(
    `columns:${base}`,
    'Colonnes',
    schema,
    details.columns.map((column) => item('column', column.name, columnLabel(column))),
  )
  if (kind !== 'table') {
    return [columns]
  }
  return [
    columns,
    folder(
      `indexes:${base}`,
      'Index',
      schema,
      details.indexes.map((index) => item('index', index.name, indexLabel(index))),
    ),
    folder(
      `foreign-keys:${base}`,
      'Clefs étrangères',
      schema,
      details.foreign_keys.map((key) => item('fk', key.name, foreignKeyLabel(schema, key))),
    ),
    folder(
      `triggers:${base}`,
      'Déclencheurs',
      schema,
      details.triggers.map((trigger) => item('trigger', trigger.name, triggerLabel(trigger))),
    ),
  ]
}

/**
 * Dossier avec le nombre de ses éléments dans le libellé ; un dossier vide est une feuille.
 *
 * @param key Clef unique du dossier.
 * @param label Nom du dossier.
 * @param schema Nom de la base.
 * @param children Éléments du dossier.
 */
function folder(
  key: string,
  label: string,
  schema: string,
  children: ExplorerNode[],
): ExplorerNode {
  return {
    key: `folder:${key}`,
    label: `${label} (${children.length})`,
    isLeaf: children.length === 0,
    schema,
    children,
  }
}

/**
 * Nœud provisoire, affiché sous une partie du catalogue pas encore reçue.
 *
 * @param parent Clef du nœud parent.
 * @param schema Nom de la base.
 */
function loadingNode(parent: string, schema: string): ExplorerNode {
  return {
    key: `${parent}:loading`,
    label: 'chargement…',
    isLeaf: true,
    schema,
    disabled: true,
  }
}

/**
 * Libellé d'une colonne : nom, type, rôle dans les index, nullabilité.
 *
 * @param column Colonne.
 */
export function columnLabel(column: ColumnInfo): string {
  const details = [
    displayedType(column.column_type),
    column.key,
    column.nullable ? 'NULL' : '',
  ].filter(Boolean)
  return `${column.name} · ${details.join(' · ')}`
}

/**
 * Type affiché d'une colonne : sans la largeur d'affichage des entiers
 * (`int(10) unsigned` devient `int unsigned`), qui ne change rien aux valeurs
 * stockées. La précision des décimaux (`decimal(10,2)`) et la taille des
 * chaînes et des bits sont conservées.
 *
 * @param columnType Type complet, tel que déclaré.
 */
export function displayedType(columnType: string): string {
  return columnType.replace(/^(tinyint|smallint|mediumint|int|integer|bigint)\(\d+\)/i, '$1')
}

/**
 * Libellé d'un index : nom, colonnes, unicité.
 *
 * @param index Index.
 */
export function indexLabel(index: IndexInfo): string {
  const unique = index.unique && index.name !== 'PRIMARY' ? ' · unique' : ''
  return `${index.name} (${index.columns.join(', ')})${unique}`
}

/**
 * Libellé d'une clef étrangère : nom, colonnes, table et colonnes référencées.
 * La base référencée n'apparaît que si elle diffère de celle de la table.
 *
 * @param schema Nom de la base de la table.
 * @param key Clef étrangère.
 */
export function foreignKeyLabel(schema: string, key: ForeignKeyInfo): string {
  const target =
    key.referenced_schema === schema
      ? key.referenced_table
      : `${key.referenced_schema}.${key.referenced_table}`
  return `${key.name} (${key.columns.join(', ')}) → ${target} (${key.referenced_columns.join(', ')})`
}

/**
 * Libellé d'un déclencheur : nom, moment et événement.
 *
 * @param trigger Déclencheur.
 */
export function triggerLabel(trigger: TriggerInfo): string {
  return `${trigger.name} · ${trigger.timing} ${trigger.event}`
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
