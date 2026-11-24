/**
 * Saved connections, connection selected in the interface, and opening of
 * the connections manager.
 *
 * The server flavor of each connection (navigator badge) is detected in the
 * background, without waiting for the source to be opened: after each load
 * of the list, a connection of unknown flavor is tested once
 * (`probeConnection`, which the server remembers, with the date of the
 * connection); a changed connection is tested again. A failure leaves the
 * badge absent.
 *
 * The connections manager (`components/connections/ConnectionManager.vue`)
 * is mounted once; the top bar and the navigator open it here, on a
 * connection or on the creation of a new one.
 */

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import {
  copyConnection,
  createConnection,
  deleteConnection,
  listConnections,
  probeConnection,
  updateConnection,
} from '@/api/connections'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { NewConnection } from '@/api/generated/NewConnection'

/**
 * What the connections manager shows when it opens: a connection, by id,
 * the creation of a new one (`new`), or, `null`, the connection it showed
 * last.
 */
export type ManagerTarget = number | 'new' | null

export const useConnectionsStore = defineStore('connections', () => {
  /**
   * Connections of the user, by name.
   */
  const connections = ref<ConnectionSummary[]>([])

  /**
   * Id of the selected connection, `null` if none.
   */
  const selectedId = ref<number | null>(null)

  /**
   * Number of changes of each connection since the interface was opened, by
   * id: a session open on a changed connection must be reopened.
   */
  const versions = ref<Record<number, number>>({})

  /**
   * Is the connections manager open, and on what? `target` changes with each
   * opening, even on the same connection: the manager follows it.
   */
  const manager = ref<{ open: boolean; target: ManagerTarget; opening: number }>({
    open: false,
    target: null,
    opening: 0,
  })

  /**
   * Connections already tested to know their flavor, since the interface was
   * opened.
   */
  const probed = new Set<number>()

  /**
   * Selected connection, `null` if none.
   */
  const selected = computed(
    () => connections.value.find((connection) => connection.id === selectedId.value) ?? null,
  )

  /**
   * Reloads the list from the server.
   */
  async function load(): Promise<void> {
    connections.value = await listConnections()
    detectFlavors()
  }

  /**
   * First reading of the list, shared by its callers; `null` before it, or
   * after it failed.
   */
  let listed: Promise<void> | null = null

  /**
   * Reads the list from the server once, for every navigator (one per
   * editor); a failed reading is tried again by the next call.
   */
  function ensureLoaded(): Promise<void> {
    listed ??= load().catch((failure: unknown) => {
      listed = null
      throw failure
    })
    return listed
  }

  /**
   * Tests in the background the connections whose server is unknown, not
   * tested yet, and records the received flavor and version, and the date of
   * the connection, which the server remembers too.
   */
  function detectFlavors(): void {
    for (const connection of connections.value) {
      if (connection.probe !== null || probed.has(connection.id)) {
        continue
      }
      probed.add(connection.id)
      const id = connection.id
      Promise.resolve()
        .then(() => probeConnection(id))
        .then((server) => {
          const current = connections.value.find((item) => item.id === id)
          if (current) {
            current.probe = {
              flavor: server.flavor,
              version: server.version,
              connected_at: new Date().toISOString(),
            }
          }
        })
        .catch(() => {
          // Server unreachable: no badge, new attempt on the next page load.
        })
    }
  }

  /**
   * Saves a connection, reloads the list and selects the new connection.
   *
   * @param connection Connection typed by the user.
   * @returns The id of the new connection.
   * @throws {ApiError} If the server refuses the input (name already taken, invalid field...).
   */
  async function create(connection: NewConnection): Promise<number> {
    const { id } = await createConnection(connection)
    await load()
    selectedId.value = id
    return id
  }

  /**
   * Saves a copy of a connection, reloads the list and selects the copy.
   *
   * @param sourceId Id of the copied connection.
   * @param connection Settings of the copy; empty password: that of the copied connection.
   * @returns The id of the copy.
   * @throws {ApiError} If the server refuses the input (name already taken, invalid field...).
   */
  async function copy(sourceId: number, connection: NewConnection): Promise<number> {
    const { id } = await copyConnection(sourceId, connection)
    await load()
    selectedId.value = id
    return id
  }

  /**
   * Changes a connection and reloads the list.
   *
   * @param id Id of the connection.
   * @param connection Typed connection; empty password: unchanged.
   * @throws {ApiError} If the server refuses the input (name already taken, invalid field...).
   */
  async function update(id: number, connection: NewConnection): Promise<void> {
    await updateConnection(id, connection)
    versions.value[id] = (versions.value[id] ?? 0) + 1
    // Host or port changed: the flavor must be tested again.
    probed.delete(id)
    connections.value = await listConnections()
    const changed = connections.value.find((item) => item.id === id)
    if (changed) {
      changed.probe = null
    }
    detectFlavors()
  }

  /**
   * Deletes a connection and reloads the list. If it was the chosen connection, none is anymore.
   *
   * @param id Id of the connection.
   */
  async function remove(id: number): Promise<void> {
    await deleteConnection(id)
    if (selectedId.value === id) {
      selectedId.value = null
    }
    await load()
  }

  /**
   * Opens the connections manager.
   *
   * @param target
   *     Connection to show, `new` to create one; absent, the connection
   *     shown last.
   */
  function openManager(target: ManagerTarget = null): void {
    manager.value = { open: true, target, opening: manager.value.opening + 1 }
  }

  /**
   * Closes the connections manager.
   */
  function closeManager(): void {
    manager.value.open = false
  }

  return {
    connections,
    selectedId,
    versions,
    manager,
    selected,
    load,
    ensureLoaded,
    create,
    copy,
    update,
    remove,
    openManager,
    closeManager,
  }
})
