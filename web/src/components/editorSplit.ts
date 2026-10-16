/**
 * Split of the SQL cell's height between the editor, at the top, and the
 * results, at the bottom: share of the height given to the editor, set with
 * the mouse and remembered from one visit to the next.
 *
 * The share is kept in the browser's local storage: it is a display
 * preference specific to this machine. If storage is unavailable (private
 * browsing, blocked storage), the default share applies.
 */

/**
 * Default share: the top third for the editor.
 */
export const DEFAULT_EDITOR_SPLIT = 1 / 3

/**
 * Minimum share of the editor: a few lines stay visible.
 */
export const MIN_EDITOR_SPLIT = 0.1

/**
 * Maximum share of the editor: the results keep their room.
 */
export const MAX_EDITOR_SPLIT = 0.8

/**
 * Key of the share in local storage.
 */
const STORAGE_KEY = 'prisme.editorSplit'

/**
 * Brings a share back between the minimum and the maximum.
 *
 * @param split Wanted share, from 0 to 1.
 */
export function clampEditorSplit(split: number): number {
  return Math.min(MAX_EDITOR_SPLIT, Math.max(MIN_EDITOR_SPLIT, split))
}

/**
 * Remembered share, or the default share if none is readable.
 */
export function loadEditorSplit(): number {
  try {
    const split = Number(window.localStorage.getItem(STORAGE_KEY) ?? Number.NaN)
    return Number.isFinite(split) && split > 0 ? clampEditorSplit(split) : DEFAULT_EDITOR_SPLIT
  } catch {
    return DEFAULT_EDITOR_SPLIT
  }
}

/**
 * Remembers the share; no effect if storage is unavailable.
 *
 * @param split Share, from 0 to 1.
 */
export function saveEditorSplit(split: number): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, String(split))
  } catch {
    // Display preference only: losing it harms nothing.
  }
}
