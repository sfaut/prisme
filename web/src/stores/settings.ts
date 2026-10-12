/**
 * Réglages de l'utilisateur : limite de lignes par défaut et choix permis.
 */

import { defineStore } from 'pinia'
import { ref } from 'vue'

import type { Settings } from '@/api/generated/Settings'
import { getSettings, updateSettings } from '@/api/settings'

export const useSettingsStore = defineStore('settings', () => {
  /**
   * Réglages lus sur le serveur, `null` avant le premier chargement.
   */
  const settings = ref<Settings | null>(null)

  /**
   * Charge les réglages depuis le serveur.
   */
  async function load(): Promise<void> {
    settings.value = await getSettings()
  }

  /**
   * Change la limite de lignes par défaut.
   *
   * @param rowLimit Nouvelle limite, parmi `settings.row_limit_choices`.
   * @throws {ApiError} Si la valeur n'est pas permise.
   */
  async function setRowLimit(rowLimit: number): Promise<void> {
    settings.value = await updateSettings({ row_limit: rowLimit })
  }

  return { settings, load, setRowLimit }
})
