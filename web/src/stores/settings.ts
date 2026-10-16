/**
 * Settings of the user: default row limit and allowed choices.
 */

import { defineStore } from 'pinia'
import { ref } from 'vue'

import type { Settings } from '@/api/generated/Settings'
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
   * Changes the default row limit.
   *
   * @param rowLimit New limit, among `settings.row_limit_choices`.
   * @throws {ApiError} If the value is not allowed.
   */
  async function setRowLimit(rowLimit: number): Promise<void> {
    settings.value = await updateSettings({ row_limit: rowLimit })
  }

  return { settings, load, setRowLimit }
})
