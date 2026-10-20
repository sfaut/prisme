<!--
  Data explorer: panel on the right of the screen that shows the JSON or XML
  values selected in the result grid on screen (`dataExplorer` store), in a
  read-only CodeMirror editor.

  - Values: one cell, cells of a column, a range or several ranges of one
    column. A single value shows alone; several are gathered into one
    document, `[…]` for JSON, `<data>…</data>` for XML (`dataFormat.ts`).
    A selection over several columns, or a column that holds neither JSON
    nor XML, shows a message instead.
  - Reading: indented with four spaces, highlighted, everything unfolded at
    first. Each node folds by its arrow in the gutter; the buttons fold or
    unfold everything, or the level of the cursor (`dataFolding.ts`).
  - Long lines: wrapped to the width of the panel or scrolled sideways, by a
    button; the choice is remembered (`dataExplorer` store), off by default.
  - Copy: the text is that of the document, without the folding marks:
    Ctrl+A then Ctrl+C, or the copy button, give valid JSON or XML.
  - The panel follows the selection of the grid on screen, and the grid of
    the active editor when another editor becomes active.
-->
<script setup lang="ts">
import { json } from '@codemirror/lang-json'
import { xml } from '@codemirror/lang-xml'
import { unfoldAll } from '@codemirror/language'
import { Compartment, EditorState } from '@codemirror/state'
import { EditorView, basicSetup } from 'codemirror'
import {
  ChevronsDownUp,
  ChevronsUpDown,
  Copy,
  FoldVertical,
  PanelRightClose,
  TextWrap,
  UnfoldVertical,
} from 'lucide-vue-next'
import { computed, onBeforeUnmount, onMounted, ref, useTemplateRef, watch } from 'vue'

import { useDataExplorerStore } from '@/stores/dataExplorer'

import { dataDocument } from './dataFormat'
import { cursorLevel, foldableNodes, foldingEffects } from './dataFolding'
import { editorTheme } from './editorTheme'
import UiButton from './ui/UiButton.vue'
import { notify } from './ui/toasts'

const explorer = useDataExplorerStore()

const host = useTemplateRef<HTMLDivElement>('host')

/**
 * Editor that shows the document, created when mounting.
 */
let view: EditorView | null = null

/**
 * Language of the editor, changed with the format of the values.
 */
const language = new Compartment()

/**
 * Wrapping of long lines, changed with the preference of the store.
 */
const wrapping = new Compartment()

/**
 * Wrapping extension for the preference of the store.
 */
function currentWrapping() {
  return explorer.wrap ? EditorView.lineWrapping : []
}

/**
 * Document of the selected values, `null` without values to show.
 */
const formatted = computed(() => {
  const content = explorer.content
  return content.state === 'values' ? dataDocument(content.kind, content.cells) : null
})

/**
 * Text of the editor, `null` when a message replaces it.
 */
const text = computed(() =>
  formatted.value !== null && 'text' in formatted.value ? formatted.value.text : null,
)

/**
 * Message shown instead of the document.
 */
const message = computed(() => {
  const content = explorer.content
  switch (content.state) {
    case 'none':
      return 'Sélectionnez des valeurs JSON ou XML d’une colonne de la grille.'
    case 'several':
      return 'La sélection couvre plusieurs colonnes : sélectionnez des valeurs d’une seule colonne.'
    case 'scalar':
      return `La colonne « ${content.column} » ne contient ni JSON ni XML.`
    default:
      return formatted.value !== null && 'message' in formatted.value ? formatted.value.message : ''
  }
})

/**
 * Column and number of the shown values, under the title.
 */
const subtitle = computed(() => {
  const content = explorer.content
  if (content.state !== 'values') {
    return ''
  }
  const count = content.cells.length
  return `${content.column} · ${content.kind.toUpperCase()} · ${count.toLocaleString('fr-FR')} ${count > 1 ? 'valeurs' : 'valeur'}`
})

/**
 * Does the level of the cursor hold a foldable node? Otherwise, the buttons
 * of the level have nothing to do.
 */
const levelFoldable = ref(false)

/**
 * Updates `levelFoldable` after the state of the editor.
 *
 * @param state State of the editor.
 */
function updateLevel(state: EditorState): void {
  const nodes = foldableNodes(state)
  const level = cursorLevel(state, nodes)
  levelFoldable.value = nodes.some((node) => node.level === level)
}

/**
 * Folds or unfolds the nodes of a level, or of all the levels.
 *
 * @param fold Folds if `true`, unfolds otherwise.
 * @param level `cursor` for the level of the cursor, `all` for every level.
 */
function applyFolding(fold: boolean, level: 'cursor' | 'all'): void {
  if (!view) {
    return
  }
  if (!fold && level === 'all') {
    unfoldAll(view)
  } else {
    const state = view.state
    const wanted = level === 'all' ? null : cursorLevel(state, foldableNodes(state))
    view.dispatch({ effects: foldingEffects(state, fold, wanted) })
  }
  view.focus()
}

