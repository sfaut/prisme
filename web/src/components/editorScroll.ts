/**
 * Scrolling of the SQL editor with the keyboard, without moving the cursor
 * (ticket #17): Ctrl+Up and Ctrl+Down scroll the text by one line, as in
 * SQL Server Management Studio or VS Code. The cursor stays where it is, even
 * when it leaves the visible area.
 */

import type { Command, KeyBinding } from '@codemirror/view'

/**
 * Command that scrolls the text by a number of lines, without touching the
 * cursor nor the selection.
 *
 * @param lines Number of lines, negative to scroll up.
 */
export function scrollLines(lines: number): Command {
  return (view) => {
    view.scrollDOM.scrollTop += lines * view.defaultLineHeight
    return true
  }
}

/**
 * Shortcuts of the scrolling: Ctrl+Up scrolls up, Ctrl+Down scrolls down.
 */
export const scrollKeymap: KeyBinding[] = [
  { key: 'Ctrl-ArrowUp', run: scrollLines(-1) },
  { key: 'Ctrl-ArrowDown', run: scrollLines(1) },
]
