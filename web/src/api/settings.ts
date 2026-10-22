/**
 * Settings of the user: default row limit, read-only sessions and allowed
 * writes, retention of the console, indentation.
 */

import { request } from './http'
import type { Settings } from './generated/Settings'
import type { SettingsUpdate } from './generated/SettingsUpdate'

/**
 * Reads the settings, with the allowed row limits.
 */
export function getSettings(): Promise<Settings> {
  return request('GET', '/api/settings')
}

/**
 * Changes the settings and returns the updated settings.
 *
 * @param update
 *     New settings: row limit among the allowed choices, retentions between
 *     1 and 3,650 days.
 * @throws {ApiError} If a value is not allowed.
 */
export function updateSettings(update: SettingsUpdate): Promise<Settings> {
  return request('PUT', '/api/settings', update)
}
