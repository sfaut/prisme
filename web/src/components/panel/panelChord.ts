/**
 * Keyboard shortcuts of the activity bar (`PanelActivityBar.vue`): chords of
 * two keys, as in VS Code. Ctrl-K (Cmd-K under macOS) arms the chord, then
 * a single key, without modifier, chooses the action: a letter opens or
 * closes a panel, `0` resets the display of the editor, `=` shares the room
 * again. Any other key, Escape
 * included, or a delay without a key, disarms it. The second key of an
 * armed chord goes to the chord alone: it reaches neither the SQL editor nor
 * the browser.
 *
 * Ctrl with a digit changes the browser's tab, and AltGr (Ctrl+Alt under
 * Windows) types characters on a French keyboard: the chord keeps clear of
 * both. CodeMirror binds Ctrl-Shift-K (delete the line), not Ctrl-K.
 */

/**
 * Delay after Ctrl-K beyond which the chord is disarmed, in milliseconds.
 */
export const CHORD_DELAY_MS = 2000

/**
 * Does a key event arm the chord? Ctrl-K, or Cmd-K, without other modifier.
 *
 * @param event Key event.
 */
export function armsChord(event: KeyboardEvent): boolean {
  return (
    (event.ctrlKey || event.metaKey) &&
    !event.altKey &&
    !event.shiftKey &&
    event.key.toLowerCase() === 'k'
  )
}

/**
 * Second key of a chord, lowercased, `null` for a key with a modifier (Ctrl,
 * Cmd, Alt) or a modifier alone. Shift is allowed: `=` takes it on some
 * keyboards.
 *
 * @param event Key event.
 */
export function chordKey(event: KeyboardEvent): string | null {
  if (event.ctrlKey || event.metaKey || event.altKey) {
    return null
  }
  if (['Shift', 'Control', 'Meta', 'Alt', 'AltGraph'].includes(event.key)) {
    return null
  }
  return event.key.toLowerCase()
}

/**
 * Text of a shortcut in a tooltip: "Ctrl-K puis N".
 *
 * @param key Second key of the chord.
 */
export function chordLabel(key: string): string {
  return `Ctrl-K puis ${key.toUpperCase()}`
}

/**
 * Follows the chords on the window, in the capture phase, ahead of the SQL
 * editor: `run` receives the second key of each chord, and says whether it
 * is one of its actions.
 *
 * @param run Action of a second key; `true` if the key is one.
 * @returns The function that stops following them.
 */
export function followChords(run: (key: string) => boolean): () => void {
  let armedUntil = 0

  function onKeydown(event: KeyboardEvent): void {
    if (armsChord(event)) {
      event.preventDefault()
      event.stopPropagation()
      armedUntil = Date.now() + CHORD_DELAY_MS
      return
    }
    if (armedUntil === 0) {
      return
    }
    const key = chordKey(event)
    if (key === null && !event.ctrlKey && !event.metaKey && !event.altKey) {
      // A modifier alone (Shift before `=`): the chord stays armed.
      return
    }
    const armed = Date.now() <= armedUntil
    armedUntil = 0
    if (!armed || key === null) {
      return
    }
    // The second key of the chord goes to it alone, an unknown one included.
    event.preventDefault()
    event.stopPropagation()
    run(key)
  }

  window.addEventListener('keydown', onKeydown, { capture: true })
  return () => window.removeEventListener('keydown', onKeydown, { capture: true })
}
