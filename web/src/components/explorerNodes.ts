/**
 * Nodes of the object explorer's tree.
 *
 * Structure of the tree (labels as displayed):
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
 * Several sources sit side by side: each saved connection is a branch under
 * the "Sources de données" root, and the keys of the nodes start with the id of
 * their connection, to stay unique from one source to the other.
 *
 * The tree is built from the catalog, even incomplete: a part not received
 * yet shows "chargement…" (loading). The functions of this module do not
 * depend on the display.
 */

import type { ColumnInfo } from '@/api/generated/ColumnInfo'
import type { EventInfo } from '@/api/generated/EventInfo'
import type { Flavor } from '@/api/generated/Flavor'
import { serverName } from '@/api/flavors'
import type { ForeignKeyInfo } from '@/api/generated/ForeignKeyInfo'
import type { IndexInfo } from '@/api/generated/IndexInfo'
import type { RoutineInfo } from '@/api/generated/RoutineInfo'
import type { TableDetails } from '@/api/generated/TableDetails'
import type { TableKind } from '@/api/generated/TableKind'
import type { TriggerInfo } from '@/api/generated/TriggerInfo'
import type { Catalog, CatalogSchema, CatalogTable } from '@/stores/catalog'

/**
 * Place of a node: its connection and its database.
 */
export interface Place {
  /**
   * Id of the connection, 0 for the roots.
   */
  connectionId: number
  /**
   * Name of the database, empty string for the roots and for a source's node.
   */
  schema: string
}

/**
 * Data source shown at the root of the tree.
 */
export interface Source {
  /**
   * Id of the connection.
   */
  connectionId: number
  /**
   * Name of the connection, chosen by the user.
   */
  name: string
  /**
   * Catalog of the connection, `undefined` if it was never read.
   */
  catalog: Catalog | undefined
  /**
   * Current database of this source's query session, `null` if none.
   */
  currentDatabase: string | null
  /**
   * Flavor of the server (badge next to the name), `null` if still unknown.
   */
  flavor: Flavor | null
  /**
   * Version of the server (raw result of `SELECT VERSION()`), `null` if still unknown.
   */
  version: string | null
  /**
   * Are the system databases (`SYSTEM_SCHEMAS`) shown?
   */
  showSystemSchemas: boolean
}

/**
 * Databases provided by the server itself, which the connection's setting
 * can hide in the explorer.
 */
export const SYSTEM_SCHEMAS = ['information_schema', 'mysql', 'performance_schema', 'sys']

/**
 * Nature of a catalog node that has no role, to choose its icon.
 */
export type EntryKind =
  | 'folder'
  | 'table'
  | 'view'
  | 'procedure'
  | 'function'
  | 'event'
  | 'column'
  | 'index'
  | 'foreign-key'
  | 'trigger'
  | 'loading'

/**
 * Node of the tree, in the format expected by Reka UI's `TreeRoot`.
 */
export interface ExplorerNode extends Place {
  /**
   * Unique key in the tree.
   */
  key: string
  /**
   * Displayed text.
   */
  label: string
  /**
   * Leaf: nothing below.
   */
  isLeaf: boolean
  /**
   * Role of the nodes that can be acted on (menu, double click): the roots
   * "Sources de données" (`sources`) and "Notebooks" (`notebooks`), a source
   * (`source`) or a database (`database`).
   */
  role?: 'sources' | 'notebooks' | 'source' | 'database'
  /**
   * Nature of a catalog node without a role (folder, table, column...).
   */
  entry?: EntryKind
  /**
   * Table or view designated by the node, to show its content (Ctrl+click or the button of its line).
   */
  table?: string
  /**
   * Index designated by the node, to write the operations on it (Ctrl+click
   * or the button of its line): its table and its name.
   */
  index?: { table: string; name: string }
  /**
   * Children.
   */
  children?: ExplorerNode[]
  /**
   * Node that cannot be clicked ("chargement…").
   */
  disabled?: boolean
  /**
   * Current database of the query session (shown in bold).
   */
  current?: boolean
  /**
   * Badge shown after the label of a source: the flavor of its server
   * (`mysql`, `mariadb`).
   */
  badge?: string
  /**
   * Tooltip of the badge: name and version of the server.
   */
  badgeTooltip?: string
  /**
   * Note shown after the label, in capitals, smaller and lighter:
   * `PRIMARY KEY` for the primary key, `UNIQUE` for a unique index.
   */
  note?: string
  /**
   * For a column: what is needed to show it in three columns (symbols,
   * name, detail), aligned over all the columns of the table.
   */
  column?: {
    name: string
    /**
     * Type, nullability and auto-increment, in capitals (`VARCHAR(20) NULL`,
     * `INT AUTO_INCREMENT`).
     */
    details: string
    /**
     * Width of the names column, in characters: the longest name of the table.
     */
    nameWidth: number
    /**
     * Width of the symbols column, in symbols: 0 if no column of the table
     * has any, 2 if one has a key and a hash.
     */
    markSlots: number
    /**
     * The column is part of the primary key (preceded by a key).
     */
    primaryKey: boolean
    /**
     * The column is part of at least one index, primary key included
     * (shown in bold).
     */
    indexed: boolean
    /**
     * The column is part of a foreign key (preceded by a hash).
     */
    foreignKey: boolean
  }
}

