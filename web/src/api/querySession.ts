/**
 * Session de requêtes sur une connexion enregistrée, par WebSocket.
 *
 * Le client envoie (`run`) le texte complet de l'éditeur, avec la position du curseur
 * pour n'exécuter que la requête qui s'y trouve (Ctrl-Entrée), ou sans curseur
 * pour toutes les exécuter en parallèle (Ctrl-Maj-Entrée). Le serveur annonce
 * les requêtes du lot (`batch`), puis envoie leurs résultats en streaming,
 * chaque message portant le numéro de sa requête : `started` quand elle part
 * (en parallèle, elle peut attendre son tour), `columns`, des lots
 * `rows`, puis `done` (ou `error`). `batch_done` clôt le lot. `cancel` arrête
 * toutes les requêtes en cours. Le protocole est
 * décrit dans `crates/server/src/query_session.rs`.
 *
 * `applyMessage` met l'état à jour message par message ; elle ne dépend ni de
 * Vue ni du WebSocket, ce qui la rend simple à tester. `useQuerySession`
 * l'utilise pour exposer un état réactif aux composants.
 */

import { onScopeDispose, reactive, shallowReactive } from 'vue'

import type { LimitReport } from './generated/LimitReport'
import type { ClientMessage } from './generated/ClientMessage'
import type { ServerInfo } from './generated/ServerInfo'
import type { ServerMessage } from './generated/ServerMessage'

/**
 * Délai de regroupement des messages reçus, en millisecondes : l'interface
 * est redessinée au plus une fois par délai, quel que soit le nombre de lots
 * de lignes reçus, ce qui la garde réactive pendant l'exécution.
 */
const FLUSH_DELAY = 100

/**
 * Cellule de résultat : du texte, ou `null` pour NULL.
 */
export type Cell = string | null

/**
 * Étape de la session.
 *
 * - `connecting` : ouverture du WebSocket et connexion au serveur cible ;
 * - `ready` : prête à exécuter des requêtes ;
 * - `running` : un lot de requêtes est en cours ;
 * - `closed` : WebSocket fermé, la session doit être rouverte.
 */
export type SessionStatus = 'connecting' | 'ready' | 'running' | 'closed'

/**
 * Résultat d'une requête, complété au fil des messages.
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
 * Requête d'un lot, affichée dans son propre onglet.
 */
export interface QueryTab {
  /**
   * Numéro de la requête dans le lot.
   */
  query: number
  /**
   * Texte de la requête, tel que découpé dans l'éditeur.
   */
  sql: string
  /**
   * Résultat, `null` tant que les colonnes ne sont pas reçues.
   */
  result: QueryResult | null
  /**
   * Erreur de la requête.
   */
  error: string | null
  /**
   * La requête est-elle terminée (avec succès ou en erreur) ?
   */
  done: boolean
  /**
   * Instant du départ de la requête (`Date.now()`), `null` tant qu'elle attend son tour.
   */
  startedAt: number | null
  /**
   * Instant de la fin de la requête, `null` tant qu'elle n'est pas terminée.
   */
  endedAt: number | null
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
   * Base en cours sur le serveur cible (elle change avec `USE`), `null` si aucune.
   */
  database: string | null
  /**
   * Requêtes du dernier lot, une par onglet.
   */
  tabs: QueryTab[]
  /**
   * Dernière erreur de la session elle-même (connexion, demande invalide).
   */
  error: string | null
}

/**
 * Crée l'état d'une session qui s'ouvre.
 */
export function initialState(): SessionState {
  return { status: 'connecting', server: null, database: null, tabs: [], error: null }
}

/**
 * Met à jour l'état avec un message du serveur.
 *
 * Modifie `state` sur place : les lignes s'ajoutent au tableau existant, sans
 * recopier un résultat qui peut compter des milliers de lignes.
 *
 * @param state État de la session.
 * @param message Message reçu du serveur.
 * @param now Instant de réception, pour mesurer la durée des requêtes.
 */
