/**
 * Nœuds de l'arbre de l'explorateur d'objets.
 *
 * Structure de l'arbre :
 *
 * ```text
 * Sources de données
 * └─ source (connexion)
 *    └─ base
 *       ├─ Tables (n)     ── table ─┬─ Colonnes (n)
 *       │                           ├─ Index (n)
 *       │                           ├─ Clefs étrangères (n)
 *       │                           └─ Déclencheurs (n)
 *       ├─ Vues (n)       ── vue ─── Colonnes (n)
 *       ├─ Procédures (n)
 *       └─ Fonctions (n)
 * Notebooks
 * ```
 *
 * Plusieurs sources se côtoient : chaque connexion enregistrée est une
 * branche sous la racine « Sources de données », et les clefs des nœuds commencent par l'identifiant de leur
 * connexion, pour rester uniques d'une source à l'autre.
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
import type { Catalog, CatalogSchema, CatalogTable } from '@/stores/catalog'

/**
 * Emplacement d'un nœud : sa connexion et sa base.
 */
export interface Place {
  /**
   * Identifiant de la connexion, 0 pour les racines.
   */
  connectionId: number
  /**
   * Nom de la base, chaîne vide pour les racines et pour le nœud d'une source.
   */
  schema: string
}

/**
 * Source de données affichée en racine de l'arbre.
 */
export interface Source {
  /**
   * Identifiant de la connexion.
   */
  connectionId: number
  /**
   * Nom de la connexion, choisi par l'utilisateur.
   */
  name: string
  /**
   * Catalogue de la connexion, `undefined` s'il n'a jamais été lu.
   */
  catalog: Catalog | undefined
  /**
   * Base en cours de la session de requêtes de cette source, `null` si aucune.
   */
  currentDatabase: string | null
}

/**
 * Nœud de l'arbre, au format attendu par `NTree` de Naive UI.
 */
export interface ExplorerNode extends Place {
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
   * Rôle des nœuds sur lesquels on agit (menu, double clic) : les racines
   * « Sources de données » (`sources`) et « Notebooks » (`notebooks`), une
   * source (`source`) ou une base (`database`).
   */
  role?: 'sources' | 'notebooks' | 'source' | 'database'
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
  /**
   * Base en cours de la session de requêtes (affichée en gras).
   */
  current?: boolean
  /**
   * Mention affichée après le libellé, en capitales, plus petite et plus
   * claire : `PRIMARY KEY` pour la clef primaire, `UNIQUE` pour un index unique.
   */
  note?: string
  /**
   * Pour une colonne : ce qu'il faut pour l'afficher en trois colonnes
   * (symboles, nom, détail), alignées sur toutes les colonnes de la table.
   */
  column?: {
    name: string
    /**
     * Type, nullabilité et auto-incrément, en capitales (`VARCHAR(20) NULL`,
     * `INT AUTO_INCREMENT`).
     */
    details: string
    /**
     * Largeur de la colonne des noms, en caractères : le nom le plus long de la table.
     */
    nameWidth: number
    /**
     * Largeur de la colonne des symboles, en symboles : 0 si aucune colonne
     * de la table n'en a, 2 si l'une a une clef et un croisillon.
     */
    markSlots: number
    /**
     * La colonne fait partie de la clef primaire (précédée d'une clef).
     */
    primaryKey: boolean
    /**
     * La colonne fait partie d'au moins un index, clef primaire comprise
     * (affichée en gras).
     */
    indexed: boolean
    /**
     * La colonne fait partie d'une clef étrangère (précédée d'un croisillon).
     */
    foreignKey: boolean
  }
}

/**
 * Clef de la racine « Sources de données ».
 */
export const SOURCES_KEY = 'sources'

/**
 * Clef de la racine « Notebooks ».
 */
export const NOTEBOOKS_KEY = 'notebooks'

/**
 * Racines de l'arbre : « Sources de données », avec une branche par source,
 * puis « Notebooks », vide pour l'instant.
 *
 * @param sources Sources, dans l'ordre d'affichage.
 * @param receiving Un catalogue est-il en cours de réception (mention « chargement… ») ?
 */
export function explorerTree(sources: Source[], receiving: boolean): ExplorerNode[] {
  return [
    {
      key: SOURCES_KEY,
      label: 'Sources de données',
      isLeaf: sources.length === 0,
      connectionId: 0,
      schema: '',
      role: 'sources',
      note: receiving ? 'chargement…' : undefined,
      children: sourceNodes(sources),
    },
    {
      key: NOTEBOOKS_KEY,
      label: 'Notebooks',
      isLeaf: true,
      connectionId: 0,
      schema: '',
      role: 'notebooks',
    },
  ]
}

