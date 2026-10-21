/**
 * Catalogs of the connections (databases, tables, columns), filled
 * progressively and kept in memory.
 *
 * The catalog of a connection fills as the server's messages arrive: the
 * databases first, then their objects (tables, views, procedures, functions),
 * then the details of the tables. A part not received yet is `null`. Going
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

import { streamCatalog } from '@/api/explorer'
import type { CatalogEvent } from '@/api/generated/CatalogEvent'
import type { Flavor } from '@/api/generated/Flavor'
import type { EventInfo } from '@/api/generated/EventInfo'
import type { RoutineInfo } from '@/api/generated/RoutineInfo'
import type { TableDetails } from '@/api/generated/TableDetails'
import type { TableKind } from '@/api/generated/TableKind'
import { describe } from '@/api/messages'

/**
 * Table or view of the catalog.
 */
export interface CatalogTable {
  name: string
  kind: TableKind
  /**
   * Columns, indexes, foreign keys and triggers, `null` as long as they are not received.
   */
  details: TableDetails | null
}

/**
 * Database of the catalog.
 */
export interface CatalogSchema {
  name: string
  /**
   * Tables and views, `null` as long as they are not received.
   */
  tables: CatalogTable[] | null
  /**
   * Procedures and functions, `null` as long as they are not received.
   */
  routines: RoutineInfo[] | null
  /**
   * Scheduled events, `null` as long as they are not received.
   */
  events: EventInfo[] | null
}

/**
 * Catalog of a connection, being received or complete.
 */
export interface Catalog {
  schemas: CatalogSchema[]
  /**
   * Flavor of the server, announced with the databases; `null` before.
   */
  flavor: Flavor | null
  /**
   * Version of the server (raw result of `SELECT VERSION()`), announced with
   * the databases; `null` before.
   */
  version: string | null
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
  return { schemas: [], flavor: null, version: null, done: false, errors: [] }
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
 * Stores a message of the server in the catalog.
 *
 * @param catalog Catalog to complete, changed in place.
 * @param event Received message.
 */
export function applyCatalogEvent(catalog: Catalog, event: CatalogEvent): void {
  switch (event.type) {
    case 'schemas':
      catalog.schemas = event.schemas.map((name) => ({
        name,
        tables: null,
        routines: null,
        events: null,
      }))
      catalog.flavor = event.flavor
      catalog.version = event.version
      break
    case 'objects': {
      const schema = catalog.schemas.find((candidate) => candidate.name === event.schema)
      if (schema) {
        schema.tables = event.objects.tables.map((table) => ({ ...table, details: null }))
        schema.routines = event.objects.routines
        schema.events = event.objects.events
      }
      break
    }
    case 'details': {
      const tables = catalog.schemas.find((candidate) => candidate.name === event.schema)?.tables
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
   * @throws {ApiError} If the target server is unreachable.
   */
  async function load(connectionId: number, refresh = false): Promise<void> {
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
      await streamCatalog(connectionId, (event) => {
        applyCatalogEvent(catalog, event)
        timer ??= setTimeout(signal, REVISION_DELAY)
      })
    } catch (failure) {
      // Nothing readable: the next opening of the explorer will try again.
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
   * Names of the catalog as defined, for the beautifier: databases, tables
   * and views, columns, functions and procedures, all databases together.
   * The parts not read yet are simply absent.
   *
   * @param connectionId Id of the connection.
   */
  function catalogNames(connectionId: number): BeautifyCatalog {
    const schemas = catalogs.value[connectionId]?.schemas ?? []
    const tables = schemas.flatMap((schema) => schema.tables ?? [])
    return {
      schemas: schemas.map((schema) => schema.name),
      tables: tables.map((table) => table.name),
      columns: tables.flatMap((table) =>
        (table.details?.columns ?? []).map((column) => column.name),
      ),
      routines: schemas.flatMap((schema) => (schema.routines ?? []).map((routine) => routine.name)),
    }
  }

  return { catalogs, revisions, load, catalogOf, catalogNames }
})
