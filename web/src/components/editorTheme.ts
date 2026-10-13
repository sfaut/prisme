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
  '&': { backgroundColor: 'var(--surface)', color: 'var(--fg-strong)' },
  '.cm-content': { caretColor: 'var(--fg-strong)' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--fg-strong)' },
  '.cm-gutters': {
    backgroundColor: 'var(--surface-muted)',
    color: 'var(--fg-dimmed)',
    borderRight: '1px solid var(--line)',
  },
  '.cm-activeLine': { backgroundColor: tint('--fg', 4) },
  '.cm-activeLineGutter': { backgroundColor: tint('--fg', 8) },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection':
    { backgroundColor: tint('--accent', 25) },
  '.cm-selectionMatch': { backgroundColor: tint('--accent', 12) },
  '&.cm-focused .cm-matchingBracket': { backgroundColor: tint('--success', 25) },
  '&.cm-focused .cm-nonmatchingBracket': { backgroundColor: tint('--error', 25) },
  '.cm-tooltip': {
    backgroundColor: 'var(--surface-elevated)',
    color: 'var(--fg)',
    border: '1px solid var(--line)',
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