/**
 * Clef du nœud d'une source.
 *
 * @param connectionId Identifiant de la connexion.
 */
export function sourceKey(connectionId: number): string {
  return `source:${connectionId}`
}

/**
 * Nœuds des sources, leurs bases en dessous. Le catalogue
 * d'une source jamais lue, ou dont les bases ne sont pas encore reçues,
 * s'affiche « chargement… » : il se lit quand on ouvre la source.
 *
 * @param sources Sources, dans l'ordre d'affichage.
 */
export function sourceNodes(sources: Source[]): ExplorerNode[] {
  return sources.map((source) => {
    const place = { connectionId: source.connectionId, schema: '' }
    const key = sourceKey(source.connectionId)
    const catalog = source.catalog
    return {
      key,
      label: source.name,
      isLeaf: false,
      ...place,
      role: 'source' as const,
      children:
        catalog === undefined || (!catalog.done && catalog.schemas.length === 0)
          ? [loadingNode(key, place)]
          : catalogNodes(source.connectionId, catalog.schemas, source.currentDatabase),
    }
  })
}

/**
 * Arbre du catalogue d'une source, une branche par base.
 *
 * @param connectionId Identifiant de la connexion.
 * @param schemas Catalogue du serveur, éventuellement incomplet.
 * @param currentDatabase Base en cours de la session de requêtes, `null` si aucune.
 */
export function catalogNodes(
  connectionId: number,
  schemas: CatalogSchema[],
  currentDatabase: string | null = null,
): ExplorerNode[] {
  return schemas.map((schema) => {
    const place = { connectionId, schema: schema.name }
    const key = `schema:${placeKey(place)}`
    return {
      key,
      label: schema.name,
      isLeaf: false,
      ...place,
      role: 'database' as const,
      current: schema.name === currentDatabase,
      children:
        schema.tables === null || schema.routines === null
          ? [loadingNode(key, place)]
          : schemaFolders(place, schema.tables, schema.routines),
    }
  })
}

/**
 * Début des clefs des nœuds d'une base : connexion et base.
 *
 * @param place Connexion et base.
 */
function placeKey(place: Place): string {
  return `${place.connectionId}/${place.schema}`
}

/**
 * Dossiers d'une base : tables, vues, procédures, fonctions.
 *
 * @param place Connexion et base.
 * @param tables Tables et vues de la base.
 * @param routines Procédures et fonctions de la base.
 */
function schemaFolders(
  place: Place,
  tables: CatalogTable[],
  routines: RoutineInfo[],
): ExplorerNode[] {
  const base = placeKey(place)
  const routineNode = (routine: RoutineInfo): ExplorerNode => ({
    key: `routine:${base}.${routine.name}:${routine.kind}`,
    label: routine.name,
    isLeaf: true,
    ...place,
  })
  return [
    folder(
      `tables:${base}`,
      'Tables',
      place,
      tables.filter((table) => table.kind === 'table').map((table) => tableNode(place, table)),
    ),
    folder(
      `views:${base}`,
      'Vues',
      place,
      tables.filter((table) => table.kind !== 'table').map((table) => tableNode(place, table)),
    ),
    folder(
      `procedures:${base}`,
      'Procédures',
      place,
      routines.filter((routine) => routine.kind === 'procedure').map(routineNode),
    ),
    folder(
      `functions:${base}`,
      'Fonctions',
      place,
      routines.filter((routine) => routine.kind === 'function').map(routineNode),
    ),
  ]
}

/**
 * Nœud d'une table ou d'une vue, avec ses dossiers de détail.
 *
 * @param place Connexion et base.
 * @param table Table ou vue.
 */
function tableNode(place: Place, table: CatalogTable): ExplorerNode {
  const key = `table:${placeKey(place)}.${table.name}`
  return {
    key,
    label: table.kind === 'system' ? `${table.name} (système)` : table.name,
    isLeaf: false,
    ...place,
    table: table.name,
    children:
      table.details === null
        ? [loadingNode(key, place)]
        : tableFolders(place, table.kind, table.details),
  }
}

/**
 * Dossiers de détail d'une table : colonnes, index, clefs étrangères,
 * déclencheurs. Une vue n'a que ses colonnes.
 *
 * @param place Connexion et base.
 * @param kind Nature de la table.
 * @param details Détail reçu du serveur.
 */
