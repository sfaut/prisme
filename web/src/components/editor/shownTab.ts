/**
 * Choice of the result tab shown by the view of an editor (`EditorView`), as
 * its tabs change: the first tab of a new batch; the tab saved with the
 * display of the editor, when the saved tabs are read with it; the first
 * remaining tab when the one shown disappears.
 */

/**
 * Tab to show once the tabs changed.
 *
 * @param tabs Ids of the tabs, in order.
 * @param previous Ids of the tabs before the change; `undefined` at first.
 * @param shown Id of the tab shown before the change, or saved with the
 *     editor; `0` for none.
 */
export function nextShownTab(
  tabs: readonly number[],
  previous: readonly number[] | undefined,
  shown: number,
): number {
  const fresh = tabs.find((id) => !previous?.includes(id))
  // The saved tabs read with the editor keep the one it showed.
  const restored = previous?.length === 0 && tabs.includes(shown)
  if (fresh !== undefined && !restored) {
    return fresh
  }
  return tabs.includes(shown) ? shown : (tabs[0] ?? 0)
}
