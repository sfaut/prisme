/**
 * Thème de l'éditeur SQL (CodeMirror), clair ou sombre comme le reste de
 * l'interface.
 *
 * Toutes les couleurs sont des variables CSS du thème (`assets/main.css`) :
 * fond, texte, bordures et bulles, surlignages (`--editor-*`) et
 * coloration syntaxique (`--sql-*`). Le passage du clair au
 * sombre change ces variables, sans reconfigurer l'éditeur.
 */

import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import type { Extension } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import { tags } from '@lezer/highlight'

/**
 * Fond, gouttière, curseur, sélection, bulles et liste de complétion.
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
 * Coloration syntaxique du SQL, d'après les catégories de `@codemirror/lang-sql`.
 */
const highlight = HighlightStyle.define([
  { tag: tags.keyword, color: 'var(--sql-keyword)' },
  { tag: [tags.typeName, tags.standard(tags.name)], color: 'var(--sql-type)' },
  { tag: [tags.string, tags.special(tags.string)], color: 'var(--sql-string)' },
  { tag: [tags.number, tags.bool, tags.null], color: 'var(--sql-number)' },
  { tag: tags.special(tags.name), color: 'var(--sql-variable)' },
  { tag: [tags.lineComment, tags.blockComment], color: 'var(--sql-comment)', fontStyle: 'italic' },
  { tag: [tags.operator, tags.punctuation], color: 'var(--fg-muted)' },
])

/**
 * Thème complet de l'éditeur, à placer après `basicSetup`.
 */
export function editorTheme(): Extension {
  return [chrome, syntaxHighlighting(highlight)]
}
