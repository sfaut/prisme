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
  return request('POST', `/api/connections/${id}/probe`)
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
  return request('POST', '/api/connections/probe', probe)
}

/**
 * Flavor and version of the server, as displayed (`MariaDB 11.4.2-MariaDB`).
 *
 * @param info Identity of the server.
 */
export function serverLabel(info: ServerInfo): string {
  return `${info.flavor === 'mariadb' ? 'MariaDB' : 'MySQL'} ${info.version}`
}
