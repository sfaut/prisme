/**
 * Connexions enregistrées et connexion sélectionnée dans l'interface.
 */

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import {
  createConnection,
  deleteConnection,
  listConnections,
  updateConnection,
} from '@/api/connections'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { NewConnection } from '@/api/generated/NewConnection'

export const useConnectionsStore = defineStore('connections', () => {
  /**
   * Connexions de l'utilisateur, par nom.
   */
  const connections = ref<ConnectionSummary[]>([])

  /**
   * Identifiant de la connexion sélectionnée, `null` si aucune.
   */
  const selectedId = ref<number | null>(null)

  /**
   * Nombre de modifications de chaque connexion depuis l'ouverture de
   * l'interface, par identifiant : une session ouverte sur une connexion
   * modifiée doit être rouverte.
   */
  const versions = ref<Record<number, number>>({})

  /**
   * Connexion sélectionnée, `null` si aucune.
   */
  const selected = computed(
    () => connections.value.find((connection) => connection.id === selectedId.value) ?? null,
  )

  /**
   * Recharge la liste depuis le serveur.
   */
  async function load(): Promise<void> {
    connections.value = await listConnections()
  }

  /**
   * Enregistre une connexion, recharge la liste et sélectionne la nouvelle connexion.
   *
   * @param connection Connexion saisie par l'utilisateur.
   * @throws {ApiError} Si le serveur refuse la saisie (nom déjà pris, champ invalide...).
   */
  async function create(connection: NewConnection): Promise<void> {
    const { id } = await createConnection(connection)
    await load()
    selectedId.value = id
  }

  /**
   * Modifie une connexion et recharge la liste.
   *
   * @param id Identifiant de la connexion.
   * @param connection Connexion saisie ; mot de passe vide : inchangé.
   * @throws {ApiError} Si le serveur refuse la saisie (nom déjà pris, champ invalide...).
   */
  async function update(id: number, connection: NewConnection): Promise<void> {
    await updateConnection(id, connection)
    versions.value[id] = (versions.value[id] ?? 0) + 1
    await load()
  }

  /**
   * Supprime une connexion et recharge la liste. Si c'était la connexion choisie, plus aucune ne l'est.
   *
   * @param id Identifiant de la connexion.
   */
  async function remove(id: number): Promise<void> {
    await deleteConnection(id)
    if (selectedId.value === id) {
      selectedId.value = null
    }
    await load()
  }

  return { connections, selectedId, versions, selected, load, create, update, remove }
})