/**
 * Key of the "Sources de données" root.
 */
export const SOURCES_KEY = 'sources'

/**
 * Key of the "Notebooks" root.
 */
export const NOTEBOOKS_KEY = 'notebooks'

/**
 * Roots of the tree: "Sources de données", with one branch per source, then
 * "Notebooks", empty for now.
 *
 * @param sources Sources, in display order.
 * @param receiving Is a catalog being received ("chargement…" note)?
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
 * Key of a source's node.
 *
 * @param connectionId Id of the connection.
 */
export function sourceKey(connectionId: number): string {
  return `source:${connectionId}`
}

/**
 * Nodes of the sources, their databases below. The catalog of a source
 * never read, or whose databases are not received yet, shows
 * "chargement…": it is read when the source is opened.
 *
 * @param sources Sources, in display order.
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
      badge: source.flavor ?? undefined,
      badgeTooltip: source.flavor === null ? undefined : serverName(source.flavor, source.version),
      children:
        catalog === undefined || (!catalog.done && catalog.schemas.length === 0)
          ? [loadingNode(key, place)]
          : catalogNodes(
              source.connectionId,
              source.showSystemSchemas
                ? catalog.schemas
                : catalog.schemas.filter((schema) => !SYSTEM_SCHEMAS.includes(schema.name)),
              source.currentDatabase,
            ),
    }
  })
}

/**
 * Tree of a source's catalog, one branch per database.
 *
 * @param connectionId Id of the connection.
 * @param schemas Catalog of the server, possibly incomplete.
 * @param currentDatabase Current database of the query session, `null` if none.
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
        schema.tables === null || schema.routines === null || schema.events === null
          ? [loadingNode(key, place)]
          : schemaFolders(place, schema.tables, schema.routines, schema.events),
    }
  })
}

/**
 * Start of the keys of a database's nodes: connection and database.
 *
 * @param place Connection and database.
 */
function placeKey(place: Place): string {
  return `${place.connectionId}/${place.schema}`
}

/**
 * Folders of a database: tables, views, procedures, functions, events.
 *
 * @param place Connection and database.
 * @param tables Tables and views of the database.
 * @param routines Procedures and functions of the database.
 * @param events Scheduled events of the database.
 */
function schemaFolders(
  place: Place,
  tables: CatalogTable[],
  routines: RoutineInfo[],
  events: EventInfo[],
): ExplorerNode[] {
  const base = placeKey(place)
  const routineNode = (routine: RoutineInfo): ExplorerNode => ({
    key: `routine:${base}.${routine.name}:${routine.kind}`,
    label: routine.name,
    isLeaf: true,
    entry: routine.kind === 'procedure' ? 'procedure' : 'function',
    ...place,
  })
  const eventNode = (event: EventInfo): ExplorerNode => ({
    key: `event:${base}.${event.name}`,
    label: event.enabled ? event.name : `${event.name} · désactivé`,
    isLeaf: true,
    entry: 'event',
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
    folder(`events:${base}`, 'Événements', place, events.map(eventNode)),
  ]
}

/**
 * Node of a table or a view, with its detail folders.
 *
 * @param place Connection and database.
 * @param table Table or view.
 */
function tableNode(place: Place, table: CatalogTable): ExplorerNode {
  const key = `table:${placeKey(place)}.${table.name}`
  return {
    key,
    label: table.kind === 'system' ? `${table.name} (système)` : table.name,
    isLeaf: false,
    entry: table.kind === 'view' ? 'view' : 'table',
    ...place,
    table: table.name,
    children:
      table.details === null
        ? [loadingNode(key, place)]
        : tableFolders(place, table.kind, table.details),
  }
}

/**
 * Detail folders of a table: columns, indexes, foreign keys, triggers. A
 * view only has its columns.
 *
 * @param place Connection and database.
 * @param kind Nature of the table.
 * @param details Details received from the server.
 */
function tableFolders(place: Place, kind: TableKind, details: TableDetails): ExplorerNode[] {
  const base = `${placeKey(place)}.${details.table}`
  const item = (entry: EntryKind, prefix: string, name: string, label: string): ExplorerNode => ({
    key: `${prefix}:${base}.${name}`,
    label,
    isLeaf: true,
    entry,
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
        ...item('column', 'column', column.name, `${column.name} ${columnDetails(column)}`),
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
        ...item('index', 'index', index.name, indexLabel(index)),
        note: indexNote(index),
        index: { table: details.table, name: index.name },
      })),
    ),
    folder(
      `foreign-keys:${base}`,
      'Clefs étrangères',
      place,
      details.foreign_keys.map((key) =>
        item('foreign-key', 'fk', key.name, foreignKeyLabel(place.schema, key)),
      ),
    ),
    folder(
      `triggers:${base}`,
      'Déclencheurs',
      place,
      details.triggers.map((trigger) =>
        item('trigger', 'trigger', trigger.name, triggerLabel(trigger)),
      ),
    ),
  ]
}

