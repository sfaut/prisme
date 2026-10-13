<!--
  Grille de résultats compacte.

  - Tri : bouton ▲▼ de l'en-tête (croissant, décroissant, aucun).
  - Filtre : texte cherché dans une colonne ou dans toutes.
  - Sélection : clic sur une cellule, sur un numéro de ligne ou sur un nom de
    colonne ; Maj-clic étend, Ctrl-clic ajoute, glisser étend aussi.
  - Copie : Ctrl-C (Ctrl-Maj-C avec les en-têtes), ou les boutons. Ctrl-A
    sélectionne tout, Échap vide la sélection.

  Seules les lignes visibles existent dans la page (défilement virtuel) : la
  grille reste fluide avec des dizaines de milliers de lignes. La logique
  (tri, filtre, sélection, copie) est dans `resultGrid.ts`.
-->
<script setup lang="ts">
import { useVirtualizer } from '@tanstack/vue-virtual'
import { NButton, NInput, NSelect, useMessage } from 'naive-ui'
import { computed, reactive, ref, shallowRef, useTemplateRef, watch } from 'vue'

import type { Cell } from '@/api/querySession'

import {
  type GridFilter,
  type SelectMode,
  type SortState,
  NULL_TEXT,
  emptySelection,
  isSelected,
  nextSort,
  selectAll,
  selectCell,
  selectColumn,
  selectRow,
  selectionToText,
  visibleRows,
} from './resultGrid'

const props = defineProps<{
  /**
   * Noms des colonnes.
   */
  columns: string[]
  /**
   * Lignes du résultat ; le tableau peut grandir pendant la réception, sans
   * être réactif : c'est `rowCount` qui signale les nouvelles lignes.
   */
  rows: Cell[][]
  /**
   * Nombre de lignes reçues.
   */
  rowCount: number
}>()

/**
 * Hauteur d'une ligne, en pixels.
 */
const ROW_HEIGHT = 22

/**
 * Largeur d'un caractère, en pixels, pour estimer la largeur des colonnes.
 */
const CHAR_WIDTH = 7

/**
 * Nombre de lignes lues pour estimer la largeur des colonnes.
 */
const WIDTH_SAMPLE = 200

const message = useMessage()
const scroller = useTemplateRef<HTMLDivElement>('scroller')

const filter = reactive<GridFilter>({ text: '', column: null })
const sort = ref<SortState | null>(null)
const selection = shallowRef(emptySelection())

/**
 * Glissement de souris en cours (sélection d'une plage) ?
 */
let dragging = false

/**
 * Vue : indices des lignes sources, filtrées et triées.
 */
const view = computed(() => {
  // Lu pour que la vue se recalcule à l'arrivée de nouvelles lignes.
  void props.rowCount
  return visibleRows(props.rows, filter, sort.value)
})

/**
 * Largeurs des colonnes, estimées sur l'en-tête et les premières lignes.
 */
const template = computed(() => {
  // Lu pour que les largeurs se recalculent à l'arrivée de nouvelles lignes.
  void props.rowCount
  const widths = props.columns.map((name, column) => {
    let longest = name.length + 2
    for (const row of props.rows.slice(0, WIDTH_SAMPLE)) {
      longest = Math.max(longest, (row[column] ?? NULL_TEXT).length)
    }
    return Math.min(360, Math.max(48, longest * CHAR_WIDTH + 16))
  })
  const numberWidth = String(props.rowCount).length * CHAR_WIDTH + 16
  return [numberWidth, ...widths].map((width) => `${width}px`).join(' ')
})

const filterOptions = computed(() => [
  { label: 'Toutes les colonnes', value: -1 },
  ...props.columns.map((name, index) => ({ label: name, value: index })),
])

const virtualizer = useVirtualizer(
  computed(() => ({
    count: view.value.length,
    getScrollElement: () => scroller.value,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  })),
)

// Tri ou filtre modifié : les positions changent, la sélection n'a plus de sens.
watch([filter, sort], () => {
  selection.value = emptySelection()
})

/**
 * Mode de sélection d'après les touches tenues pendant le clic.
 *
 * @param event Clic.
 */
function modeOf(event: MouseEvent): SelectMode {
  if (event.shiftKey) {
    return 'extend'
  }
  return event.ctrlKey || event.metaKey ? 'add' : 'replace'
}

/**
 * Début d'une sélection de cellule (et d'un éventuel glissement).
 */
function onCellDown(event: MouseEvent, row: number, column: number): void {
  selection.value = selectCell(selection.value, row, column, modeOf(event))
  dragging = true
  window.addEventListener('mouseup', () => (dragging = false), { once: true })
}

/**
 * Glissement sur une cellule : la plage s'étend jusqu'à elle.
 */
function onCellEnter(row: number, column: number): void {
  if (dragging) {
    selection.value = selectCell(selection.value, row, column, 'extend')
  }
}

/**
 * Clic sur un numéro de ligne : sélection de la ligne entière.
 */
function onRowHeader(event: MouseEvent, row: number): void {
  selection.value = selectRow(selection.value, row, props.columns.length, modeOf(event))
}

/**
 * Clic sur un nom de colonne : sélection de la colonne entière.
 */
function onColumnHeader(event: MouseEvent, column: number): void {
  selection.value = selectColumn(selection.value, column, view.value.length, modeOf(event))
}

/**
 * Change le tri d'une colonne : croissant, décroissant, aucun.
 */
function toggleSort(column: number): void {
  sort.value = nextSort(sort.value, column)
}

/**
 * Copie la sélection dans le presse-papiers.
 *
 * @param withHeaders Ajoute les noms des colonnes en première ligne.
 */
