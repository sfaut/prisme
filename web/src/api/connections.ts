/**
 * Saved connections: list, creation, copy, change, deletion and test.
 */

import { serverName } from './flavors'
import { connectMessage } from './messages'
import { request } from './http'
import type { ConnectionSummary } from './generated/ConnectionSummary'
import type { Created } from './generated/Created'
import type { NewConnection } from './generated/NewConnection'
import type { ProbeRequest } from './generated/ProbeRequest'
import type { SourceInfo } from './generated/SourceInfo'

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
 * Saves a copy of a connection, with the settings typed for the copy. An
 * empty password takes that of the copied connection.
 *
 * @param id Id of the copied connection.
 * @param connection Settings of the copy.
 * @returns The id of the copy.
 */
export function copyConnection(id: number, connection: NewConnection): Promise<Created> {
  return request('POST', `/api/connections/${id}/copies`, connection)
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
export function probeConnection(id: number): Promise<SourceInfo> {
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
export function probeSettings(probe: ProbeRequest): Promise<SourceInfo> {
  return request('POST', '/api/connections/probe', probe, { sync: false })
}

/**
 * Flavor and version of the server, as displayed (`MariaDB 11.4.2-MariaDB`).
 *
 * @param info Identity of the server.
 */
export function serverLabel(info: SourceInfo): string {
  return serverName(info.flavor, info.version)
}

/**
 * Warning on a server older than the oldest version Prisme supports
 * (`docs/V1.md`, section 2.10), `null` for a supported server.
 *
 * @param info Identity of the server.
 */
export function versionWarning(info: SourceInfo): string | null {
  if (info.oldest_supported === null) {
    return null
  }
  return `Version antérieure à ${serverName(info.flavor, info.oldest_supported)}, la plus ancienne prise en charge par Prisme : les requêtes s’exécutent, sans garantie.`
}

/**
 * Warning on the databases of a source held in files that could not be
 * opened, each with why: the sessions open without them (`docs/TODO-SQLITE.md`,
 * decision 7); `null` when every one opened.
 *
 * @param info Identity of the source, with the state of its databases.
 */
export function unavailableWarning(info: SourceInfo): string | null {
  const unavailable = (info.databases ?? []).flatMap((database) =>
    database.status === 'unavailable'
      ? [{ schema: database.schema, why: connectMessage(database.failure) }]
      : [],
  )
  if (unavailable.length === 0) {
    return null
  }
  if (unavailable.length === 1) {
    const [{ schema, why }] = unavailable as [(typeof unavailable)[number]]
    return `La base « ${schema} » ne s’ouvre pas : les sessions s’ouvrent sans elle.\n${why}`
  }
  const lines = unavailable.map(({ schema, why }) => `« ${schema} » : ${why}`)
  return [
    `${unavailable.length} bases ne s’ouvrent pas : les sessions s’ouvrent sans elles.`,
    ...lines,
  ].join('\n')
}

/**
 * Message of a successful connection test: the flavor and the version of the
 * source, then, each on lines of its own, the warning of a version older
 * than those supported and that of the databases that could not be opened,
 * which turn the message into a warning.
 *
 * @param info Identity of the source.
 * @param databases
 *     Does the message tell the databases that could not be opened? Not when
 *     the state of each database is shown beside it (step "Vérification").
 */
export function probeMessage(
  info: SourceInfo,
  databases = true,
): { tone: 'success' | 'warning'; title: string } {
  const warnings = [versionWarning(info), databases ? unavailableWarning(info) : null].filter(
    (warning) => warning !== null,
  )
  return warnings.length === 0
    ? { tone: 'success', title: serverLabel(info) }
    : { tone: 'warning', title: [serverLabel(info), ...warnings].join('\n') }
}