/**
 * Copies the whole document, without the folding marks.
 */
async function copy(): Promise<void> {
  if (text.value === null) {
    return
  }
  try {
    await navigator.clipboard.writeText(text.value)
    notify('Document copié')
  } catch {
    notify('Copie refusée par le navigateur', 'error')
  }
}

/**
 * Language of the format of the shown values.
 */
function currentLanguage() {
  const content = explorer.content
  return content.state === 'values' && content.kind === 'xml' ? xml() : json()
}

onMounted(() => {
  view = new EditorView({
    parent: host.value!,
    doc: text.value ?? '',
    extensions: [
      basicSetup,
      EditorState.readOnly.of(true),
      EditorState.tabSize.of(4),
      wrapping.of(currentWrapping()),
      editorTheme(),
      language.of(currentLanguage()),
      EditorView.updateListener.of((update) => {
        if (update.docChanged || update.selectionSet) {
          updateLevel(update.state)
        }
      }),
    ],
  })
  updateLevel(view.state)
})

onBeforeUnmount(() => {
  view?.destroy()
  view = null
})

// New values: the document replaces the previous one, all unfolded, the
// cursor at the start.
watch(text, (value) => {
  if (!view || value === null) {
    return
  }
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: value },
    selection: { anchor: 0 },
    effects: language.reconfigure(currentLanguage()),
    scrollIntoView: true,
  })
})

watch(
  () => explorer.wrap,
  () => view?.dispatch({ effects: wrapping.reconfigure(currentWrapping()) }),
)
</script>

<template>
  <div class="data-explorer">
    <div class="flex items-center gap-gap">
      <div class="flex min-w-0 flex-1 flex-col gap-0.5">
        <h2 class="text-title leading-none font-semibold text-fg-strong">Explorateur de données</h2>
        <span class="truncate text-small text-fg-muted" :title="subtitle">{{
          subtitle || ' '
        }}</span>
      </div>
      <div class="flex items-center">
        <ui-button
          :icon="ChevronsDownUp"
          variant="ghost"
          small
          aria-label="Tout replier"
          title="Tout replier"
          :disabled="text === null"
          @click="applyFolding(true, 'all')"
        />
        <ui-button
          :icon="ChevronsUpDown"
          variant="ghost"
          small
          aria-label="Tout déplier"
          title="Tout déplier"
          :disabled="text === null"
          @click="applyFolding(false, 'all')"
        />
        <ui-button
          :icon="FoldVertical"
          variant="ghost"
          small
          aria-label="Replier le niveau du curseur"
          title="Replier le niveau du curseur : tous les nœuds de même profondeur"
          :disabled="text === null || !levelFoldable"
          @click="applyFolding(true, 'cursor')"
        />
        <ui-button
          :icon="UnfoldVertical"
          variant="ghost"
          small
          aria-label="Déplier le niveau du curseur"
          title="Déplier le niveau du curseur : tous les nœuds de même profondeur"
          :disabled="text === null || !levelFoldable"
          @click="applyFolding(false, 'cursor')"
        />
        <ui-button
          :icon="TextWrap"
          variant="ghost"
          :tone="explorer.wrap ? 'accent' : 'neutral'"
          small
          :aria-pressed="explorer.wrap"
          aria-label="Retour à la ligne"
          title="Retour à la ligne : coupe les lignes longues à la largeur du panneau"
          :disabled="text === null"
          @click="explorer.wrap = !explorer.wrap"
        />
        <ui-button
          :icon="Copy"
          variant="ghost"
          small
          aria-label="Copier le document"
          title="Copier le document (JSON ou XML valide, sans les replis)"
          :disabled="text === null"
          @click="copy"
        />
        <ui-button
          :icon="PanelRightClose"
          variant="ghost"
          small
          aria-label="Fermer l’explorateur de données"
          title="Fermer l’explorateur de données"
          @click="explorer.open = false"
        />
      </div>
    </div>
    <p v-if="text === null" class="data-explorer-message">{{ message }}</p>
    <div v-show="text !== null" ref="host" class="data-explorer-editor" />
  </div>
</template>

<style scoped>
.data-explorer {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: var(--gap);
  min-block-size: 0;
}

.data-explorer-message {
  color: var(--fg-muted);
  text-wrap: balance;
}

.data-explorer-editor {
  flex: 1;
  min-block-size: 0;
  border: var(--line-width) solid var(--line);
  border-radius: var(--control-radius);
  overflow: hidden;
}

.data-explorer-editor :deep(.cm-editor) {
  block-size: 100%;
  font-size: var(--editor-font-size);
}

.data-explorer-editor :deep(.cm-scroller) {
  font-family: var(--font-code);
}
</style>
