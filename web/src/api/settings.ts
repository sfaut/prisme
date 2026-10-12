/**
 * Réglages de l'utilisateur : pour l'instant, la limite de lignes par défaut.
 */

import { request } from './http'
import type { Settings } from './generated/Settings'
import type { SettingsUpdate } from './generated/SettingsUpdate'

/**
 * Lit les réglages, avec les limites de lignes permises.
 */
export function getSettings(): Promise<Settings> {
  return request('GET', '/api/settings')
}

/**
 * Modifie les réglages et renvoie les réglages à jour.
 *
 * @param update Nouvelle limite de lignes, parmi les choix permis.
 */
export function updateSettings(update: SettingsUpdate): Promise<Settings> {
  return request('PUT', '/api/settings', update)
}
