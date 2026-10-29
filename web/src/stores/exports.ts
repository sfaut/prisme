/**
 * Exports of full results (`api/exports.ts`): their list, read from the
 * server then kept up to date by its events, the use of the exports folder,
 * and the messages of their end.
 *
 * The list holds every export of the user, the most recent first: the
 * running ones with their progress, the ended ones until the user removes
 * them, which deletes their file; nothing is deleted automatically. The
 * room their files take (`usage`) adds up their sizes, those of the running
 * exports as they grow. The files of the folder that no export references
 * (`orphans`) are read with the list.
 *
 * An export that ends while the interface follows it gets a message: ready,
 * with its "Télécharger" action; failed, with its cause. A stopped export
 * gets none: the user stopped it.
 *
 * The events received while the list is being read are applied again once
 * it is read: the list, read before them, would otherwise bring back an
 * export as it was (running, or removed meanwhile).
 */

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import { errorMessage } from '@/api/errorMessage'
import {
  cancelExport,
  exportFileUrl,
  followExports,
  listExports,
  removeExport,
  removeOrphans,
  startExport,
} from '@/api/exports'
import type { ExportEvent } from '@/api/generated/ExportEvent'
import type { ExportInfo } from '@/api/generated/ExportInfo'
import type { ExportRequest } from '@/api/generated/ExportRequest'
import type { FolderUsage } from '@/api/generated/FolderUsage'
import { describe } from '@/api/messages'
import { formatRowCount } from '@/api/querySession'
import { saveUrl } from '@/components/download'
import { notify } from '@/components/ui/toasts'

/**
 * Most rows Excel opens: 1,048,576 rows, the header included.
 */
export const EXCEL_MAX_ROWS = 1_048_575

/**
 * Files no export references, before the list is read.
 */
const NO_FILES: FolderUsage = { count: 0, bytes: 0 }

export const useExportsStore = defineStore('exports', () => {
  /**
   * Exports of the user, the most recent first.
   */
  const exports = ref<ExportInfo[]>([])

  /**
   * Files of these exports in the folder: those that are not missing.
   */
  const usage = computed<FolderUsage>(() => {
    const files = exports.value.filter((entry) => !entry.file_missing)
    return { count: files.length, bytes: files.reduce((sum, entry) => sum + entry.bytes, 0) }
  })

  /**
   * Files of Prisme in the folder that no export references.
   */
  const orphans = ref<FolderUsage>(NO_FILES)

  /**
   * Failure of the last reading of the list, `null` if it succeeded.
   */
  const error = ref<string | null>(null)

  /**
   * Number of running exports.
   */
  const runningCount = computed(
    () => exports.value.filter((entry) => entry.state === 'running').length,
  )

  /**
   * Events received during each reading of the list in progress.
   */
  const readings = new Set<ExportEvent[]>()

  /**
   * Reads the list and the files no export references again.
   */
  async function reload(): Promise<void> {
    const received: ExportEvent[] = []
    readings.add(received)
    try {
      const list = await listExports()
      exports.value = list.exports
      orphans.value = list.orphans
      error.value = null
      for (const event of received) {
        apply(event)
      }
    } catch (failure) {
      error.value = errorMessage(failure)
    } finally {
      readings.delete(received)
    }
  }

  /**
   * Applies an event to the list. An export never goes back to running: a
   * late answer of its start does not undo its end.
   *
   * @param event Event of an export.
   * @returns The export as it was, `null` if it was not listed.
   */
  function apply(event: ExportEvent): ExportInfo | null {
    const id = event.type === 'changed' ? event.export.id : event.id
    const index = exports.value.findIndex((entry) => entry.id === id)
    const known = exports.value[index] ?? null
    if (event.type === 'removed') {
      if (known !== null) {
        exports.value.splice(index, 1)
      }
    } else if (known === null) {
      exports.value.unshift(event.export)
    } else if (known.state === 'running' || event.export.state !== 'running') {
      exports.value[index] = event.export
    }
    return known
  }

  /**
   * Receives an event: applies it, and announces an export that ends.
   *
   * @param event Event of an export.
   */
  function receive(event: ExportEvent): void {
    for (const received of readings) {
      received.push(event)
    }
    const known = apply(event)
    if (event.type === 'changed' && (known === null || known.state === 'running')) {
      announce(event.export)
    }
  }

  /**
   * Message of an export that has just ended: ready, with its download, or
   * failed, with its cause. Nothing for another state.
   *
   * @param entry The export.
   */
  function announce(entry: ExportInfo): void {
    if (entry.state === 'ready') {
      notify(`Export prêt : ${entry.name}, ${formatRowCount(entry.rows, null)}.`, 'success', {
        label: 'Télécharger',
        alternative: 'Télécharger depuis le gestionnaire d’exports, en haut de la page',
        run: () => download(entry.id),
      })
    } else if (entry.state === 'failed') {
      const cause = entry.error === null ? '' : `\n${describe(entry.error)}`
      notify(`Export échoué : ${entry.name}.${cause}`, 'error')
    }
  }

  /**
   * Stops following the events, `null` when not following.
   */
  let stop: (() => void) | null = null

  /**
   * Starts following the exports: reads the list, then follows its
   * changes. No effect if already following.
   */
  function start(): void {
    if (stop !== null) {
      return
    }
    stop = followExports({ onEvent: receive, onGap: () => void reload() })
    void reload()
  }

  /**
   * Stops following the exports.
   */
  function dispose(): void {
    stop?.()
    stop = null
  }

  /**
   * Starts an export.
   *
   * @param requested Query, columns and name of the file.
   * @throws {ApiError} If the server refuses it (a write, an unknown
   *     connection, a connection that fails).
   */
  async function begin(requested: ExportRequest): Promise<void> {
    receive({ type: 'changed', export: await startExport(requested) })
  }

  /**
   * Runs an action on the exports; its failure becomes an error message.
   *
   * @param action Request to the server.
   */
  async function attempt(action: () => Promise<void>): Promise<void> {
    try {
      await action()
    } catch (failure) {
      notify(errorMessage(failure), 'error')
    }
  }

  /**
   * Stops a running export; its partial file stays.
   *
   * @param id Id of the export.
   */
  function cancel(id: number): Promise<void> {
    return attempt(() => cancelExport(id))
  }

  /**
   * Removes an export and deletes its file.
   *
   * @param id Id of the export.
   */
  function remove(id: number): Promise<void> {
    return attempt(async () => {
      await removeExport(id)
      receive({ type: 'removed', id })
    })
  }

  /**
   * Deletes the files of the folder that no export references.
   */
  function clearOrphans(): Promise<void> {
    return attempt(async () => {
      await removeOrphans()
      await reload()
    })
  }

  /**
   * Downloads the file of an export, under its name.
   *
   * @param id Id of the export.
   */
  function download(id: number): void {
    saveUrl(exportFileUrl(id))
  }

  return {
    exports,
    usage,
    orphans,
    error,
    runningCount,
    start,
    dispose,
    reload,
    begin,
    cancel,
    remove,
    clearOrphans,
    download,
  }
})
