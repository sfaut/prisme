import { afterEach, describe, expect, it, vi } from 'vitest'

import { CHORD_DELAY_MS, armsChord, chordKey, chordLabel, followChords } from '../panelChord'

/**
 * Key event on the window, cancelable, as the browser sends it.
 *
 * @param key Key.
 * @param modifiers Modifiers held.
 */
function press(key: string, modifiers: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key, cancelable: true, bubbles: true, ...modifiers })
  window.dispatchEvent(event)
  return event
}

afterEach(() => {
  vi.useRealTimers()
})

describe('panelChord', () => {
  it('arms on Ctrl-K or Cmd-K alone', () => {
    expect(armsChord(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true }))).toBe(true)
    expect(armsChord(new KeyboardEvent('keydown', { key: 'K', metaKey: true }))).toBe(true)
    // Ctrl-Shift-K deletes the line in CodeMirror; Ctrl-Alt is AltGr.
    expect(
      armsChord(new KeyboardEvent('keydown', { key: 'K', ctrlKey: true, shiftKey: true })),
    ).toBe(false)
    expect(armsChord(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, altKey: true }))).toBe(
      false,
    )
    expect(armsChord(new KeyboardEvent('keydown', { key: 'k' }))).toBe(false)
  })

  it('reads the second key without modifier, lowercased', () => {
    expect(chordKey(new KeyboardEvent('keydown', { key: 'N' }))).toBe('n')
    expect(chordKey(new KeyboardEvent('keydown', { key: '=', shiftKey: true }))).toBe('=')
    expect(chordKey(new KeyboardEvent('keydown', { key: 'n', ctrlKey: true }))).toBeNull()
    expect(chordKey(new KeyboardEvent('keydown', { key: 'Shift', shiftKey: true }))).toBeNull()
    expect(chordLabel('n')).toBe('Ctrl-K puis N')
  })

  it('runs the second key, which reaches nothing else', () => {
    const run = vi.fn<(key: string) => boolean>(() => true)
    const stop = followChords(run)
    expect(press('k', { ctrlKey: true }).defaultPrevented).toBe(true)
    expect(press('r').defaultPrevented).toBe(true)
    expect(run).toHaveBeenCalledWith('r')
    // Disarmed after it: a key alone goes on.
    expect(press('r').defaultPrevented).toBe(false)
    expect(run).toHaveBeenCalledTimes(1)
    stop()
    expect(press('k', { ctrlKey: true }).defaultPrevented).toBe(false)
  })

  it('waits for the second key past Shift, and disarms after its delay', () => {
    vi.useFakeTimers()
    const run = vi.fn<(key: string) => boolean>(() => true)
    const stop = followChords(run)
    press('k', { ctrlKey: true })
    press('Shift', { shiftKey: true })
    press('=', { shiftKey: true })
    expect(run).toHaveBeenCalledWith('=')

    press('k', { ctrlKey: true })
    vi.advanceTimersByTime(CHORD_DELAY_MS + 1)
    expect(press('r').defaultPrevented).toBe(false)
    expect(run).toHaveBeenCalledTimes(1)
    stop()
  })
})
