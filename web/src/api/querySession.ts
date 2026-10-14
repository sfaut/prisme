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
 * toutes les requêtes en cours, `cancel_query` une seule. Le protocole est
 * décrit dans `crates/server/src/query_session.rs`.
 *
 * Chaque requête a son onglet. Un nouveau lot remplace les onglets du
 * précédent, sauf ceux que l'utilisateur a épinglés ; un onglet peut aussi
 * être renommé ou fermé (sa requête est alors arrêtée si elle tourne).
 *
 * `applyMessage` met l'état à jour message par message ; elle ne dépend ni de
 * Vue ni du WebSocket, ce qui la rend simple à tester. `useQuerySession`
 * l'utilise pour exposer un état réactif aux composants.
 */

import { markRaw, onScopeDispose, reactive } from 'vue'

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
   * Nombre de lignes au plus, annoncé par le serveur (sa limite, ou celle
   * écrite par l'utilisateur) ; `null` si inconnu.
   */
  expectedRows: number | null
  /**
   * Temps d'exécution mesuré par le serveur, en microsecondes.
   */
  executionUs: number
  /**
   * Noms des colonnes.
   */
  columns: string[]
  /**
   * Pour chaque colonne, ses valeurs s'alignent-elles à droite (nombres,
   * dates et heures) ?
   */
  rightAligned: boolean[]
  /**
   * Lignes reçues jusqu'ici. Dans l'interface, ce tableau n'est pas réactif
   * (ajouter des milliers de lignes une à une coûterait cher) : c'est
   * `rowCount` qui signale l'arrivée de nouvelles lignes.
   */
  rows: Cell[][]
  /**
   * Nombre de lignes reçues : la valeur réactive à suivre pour redessiner.
   */
  rowCount: number
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
   * Identifiant de l'onglet, unique dans la session (deux lots ont chacun
   * leur requête numéro 0).
   */
  id: number
  /**
   * Numéro du lot de la requête : seuls les onglets du lot en cours
   * reçoivent les messages du serveur.
   */
  batch: number
  /**
   * Numéro de la requête dans son lot.
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
   * Instant du départ de la requête (`performance.now()`), `null` tant qu'elle attend son tour.
   */
  startedAt: number | null
  /**
   * Instant de réception des colonnes (`performance.now()`) : le serveur a fini
   * d'exécuter la requête et commence à envoyer les lignes. `null` avant,
   * ou pour une requête sans résultat.
   */
  columnsAt: number | null
  /**
   * Instant de la fin de la requête, `null` tant qu'elle n'est pas terminée.
   */
  endedAt: number | null
  /**
   * Volume reçu pour la requête, en octets : taille des messages de son
   * résultat (colonnes, lignes, fin) tels qu'ils passent sur le WebSocket.
   */
  bytes: number
  /**
   * Onglet épinglé : il reste quand un nouveau lot est lancé.
   */
  pinned: boolean
  /**
   * Nom donné par l'utilisateur, `null` pour le libellé par défaut.
   */
  name: string | null
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
   * Onglets : ceux épinglés des lots précédents, puis les requêtes du lot en cours.
   */
  tabs: QueryTab[]
  /**
   * Numéro du lot en cours (0 avant le premier).
   */
  batch: number
  /**
   * Identifiant du prochain onglet créé.
   */
  nextTabId: number
  /**
   * Dernière erreur de la session elle-même (connexion, demande invalide).
   */
  error: string | null
}

/**
 * Crée l'état d'une session qui s'ouvre.
 */
