/**
 * Saved connections and connection selected in the interface.
 *
 * The server flavor of each connection (explorer badge) is detected in the
 * background, without waiting for the source to be opened: after each load
 * of the list, a connection of unknown flavor is tested once
 * (`probeConnection`, which the server remembers); a changed connection is
 * tested again. A failure leaves the badge absent.
 */

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import {
  createConnection,
  deleteConnection,
  listConnections,
  probeConnection,
  updateConnection,
} from '@/api/connections'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { NewConnection } from '@/api/generated/NewConnection'

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
   * Tests in the background the connections whose server is unknown, not
   * tested yet, and records the received flavor and version. A connection
   * whose flavor was saved before the version is tested once more.
   */
  function detectFlavors(): void {
    for (const connection of connections.value) {
      if (connection.server_version !== null || probed.has(connection.id)) {
        continue
      }
      probed.add(connection.id)
      const id = connection.id
      Promise.resolve()
        .then(() => probeConnection(id))
        .then((server) => {
          const current = connections.value.find((item) => item.id === id)
          if (current) {
            current.server_flavor = server.flavor
            current.server_version = server.version
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
   * @throws {ApiError} If the server refuses the input (name already taken, invalid field...).
   */
  async function create(connection: NewConnection): Promise<void> {
    const { id } = await createConnection(connection)
    await load()
    selectedId.value = id
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
      changed.server_flavor = null
      changed.server_version = null
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

  return { connections, selectedId, versions, selected, load, create, update, remove }
})