async function copy(withHeaders: boolean): Promise<void> {
  if (selection.value.ranges.length === 0) {
    return
  }
  const text = selectionToText(selection.value, view.value, props.rows, props.columns, withHeaders)
  try {
    await navigator.clipboard.writeText(text)
    message.success('Sélection copiée')
  } catch {
    message.error('Copie refusée par le navigateur')
  }
}

/**
 * Raccourcis clavier de la grille.
 */
function onKeydown(event: KeyboardEvent): void {
  const command = event.ctrlKey || event.metaKey
  if (command && event.key.toLowerCase() === 'c') {
    event.preventDefault()
    void copy(event.shiftKey)
  } else if (command && event.key.toLowerCase() === 'a') {
    event.preventDefault()
    selection.value = selectAll(view.value.length, props.columns.length)
  } else if (event.key === 'Escape') {
    selection.value = emptySelection()
  }
}

/**
 * Valeur de la liste « colonne filtrée » : -1 pour toutes les colonnes.
 */
const filterColumn = computed({
  get: () => filter.column ?? -1,
  set: (value: number) => {
    filter.column = value < 0 ? null : value
  },
})
</script>

<template>
  <div class="result-grid">
    <div class="grid-toolbar">
      <n-input
        v-model:value="filter.text"
        size="tiny"
        clearable
        placeholder="Filtrer"
        class="grid-filter"
      />
      <n-select
        v-model:value="filterColumn"
        :options="filterOptions"
        size="tiny"
        class="grid-filter-column"
      />
      <span class="grid-count">{{ view.length }} / {{ rowCount }} lignes</span>
      <n-button size="tiny" :disabled="selection.ranges.length === 0" @click="copy(false)">
        Copier
      </n-button>
      <n-button size="tiny" :disabled="selection.ranges.length === 0" @click="copy(true)">
        Copier avec en-têtes
      </n-button>
    </div>

    <div ref="scroller" class="grid-scroller" tabindex="0" @keydown="onKeydown">
      <div class="grid-row grid-header" :style="{ gridTemplateColumns: template }">
        <div class="grid-corner" />
        <div
          v-for="(name, column) in columns"
          :key="column"
          class="grid-column-header"
          :class="{ selected: view.length > 0 && isSelected(selection, 0, column) }"
        >
          <span
            class="grid-column-name"
            :title="name"
            @mousedown.prevent="onColumnHeader($event, column)"
          >
            {{ name }}
          </span>
          <button
            class="grid-sort"
            type="button"
            :title="`Trier par ${name}`"
            @click="toggleSort(column)"
          >
            {{ sort?.column === column ? (sort.direction === 'asc' ? '▲' : '▼') : '⇅' }}
          </button>
        </div>
      </div>

      <div class="grid-body" :style="{ height: `${virtualizer.getTotalSize()}px` }">
        <div
          v-for="item in virtualizer.getVirtualItems()"
          :key="item.index"
          class="grid-row"
          :style="{ gridTemplateColumns: template, transform: `translateY(${item.start}px)` }"
        >
          <div class="grid-row-header" @mousedown.prevent="onRowHeader($event, item.index)">
            {{ item.index + 1 }}
          </div>
          <div
            v-for="(cell, column) in rows[view[item.index]!]"
            :key="column"
            class="grid-cell"
            :class="{ null: cell === null, selected: isSelected(selection, item.index, column) }"
            :title="cell ?? NULL_TEXT"
            @mousedown.prevent="onCellDown($event, item.index, column)"
            @mouseenter="onCellEnter(item.index, column)"
          >
            {{ cell ?? NULL_TEXT }}
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.result-grid {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 6px;
  min-height: 0;
}

.grid-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
}

.grid-filter {
  max-width: 240px;
}

.grid-filter-column {
  max-width: 200px;
}

.grid-count {
  margin-right: auto;
  color: #666;
  font-size: 0.8rem;
}

.grid-scroller {
  position: relative;
  flex: 1;
  min-height: 120px;
  /* Garde-fou : une zone de défilement plus haute que la fenêtre dessinerait
     toutes les lignes et figerait l'interface. */
  max-height: 100vh;
  overflow: auto;
  border: 1px solid #ddd;
  font-size: 12px;
  outline: none;
  user-select: none;
}

.grid-row {
  display: grid;
  height: 22px;
  line-height: 22px;
}

.grid-header {
  position: sticky;
  top: 0;
  z-index: 1;
  width: max-content;
  min-width: 100%;
  background: #f4f4f5;
  font-weight: 600;
}

.grid-body {
  position: relative;
}

.grid-body .grid-row {
  position: absolute;
  top: 0;
  left: 0;
  width: max-content;
  min-width: 100%;
}

.grid-corner,
.grid-row-header {
  background: #f4f4f5;
  color: #888;
  text-align: right;
}

.grid-row-header {
  padding: 0 6px;
  cursor: e-resize;
}

.grid-column-header {
  display: flex;
  align-items: center;
  border-left: 1px solid #ddd;
}

.grid-column-name {
  flex: 1;
  padding: 0 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: s-resize;
}

.grid-sort {
  padding: 0 4px;
  border: none;
  background: none;
  color: #888;
  font-size: 10px;
  cursor: pointer;
}

.grid-cell {
  padding: 0 4px;
  overflow: hidden;
  border-bottom: 1px solid #eee;
  border-left: 1px solid #eee;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: cell;
}

.grid-cell.null {
  color: #aaa;
  font-style: italic;
}

.selected {
  background: #dbeafe;
}
</style>