export function applyMessage(
  state: SessionState,
  message: ServerMessage,
  now: number = Date.now(),
): void {
  switch (message.type) {
    case 'ready':
      state.status = 'ready'
      state.server = message.server
      state.database = message.database
      break
    case 'batch':
      state.tabs = message.queries.map((query) => ({
        query: query.query,
        sql: query.sql,
        result: null,
        error: null,
        done: false,
        startedAt: null,
        endedAt: null,
      }))
      break
    case 'started': {
      const tab = tabOf(state, message.query)
      if (tab) {
        tab.startedAt = now
      }
      break
    }
    case 'columns': {
      const tab = tabOf(state, message.query)
      if (tab) {
        tab.result = {
          sql: message.sql,
          limit: message.limit,
          columns: message.columns,
          rows: [],
          truncated: false,
          affectedRows: 0,
          elapsedMs: 0,
        }
      }
      break
    }
    case 'rows':
      tabOf(state, message.query)?.result?.rows.push(...message.rows)
      break
    case 'done': {
      const tab = tabOf(state, message.query)
      if (tab) {
        tab.done = true
        tab.endedAt = now
        if (tab.result) {
          tab.result.truncated = message.truncated
          tab.result.affectedRows = message.affected_rows
          tab.result.elapsedMs = message.elapsed_ms
        }
      }
      state.database = message.database
      break
    }
    case 'error': {
      if (message.query !== null) {
        const tab = tabOf(state, message.query)
        if (tab) {
          tab.error = message.message
          tab.done = true
          tab.endedAt = now
        }
        break
      }
      state.error = message.message
      // Avant `ready`, l'erreur est celle de la connexion au serveur cible :
      // le serveur ferme alors la session.
      state.status = state.status === 'connecting' ? 'closed' : 'ready'
      break
    }
    case 'batch_done':
      state.status = 'ready'
      break
  }
}

/**
 * Onglet de la requête numéro `query` du lot en cours.
 *
 * @param state État de la session.
 * @param query Numéro de la requête.
 */
function tabOf(state: SessionState, query: number): QueryTab | undefined {
  return state.tabs.find((tab) => tab.query === query)
}

/**
 * Avancement d'une requête, affiché par la pastille de son onglet.
 *
 * - `waiting` : en attente de son tour (exécution parallèle limitée) ;
 * - `executing` : partie, aucun résultat reçu ;
 * - `loading` : colonnes reçues, lignes en cours de réception ;
 * - `done` : résultat complet ;
 * - `error` : échec.
 */
export type TabProgress = 'waiting' | 'executing' | 'loading' | 'done' | 'error'

/**
 * Avancement de la requête d'un onglet.
 *
 * @param tab Onglet de la requête.
 */
export function tabProgress(tab: QueryTab): TabProgress {
  if (tab.error !== null) {
    return 'error'
  }
  if (tab.done) {
    return 'done'
  }
  if (tab.startedAt === null) {
    return 'waiting'
  }
  return tab.result === null ? 'executing' : 'loading'
}

/**
 * Libellé de chaque avancement.
 */
const PROGRESS_LABELS: Record<TabProgress, string> = {
  waiting: 'En attente',
  executing: 'Exécution',
  loading: 'Chargement des lignes',
  done: 'Terminé',
  error: 'Échec',
}

/**
 * Résumé d'une requête pour l'info-bulle de sa pastille : statut, durée,
 * nombre de lignes. Pendant l'exécution, la durée court jusqu'à `now`.
 *
 * @param tab Onglet de la requête.
 * @param now Instant présent (`Date.now()`).
 * @returns Trois lignes de texte.
 */
