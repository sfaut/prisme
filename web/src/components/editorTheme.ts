/**
 * Theme of the SQL editor (CodeMirror), light or dark like the rest of the
 * interface.
 *
 * All the colors are CSS variables of the theme (`assets/main.css`):
 * background, text, borders and tooltips, highlights (`--editor-*`) and
 * syntax highlighting (`--sql-*`). Switching from light to dark changes these
 * variables, without reconfiguring the editor.
 */

import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import type { Extension } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import { tags } from '@lezer/highlight'

/**
 * Background, gutter, cursor, selection, tooltips and completion list.
 */
const chrome = EditorView.theme({
  '&': { backgroundColor: 'var(--surface)', color: 'var(--fg-strong)' },
  '.cm-content': { caretColor: 'var(--fg-strong)' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--fg-strong)' },
  '.cm-gutters': {
    backgroundColor: 'var(--surface-muted)',
    color: 'var(--fg-dimmed)',
    borderRight: 'var(--line-width) solid var(--line)',
  },
  // Line numbers smaller than the text, centered on their line.
  '.cm-lineNumbers .cm-gutterElement': {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'flex-end',
    fontSize: 'var(--editor-gutter-font-size)',
  },
  '.cm-activeLine': { backgroundColor: 'var(--editor-active-line)' },
  '.cm-activeLineGutter': { backgroundColor: 'var(--editor-active-gutter)' },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection':
    { backgroundColor: 'var(--editor-selection)' },
  '.cm-selectionMatch': { backgroundColor: 'var(--editor-selection-match)' },
  '&.cm-focused .cm-matchingBracket': { backgroundColor: 'var(--editor-bracket-match)' },
  '&.cm-focused .cm-nonmatchingBracket': { backgroundColor: 'var(--editor-bracket-mismatch)' },
  '.cm-tooltip': {
    backgroundColor: 'var(--surface-elevated)',
    color: 'var(--fg)',
    border: 'var(--line-width) solid var(--line)',
    borderRadius: 'var(--control-radius)',
  },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    backgroundColor: 'var(--accent)',
    color: 'var(--surface)',
  },
  '.cm-completionDetail': { color: 'var(--fg-muted)' },
  '.cm-panels': { backgroundColor: 'var(--surface-elevated)', color: 'var(--fg)' },
})

/**
 * Syntax highlighting of SQL, after the categories of `@codemirror/lang-sql`.
 */
const highlight = HighlightStyle.define([
  { tag: tags.keyword, color: 'var(--sql-keyword)' },
  { tag: tags.typeName, color: 'var(--sql-type)' },
  { tag: [tags.string, tags.special(tags.string)], color: 'var(--sql-string)' },
  // Numbers and values: NULL, booleans, words that designate a value (CURRENT_DATE...).
  {
    tag: [tags.number, tags.bool, tags.null, tags.standard(tags.name)],
    color: 'var(--sql-number)',
  },
  { tag: tags.special(tags.name), color: 'var(--sql-variable)' },
  { tag: [tags.lineComment, tags.blockComment], color: 'var(--sql-comment)', fontStyle: 'italic' },
  { tag: [tags.operator, tags.punctuation], color: 'var(--fg-muted)' },
])

/**
 * Whole theme of the editor, to place after `basicSetup`.
 */
export function editorTheme(): Extension {
  return [chrome, syntaxHighlighting(highlight)]
}
