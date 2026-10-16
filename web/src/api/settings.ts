/**
 * Settings of the user: for now, the default row limit.
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
 * @param update New row limit, among the allowed choices.
 */
export function updateSettings(update: SettingsUpdate): Promise<Settings> {
  return request('PUT', '/api/settings', update)
}
