/**
 * Éditeurs SQL ouverts (onglets du haut de l'écran) et éditeur actif.
 *
 * Tout est enregistré côté serveur : le texte au fil de la frappe (avec un
 * délai), le nom, la connexion et l'ordre aussitôt changés. À l'ouverture
 * de la page, les éditeurs reviennent (chaque cellule relit ses onglets de
 * résultat enregistrés) ; l'éditeur actif est retenu dans le navigateur. Il y a
 * toujours au moins un éditeur ouvert.
 */

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import {
  closeEditor,
  createEditor,
  listClosedEditors,
  listEditors,
  reopenEditor,
  reorderEditors,
  updateEditor,
} from '@/api/editors'
import type { ClosedEditor } from '@/api/generated/ClosedEditor'
import type { Editor } from '@/api/generated/Editor'

/**
 * Éditeur ouvert dans l'interface.
 */
export interface EditorEntry {
  /**
   * Identifiant de l'éditeur.
   */
  id: number
  /**
   * Nom donné par l'utilisateur, chaîne vide pour le nom par défaut.
   */
  name: string
  /**
   * Connexion de l'éditeur, `null` tant qu'aucune n'est choisie.
   */
  connectionId: number | null
  /**
   * Texte SQL.
   */
  sql: string
}

/**
 * Délai entre la dernière frappe et l'enregistrement du texte, en millisecondes.
 */
const SAVE_DELAY = 500

/**
 * Clef de l'éditeur actif dans le stockage local.
 */
const ACTIVE_KEY = 'prisme.activeEditor'

/**
 * Éditeur de l'interface à partir de celui du serveur.
 *
 * @param editor Éditeur reçu du serveur.
 */
function entryOf(editor: Editor): EditorEntry {
  return {
    id: editor.id,
    name: editor.name,
    connectionId: editor.connection_id,
    sql: editor.sql,
  }
}

/**
 * Éditeur actif retenu, `null` si aucun n'est lisible.
 */
function storedActive(): number | null {
  try {
    const id = Number(localStorage.getItem(ACTIVE_KEY))
    return Number.isInteger(id) && id > 0 ? id : null
  } catch {
    return null
  }
}

