/**
 * Catalogues des connexions (bases, tables, colonnes), remplis au fil de l'eau
 * et gardés en mémoire.
 *
 * Le catalogue d'une connexion se remplit à mesure que les messages du
 * serveur arrivent : les bases d'abord, puis leurs objets (tables, vues,
 * procédures, fonctions), puis le détail des tables.
 * Une partie pas encore reçue vaut `null`. Revenir sur une connexion déjà
 * explorée réutilise son catalogue ; « Rafraîchir » le relit.
 *
 * Un catalogue compte des dizaines de milliers d'objets (tables, colonnes,
 * index...) : il reste hors de la réactivité de Vue, qui suivrait chacun
 * d'eux. Mesuré sur 20 bases de 50 tables, le parcours profond du catalogue
 * à chaque message reçu figeait l'interface pendant 3 minutes. Seul un
 * numéro de révision par connexion est réactif : il augmente au plus toutes
 * les 100 ms pendant la réception, et à la fin.
 */

import type { BeautifyCatalog } from '@/api/generated/BeautifyCatalog'
import { defineStore } from 'pinia'
import { markRaw, ref, shallowRef, triggerRef } from 'vue'

import { streamCatalog } from '@/api/explorer'
import type { CatalogEvent } from '@/api/generated/CatalogEvent'
import type { Flavor } from '@/api/generated/Flavor'
import type { RoutineInfo } from '@/api/generated/RoutineInfo'
import type { TableDetails } from '@/api/generated/TableDetails'
import type { TableKind } from '@/api/generated/TableKind'

/**
 * Table ou vue du catalogue.
 */
export interface CatalogTable {
  name: string
  kind: TableKind
  /**
   * Colonnes, index, clefs étrangères et déclencheurs, `null` tant qu'ils ne sont pas reçus.
   */
  details: TableDetails | null
}

/**
 * Base du catalogue.
 */
export interface CatalogSchema {
  name: string
  /**
   * Tables et vues, `null` tant qu'elles ne sont pas reçues.
   */
  tables: CatalogTable[] | null
  /**
   * Procédures et fonctions, `null` tant qu'elles ne sont pas reçues.
   */
  routines: RoutineInfo[] | null
}

/**
 * Catalogue d'une connexion, en cours de réception ou complet.
 */
export interface Catalog {
  schemas: CatalogSchema[]
  /**
   * Variante du serveur, annoncée avec les bases ; `null` avant.
   */
  flavor: Flavor | null
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
  return { schemas: [], flavor: null, done: false, errors: [] }
}

/**
 * Détail vide d'une table dont le serveur n'a rien renvoyé (droits restreints...).
 *
 * @param table Nom de la table.
 */
function emptyDetails(table: string): TableDetails {
  return { table, columns: [], indexes: [], foreign_keys: [], triggers: [] }
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
      catalog.schemas = event.schemas.map((name) => ({ name, tables: null, routines: null }))
      catalog.flavor = event.flavor
      break
    case 'objects': {
      const schema = catalog.schemas.find((candidate) => candidate.name === event.schema)
      if (schema) {
        schema.tables = event.objects.tables.map((table) => ({ ...table, details: null }))
        schema.routines = event.objects.routines
      }
      break
    }
    case 'details': {
      const tables = catalog.schemas.find((candidate) => candidate.name === event.schema)?.tables
      for (const details of event.tables) {
        const target = tables?.find((candidate) => candidate.name === details.table)
        if (target) {
          target.details = details
        }
      }
      // Tables sans détail reçu : rien n'arrivera plus.
      for (const table of tables ?? []) {
        table.details ??= emptyDetails(table.name)
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

/**
 * Délai entre deux signalements de nouvelles parties du catalogue, en millisecondes.
 */
const REVISION_DELAY = 100

export const useCatalogStore = defineStore('catalog', () => {
  /**
   * Catalogues, par identifiant de connexion. Les objets sont bruts (`markRaw`) :
   * lire `revisions` pour être prévenu de leurs changements.
   */
  const catalogs = shallowRef<Record<number, Catalog>>({})

  /**
   * Numéro de révision de chaque catalogue : il augmente quand de nouvelles
   * parties sont arrivées.
   */
  const revisions = ref<Record<number, number>>({})

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
    const catalog = markRaw(emptyCatalog())
    catalogs.value[connectionId] = catalog
    triggerRef(catalogs)
    revisions.value[connectionId] = (revisions.value[connectionId] ?? 0) + 1

    /**
     * Signalement différé des nouvelles parties, absent si rien n'attend.
     */
    let timer: ReturnType<typeof setTimeout> | undefined
    const signal = () => {
      timer = undefined
      revisions.value[connectionId] = (revisions.value[connectionId] ?? 0) + 1
    }
    try {
      await streamCatalog(connectionId, (event) => {
        applyCatalogEvent(catalog, event)
        timer ??= setTimeout(signal, REVISION_DELAY)
      })
    } catch (failure) {
      // Rien de lisible : la prochaine ouverture de l'explorateur réessaiera.
      delete catalogs.value[connectionId]
      triggerRef(catalogs)
      throw failure
    } finally {
      clearTimeout(timer)
      signal()
    }
  }

  /**
   * Catalogue d'une connexion, `undefined` s'il n'a jamais été lu. Lire cette
   * fonction dans un `computed` le fait recalculer à chaque révision.
   *
   * @param connectionId Identifiant de la connexion.
   */
  function catalogOf(connectionId: number): Catalog | undefined {
    void revisions.value[connectionId]
    return catalogs.value[connectionId]
  }

  /**
   * Noms du catalogue tels que définis, pour l'enjoliveur : bases, tables
   * et vues, colonnes, fonctions et procédures, toutes bases confondues.
   * Les parties pas encore lues sont simplement absentes.
   *
   * @param connectionId Identifiant de la connexion.
   */
  function catalogNames(connectionId: number): BeautifyCatalog {
    const schemas = catalogs.value[connectionId]?.schemas ?? []
    const tables = schemas.flatMap((schema) => schema.tables ?? [])
    return {
      schemas: schemas.map((schema) => schema.name),
      tables: tables.map((table) => table.name),
      columns: tables.flatMap((table) =>
        (table.details?.columns ?? []).map((column) => column.name),
      ),
      routines: schemas.flatMap((schema) => (schema.routines ?? []).map((routine) => routine.name)),
    }
  }

  return { catalogs, revisions, load, catalogOf, catalogNames }
})
