/**
 * Session de requêtes sur une connexion enregistrée, par WebSocket.
 *
 * Le serveur garde une connexion ouverte au serveur cible pendant toute la
 * session, exécute une requête à la fois et envoie le résultat en streaming :
 * `columns`, puis des lots `rows`, puis `done` (ou `error`). Le protocole est
 * décrit dans `crates/server/src/query_session.rs`.
 *
 * `applyMessage` met l'état à jour message par message ; elle ne dépend ni de
 * Vue ni du WebSocket, ce qui la rend simple à tester. `useQuerySession`
 * l'utilise pour exposer un état réactif aux composants.
 */

import { onScopeDispose, reactive, shallowReactive } from 'vue'

import type { LimitReport } from './generated/LimitReport'
import type { QueryRequest } from './generated/QueryRequest'
import type { ServerInfo } from './generated/ServerInfo'
import type { ServerMessage } from './generated/ServerMessage'

/**
 * Cellule de résultat : du texte, ou `null` pour NULL.
 */
export type Cell = string | null

/**
 * Étape de la session.
 *
 * - `connecting` : ouverture du WebSocket et connexion au serveur cible ;
 * - `ready` : prête à exécuter une requête ;
 * - `running` : une requête est en cours ;
 * - `closed` : WebSocket fermé, la session doit être rouverte.
 */
export type SessionStatus = 'connecting' | 'ready' | 'running' | 'closed'

/**
 * Résultat de la dernière requête, complété au fil des messages.
 */
export interface QueryResult {
  /**
   * Texte réellement exécuté, avec le `LIMIT` éventuellement ajouté.
   */
  sql: string
  /**
   * Traitement de la limite de lignes.
   */
  limit: LimitReport
  /**
   * Noms des colonnes.
   */
  columns: string[]
  /**
   * Lignes reçues jusqu'ici.
   */
  rows: Cell[][]
  /**
   * La requête est-elle terminée ?
   */
  done: boolean
  /**
   * Des lignes au-delà de la limite ont-elles été écartées ?
   */
  truncated: boolean
  /**
   * Nombre de lignes modifiées, pour une écriture.
   */
  affectedRows: number
  /**
   * Durée d'exécution, en millisecondes.
   */
  elapsedMs: number
}

/**
 * État complet d'une session.
 */
export interface SessionState {
  /**
   * Étape de la session.
   */
  status: SessionStatus
  /**
   * Identité du serveur cible, connue une fois la session prête.
   */
  server: ServerInfo | null
  /**
   * Résultat de la dernière requête.
   */
  result: QueryResult | null
  /**
   * Dernière erreur, à afficher à l'utilisateur.
   */
  error: string | null
}

/**
 * Crée l'état d'une session qui s'ouvre.
 */
export function initialState(): SessionState {
  return { status: 'connecting', server: null, result: null, error: null }
}

/**
 * Met à jour l'état avec un message du serveur.
 *
 * Modifie `state` sur place : les lignes s'ajoutent au tableau existant, sans
 * recopier un résultat qui peut compter des milliers de lignes.
 *
 * @param state État de la session.
 * @param message Message reçu du serveur.
 */
export function applyMessage(state: SessionState, message: ServerMessage): void {
  switch (message.type) {
    case 'ready':
      state.status = 'ready'
      state.server = message.server
      break
    case 'columns':
      state.result = {
        sql: message.sql,
        limit: message.limit,
        columns: message.columns,
        rows: [],
        done: false,
        truncated: false,
        affectedRows: 0,
        elapsedMs: 0,
      }
      break
    case 'rows':
      state.result?.rows.push(...message.rows)
      break
    case 'done':
      if (state.result) {
        state.result.done = true
        state.result.truncated = message.truncated
        state.result.affectedRows = message.affected_rows
        state.result.elapsedMs = message.elapsed_ms
      }
      state.status = 'ready'
      break
    case 'error':
      state.error = message.message
      // Avant `ready`, l'erreur est celle de la connexion au serveur cible :
      // le serveur ferme alors la session.
      state.status = state.status === 'connecting' ? 'closed' : 'ready'
      break
  }
}

/**
 * Note le début d'une requête dans l'état.
 *
 * @param state État de la session.
 */
export function startQuery(state: SessionState): void {
  state.status = 'running'
  state.result = null
  state.error = null
}

/**
 * Construit l'adresse WebSocket de la session d'une connexion, sur l'hôte de la page.
 *
 * @param connectionId Identifiant de la connexion.
 * @param location Adresse de la page.
 */
export function sessionUrl(connectionId: number, location: Location): string {
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${protocol}//${location.host}/api/connections/${connectionId}/query`
}

/**
 * Ouvre une session de requêtes sur la connexion `connectionId`.
 *
 * La session se ferme d'elle-même quand le composant qui l'a ouverte disparaît.
 *
 * @param connectionId Identifiant de la connexion.
 * @returns L'état réactif de la session, et les fonctions pour exécuter une requête et fermer la session.
 */
export function useQuerySession(connectionId: number) {
  const state = reactive(initialState())
  const socket = new WebSocket(sessionUrl(connectionId, window.location))

  socket.addEventListener('message', (event: MessageEvent<string>) => {
    const message = JSON.parse(event.data) as ServerMessage
    applyMessage(state, message)
    if (message.type === 'columns' && state.result) {
      // Réactivité superficielle : l'arrivée d'un lot rafraîchit l'affichage,
      // sans rendre réactive chacune des milliers de lignes.
      state.result.rows = shallowReactive([])
    }
  })
  socket.addEventListener('close', () => {
    state.status = 'closed'
  })

  /**
   * Exécute une requête, si la session est prête.
   *
   * @param sql Texte SQL, une seule requête.
   * @param rowLimit Limite de lignes pour cette requête ; absente, celle des réglages s'applique.
   */
  function run(sql: string, rowLimit?: number): void {
    if (state.status !== 'ready') {
      return
    }
    startQuery(state)
    const request: QueryRequest = { sql, row_limit: rowLimit }
    socket.send(JSON.stringify(request))
  }

  /**
   * Ferme la session.
   */
  function close(): void {
    socket.close()
  }

  onScopeDispose(close)

  return { state, run, close }
}
