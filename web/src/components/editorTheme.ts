/**
 * Thème de l'éditeur SQL (CodeMirror), clair ou sombre comme le reste de
 * l'interface.
 *
 * Toutes les couleurs sont des variables CSS : celles du thème de Nuxt UI
 * pour le fond, le texte, les bordures et les bulles, et les `--sql-*` de
 * `assets/main.css` pour la coloration syntaxique. Le passage du clair au
 * sombre change ces variables, sans reconfigurer l'éditeur.
 */

import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import type { Extension } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import { tags } from '@lezer/highlight'

/**
 * Couleur de l'interface mêlée de transparence, pour les surlignages.
 *
 * @param color Variable CSS de la couleur.
 * @param percent Part de la couleur, en pourcents.
 */
function tint(color: string, percent: number): string {
  return `color-mix(in oklab, var(${color}) ${percent}%, transparent)`
}

/**
 * Fond, gouttière, curseur, sélection, bulles et liste de complétion.
 */
const chrome = EditorView.theme({
  '&': { backgroundColor: 'var(--ui-bg)', color: 'var(--ui-text-highlighted)' },
  '.cm-content': { caretColor: 'var(--ui-text-highlighted)' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--ui-text-highlighted)' },
  '.cm-gutters': {
    backgroundColor: 'var(--ui-bg-muted)',
    color: 'var(--ui-text-dimmed)',
    borderRight: '1px solid var(--ui-border)',
  },
  '.cm-activeLine': { backgroundColor: tint('--ui-text', 4) },
  '.cm-activeLineGutter': { backgroundColor: tint('--ui-text', 8) },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection':
    { backgroundColor: tint('--ui-primary', 25) },
  '.cm-selectionMatch': { backgroundColor: tint('--ui-primary', 12) },
  '&.cm-focused .cm-matchingBracket': { backgroundColor: tint('--ui-success', 25) },
  '&.cm-focused .cm-nonmatchingBracket': { backgroundColor: tint('--ui-error', 25) },
  '.cm-tooltip': {
    backgroundColor: 'var(--ui-bg-elevated)',
    color: 'var(--ui-text)',
    border: '1px solid var(--ui-border)',
    borderRadius: 'var(--ui-radius)',
  },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    backgroundColor: 'var(--ui-primary)',
    color: 'var(--ui-bg)',
  },
  '.cm-completionDetail': { color: 'var(--ui-text-muted)' },
  '.cm-panels': { backgroundColor: 'var(--ui-bg-elevated)', color: 'var(--ui-text)' },
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
  { tag: [tags.operator, tags.punctuation], color: 'var(--ui-text-muted)' },
])

/**
 * Thème complet de l'éditeur, à placer après `basicSetup`.
 */
export function editorTheme(): Extension {
  return [chrome, syntaxHighlighting(highlight)]
}
