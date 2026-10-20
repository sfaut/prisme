/**
 * Layout of the screen: which panels are open. The object explorer, on the
 * left, is open by default; a closed panel leaves a rail on its edge, which
 * opens it again (`PanelRail.vue`). The data explorer, on the right, has its
 * own store (`dataExplorer.ts`): it is closed when the application starts.
 *
 * The state is a display preference specific to this machine, kept in the
 * browser's local storage; when storage is unavailable, the defaults apply.
 */

import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

/**
 * Key of the object explorer's state in local storage.
 */
const OBJECTS_OPEN_KEY = 'prisme.objectsOpen'

/**
 * Reads a remembered flag.
 *
 * @param key Key in local storage.
 * @param fallback Value when nothing readable is remembered.
 */
export function loadFlag(key: string, fallback: boolean): boolean {
  try {
    const value = window.localStorage.getItem(key)
    return value === null ? fallback : value === 'true'
  } catch {
    return fallback
  }
}

/**
 * Remembers a flag; no effect if storage is unavailable.
 *
 * @param key Key in local storage.
 * @param value Value to remember.
 */
export function saveFlag(key: string, value: boolean): void {
  try {
    window.localStorage.setItem(key, String(value))
  } catch {
    // Display preference only: losing it harms nothing.
  }
}

export const useLayoutStore = defineStore('layout', () => {
  /**
   * Is the object explorer open?
   */
  const objectsOpen = ref(loadFlag(OBJECTS_OPEN_KEY, true))

  watch(objectsOpen, (open) => saveFlag(OBJECTS_OPEN_KEY, open))

  return { objectsOpen }
})
