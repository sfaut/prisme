/**
 * Layout of an editor's cell (`QueryCell`): the heights of its editor and of
 * its results, and which one is folded. The splitter of Reka UI remembers it
 * in the browser's local storage (`autoSaveId`), one entry per editor, so
 * that folding the editor of one tab leaves the others as they are. Closing
 * an editor forgets its entry: a reopened editor starts from the default
 * layout.
 */

/**
 * Name of an editor's layout, the `autoSaveId` of its cell's splitter.
 *
 * @param editorId Id of the editor.
 */
export function cellLayoutId(editorId: number): string {
  return `prisme.cellLayout.${editorId}`
}

/**
 * Forgets the layout of a closed editor. Reka UI keeps it under its
 * `autoSaveId`, prefixed by `reka:`. If storage is unavailable (private
 * browsing, blocked storage), there is nothing to forget.
 *
 * @param editorId Id of the editor.
 */
export function forgetCellLayout(editorId: number): void {
  try {
    window.localStorage.removeItem(`reka:${cellLayoutId(editorId)}`)
  } catch {
    // Storage unavailable: nothing was saved.
  }
}
