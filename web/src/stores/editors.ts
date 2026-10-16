/**
 * Open SQL editors (tabs at the top of the screen) and active editor.
 *
 * Everything is saved on the server side: the text as it is typed (with a
 * delay), the name, the connection and the order as soon as they change. When
 * the page opens, the editors come back (each cell reads its saved result
 * tabs again); the active editor is remembered in the browser. There is
 * always at least one open editor.
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
 * Editor open in the interface.
 */
export interface EditorEntry {
  /**
   * Id of the editor.
   */
  id: number
  /**
   * Name given by the user, empty string for the default name.
   */
  name: string
  /**
   * Connection of the editor, `null` as long as none is chosen.
   */
  connectionId: number | null
  /**
   * SQL text.
   */
  sql: string
}

/**
 * Delay between the last keystroke and saving the text, in milliseconds.
 */
const SAVE_DELAY = 500

/**
 * Key of the active editor in local storage.
 */
const ACTIVE_KEY = 'prisme.activeEditor'

/**
 * Interface editor built from the server's.
 *
 * @param editor Editor received from the server.
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
 * Remembered active editor, `null` if none is readable.
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
   * Open editors, in the order of the tabs.
   */
  const editors = ref<EditorEntry[]>([])

  /**
   * Id of the active editor, `null` before loading.
   */
  const activeId = ref<number | null>(null)

  /**
   * Editors closed less than 24 hours ago, which can be reopened.
   */
  const closed = ref<ClosedEditor[]>([])

  /**
   * Pending text saves, by editor.
   */
  const timers = new Map<number, ReturnType<typeof setTimeout>>()

  /**
   * Active editor, `null` before loading.
   */
  const active = computed(
    () => editors.value.find((editor) => editor.id === activeId.value) ?? null,
  )

  /**
   * Makes an editor active, and remembers it.
   *
   * @param id Id of the editor.
   */
  function activate(id: number): void {
    activeId.value = id
    try {
      localStorage.setItem(ACTIVE_KEY, String(id))
    } catch {
      // Display preference only.
    }
  }

  /**
   * Loads the open editors; with none, creates one.
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
   * Creates an editor after the others and makes it active.
   *
   * @param connectionId Initial connection, `null` for none.
   */
  async function create(connectionId: number | null): Promise<void> {
    const editor = await createEditor(connectionId)
    editors.value = [...editors.value, entryOf(editor)]
    activate(editor.id)
  }

  /**
   * Saves the name, the text and the connection of an editor, immediately.
   *
   * @param id Id of the editor.
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
   * Changes the text of an editor; it is saved shortly after the last keystroke.
   *
   * @param id Id of the editor.
   * @param sql New text.
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
   * Renames an editor; empty string for the default name.
   *
   * @param id Id of the editor.
   * @param name New name.
   */
  async function rename(id: number, name: string): Promise<void> {
    const editor = editors.value.find((item) => item.id === id)
    if (editor) {
      editor.name = name
      await save(id)
    }
  }

  /**
   * Changes the connection of an editor.
   *
   * @param id Id of the editor.
   * @param connectionId Connection, `null` for none.
   */
  async function setConnection(id: number, connectionId: number | null): Promise<void> {
    const editor = editors.value.find((item) => item.id === id)
    if (editor && editor.connectionId !== connectionId) {
      editor.connectionId = connectionId
      await save(id)
    }
  }

  /**
   * Closes an editor (can be reopened for 24 hours); its neighbour becomes
   * active. The last editor closed is replaced by an empty editor.
   *
   * @param id Id of the editor.
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
   * Puts the editors in the given order.
   *
   * @param ids Ids, from the first to the last.
   */
  async function reorder(ids: number[]): Promise<void> {
    editors.value = ids
      .map((id) => editors.value.find((editor) => editor.id === id))
      .filter((editor) => editor !== undefined)
    await reorderEditors(ids)
  }

  /**
   * Reads again the list of the editors closed less than 24 hours ago.
   */
  async function loadClosed(): Promise<void> {
    closed.value = await listClosedEditors()
  }

  /**
   * Reopens a closed editor and makes it active; its cell will read its result tabs again.
   *
   * @param id Id of the editor.
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
