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
  - Search: a JSONPath (JSON) or XPath (XML) expression, run as it is typed
    on the document shown (`dataSearch.ts`). By default, the results are
    highlighted in the document (`dataMatches.ts`); the funnel button
    shows only them, the current one highlighted. In both modes, Enter and
    Shift+Enter go from one result to the next. While the expression is invalid, the previous
    results stay.
  - Copy: the text is that of the document shown, without the folding
    marks: Ctrl+A then Ctrl+C, or the copy button, give valid JSON or XML.
  - The panel follows the selection of the grid on screen, and the grid of
    the active editor when another editor becomes active.
-->
<script setup lang="ts">
import { json } from '@codemirror/lang-json'
import { xml } from '@codemirror/lang-xml'
import { unfoldAll } from '@codemirror/language'
import { Compartment, EditorState, type StateEffect } from '@codemirror/state'
import { EditorView, basicSetup } from 'codemirror'
import {
  ChevronDown,
  ChevronUp,
  ChevronsDownUp,
  ChevronsUpDown,
  Copy,
  FoldVertical,
  Funnel,
  Search,
  TextWrap,
  UnfoldVertical,
} from 'lucide-vue-next'
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  shallowRef,
  useTemplateRef,
  watch,
} from 'vue'

import { useDataExplorerStore } from '@/stores/dataExplorer'

import { dataDocument } from './dataFormat'
import { cursorLevel, foldableNodes, foldingEffects } from './dataFolding'
import { type MatchHighlight, matchHighlight, revealEffects, setMatches } from './dataMatches'
import { type SearchRange, type SearchResult, searchData } from './dataSearch'
import { editorTheme } from './editorTheme'
import PanelHeader from './PanelHeader.vue'
import UiButton from './ui/UiButton.vue'
import UiInput from './ui/UiInput.vue'
import { notify } from './ui/toasts'

const explorer = useDataExplorerStore()

const host = useTemplateRef<HTMLDivElement>('host')

const searchBar = useTemplateRef<HTMLDivElement>('searchBar')

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
 * Is the search field shown?
 */
const searchOpen = ref(false)

/**
 * Expression typed in the search field.
 */
const expression = ref('')

/**
 * Filter mode: only the results are shown. Otherwise (selection mode), they
 * are highlighted in the document.
 */
const filtering = ref(false)

/**
 * Index of the current result among the stops.
 */
const current = ref(0)

/**
 * Format of the shown values, `null` without values.
 */
const kind = computed(() => (explorer.content.state === 'values' ? explorer.content.kind : null))

/**
 * Result of the expression as typed, on the document.
 */
const search = computed<SearchResult>(() =>
  searchOpen.value && text.value !== null && kind.value !== null
    ? searchData(kind.value, text.value, expression.value)
    : { state: 'empty' },
)

/**
 * Last result without error: while an expression is being typed, the
 * previous results stay. Emptied when the document changes.
 */
const found = shallowRef<SearchResult>({ state: 'empty' })

watch([search, text], ([next, value], [, previous]) => {
  if (next.state !== 'error') {
    found.value = next
  } else if (value !== previous) {
    found.value = { state: 'empty' }
  }
})

/**
 * Error of the expression typed, empty if none.
 */
const searchError = computed(() => (search.value.state === 'error' ? search.value.message : ''))

/**
 * Text of the editor: the document, or only the results in filter mode;
 * `null` when a message replaces it.
 */
const shown = computed(() => {
  const result = found.value
  if (text.value === null) {
    return null
  }
  return filtering.value && (result.state === 'nodes' || result.state === 'value')
    ? result.text
    : text.value
})

/**
 * Results highlighted in the document, in selection mode.
 */
const matches = computed<SearchRange[]>(() =>
  !filtering.value && found.value.state === 'nodes' ? found.value.matches : [],
)

/**
 * Places the arrows go through: the results highlighted in the document, or
 * the results shown alone in filter mode.
 */
