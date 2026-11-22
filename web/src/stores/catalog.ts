/**
 * Catalogs of the connections (databases, tables, columns), filled
 * progressively and kept in memory.
 *
 * The catalog of a connection fills as the server's messages arrive: the
 * namespaces first (databases of MySQL), then their objects (tables, views,
 * routines, events... as the engine declares them), then the details of the
 * tables and views. A part not received yet is `null`. Going
 * back to a connection already explored reuses its catalog; "Rafraîchir"
 * (refresh) reads it again.
 *
 * A catalog counts tens of thousands of objects (tables, columns, indexes...):
 * it stays outside Vue's reactivity, which would track each of them. Measured
 * on 20 databases of 50 tables, the deep traversal of the catalog on each
 * received message froze the interface for 3 minutes. Only a revision number
 * per connection is reactive: it increases at most every 100 ms during the
 * reception, and at the end.
 */

import type { BeautifyCatalog } from '@/api/generated/BeautifyCatalog'
import { defineStore } from 'pinia'
import { markRaw, ref, shallowRef, triggerRef } from 'vue'

import { streamCatalog } from '@/api/navigator'
import type { CatalogEvent } from '@/api/generated/CatalogEvent'
import type { ColumnOrigin } from '@/api/generated/ColumnOrigin'
import type { Namespace } from '@/api/generated/Namespace'
import type { ObjectInfo } from '@/api/generated/ObjectInfo'
import type { ObjectKind } from '@/api/generated/ObjectKind'
import type { SourceInfo } from '@/api/generated/SourceInfo'
import type { TableDetails } from '@/api/generated/TableDetails'
import { describe } from '@/api/messages'

/**
 * Kinds of objects that have columns, and so details: tables and views.
 */
export const RELATION_KINDS: readonly ObjectKind[] = ['table', 'view', 'system_table']

/**
 * Object of the catalog: what the source sent of it (name, kind, estimated
 * row count...), then the details of a table or a view.
 */
export interface CatalogObject extends ObjectInfo {
  /**
   * Columns, indexes, foreign keys and triggers of a table or a view, `null`
   * as long as they are not received, and for the other kinds.
   */
  details: TableDetails | null
}

/**
 * Namespace of the catalog (database of MySQL).
 */
export interface CatalogSchema {
  /**
   * Innermost name of the namespace, the one the SQL qualifies the objects with.
   */
  name: string
  /**
   * Whole path of the namespace.
   */
  namespace: Namespace
  /**
   * Is it provided by the source itself (`information_schema`...)?
   */
  system: boolean
  /**
   * Objects, in the order of the source, `null` as long as they are not received.
   */
  objects: CatalogObject[] | null
}

/**
 * Catalog of a connection, being received or complete.
 */
export interface Catalog {
  schemas: CatalogSchema[]
  /**
   * Identity of the source (flavor, version, capabilities), announced with
   * the namespaces; `null` before.
   */
  source: SourceInfo | null
  /**
   * Has the whole catalog been received?
   */
  done: boolean
  /**
   * Reading errors reported by the server.
   */
  errors: string[]
}

/**
 * Empty catalog, waiting for the first message.
 */
export function emptyCatalog(): Catalog {
  return { schemas: [], source: null, done: false, errors: [] }
}

/**
 * Is an object a table or a view, which has columns?
 *
 * @param object Object of the catalog.
 */
export function isRelation(object: ObjectInfo): boolean {
  return RELATION_KINDS.includes(object.kind)
}

/**
 * Tables and views of a namespace, none as long as its objects are not received.
 *
 * @param schema Namespace of the catalog.
 */
export function relationsOf(schema: CatalogSchema): CatalogObject[] {
  return (schema.objects ?? []).filter(isRelation)
}

/**
 * Do two paths name the same namespace?
 *
 * @param a First path.
 * @param b Second path.
 */
function sameNamespace(a: Namespace, b: Namespace): boolean {
  return a.length === b.length && a.every((name, index) => name === b[index])
}

/**
 * Empty details of a table for which the server returned nothing (restricted privileges...).
 *
 * @param table Name of the table.
 */
function emptyDetails(table: string): TableDetails {
  return { table, columns: [], indexes: [], foreign_keys: [], triggers: [] }
}

/**
 * Is a column of a table part of a foreign key of that table? `false` as long as the catalog has
 * not received the details of the table, and for a name it does not know: a view, or the alias of
 * a derived table.
 *
 * @param catalog Catalog of the connection, absent if it was never read.
 * @param origin Column of a table, absent for an expression.
 */
export function isForeignKey(
  catalog: Catalog | undefined,
  origin: ColumnOrigin | undefined,
): boolean {
  if (catalog === undefined || origin === undefined) {
    return false
  }
  const schema = catalog.schemas.find((item) => item.name === origin.schema)
  const table = schema && relationsOf(schema).find((item) => item.name === origin.table)
  return table?.details?.foreign_keys.some((key) => key.columns.includes(origin.column)) ?? false
}

