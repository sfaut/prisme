/**
 * Notions shared by the connections manager and its parts: what its right
 * pane shows, a line of its list, and the creation in progress (wizard).
 */

import type { ConnectionForm } from './connectionForm'
import type { SourceTypeKey } from './sourceTypes'

/**
 * What the right pane of the manager shows: a saved connection, by id, the
 * creation in progress (`new`), or nothing (`null`, no connection yet).
 */
export type ManagerSelection = number | 'new' | null

/**
 * A saved connection in the list of the manager.
 */
export interface ListEntry {
  /**
   * Id of the connection.
   */
  id: number
  /**
   * Name, as typed when changed.
   */
  name: string
  /**
   * Where it points: `host:port`.
   */
  place: string
  /**
   * Changed and not saved?
   */
  changed: boolean
  /**
   * Has it ever connected?
   */
  connected: boolean
}

/**
 * Steps of a creation: the kind of source, its settings, then the result of
 * their test, before saving.
 */
export const CREATION_STEPS = ['Type', 'Réglages', 'Vérification'] as const

/**
 * Number of a step of a creation, from 1.
 */
export type CreationStep = 1 | 2 | 3

/**
 * Creation in progress, kept until it is saved or abandoned, or the manager
 * closes.
 */
export interface Creation {
  /**
   * Current step.
   */
  step: CreationStep
  /**
   * Kind of source chosen.
   */
  type: SourceTypeKey
  /**
   * Input of the settings.
   */
  form: ConnectionForm
  /**
   * Id of the copied connection for a copy, whose password an empty field
   * takes; `null` otherwise.
   */
  copyOf: number | null
}