const stops = computed<SearchRange[]>(() => {
  const result = found.value
  if (result.state !== 'nodes') {
    return []
  }
  return filtering.value ? result.results : result.matches
})

/**
 * Results to highlight, with the current one: all of them in selection mode;
 * in filter mode, where the whole document is results, the current one
 * alone.
 *
 * @param index Index of the current result among the stops.
 */
function highlight(index: number): MatchHighlight {
  return filtering.value
    ? { matches: stops.value.slice(index, index + 1), current: 0 }
    : { matches: matches.value, current: index }
}

/**
 * Number of results, or place of the current one, beside the field.
 */
const searchStatus = computed(() => {
  const result = found.value
  if (result.state === 'value') {
    return `= ${result.text}`
  }
  if (result.state !== 'nodes') {
    return ''
  }
  if (result.count === 0) {
    return 'Aucun résultat'
  }
  if (stops.value.length > 0) {
    return `${(current.value + 1).toLocaleString('fr-FR')} / ${stops.value.length.toLocaleString('fr-FR')}`
  }
  return `${result.count.toLocaleString('fr-FR')} ${result.count > 1 ? 'résultats' : 'résultat'}`
})

/**
 * Example of expression in the empty field.
 */
const searchPlaceholder = computed(() =>
  kind.value === 'xml'
    ? 'XPath, par exemple //client/nom'
    : 'JSONPath, par exemple $.clients[*].nom',
)

/**
 * Shows or hides the search field; when shown, it takes the focus.
 */
async function toggleSearch(): Promise<void> {
  searchOpen.value = !searchOpen.value
  if (searchOpen.value) {
    await nextTick()
    searchBar.value?.querySelector('input')?.focus()
  }
}

/**
 * Goes to the next or previous result, from the last to the first and back.
 *
 * @param step `1` for the next one, `-1` for the previous one.
 */
function goToMatch(step: 1 | -1): void {
  const ranges = stops.value
  if (!view || ranges.length === 0) {
    return
  }
  current.value = (current.value + step + ranges.length) % ranges.length
  const range = ranges[current.value]!
  view.dispatch({
    selection: { anchor: range.from },
    effects: [setMatches.of(highlight(current.value)), ...revealEffects(view.state, range)],
  })
}

/**
 * Keys of the search field: Enter and Shift+Enter go from one result to the
 * next, Escape hides the field.
 *
 * @param event Key pressed.
 */
function onSearchKeydown(event: KeyboardEvent): void {
  if (event.key === 'Enter') {
    event.preventDefault()
    goToMatch(event.shiftKey ? -1 : 1)
  } else if (event.key === 'Escape') {
    event.preventDefault()
    searchOpen.value = false
    view?.focus()
  }
}

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
 * Copies the whole document shown, without the folding marks.
 */