export function progressSummary(tab: QueryTab, now: number): string[] {
  const progress = tabProgress(tab)
  let duration = '—'
  if (progress === 'done' && tab.result) {
    duration = formatDuration(tab.result.elapsedMs)
  } else if (tab.startedAt !== null) {
    duration = formatDuration((tab.endedAt ?? now) - tab.startedAt)
  }
  let rows = '—'
  if (tab.result) {
    const count = tab.result.rows.length
    rows = `${count.toLocaleString('fr-FR')} ${count > 1 ? 'lignes' : 'ligne'}`
    if (tab.result.truncated) {
      rows += ' (tronqué)'
    }
  }
  return [PROGRESS_LABELS[progress], duration, rows]
}

/**
 * Formate une durée : en millisecondes sous la seconde, sinon en secondes avec une décimale.
 *
 * @param milliseconds Durée.
 */
export function formatDuration(milliseconds: number): string {
  if (milliseconds < 1000) {
    return `${Math.max(0, Math.round(milliseconds))} ms`
  }
  return `${(milliseconds / 1000).toLocaleString('fr-FR', { maximumFractionDigits: 1, minimumFractionDigits: 1 })} s`
}

/**
 * Note le début d'un lot dans l'état : les onglets précédents disparaissent.
 *
 * @param state État de la session.
 */
export function startBatch(state: SessionState): void {
  state.status = 'running'
  state.tabs = []
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
 * @returns L'état réactif de la session, et les fonctions pour exécuter des requêtes et fermer la session.
 */
export function useQuerySession(connectionId: number) {
  const state = reactive(initialState())
  const socket = new WebSocket(sessionUrl(connectionId, window.location))

  /**
   * Messages reçus, pas encore appliqués, avec leur instant de réception.
   */
  let pending: [ServerMessage, number][] = []

  /**
   * Application différée des messages en attente, absente si rien n'attend.
   */
  let flushTimer: ReturnType<typeof setTimeout> | undefined

  /**
   * Applique d'un coup tous les messages en attente : l'interface n'est
   * redessinée qu'une fois pour tous les lots de lignes reçus entre-temps.
   */
  function flush(): void {
    flushTimer = undefined
    const messages = pending
    pending = []
    for (const [message, receivedAt] of messages) {
      applyMessage(state, message, receivedAt)
      if (message.type === 'columns') {
        const result = tabOf(state, message.query)?.result
        if (result) {
          // Réactivité superficielle : l'arrivée d'un lot rafraîchit l'affichage,
          // sans rendre réactive chacune des milliers de lignes.
          result.rows = shallowReactive([])
        }
      }
    }
  }

  socket.addEventListener('message', (event: MessageEvent<string>) => {
    pending.push([JSON.parse(event.data) as ServerMessage, Date.now()])
    flushTimer ??= setTimeout(flush, FLUSH_DELAY)
  })
  socket.addEventListener('close', () => {
    clearTimeout(flushTimer)
    flush()
    state.status = 'closed'
  })

  /**
   * Envoie le texte de l'éditeur, si la session est prête.
   *
   * @param sql Texte complet de l'éditeur.
   * @param cursor
   *     Position du curseur (unités UTF-16) : seule la requête qui s'y trouve
   *     s'exécute. Absente : toutes, en parallèle.
   * @param rowLimit Limite de lignes ; absente, celle des réglages s'applique.
   */
  function run(sql: string, cursor?: number, rowLimit?: number): void {
    if (state.status !== 'ready') {
      return
    }
    startBatch(state)
    send({ type: 'run', sql, cursor, row_limit: rowLimit })
  }

  /**
   * Arrête toutes les requêtes en cours : elles se terminent en erreur
   * « requête interrompue », et celles qui attendaient leur tour ne partent pas.
   */
  function cancel(): void {
    if (state.status === 'running') {
      send({ type: 'cancel' })
    }
  }

  /**
   * Envoie un message au serveur.
   *
   * @param message Message du client.
   */
  function send(message: ClientMessage): void {
    socket.send(JSON.stringify(message))
  }

  /**
   * Ferme la session.
   */
  function close(): void {
    socket.close()
  }

  onScopeDispose(close)

  return { state, run, cancel, close }
}
