/**
 * Connexions enregistrées et connexion sélectionnée dans l'interface.
 *
 * La variante du serveur de chaque connexion (badge de l'explorateur) est
 * détectée en arrière-plan, sans attendre qu'on ouvre la source : après
 * chaque chargement de la liste, une connexion de variante inconnue est
 * testée une fois (`probeConnection`, qui la retient côté serveur) ; une
 * connexion modifiée est testée de nouveau. Un échec laisse le badge absent.
 */

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import {
  createConnection,
  deleteConnection,
  listConnections,
  probeConnection,
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
   * Connexions déjà testées pour connaître leur variante, depuis l'ouverture
   * de l'interface.
   */
  const probed = new Set<number>()

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
    detectFlavors()
  }

  /**
   * Teste en arrière-plan les connexions de variante inconnue, pas encore
   * testées, et note la variante reçue.
   */
  function detectFlavors(): void {
    for (const connection of connections.value) {
      if (connection.server_flavor !== null || probed.has(connection.id)) {
        continue
      }
      probed.add(connection.id)
      const id = connection.id
      Promise.resolve()
        .then(() => probeConnection(id))
        .then((server) => {
          const current = connections.value.find((item) => item.id === id)
          if (current) {
            current.server_flavor = server.flavor
          }
        })
        .catch(() => {
          // Serveur injoignable : pas de badge, nouvel essai au prochain chargement de la page.
        })
    }
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
    // Hôte ou port changés : la variante est à tester de nouveau.
    probed.delete(id)
    connections.value = await listConnections()
    const changed = connections.value.find((item) => item.id === id)
    if (changed) {
      changed.server_flavor = null
    }
    detectFlavors()
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