async function copy(): Promise<void> {
  if (shown.value === null) {
    return
  }
  try {
    await navigator.clipboard.writeText(shown.value)
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
    doc: shown.value ?? '',
    extensions: [
      basicSetup,
      EditorState.readOnly.of(true),
      EditorState.tabSize.of(4),
      wrapping.of(currentWrapping()),
      editorTheme(),
      language.of(currentLanguage()),
      matchHighlight,
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

// New document (new values, or the results in filter mode): it replaces the
// previous one, all unfolded, the cursor at the start. New results: they are
// highlighted (the current one alone in filter mode), the cursor goes to the
// first one.
watch([shown, stops], ([value, ranges], [previous]) => {
  if (!view || value === null) {
    return
  }
  const replaced = value !== previous
  const first = ranges[0]
  const effects: StateEffect<unknown>[] = [setMatches.of(highlight(0))]
  if (replaced) {
    effects.push(language.reconfigure(currentLanguage()))
  }
  if (first !== undefined) {
    // A replaced document has no folded part left: scrolling is enough.
    effects.push(
      ...(replaced
        ? [EditorView.scrollIntoView(first.from, { y: 'center' })]
        : revealEffects(view.state, first)),
    )
  }
  current.value = 0
  view.dispatch({
    changes: replaced ? { from: 0, to: view.state.doc.length, insert: value } : undefined,
    selection: first !== undefined ? { anchor: first.from } : replaced ? { anchor: 0 } : undefined,
    effects,
    scrollIntoView: replaced && first === undefined,
  })
})

watch(
  () => explorer.wrap,
  () => view?.dispatch({ effects: wrapping.reconfigure(currentWrapping()) }),
)
</script>

<template>
  <div class="data-explorer">
    <panel-header title="Explorateur de données" side="end" @collapse="explorer.open = false">
      <template #subtitle>
        <span class="truncate text-small text-fg-muted" :title="subtitle">{{
          subtitle || ' '
        }}</span>
      </template>
      <template #actions>
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
          :icon="Search"
          variant="ghost"
          :tone="searchOpen ? 'accent' : 'neutral'"
          small
          :aria-pressed="searchOpen"
          aria-label="Rechercher"
          title="Rechercher : JSONPath pour le JSON, XPath pour le XML"
          :disabled="text === null"
          @click="toggleSearch"
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
          title="Copier le document affiché (JSON ou XML valide, sans les replis)"
          :disabled="text === null"
          @click="copy"
        />
      </template>
    </panel-header>
    <div v-if="searchOpen && text !== null" ref="searchBar" class="data-explorer-search">
      <div class="flex items-center gap-2">
        <ui-input
          v-model="expression"
          :icon="Search"
          :placeholder="searchPlaceholder"
          aria-label="Expression de recherche"
          :aria-invalid="searchError !== ''"
          spellcheck="false"
          autocomplete="off"
          class="min-w-0 flex-1"
          @keydown="onSearchKeydown"
        />
        <span
          v-if="searchStatus !== ''"
          class="data-explorer-search-status"
          :title="searchStatus"
          aria-live="polite"
          >{{ searchStatus }}</span
        >
        <ui-button
          :icon="ChevronUp"
          variant="ghost"
          small
          aria-label="Résultat précédent"
          title="Résultat précédent (Maj+Entrée)"
          :disabled="stops.length === 0"
          @click="goToMatch(-1)"
        />
        <ui-button
          :icon="ChevronDown"
          variant="ghost"
          small
          aria-label="Résultat suivant"
          title="Résultat suivant (Entrée)"
          :disabled="stops.length === 0"
          @click="goToMatch(1)"
        />
        <ui-button
          :icon="Funnel"
          variant="ghost"
          :tone="filtering ? 'accent' : 'neutral'"
          small
          :aria-pressed="filtering"
          aria-label="Afficher seulement les résultats"
          title="Filtrer : afficher seulement les résultats, au lieu de les surligner dans le document"
          @click="filtering = !filtering"
        />
      </div>
      <p v-if="searchError !== ''" class="data-explorer-search-error">{{ searchError }}</p>
    </div>
    <p v-if="text === null" class="data-explorer-message">{{ message }}</p>
    <div v-show="text !== null" ref="host" class="data-explorer-editor" />
  </div>
</template>

<style scoped>
@reference "@/assets/main.css";

.data-explorer {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: --spacing(2);
  min-block-size: 0;
}

.data-explorer-message {
  color: var(--fg-muted);
  text-wrap: balance;
}

.data-explorer-search {
  display: flex;
  flex-direction: column;
  gap: --spacing(2);
}

.data-explorer-search :deep(input[aria-invalid='true']) {
  border-color: var(--error);
}

.data-explorer-search-status {
  max-inline-size: --spacing(40);
  overflow: hidden;
  color: var(--fg-muted);
  font-size: var(--font-size-small);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.data-explorer-search-error {
  color: var(--error);
  font-size: var(--font-size-small);
  overflow-wrap: anywhere;
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
