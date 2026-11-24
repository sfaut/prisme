/**
 * Builders of catalogs in the form of the store (`@/stores/catalog`), for the
 * tests: namespaces of one level, and their objects.
 */

import type { TableDetails } from '@/api/generated/TableDetails'
import type { CatalogObject, CatalogSchema } from '@/stores/catalog'

/**
 * Namespace of one level.
 *
 * @param name Name of the namespace.
 * @param objects Its objects, `null` as long as they are not received.
 * @param system Is it provided by the source itself?
 */
export function schema(
  name: string,
  objects: CatalogObject[] | null,
  system = false,
): CatalogSchema {
  return { name, namespace: [name], system, objects, comment: '' }
}

/**
 * Table.
 *
 * @param name Name of the table.
 * @param rows Row count estimated by the source.
 * @param details Its details, `null` as long as they are not received.
 */
export function table(
  name: string,
  rows: number | null,
  details: TableDetails | null = null,
): CatalogObject {
  return { name, kind: 'table', rows, enabled: null, details, comment: '' }
}

/**
 * View, without its details.
 *
 * @param name Name of the view.
 */
export function view(name: string): CatalogObject {
  return { name, kind: 'view', rows: null, enabled: null, details: null, comment: '' }
}

/**
 * Stored procedure or function.
 *
 * @param name Name of the routine.
 * @param kind Procedure or function.
 */
export function routine(name: string, kind: 'procedure' | 'function'): CatalogObject {
  return { name, kind, rows: null, enabled: null, details: null, comment: '' }
}

/**
 * Scheduled event.
 *
 * @param name Name of the event.
 * @param enabled Does it run on its schedule?
 */
export function event(name: string, enabled: boolean): CatalogObject {
  return { name, kind: 'event', rows: null, enabled, details: null, comment: '' }
}