/**
 * Folder with the number of its items in the label; an empty folder is a leaf.
 *
 * @param key Unique key of the folder.
 * @param label Name of the folder.
 * @param place Connection and database.
 * @param children Items of the folder.
 */
function folder(key: string, label: string, place: Place, children: ExplorerNode[]): ExplorerNode {
  return {
    key: `folder:${key}`,
    label: `${label} (${children.length})`,
    isLeaf: children.length === 0,
    entry: 'folder',
    ...place,
    children,
  }
}

/**
 * Temporary node, shown under a part of the catalog not received yet.
 *
 * @param parent Key of the parent node.
 * @param place Connection and database.
 */
function loadingNode(parent: string, place: Place): ExplorerNode {
  return {
    key: `${parent}:loading`,
    label: 'chargement…',
    entry: 'loading',
    isLeaf: true,
    ...place,
    disabled: true,
  }
}

/**
 * Detail of a column, in capitals as in a `CREATE TABLE`: displayed type,
 * nullability and auto-increment (`VARCHAR(20) NULL`,
 * `INT AUTO_INCREMENT`). The role in the indexes
 * (PRI, UNI, MUL) is not repeated: the indexed columns are in bold and the
 * indexes have their own folder.
 *
 * @param column Column.
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
 * Type in capitals, except the quoted values of an `enum` or a `set`, which
 * keep their case (`enum('a','B')` becomes `ENUM('a','B')`).
 *
 * @param columnType Type of the column.
 */
export function upperType(columnType: string): string {
  return columnType
    .split(/('(?:[^']|'')*')/)
    .map((part) => (part.startsWith("'") ? part : part.toUpperCase()))
    .join('')
}

/**
 * Displayed type of a column: without the display width of integers
 * (`int(10) unsigned` becomes `int unsigned`), which changes nothing to the
 * stored values. The precision of decimals (`decimal(10,2)`) and the size of
 * strings and bits are kept.
 *
 * @param columnType Whole type, as declared.
 */
export function displayedType(columnType: string): string {
  return columnType.replace(/^(tinyint|smallint|mediumint|int|integer|bigint)\(\d+\)/i, '$1')
}

/**
 * Label of an index: name and columns.
 *
 * @param index Index.
 */
export function indexLabel(index: IndexInfo): string {
  return `${index.name} (${index.columns.join(', ')})`
}

/**
 * Note of an index: `PRIMARY KEY` for the primary key (named `PRIMARY`),
 * `UNIQUE` for another unique index, nothing otherwise.
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
 * Label of a foreign key: name, columns, referenced table and columns.
 * The referenced database only appears if it differs from the table's.
 *
 * @param schema Name of the table's database.
 * @param key Foreign key.
 */
export function foreignKeyLabel(schema: string, key: ForeignKeyInfo): string {
  const target =
    key.referenced_schema === schema
      ? key.referenced_table
      : `${key.referenced_schema}.${key.referenced_table}`
  return `${key.name} (${key.columns.join(', ')}) → ${target} (${key.referenced_columns.join(', ')})`
}

/**
 * Label of a trigger: name, timing and event.
 *
 * @param trigger Trigger.
 */
export function triggerLabel(trigger: TriggerInfo): string {
  return `${trigger.name} · ${trigger.timing} ${trigger.event}`
}

/**
 * Query that shows the content of a table (`SELECT ALL`, quantifier written
 * out), with quoted names.
 *
 * @param schema Name of the database.
 * @param table Name of the table.
 * @param ansiQuotes Is the `ANSI_QUOTES` mode active on the session?
 */
export function selectAllQuery(schema: string, table: string, ansiQuotes = false): string {
  return `SELECT ALL * FROM ${quoteIdentifier(schema, ansiQuotes)}.${quoteIdentifier(table, ansiQuotes)}`
}

/**
 * Quotes a MySQL identifier: double quotes if the `ANSI_QUOTES` mode is
 * active, backticks otherwise; the delimiter present in the name is
 * doubled.
 *
 * @param name Identifier to quote.
 * @param ansiQuotes Is the `ANSI_QUOTES` mode active on the session?
 */
export function quoteIdentifier(name: string, ansiQuotes: boolean): string {
  const quote = ansiQuotes ? '"' : '`'
  return `${quote}${name.split(quote).join(quote + quote)}${quote}`
}