export function initialState(): SessionState {
  return {
    status: 'connecting',
    server: null,
    database: null,
    tabs: [],
    batch: 0,
    nextTabId: 1,
    error: null,
  }
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
 * @param bytes
 *     Taille du message en octets, comptée dans le volume reçu par sa
 *     requête (colonnes, lignes et fin d'un résultat).
 */
export function applyMessage(
  state: SessionState,
  message: ServerMessage,
  now: number = performance.now(),
  bytes: number = 0,
): void {
  if (message.type === 'columns' || message.type === 'rows' || message.type === 'done') {
    const tab = tabOf(state, message.query)
    if (tab) {
      tab.bytes += bytes
    }
  }
  switch (message.type) {
    case 'ready':
      state.status = 'ready'
      state.server = message.server
      state.database = message.database
      break
    case 'batch':
      state.tabs = [
        ...state.tabs,
        ...message.queries.map((query) => ({
          id: state.nextTabId++,
          batch: state.batch,
          query: query.query,
          sql: query.sql,
          result: null,
          error: null,
          done: false,
          startedAt: null,
          columnsAt: null,
          endedAt: null,
          bytes: 0,
          pinned: false,
          name: null,
        })),
      ]
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
        tab.columnsAt = now
        tab.result = {
          sql: message.sql,
          limit: message.limit,
          expectedRows: message.expected_rows,
          executionUs: message.execution_us,
          columns: message.columns,
          rightAligned: message.right_aligned,
          rows: [],
          rowCount: 0,
          truncated: false,
          affectedRows: 0,
          elapsedMs: 0,
        }
      }
      break
    }
    case 'rows': {
      const result = tabOf(state, message.query)?.result
      if (result) {
        result.rows.push(...message.rows)
        result.rowCount = result.rows.length
      }
      break
    }
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
    case 'database':
      state.database = message.database
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
  return state.tabs.find((tab) => tab.batch === state.batch && tab.query === query)
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
 * Temps d'exécution d'une requête, en millisecondes : celui mesuré par le
 * serveur, à la microseconde, une fois les colonnes reçues ; sinon, depuis
 * son départ (jusqu'à sa fin pour une requête sans résultat, jusqu'à `now`
 * tant qu'elle s'exécute) ; `null` tant qu'elle attend son tour.
 *
 * @param tab Onglet de la requête.
 * @param now Instant présent (`performance.now()`).
 */
export function executionDuration(tab: QueryTab, now: number): number | null {
  if (tab.result) {
    return tab.result.executionUs / 1000
  }
  if (tab.startedAt === null) {
    return null
  }
  return (tab.endedAt ?? now) - tab.startedAt
}

/**
 * Temps de transfert et d'affichage des lignes, en millisecondes : de la
 * réception des colonnes à la fin de la requête. Il court jusqu'à `now`
 * pendant la réception ; `null` sans résultat.
 *
 * @param tab Onglet de la requête.
 * @param now Instant présent (`performance.now()`).
 */
export function transferDuration(tab: QueryTab, now: number): number | null {
  return tab.columnsAt === null ? null : (tab.endedAt ?? now) - tab.columnsAt
}

/**
 * Part accomplie d'une requête, de 0 à 1, pour la barre d'avancement de son
 * onglet ; `null` quand elle est inconnue (barre indéterminée).
 *
 * - en attente : 0 ;
 * - en exécution : inconnue ;
 * - en réception : lignes reçues sur le nombre de lignes au plus annoncé
 *   par le serveur (la limite de Prisme ou celle de l'utilisateur) ;
 *   inconnue sans ce nombre ;
 * - terminée, avec succès ou en échec : 1.
 *
 * @param tab Onglet de la requête.
 */
export function tabFraction(tab: QueryTab): number | null {
  switch (tabProgress(tab)) {
    case 'waiting':
      return 0
    case 'done':
    case 'error':
      return 1
    case 'loading': {
      const expected = tab.result?.expectedRows ?? 0
      return tab.result && expected > 0 ? Math.min(1, tab.result.rowCount / expected) : null
    }
    default:
      return null
  }
}

/**
 * Taille en octets d'un texte encodé en UTF-8, tel qu'il passe sur le
 * WebSocket, sans le recopier (un message peut contenir des milliers de lignes).
 *
 * @param text Texte du message.
 */
export function utf8Length(text: string): number {
  let bytes = 0
  for (let index = 0; index < text.length; index++) {
    const code = text.charCodeAt(index)
    if (code < 0x80) {
      bytes += 1
    } else if (code < 0x800) {
      bytes += 2
    } else if (code >= 0xd800 && code < 0xdc00) {
      // Paire de substitution : un caractère hors du plan de base, 4 octets.
      bytes += 4
      index++
    } else {
      bytes += 3
    }
  }
  return bytes
}

/**
 * Formate un volume de données dans l'unité qui lui convient : octets, puis
 * ko, Mo, Go et To (par 1 000), avec une décimale au-delà de l'octet.
 *
 * @param bytes Volume en octets.
 */
export function formatBytes(bytes: number): string {
  const units = ['ko', 'Mo', 'Go', 'To']
  if (bytes < 1000) {
    return `${Math.round(Math.max(0, bytes))} o`
  }
  let value = bytes / 1000
  let unit = 0
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000
    unit++
  }
  return `${value.toLocaleString('fr-FR', { minimumFractionDigits: 1, maximumFractionDigits: 1 })} ${units[unit]}`
}

/**
 * Formate une durée dans l'unité qui lui convient : microsecondes sous la
 * milliseconde, millisecondes sous la seconde, secondes (une décimale) sous la minute, minutes et secondes sous
 * l'heure, puis heures et minutes.
 *
 * @param milliseconds Durée.
 */
export function formatDuration(milliseconds: number): string {
  const duration = Math.max(0, milliseconds)
  if (duration < 1) {
    return `${Math.round(duration * 1000)} µs`
  }
  if (duration < 1000) {
    return `${Math.round(duration)} ms`
  }
  if (duration < 60_000) {
    const seconds = Math.floor(duration / 100) / 10
    return `${seconds.toLocaleString('fr-FR', { minimumFractionDigits: 1 })} s`
  }
  const pad = (value: number) => String(value).padStart(2, '0')
  const totalSeconds = Math.floor(duration / 1000)
  if (totalSeconds < 3600) {
    return `${Math.floor(totalSeconds / 60)} min ${pad(totalSeconds % 60)} s`
  }
  return `${Math.floor(totalSeconds / 3600)} h ${pad(Math.floor(totalSeconds / 60) % 60)} min`
}

/**
 * Note le début d'un lot dans l'état : les onglets du lot précédent
 * disparaissent, sauf ceux qui sont épinglés.
 *
 * @param state État de la session.
 */
export function startBatch(state: SessionState): void {
  state.status = 'running'
  state.batch++
  state.tabs = state.tabs.filter((tab) => tab.pinned)
  state.error = null
}

/**
 * Retire un onglet de l'état.
 *
 * @param state État de la session.
 * @param id Identifiant de l'onglet.
 * @returns L'onglet retiré, absent s'il n'existait pas.
 */
export function removeTab(state: SessionState, id: number): QueryTab | undefined {
  const tab = state.tabs.find((item) => item.id === id)
  state.tabs = state.tabs.filter((item) => item.id !== id)
  return tab
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
   * Messages reçus, pas encore appliqués, avec leur instant de réception et
   * leur taille en octets.
   */
  let pending: [ServerMessage, number, number][] = []

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
    for (const [message, receivedAt, bytes] of messages) {
      applyMessage(state, message, receivedAt, bytes)
      if (message.type === 'columns') {
        const result = tabOf(state, message.query)?.result
        if (result) {
          // Lignes hors de la réactivité de Vue : seul `rowCount` signale leur arrivée.
          result.rows = markRaw([])
        }
      }
    }
  }

  socket.addEventListener('message', (event: MessageEvent<string>) => {
    const message = JSON.parse(event.data) as ServerMessage
    pending.push([message, performance.now(), utf8Length(event.data)])
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
   * Change la base en cours de la session, comme `USE`, sans onglet de
   * résultat. Pendant un lot, le changement attend sa fin. Un échec arrive
   * comme erreur de session.
   *
   * @param database Nom de la base.
   */
  function changeDatabase(database: string): void {
    if (state.status === 'ready' || state.status === 'running') {
      send({ type: 'use', database })
    }
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
   * Ferme un onglet. Si sa requête tourne encore (lot en cours), elle est
   * arrêtée ; ses derniers messages ne trouvent plus d'onglet et sont ignorés.
   *
   * @param id Identifiant de l'onglet.
   */
  function closeTab(id: number): void {
    const tab = removeTab(state, id)
    if (tab && !tab.done && tab.batch === state.batch && state.status === 'running') {
      send({ type: 'cancel_query', query: tab.query })
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

  return { state, run, changeDatabase, cancel, closeTab, close }
}
