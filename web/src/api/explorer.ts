/**
 * Explorateur d'objets : catalogue d'une connexion, reçu au fil de l'eau.
 *
 * Le serveur envoie une ligne JSON par message (`CatalogEvent`) : les bases,
 * puis les tables base par base, puis les colonnes base par base. Chaque
 * message est transmis dès sa réception, sans attendre la fin du flux.
 */

import { toApiError } from './http'
import type { CatalogEvent } from './generated/CatalogEvent'

/**
 * Lit le catalogue d'une connexion et transmet chaque message dès qu'il arrive.
 *
 * @param connectionId Identifiant de la connexion.
 * @param onEvent Appelée pour chaque message reçu.
 * @throws {ApiError} Si la connexion au serveur cible échoue.
 */
export async function streamCatalog(
  connectionId: number,
  onEvent: (event: CatalogEvent) => void,
): Promise<void> {
  const response = await fetch(`/api/connections/${connectionId}/catalog`)
  if (!response.ok || response.body === null) {
    throw await toApiError(response)
  }
  const reader = response.body.pipeThrough(new TextDecoderStream()).getReader()
  let pending = ''
  for (;;) {
    const { value, done } = await reader.read()
    if (done) {
      break
    }
    pending += value
    const lines = pending.split('\n')
    // La dernière ligne peut être incomplète : elle attend la suite.
    pending = lines.pop() ?? ''
    for (const line of lines) {
      if (line.trim() !== '') {
        onEvent(JSON.parse(line) as CatalogEvent)
      }
    }
  }
}
