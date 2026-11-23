/**
 * Input of a connection in the connections manager: empty input, input of a
 * saved connection, comparison with it (a changed input is a draft), input
 * of a copy, the connection sent to the server, and the texts that describe
 * a connection (where it points, its last connection).
 *
 * The input keeps, beside the common settings, those of each family: the
 * server (host, port, account) and the databases held in files (the main
 * one first, then those attached to it). The engine of the connection says
 * which family the server receives, under `settings`, tagged by it
 * (`"family": "server"` or `"files"`).
 *
 * The interface does not know the saved password: the input of a saved
 * connection has an empty password, which the server reads as "unchanged",
 * and as "that of the copied connection" for a copy.
 */

import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { Engine } from '@/api/generated/Engine'
import type { NewConnection } from '@/api/generated/NewConnection'
import type { NewFileDatabase } from '@/api/generated/NewFileDatabase'
import type { NewServerSettings } from '@/api/generated/NewServerSettings'
import type { NewSettings } from '@/api/generated/NewSettings'
import type { SavedSettings } from '@/api/generated/SavedSettings'

/**
 * Family of the settings of each engine: a new engine is forgotten nowhere.
 */
export const ENGINE_FAMILIES: Record<Engine, NewSettings['family']> = {
  mysql: 'server',
  sqlite: 'files',
}

/**
 * Where a database held in files comes from: a file that exists, a new
 * file, created empty when the connection is saved, or a database in
 * memory, empty at each session.
 */
export type DatabaseOrigin = 'existing' | 'new' | 'memory'

/**
 * Input of a database held in files.
 */
export interface DatabaseInput {
  /**
   * Name of its schema in the sessions: `main` for the first one.
   */
  schema: string
  /**
   * Where it comes from.
   */
  origin: DatabaseOrigin
  /**
   * Location as typed, an absolute path or a URL; unused in memory.
   */
  location: string
  /**
   * Does it come from the saved connection? Removed, it then stays in the
   * input, struck through, until saving.
   */
  saved: boolean
  /**
   * Removed from the saved connection at saving, and restorable until then.
   */
  removed: boolean
}

/**
 * Input of a connection, every field filled: the common settings, those of
 * the server and the databases held in files; its engine says which family
 * counts.
 */
export type ConnectionForm = Required<Omit<NewConnection, 'settings'>> &
  Required<NewServerSettings> & {
    /**
     * Databases held in files, the main one first.
     */
    databases: DatabaseInput[]
  }

/**
 * Maximum lengths of the fields, in characters, as the server checks them
 * (`crates/server/src/connections.rs`).
 */
export const LENGTHS = {
  name: 100,
  host: 253,
  username: 80,
  databaseName: 64,
  initSql: 4000,
  schema: 64,
  location: 2048,
} as const

/**
 * Most databases attached to the main one, as the server checks them.
 */
export const ATTACHED_MAX = 10

/**
 * Name of the schema of the main database.
 */
export const MAIN_SCHEMA = 'main'

/**
 * Longest keep-alive delay of a session, in minutes (24 hours).
 */
export const SESSION_TIMEOUT_MAX = 1440

/**
 * Empty input, with MySQL's default port, and a main database to open from
 * an existing file.
 *
 * @param engine Engine of the connection.
 */
export function emptyForm(engine: Engine = 'mysql'): ConnectionForm {
  return {
    name: '',
    engine,
    host: '127.0.0.1',
    port: 3306,
    username: '',
    password: '',
    database_name: '',
    init_sql: '',
    session_timeout_minutes: 5,
    show_system_schemas: true,
    databases: [
      { schema: MAIN_SCHEMA, origin: 'existing', location: '', saved: false, removed: false },
    ],
  }
}

/**
 * Input of a saved connection, password empty.
 *
 * @param connection Saved connection.
 */
export function formOf(connection: ConnectionSummary): ConnectionForm {
  const { name, engine, settings, init_sql, session_timeout_minutes, show_system_schemas } =
    connection
  const empty = emptyForm(engine)
  const { host, port, username, database_name } = settings.family === 'server' ? settings : empty
  const databases: DatabaseInput[] =
    settings.family === 'files'
      ? settings.databases.map(({ schema, location }) =>
          location.scheme === 'memory'
            ? { schema, origin: 'memory', location: '', saved: true, removed: false }
            : {
                schema,
                origin: 'existing',
                location: location.address,
                saved: true,
                removed: false,
              },
        )
      : empty.databases
  return {
    name,
    engine,
    host,
    port,
    username,
    password: '',
    database_name,
    init_sql,
    session_timeout_minutes,
    show_system_schemas,
    databases,
  }
}

/**
 * Database held in files sent to the server for an input: `:memory:` in
 * memory, the file to create for a new one.
 *
 * @param input Input of the database.
 */
function databaseOf(input: DatabaseInput): NewFileDatabase {
  return input.origin === 'memory'
    ? { schema: input.schema, location: ':memory:' }
    : { schema: input.schema, location: input.location, create: input.origin === 'new' }
}