export const useEditorsStore = defineStore('editors', () => {
  /**
   * Éditeurs ouverts, dans l'ordre des onglets.
   */
  const editors = ref<EditorEntry[]>([])

  /**
   * Identifiant de l'éditeur actif, `null` avant le chargement.
   */
  const activeId = ref<number | null>(null)

  /**
   * Éditeurs fermés depuis moins de 24 heures, rouvrables.
   */
  const closed = ref<ClosedEditor[]>([])

  /**
   * Enregistrements du texte en attente, par éditeur.
   */
  const timers = new Map<number, ReturnType<typeof setTimeout>>()

  /**
   * Éditeur actif, `null` avant le chargement.
   */
  const active = computed(
    () => editors.value.find((editor) => editor.id === activeId.value) ?? null,
  )

  /**
   * Rend un éditeur actif, et le retient.
   *
   * @param id Identifiant de l'éditeur.
   */
  function activate(id: number): void {
    activeId.value = id
    try {
      localStorage.setItem(ACTIVE_KEY, String(id))
    } catch {
      // Préférence d'affichage seulement.
    }
  }

  /**
   * Charge les éditeurs ouverts ; sans aucun, en crée un.
   */
  async function load(): Promise<void> {
    editors.value = (await listEditors()).map(entryOf)
    if (editors.value.length === 0) {
      await create(null)
      return
    }
    const remembered = storedActive()
    const first = editors.value[0]!.id
    activate(editors.value.some((editor) => editor.id === remembered) ? remembered! : first)
  }

  /**
   * Crée un éditeur à la suite des autres et le rend actif.
   *
   * @param connectionId Connexion de départ, `null` pour aucune.
   */
  async function create(connectionId: number | null): Promise<void> {
    const editor = await createEditor(connectionId)
    editors.value = [...editors.value, entryOf(editor)]
    activate(editor.id)
  }

  /**
   * Enregistre le nom, le texte et la connexion d'un éditeur, aussitôt.
   *
   * @param id Identifiant de l'éditeur.
   */
  async function save(id: number): Promise<void> {
    clearTimeout(timers.get(id))
    timers.delete(id)
    const editor = editors.value.find((item) => item.id === id)
    if (editor) {
      await updateEditor(id, {
        name: editor.name,
        sql: editor.sql,
        connection_id: editor.connectionId,
      })
    }
  }

  /**
   * Change le texte d'un éditeur ; il est enregistré peu après la dernière frappe.
   *
   * @param id Identifiant de l'éditeur.
   * @param sql Nouveau texte.
   */
  function setSql(id: number, sql: string): void {
    const editor = editors.value.find((item) => item.id === id)
    if (!editor || editor.sql === sql) {
      return
    }
    editor.sql = sql
    clearTimeout(timers.get(id))
    timers.set(
      id,
      setTimeout(() => void save(id), SAVE_DELAY),
    )
  }

  /**
   * Renomme un éditeur ; chaîne vide pour le nom par défaut.
   *
   * @param id Identifiant de l'éditeur.
   * @param name Nouveau nom.
   */
  async function rename(id: number, name: string): Promise<void> {
    const editor = editors.value.find((item) => item.id === id)
    if (editor) {
      editor.name = name
      await save(id)
    }
  }

  /**
   * Change la connexion d'un éditeur.
   *
   * @param id Identifiant de l'éditeur.
   * @param connectionId Connexion, `null` pour aucune.
   */
  async function setConnection(id: number, connectionId: number | null): Promise<void> {
    const editor = editors.value.find((item) => item.id === id)
    if (editor && editor.connectionId !== connectionId) {
      editor.connectionId = connectionId
      await save(id)
    }
  }

  /**
   * Ferme un éditeur (rouvrable 24 heures) ; son voisin devient actif. Le
   * dernier éditeur fermé est remplacé par un éditeur vide.
   *
   * @param id Identifiant de l'éditeur.
   */
  async function close(id: number): Promise<void> {
    if (timers.has(id)) {
      await save(id)
    }
    await closeEditor(id)
    const index = editors.value.findIndex((editor) => editor.id === id)
    editors.value = editors.value.filter((editor) => editor.id !== id)
    if (editors.value.length === 0) {
      await create(null)
    } else if (activeId.value === id) {
      activate((editors.value[index] ?? editors.value[index - 1])!.id)
    }
  }

  /**
   * Range les éditeurs dans l'ordre donné.
   *
   * @param ids Identifiants, du premier au dernier.
   */
  async function reorder(ids: number[]): Promise<void> {
    editors.value = ids
      .map((id) => editors.value.find((editor) => editor.id === id))
      .filter((editor) => editor !== undefined)
    await reorderEditors(ids)
  }

  /**
   * Relit la liste des éditeurs fermés depuis moins de 24 heures.
   */
  async function loadClosed(): Promise<void> {
    closed.value = await listClosedEditors()
  }

  /**
   * Rouvre un éditeur fermé et le rend actif ; sa cellule relira ses onglets de résultat.
   *
   * @param id Identifiant de l'éditeur.
   */
  async function reopen(id: number): Promise<void> {
    await reopenEditor(id)
    const reopened = (await listEditors()).find((editor) => editor.id === id)
    if (reopened) {
      editors.value = [...editors.value, entryOf(reopened)]
      activate(id)
    }
    closed.value = closed.value.filter((editor) => editor.id !== id)
  }

  return {
    editors,
    activeId,
    active,
    closed,
    activate,
    load,
    create,
    setSql,
    rename,
    setConnection,
    close,
    reorder,
    loadClosed,
    reopen,
  }
})