function tableFolders(place: Place, kind: TableKind, details: TableDetails): ExplorerNode[] {
  const base = `${placeKey(place)}.${details.table}`
  const item = (prefix: string, name: string, label: string): ExplorerNode => ({
    key: `${prefix}:${base}.${name}`,
    label,
    isLeaf: true,
    ...place,
  })
  const foreignKeyColumns = new Set(details.foreign_keys.flatMap((key) => key.columns))
  const indexedColumns = new Set(details.indexes.flatMap((index) => index.columns))
  const marks = (column: ColumnInfo) =>
    Number(column.key === 'PRI') + Number(foreignKeyColumns.has(column.name))
  const nameWidth = Math.max(0, ...details.columns.map((column) => column.name.length))
  const markSlots = Math.max(0, ...details.columns.map(marks))
  const columns = folder(
    `columns:${base}`,
    'Colonnes',
    place,
    details.columns.map((column) => {
      const foreignKey = foreignKeyColumns.has(column.name)
      return {
        ...item('column', column.name, `${column.name} ${columnDetails(column)}`),
        column: {
          name: column.name,
          details: columnDetails(column),
          nameWidth,
          markSlots,
          primaryKey: column.key === 'PRI',
          indexed: indexedColumns.has(column.name),
          foreignKey,
        },
      }
    }),
  )
  if (kind !== 'table') {
    return [columns]
  }
  return [
    columns,
    folder(
      `indexes:${base}`,
      'Index',
      place,
      details.indexes.map((index) => ({
        ...item('index', index.name, indexLabel(index)),
        note: indexNote(index),
      })),
    ),
    folder(
      `foreign-keys:${base}`,
      'Clefs étrangères',
      place,
      details.foreign_keys.map((key) => item('fk', key.name, foreignKeyLabel(place.schema, key))),
    ),
    folder(
      `triggers:${base}`,
      'Déclencheurs',
      place,
      details.triggers.map((trigger) => item('trigger', trigger.name, triggerLabel(trigger))),
    ),
  ]
}

/**
 * Dossier avec le nombre de ses éléments dans le libellé ; un dossier vide est une feuille.
 *
 * @param key Clef unique du dossier.
 * @param label Nom du dossier.
 * @param place Connexion et base.
 * @param children Éléments du dossier.
 */
function folder(key: string, label: string, place: Place, children: ExplorerNode[]): ExplorerNode {
  return {
    key: `folder:${key}`,
    label: `${label} (${children.length})`,
    isLeaf: children.length === 0,
    ...place,
    children,
  }
}

/**
 * Nœud provisoire, affiché sous une partie du catalogue pas encore reçue.
 *
 * @param parent Clef du nœud parent.
 * @param place Connexion et base.
 */
function loadingNode(parent: string, place: Place): ExplorerNode {
  return {
    key: `${parent}:loading`,
    label: 'chargement…',
    isLeaf: true,
    ...place,
    disabled: true,
  }
}

/**
 * Détail d'une colonne, en capitales comme dans un `CREATE TABLE` : type
 * affiché, nullabilité et auto-incrément (`VARCHAR(20) NULL`,
 * `INT AUTO_INCREMENT`). Le rôle dans les index
 * (PRI, UNI, MUL) n'est pas repris : les colonnes indexées sont en gras et
 * les index ont leur propre dossier.
 *
 * @param column Colonne.
 */
function columnDetails(column: ColumnInfo): string {
  return [
    upperType(displayedType(column.column_type)),
    column.nullable ? 'NULL' : '',
    column.auto_increment ? 'AUTO_INCREMENT' : '',
  ]
    .filter(Boolean)
    .join(' ')
}

/**
 * Type en capitales, sauf les valeurs entre apostrophes d'un `enum` ou d'un
 * `set`, qui gardent leur casse (`enum('a','B')` devient `ENUM('a','B')`).
 *
 * @param columnType Type de la colonne.
 */
export function upperType(columnType: string): string {
  return columnType
    .split(/('(?:[^']|'')*')/)
    .map((part) => (part.startsWith("'") ? part : part.toUpperCase()))
    .join('')
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
 * Libellé d'un index : nom et colonnes.
 *
 * @param index Index.
 */
export function indexLabel(index: IndexInfo): string {
  return `${index.name} (${index.columns.join(', ')})`
}

/**
 * Mention d'un index : `PRIMARY KEY` pour la clef primaire (nommée `PRIMARY`),
 * `UNIQUE` pour un autre index unique, rien sinon.
 *
 * @param index Index.
 */
export function indexNote(index: IndexInfo): string | undefined {
  if (index.name === 'PRIMARY') {
    return 'PRIMARY KEY'
  }
  return index.unique ? 'UNIQUE' : undefined
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