/**
 * Connection sent to the server for an input.
 *
 * @param form Input of the connection.
 */
export function connectionOf(form: ConnectionForm): NewConnection {
  const { name, engine, init_sql, session_timeout_minutes, show_system_schemas } = form
  const { host, port, username, password, database_name } = form
  const settings: NewSettings =
    ENGINE_FAMILIES[engine] === 'files'
      ? {
          family: 'files',
          databases: form.databases.filter((database) => !database.removed).map(databaseOf),
        }
      : { family: 'server', host, port, username, password, database_name }
  return {
    name,
    engine,
    settings,
    init_sql,
    session_timeout_minutes,
    show_system_schemas,
  }
}

/**
 * Does an input differ from the saved connection? A typed password is a
 * change.
 *
 * @param form Current input.
 * @param connection Saved connection.
 */
export function isChanged(form: ConnectionForm, connection: ConnectionSummary): boolean {
  const saved = formOf(connection)
  return (Object.keys(saved) as (keyof ConnectionForm)[]).some((key) =>
    key === 'databases'
      ? !sameDatabases(form.databases, saved.databases)
      : form[key] !== saved[key],
  )
}

/**
 * Are two lists of databases the same, in the same order, none removed? The
 * location of a database in memory, unused, does not count.
 *
 * @param left First list.
 * @param right Second list.
 */
function sameDatabases(left: DatabaseInput[], right: DatabaseInput[]): boolean {
  return (
    left.length === right.length &&
    left.every(
      (database, index) =>
        database.removed === right[index]!.removed &&
        database.schema === right[index]!.schema &&
        database.origin === right[index]!.origin &&
        (database.origin === 'memory' || database.location === right[index]!.location),
    )
  )
}

/**
 * Input of a copy: the same settings, under a name of its own, without the
 * databases removed, the others new to it. The password typed in the input
 * goes with it; empty, the server takes that of the copied connection.
 *
 * @param form Input of the copied connection.
 */
export function copyForm(form: ConnectionForm): ConnectionForm {
  return {
    ...form,
    name: `${form.name}-copie`.slice(0, LENGTHS.name),
    databases: form.databases
      .filter((database) => !database.removed)
      .map((database) => ({ ...database, saved: false })),
  }
}

/**
 * Protocol of a location as typed, as the server will read it: `file` for
 * an absolute path, `memory` for `:memory:`, the scheme of a URL as written
 * (`https`, `s3`...); `null` while nothing says it (empty, relative path).
 *
 * @param location Location typed.
 */
export function schemeOf(location: string): string | null {
  const text = location.trim()
  if (text === ':memory:') {
    return 'memory'
  }
  const url = /^([a-z][a-z0-9+.-]+):\/\//i.exec(text)
  if (url !== null) {
    return url[1]!.toLowerCase()
  }
  return /^(?:[a-z]:[\\/]|[\\/])/i.test(text) ? 'file' : null
}

/**
 * Where a connection points: `host:port` for a server, the location of the
 * main database for databases held in files.
 *
 * @param settings Settings of a saved connection.
 */
export function placeOf(settings: SavedSettings): string {
  if (settings.family === 'files') {
    const main = settings.databases[0]?.location
    return main?.scheme === 'memory' ? ':memory:' : (main?.address ?? '')
  }
  return `${settings.host}:${settings.port}`
}

/**
 * Where an input points, as `placeOf` says it of a saved connection.
 *
 * @param form Input of the connection.
 */
export function placeOfForm(form: ConnectionForm): string {
  if (ENGINE_FAMILIES[form.engine] === 'files') {
    const main = form.databases[0]
    return main === undefined || main.origin === 'memory' ? ':memory:' : main.location
  }
  return `${form.host}:${form.port}`
}

/**
 * Units of the relative dates, from the largest, with their length in seconds.
 */
const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 86_400],
  ['month', 30 * 86_400],
  ['day', 86_400],
  ['hour', 3_600],
  ['minute', 60],
]

/**
 * Relative date, in French: "il y a 5 minutes", "hier", "à l’instant".
 *
 * @param iso Date, in ISO 8601 format.
 * @param now Current time, in milliseconds since the epoch.
 */
export function sinceText(iso: string, now: number = Date.now()): string {
  const seconds = Math.round((Date.parse(iso) - now) / 1000)
  const format = new Intl.RelativeTimeFormat('fr', { numeric: 'auto' })
  for (const [unit, length] of UNITS) {
    if (Math.abs(seconds) >= length) {
      return format.format(Math.round(seconds / length), unit)
    }
  }
  return 'à l’instant'
}

/**
 * State of the last connection: "Connectée il y a 5 minutes", or "Jamais
 * connectée".
 *
 * @param connection Saved connection.
 * @param now Current time, in milliseconds since the epoch.
 */
export function connectedText(connection: ConnectionSummary, now: number = Date.now()): string {
  return connection.probe === null
    ? 'Jamais connectée'
    : `Connectée ${sinceText(connection.probe.connected_at, now)}`
}
