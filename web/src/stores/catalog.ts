/**
 * Catalogues des connexions (bases, tables, colonnes), remplis au fil de l'eau
 * et gardés en mémoire.
 *
 * Le catalogue d'une connexion se remplit à mesure que les messages du
 * serveur arrivent : les bases d'abord, puis les tables, puis les colonnes.
 * Une partie pas encore reçue vaut `null`. Revenir sur une connexion déjà
 * explorée réutilise son catalogue ; « Rafraîchir » le relit.
 */

import { defineStore } from 'pinia'
import { reactive, ref } from 'vue'

import { streamCatalog } from '@/api/explorer'
import type { CatalogEvent } from '@/api/generated/CatalogEvent'
import type { ColumnInfo } from '@/api/generated/ColumnInfo'
import type { TableKind } from '@/api/generated/TableKind'

/**
 * Table du catalogue.
 */
export interface CatalogTable {
  name: string
  kind: TableKind
  /**
   * Colonnes, `null` tant qu'elles ne sont pas reçues.
   */
  columns: ColumnInfo[] | null
}

/**
 * Base du catalogue.
 */
export interface CatalogSchema {
  name: string
  /**
   * Tables, `null` tant qu'elles ne sont pas reçues.
   */
  tables: CatalogTable[] | null
}

/**
 * Catalogue d'une connexion, en cours de réception ou complet.
 */
export interface Catalog {
  schemas: CatalogSchema[]
  /**
   * Tout le catalogue est-il reçu ?
   */
  done: boolean
  /**
   * Erreurs de lecture signalées par le serveur.
   */
  errors: string[]
}

/**
 * Catalogue vide, en attente du premier message.
 */
export function emptyCatalog(): Catalog {
  return { schemas: [], done: false, errors: [] }
}

/**
 * Range un message du serveur dans le catalogue.
 *
 * @param catalog Catalogue à compléter, modifié sur place.
 * @param event Message reçu.
 */
export function applyCatalogEvent(catalog: Catalog, event: CatalogEvent): void {
  switch (event.type) {
    case 'schemas':
      catalog.schemas = event.schemas.map((name) => ({ name, tables: null }))
      break
    case 'tables': {
      const schema = catalog.schemas.find((candidate) => candidate.name === event.schema)
      if (schema) {
        schema.tables = event.tables.map((table) => ({ ...table, columns: null }))
      }
      break
    }
    case 'columns': {
      const tables = catalog.schemas.find((candidate) => candidate.name === event.schema)?.tables
      for (const { table, columns } of event.tables) {
        const target = tables?.find((candidate) => candidate.name === table)
        if (target) {
          target.columns = columns
        }
      }
      // Tables sans colonne reçue (droits restreints...) : rien n'arrivera plus.
      for (const table of tables ?? []) {
        table.columns ??= []
      }
      break
    }
    case 'error':
      catalog.errors.push(event.message)
      break
    case 'done':
      catalog.done = true
      break
  }
}

export const useCatalogStore = defineStore('catalog', () => {
  /**
   * Catalogues, par identifiant de connexion.
   */
  const catalogs = ref<Record<number, Catalog>>({})

  /**
   * Lit le catalogue d'une connexion au fil de l'eau, sauf s'il est déjà en mémoire.
   *
   * @param connectionId Identifiant de la connexion.
   * @param refresh Relit le catalogue même s'il est en mémoire.
   * @throws {ApiError} Si le serveur cible est injoignable.
   */
  async function load(connectionId: number, refresh = false): Promise<void> {
    if (!refresh && catalogs.value[connectionId] !== undefined) {
      return
    }
    const catalog = reactive(emptyCatalog())
    catalogs.value[connectionId] = catalog
    try {
      await streamCatalog(connectionId, (event) => applyCatalogEvent(catalog, event))
    } catch (failure) {
      // Rien de lisible : la prochaine ouverture de l'explorateur réessaiera.
      delete catalogs.value[connectionId]
      throw failure
    }
  }

  return { catalogs, load }
})
