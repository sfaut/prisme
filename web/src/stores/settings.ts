/**
 * Settings of the user: default row limit and allowed choices, read-only
 * mode of the query sessions, retention of the console.
 */

import { defineStore } from 'pinia'
import { ref } from 'vue'

import type { Settings } from '@/api/generated/Settings'
import type { SettingsUpdate } from '@/api/generated/SettingsUpdate'
import { getSettings, updateSettings } from '@/api/settings'

export const useSettingsStore = defineStore('settings', () => {
  /**
   * Settings read from the server, `null` before the first load.
   */
  const settings = ref<Settings | null>(null)

  /**
   * Loads the settings from the server.
   */
  async function load(): Promise<void> {
    settings.value = await getSettings()
  }

  /**
   * Changes some settings, the others keeping their current value.
   *
   * @param change Settings to change.
   * @throws {Error} If the settings are not loaded yet.
   * @throws {ApiError} If a value is not allowed.
   */
  async function update(change: Partial<SettingsUpdate>): Promise<void> {
    const current = settings.value
    if (current === null) {
      throw new Error('Paramètres non chargés')
    }
    settings.value = await updateSettings({
      row_limit: current.row_limit,
      read_only_session: current.read_only_session,
      history_retention_days: current.history_retention_days,
      debug_retention_days: current.debug_retention_days,
      ...change,
    })
  }

  return { settings, load, update }
})