/**
 * Stores a message of the server in the catalog.
 *
 * @param catalog Catalog to complete, changed in place.
 * @param event Received message.
 */
export function applyCatalogEvent(catalog: Catalog, event: CatalogEvent): void {
  switch (event.type) {
    case 'namespaces':
      catalog.schemas = event.namespaces.map(({ namespace, system }) => ({
        name: namespace[namespace.length - 1] ?? '',
        namespace,
        system,
        objects: null,
      }))
      catalog.source = event.source
      break
    case 'objects': {
      const schema = catalog.schemas.find((candidate) =>
        sameNamespace(candidate.namespace, event.namespace),
      )
      if (schema) {
        schema.objects = event.objects.map((object) => ({ ...object, details: null }))
      }
      break
    }
    case 'details': {
      const schema = catalog.schemas.find((candidate) =>
        sameNamespace(candidate.namespace, event.namespace),
      )
      const tables = schema && relationsOf(schema)
      for (const details of event.tables) {
        const target = tables?.find((candidate) => candidate.name === details.table)
        if (target) {
          target.details = details
        }
      }
      // Tables without received details: nothing more will arrive.
      for (const table of tables ?? []) {
        table.details ??= emptyDetails(table.name)
      }
      break
    }
    case 'error':
      catalog.errors.push(describe(event.detail))
      break
    case 'done':
      catalog.done = true
      break
  }
}

/**
 * Delay between two signals of new parts of the catalog, in milliseconds.
 */
const REVISION_DELAY = 100

export const useCatalogStore = defineStore('catalog', () => {
  /**
   * Catalogs, by connection id. The objects are raw (`markRaw`): read
   * `revisions` to be notified of their changes.
   */
  const catalogs = shallowRef<Record<number, Catalog>>({})

  /**
   * Revision number of each catalog: it increases when new parts have
   * arrived.
   */
  const revisions = ref<Record<number, number>>({})

  /**
   * Reads the catalog of a connection progressively, unless it is already in memory.
   *
   * @param connectionId Id of the connection.
   * @param refresh Reads the catalog again even if it is in memory.
   * @param editorId Editor that asks for it, whose console receives the
   *     queries; `null`, outside the editors.
   * @throws {ApiError} If the target server is unreachable.
   */
  async function load(
    connectionId: number,
    refresh = false,
    editorId: number | null = null,
  ): Promise<void> {
    if (!refresh && catalogs.value[connectionId] !== undefined) {
      return
    }
    const catalog = markRaw(emptyCatalog())
    catalogs.value[connectionId] = catalog
    triggerRef(catalogs)
    revisions.value[connectionId] = (revisions.value[connectionId] ?? 0) + 1

    /**
     * Deferred signal of the new parts, absent if nothing waits.
     */
    let timer: ReturnType<typeof setTimeout> | undefined
    const signal = () => {
      timer = undefined
      revisions.value[connectionId] = (revisions.value[connectionId] ?? 0) + 1
    }
    try {
      await streamCatalog(
        connectionId,
        (event) => {
          applyCatalogEvent(catalog, event)
          timer ??= setTimeout(signal, REVISION_DELAY)
        },
        editorId,
      )
    } catch (failure) {
      // Nothing readable: the next opening of the navigator will try again.
      delete catalogs.value[connectionId]
      triggerRef(catalogs)
      throw failure
    } finally {
      clearTimeout(timer)
      signal()
    }
  }

  /**
   * Catalog of a connection, `undefined` if it was never read. Calling this
   * function in a `computed` makes it recompute on each revision.
   *
   * @param connectionId Id of the connection.
   */
  function catalogOf(connectionId: number): Catalog | undefined {
    void revisions.value[connectionId]
    return catalogs.value[connectionId]
  }

  /**
   * Names of the catalog as defined, for the beautifier: namespaces, tables
   * and views, columns, functions and procedures, all namespaces together.
   * The parts not read yet are simply absent.
   *
   * @param connectionId Id of the connection.
   */
  function catalogNames(connectionId: number): BeautifyCatalog {
    const schemas = catalogs.value[connectionId]?.schemas ?? []
    const tables = schemas.flatMap(relationsOf)
    return {
      schemas: schemas.map((schema) => schema.name),
      tables: tables.map((table) => table.name),
      columns: tables.flatMap((table) =>
        (table.details?.columns ?? []).map((column) => column.name),
      ),
      routines: schemas.flatMap((schema) =>
        (schema.objects ?? [])
          .filter((object) => object.kind === 'procedure' || object.kind === 'function')
          .map((routine) => routine.name),
      ),
    }
  }

  return { catalogs, revisions, load, catalogOf, catalogNames }
})
