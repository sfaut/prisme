/**
 * Saved connections: list, creation, change, deletion and test.
 */

import { request } from './http'
import type { ConnectionSummary } from './generated/ConnectionSummary'
import type { Created } from './generated/Created'
import type { NewConnection } from './generated/NewConnection'
import type { ProbeRequest } from './generated/ProbeRequest'
import type { ServerInfo } from './generated/ServerInfo'

/**
 * Lists the user's connections, without passwords.
 */
export function listConnections(): Promise<ConnectionSummary[]> {
  return request('GET', '/api/connections')
}

/**
 * Saves a connection; the password is encrypted on the server side.
 *
 * @param connection Connection typed by the user.
 * @returns The id of the created connection.
 */
export function createConnection(connection: NewConnection): Promise<Created> {
  return request('POST', '/api/connections', connection)
}

/**
 * Changes a connection. An empty password keeps the saved password.
 *
 * @param id Id of the connection.
 * @param connection Connection typed by the user.
 */
export function updateConnection(id: number, connection: NewConnection): Promise<void> {
  return request('PUT', `/api/connections/${id}`, connection)
}

/**
 * Deletes a connection.
 *
 * @param id Id of the connection.
 */
export function deleteConnection(id: number): Promise<void> {
  return request('DELETE', `/api/connections/${id}`)
}

/**
 * Connects to the server and returns its flavor (MySQL or MariaDB) and its version.
 *
 * @param id Id of the connection.
 */
export function probeConnection(id: number): Promise<ServerInfo> {
  return request('POST', `/api/connections/${id}/probe`, undefined, { sync: false })
}

/**
 * Connects with typed settings, not saved yet, and returns the server's
 * flavor and version.
 *
 * @param probe
 *     Typed settings, and the saved connection being edited whose password
 *     is taken if the field is empty.
 */
export function probeSettings(probe: ProbeRequest): Promise<ServerInfo> {
  return request('POST', '/api/connections/probe', probe, { sync: false })
}

/**
 * Flavor and version of the server, as displayed (`MariaDB 11.4.2-MariaDB`).
 *
 * @param info Identity of the server.
 */
export function serverLabel(info: ServerInfo): string {
  return `${flavorName(info)} ${info.version}`
}

/**
 * Name of the flavor of a server, as displayed.
 *
 * @param info Identity of the server.
 */
function flavorName(info: ServerInfo): string {
  return info.flavor === 'mariadb' ? 'MariaDB' : 'MySQL'
}

/**
 * Warning on a server older than the oldest version Prisme supports
 * (`docs/V1.md`, section 2.10), `null` for a supported server.
 *
 * @param info Identity of the server.
 */
export function versionWarning(info: ServerInfo): string | null {
  if (info.oldest_supported === null) {
    return null
  }
  return `Version antérieure à ${flavorName(info)} ${info.oldest_supported}, la plus ancienne prise en charge par Prisme : les requêtes s’exécutent, sans garantie.`
}

/**
 * Message of a successful connection test: the flavor and the version of the
 * server, then, on a line of its own, the warning of a version older than
 * those supported, which turns the message into a warning.
 *
 * @param info Identity of the server.
 */
export function probeMessage(info: ServerInfo): { tone: 'success' | 'warning'; title: string } {
  const warning = versionWarning(info)
  return warning === null
    ? { tone: 'success', title: serverLabel(info) }
    : { tone: 'warning', title: `${serverLabel(info)}\n${warning}` }
}
