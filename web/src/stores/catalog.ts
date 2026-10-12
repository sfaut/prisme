/**
 * Catalogues des connexions (bases, tables, colonnes), gardés en mémoire.
 *
 * Le catalogue d'une connexion est lu une fois, puis réutilisé : revenir sur
 * une connexion déjà ouverte affiche son explorateur sans attendre. Le bouton
 * « Rafraîchir » de l'explorateur force une nouvelle lecture.
 */

import { defineStore } from 'pinia'
import { ref } from 'vue'

import { loadCatalog } from '@/api/explorer'
import type { SchemaInfo } from '@/api/generated/SchemaInfo'

export const useCatalogStore = defineStore('catalog', () => {
  /**
   * Catalogues déjà lus, par identifiant de connexion.
   */
  const catalogs = ref<Record<number, SchemaInfo[]>>({})

  /**
   * Lit le catalogue d'une connexion, sauf s'il est déjà en mémoire.
   *
   * @param connectionId Identifiant de la connexion.
   * @param refresh Relit le catalogue même s'il est en mémoire.
   * @throws {ApiError} Si le serveur cible est injoignable.
   */
  async function load(connectionId: number, refresh = false): Promise<void> {
    if (refresh || catalogs.value[connectionId] === undefined) {
      catalogs.value[connectionId] = await loadCatalog(connectionId)
    }
  }

  return { catalogs, load }
})
